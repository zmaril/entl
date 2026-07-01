# Research brief: git object decompression (zlib *inflate*) is our perf floor

**Goal for the research agent:** find Rust solutions to make **git pack-object
decompression (inflate)** dramatically faster in a parallel git-analysis pipeline,
or to avoid it, **without changing correctness or dropping numstat**.

> Note: this is an **inflate** (decompression) problem, not deflate. We only *read*
> git objects; we never write packs. The DuckDB write side is already overlapped and
> is not the bottleneck.

## What the program does
**entl** is a Rust engine (built on **gitoxide / `gix` 0.85**) that ingests a git repo
into DuckDB: `commits`, `commit_parents`, `refs`, and `file_changes` (per-commit,
per-file **status + numstat**, i.e. lines added/deleted). To compute numstat we line-diff
every changed file's **old and new blob**, which requires **inflating the full content of
every changed blob**. On the benchmark repo (Confluent **ksql**: 39,873 commits, 751,181
file changes) that's ~1.5M blob inflations per full ingest.

Pipeline: the commit walk is a cheap single pass; the heavy per-commit work (commit decode
+ tree-diff + numstat `line_counts`) is parallelized with **rayon** across 12 threads (each
with its own `gix` repo handle, object cache, and diff resource cache), streaming results
through a bounded channel to one DuckDB writer thread.

Current result on ksql: **~6.8s** (full numstat, 12 cores). For reference,
`git log --all --numstat` does the same diff work in **20.5s** single-threaded — so entl is
already **~3× faster than git itself** — but the per-blob inflate is the floor.

## The measured bottleneck
Sampling profile (samply, release+debuginfo, manually symbolicated via `atos`/dSYM,
Apple M-series arm64, 12 physical cores). Self-time as % of total samples:

| % | function |
|---|---|
| 14.3 | `zlib_rs::inflate::inflate_fast_help` |
| 5.3 | `zlib_rs::inflate::inflate` |
| 2.0 | `zlib_rs::inflate::writer::copy_match_runtime_dispatch` |
| ~4 | `zlib_rs::inflate::inftrees::inflate_table` (several lines) |
| **~27** | **total in `zlib-rs` inflate** |
| 4.1 | `gix_pack::data::entry::decode::from_bytes` (pack entry decode) |
| 3.4 | `gix_diff` / `imara-diff` (the actual line diff — small!) |
| 1.3 | `gix_odb` index_lookup; 0.8 `uluru::LRUCache::lookup` (pack delta cache) |
| ~13 | kernel (mmap page faults reading packs); ~10 platform (thread/sync) |

**The diff algorithm is cheap; the cost is decompressing the objects.**

## Key fact about the decompressor
gix 0.85 uses the **`zlib-rs` crate** (the trifectatechfoundation pure-Rust zlib, which
*already* has runtime SIMD dispatch — we confirmed `copy_match_runtime_dispatch` and that
`-Ctarget-cpu=native` only buys ~5%). So this is **not** "zlib-rs is a bad implementation."
The open question is whether a **whole-buffer** decompressor (git objects are decompressed
entirely, not streamed) like **libdeflate** beats a zlib-API streaming impl for this access
pattern — libdeflate is widely cited as ~2× faster at inflate and is used by git-adjacent
tooling.

## Constraints
- **Rust**, parallel (rayon). Primary target macOS arm64; also Linux x86_64 / arm64.
- Built on `gix`; we can drop to lower-level crates (`gix-pack`, `gix-odb`, `gix-object`)
  or alternatives (`git2`/libgit2, raw pack reading) if needed.
- **numstat is mandatory and must match `git --numstat` exactly** — no skipping/approximating
  line counts (binaries → no counts, like git). Must be correct across loose objects, packs,
  and delta chains.

## Already ruled out
- **Can't swap zlib backend via gix features.** In gix 0.85 / `gix-features` 0.48, the
  features `zlib-ng`, `zlib-stock`, `zlib-rs` **all** alias to `gix-features/zlib`, whose
  only impl is the `zlib-rs` crate. No feature flag changes the backend.
- **`-Ctarget-cpu=native`**: ~5% (zlib-rs already SIMD-dispatches at runtime).
- **Object cache doesn't help blobs**: each blob is line-diffed once (single-use); the only
  reuse (a file's blob is "new" in commit A and "old" in A's child B) lands on different
  rayon threads.
- Orthogonal wins already done: byte-oids (BLOB not hex), batched ref writes, bounded-channel
  streaming writer, mimalloc.

## Research questions
1. **`zlib-rs` vs alternatives for whole-buffer git-object inflate** (arm64 + x86_64): real
   benchmarks against **libdeflate** (`libdeflater`), **zlib-ng**, Intel **igzip/ISA-L**,
   Cloudflare zlib. Is libdeflate meaningfully faster for the "decompress one whole object"
   pattern, and by how much on arm64?
2. **Integration path for libdeflate into a `gix` pipeline** (gix hardcodes zlib-rs):
   (a) `[patch.crates-io]` / fork `gix-features`; (b) read packs at the `gix-pack` level and
   decompress objects ourselves; (c) custom `gix-odb` backend; (d) any newer gix / config
   that allows pluggable inflate; (e) existing gitoxide issues/PRs on this (maintainer Byron
   has discussed zlib backends).
3. **Redundant inflation — the biggest measured lever (2.44×).** We inflate the *same blob
   multiple times*. On ksql: **1,401,909 blob inflations but only 574,378 distinct blobs =
   ~2.44 inflations per unique blob.** A blob is the "new" side of one commit's diff and the
   "old" side of its child's diff (plus duplicate content), so it's re-decompressed. Our
   **per-thread** gix object cache can't catch this because the two uses land on different
   rayon worker threads — so our parallelism (which gave the 3× speedup) also causes the
   2.44× redundancy. **A shared, cross-thread decoded-blob cache keyed by `oid` could cut
   inflate work ~2.44×** (1.4M → 574k inflations), stacking with a faster decompressor. Open
   questions: best concurrent-cache design (sharded LRU? `dashmap`? read-mostly?) so the
   inflate savings beat lock contention; how to integrate with gix's diff resource cache;
   memory sizing given temporal locality (parent/child commits process close together).
   Also: pack **delta-chain** base re-inflation (`gix_pack::data::delta::apply` + an LRU
   pack-base cache in the profile) — redundant across threads? multi-pack-index / bitmaps?
4. **Cheap-case shortcuts (lower ceiling for us):** 83.8% of changes are *modifications* that
   must inflate both blobs + diff (no shortcut). The skippable tail (~13%): same-oid → 0/0
   without inflate (mode-only changes); pure add/delete already inflate one side (the
   minimum). Worth adding the same-oid guard but it's ~7% of inflations, not the main lever.
5. **What do the fastest Rust git-mining / diff tools do** (difftastic, mergiraf, gix's own
   CLI, large-scale "git mining" frameworks)? Which decompressor + object-access strategy?

## Success criteria
Substantially cut the ~27% inflate cost — target ksql full-numstat ingest from ~6.8s toward
~3–4s — **without** changing numstat semantics/correctness. A `[patch]` or a lower-level
`gix-pack` decompress path is acceptable; forking gix wholesale is a last resort. Deliver:
concrete crate recommendations, integration approach, expected speedup with evidence, and
correctness/portability caveats (esp. arm64).
