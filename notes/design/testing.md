# Entl — testing design

Entl's one promise is **fidelity**: a repo's git history + forge activity, landed in the store of
your choice, unchanged. This is how we *prove* it — not with hand-written example tests, but with a
machine that generates arbitrary git+forge worlds, pushes them through the whole pipeline into every
store and language, pulls them back out, **reassembles the original forms**, and checks they match.
One property run then exercises every database, binding, and git/forge feature at once. For the map,
see [overall.md](./overall.md); the pipeline this checks is [engine.md](./engine.md) +
[multidb.md](./multidb.md) + [multilibrary.md](./multilibrary.md).

## The insight: git OIDs are content hashes

A git commit's object id is a hash of its exact bytes (tree, ordered parents, author/committer
lines, message). So a *faithful* reconstruction reproduces **byte-identical** OIDs — a free,
cryptographic equality check, no fuzzy canonicalization. The git half of the round-trip leans
entirely on this. The forge half has no such hash, so it's checked by canonicalized model equality.

## The model is the source of truth

Everything rides on one generator producing a **`World`** — an abstract git DAG (commits with full
trees + modes, signatures, refs) plus a forge state (PRs/issues/comments/reviews/labels/events/
runs/checks/users) that references it. The `World` is kept deliberately close to git's own object
model so materialization is a direct translation. Generation is property-based
([proptest](https://proptest-rs.github.io/proptest/)): it builds an always-valid *recipe* and folds
it into a `World` deterministically (indices taken modulo their valid range so every value builds,
which keeps shrinking clean). Feature coverage — merges, tags, renames, exec bits, symlinks, draft
PRs, review states — is just the generator's range.

```
World ──materialize──▶ real git repo  +  fake forge (served over a mock GitHub API)
                              │
                          ingest (the real engine)
                              ▼
                        store (DuckDB / SQLite / Postgres / JSONL)
                              │
                           extract
                              ▼
                    reassemble ──▶ git repo′  +  fake forge′   ⇒  compare to the original
```

## The three properties

Each is a distinct kind of round-trip; together they cover the whole pipeline.

- **P1 — store round-trip (the workhorse).** Ingest into DuckDB → a canonical row-set **S0** (all
  tables, normalized: oids→hex, timestamps→RFC3339, bools as bools, rows sorted). For each target
  store, sink S0 → extract **S1** → assert **S0 == S1**. Dialect- and language-agnostic; this is the
  "roundtrips through every database" guarantee. It reuses the sinks' own normalization
  (`cell_json`/`batch_to_json`).
- **P2 — git reassembly (OID-exact).** From the git tables (+ stored trees/blobs/modes),
  **rebuild a real repo** via `git fast-import`; assert its `(ref→oid)` map and reachable-commit-OID
  set equal the source repo's. Cryptographic. This is also a real product capability — `entl rebuild`
  a repo out of Postgres.
- **P3 — forge reassembly.** From the `gh_*` tables, rebuild the in-memory fake forge; assert it
  matches the generator's forge state, canonicalized (normalize ordering, drop derived fields like
  truncation/watermarks).

P1 proves the stores/languages; P2/P3 prove "reassemble into the original forms."

## Extract — reading a store back

The sinks were write-only; the reverse direction (`entl-core/src/extract.rs`) is a first-class new
capability, not just test scaffolding. It reads any store into the **same** canonical form the sinks
write, so a DuckDB snapshot and a SQLite/Postgres/JSONL snapshot of the same data compare equal:
DuckDB reuses `cell_json`; the portable stores already hold hex/RFC3339 text and only need boolean
coercion (SQLite stores 0/1, resolved via the DuckDB schema's `BOOLEAN` columns); JSONL replays its
op-tagged log to a final state. It underpins P1 and the `rebuild` feature alike.

## Full-fidelity git

Reconstructing a repo to identical OIDs needs more than today's metadata+diff snapshot: the full
tree at each commit (paths + contents + modes) and raw blob bytes. So the round-trip work also
**populates the `blobs`/`trees`/`tree_entries` tables** (which the schema already defines) with raw
content + modes, behind `--trees`/`--blobs` pull flags. That is a genuine feature — a full git
mirror you can rehydrate — the test is what forces it to be correct.

**The OID-exact envelope.** Only reproducible git is generated: valid-UTF-8 names/messages,
lightweight tags, regular/exec/symlink modes, no GPG signatures. Features git records but entl
doesn't reproduce byte-for-byte (GPG sigs, exotic encodings, annotated-tag objects) are explicitly
out of the P2 envelope and covered by P1 (metadata) only. Raw-byte fidelity for real-world non-UTF-8
repos is a later extension of the rehydrate capability, not a testing need.

## The mock GitHub server

The forge ingest has no fetch/materialize seam (octocrab is interleaved with row writes; Actions/
Checks are welded to octocrab's own model types), so rather than refactor it we drive the **real**
`ingest_github` end-to-end against a **mock GitHub server**: a localhost HTTP service that serves
GraphQL + REST shaped from a `ForgeWorld` — the exact inverse of the ingest's parsing. This exercises
the entire real pipeline (octocrab, GraphQL/REST parse, watermarks, etag gates, row writes). It
requires the ingest's API base URL to be configurable (default `https://api.github.com`). A schema
guard test asserts the mock covers every table the real ingest writes, so the mock can't silently
drift.

## Where it lives

- **`entl-core`** gets the durable features the test needs: `extract` (store → canonical rows),
  full-fidelity trees/blobs ingest, `rebuild` (store → repo via fast-import), and a
  base-URL-configurable forge ingest. The `entl rebuild` CLI command falls out of this.
- **`crates/entl-testkit`** (a dev crate depending on entl-core) holds the `World` model, the
  proptest generators, `materialize` (World → repo via fast-import; the emitter is shared with
  `rebuild`), the mock GitHub server, the canonicalize/compare helpers, and the property tests.

## Running it

`cargo test -p entl-testkit` runs the properties. The **inner loop is Rust + embedded stores**
(DuckDB/SQLite/JSONL) so it's fast and shrinks to a minimal failing `World`; **Postgres** runs when
`ENTL_TEST_PG=postgres://…` is set (a fresh schema per case). The **language matrix** (Node/Python)
runs on a shared **corpus** — proptest regressions plus curated feature cases serialized to JSON —
rather than in the proptest loop, since cross-process spawns are too slow per generated case; each
binding drives its `sink()` + extract and asserts P1. Generated repos stay small (a few commits/
files) for the shrinking loop; a handful of large curated worlds run outside it.

## Status

- **Done:** `extract` for all four stores (verified S0==S1 on a real repo); the `entl-testkit` crate
  — `World` model, `git fast-import` materialize (deterministic OIDs, merges/tags/modes), proptest
  generators; **P1 green across DuckDB/SQLite/Postgres/JSONL**. Full-fidelity object ingest
  (trees/blobs/modes/raw content, `--objects`), the shared `gitwrite` fast-import primitive,
  `rebuild` + the `entl rebuild` CLI (rehydrate a repo from any store — verified OID-identical from
  SQLite and Postgres), and **P2 green** (OID-exact reassembly). Base-URL-configurable forge ingest
  (`ENTL_GITHUB_API`), the mock GitHub server + `ForgeWorld` + forge generators, and **P3 green**:
  the forge flows through the *real* `ingest_github` against the mock and round-trips through every
  store, plus a reassembly check (the stored `gh_*` tables reconstruct the generated PRs/issues/
  events). The read plane exposed in the bindings (`extract` in Node/Python, backed by core's
  `extract_json`), a deterministic corpus generator (`gen_corpus`), and the **cross-language matrix
  green**: Node and Python each `sink` every corpus repo into SQLite/JSONL and `extract` it back to
  a snapshot byte-identical to the Rust reference.
- **All four phases of the plan are complete.** Follow-ups: check in the proptest regression corpus;
  mock Actions/Checks + sub-resource reassembly; Postgres in the language matrix; `verify-full` TLS.
