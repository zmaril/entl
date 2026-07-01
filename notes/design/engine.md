# Entl — engine design

The core: `entl-core`. It does the real work — pull git + forge activity, keep it fresh, and
stream changes; it also runs the built-in analyses — compositions over the git objects,
designed in [analysis.md](./analysis.md). The surfaces ([cli](./cli.md), [multilibrary](./multilibrary.md),
[multidb](./multidb.md)) are thin layers over this. For the *why*, see
[purpose.md](../purpose.md); for how it all fits, [overall.md](./overall.md).

## Technology choices

| Concern | Choice | Why |
|---|---|---|
| Git reading | **gitoxide (`gix`)** | pure-Rust, fast; commit/tree/blob/ref reads + tree-diff over an on-disk `.git`. The reason "handle the Linux kernel" is realistic. |
| Forge (GitHub) | **`octocrab`** (REST) + **GraphQL** | token from `gh auth token`, then `GH_TOKEN`/`GITHUB_TOKEN`. GraphQL batches the PR graph; REST for Actions. |

Where the records land is a **sink**, not the engine's concern — DuckDB by default, any target
beside it ([multidb](./multidb.md)). The schema those records form is the **contract**
([overall.md](./overall.md) + the generated schema reference), not something the engine
describes in store-specific terms.

## The git-source seam

A trait so the engine is decoupled from gix specifics (and a native-`git` subprocess fast
path stays possible for very large repos if need be):

```rust
trait GitSource {
    fn refs(&self) -> Result<Vec<RefRecord>>;
    /// Walk commits reachable from refs, stopping at any oid already seen.
    fn log(&self, since: &HashSet<Oid>) -> Result<impl Iterator<Item = CommitRecord>>;
    /// File changes for one commit, diffed vs its first parent.
    fn file_changes(&self, commit: &CommitRecord) -> Result<Vec<FileChange>>;
}
```

The default `GixGitSource` gets change *status* (A/M/D/R) from gix's tree-diff and
`additions`/`deletions` from its blob diff.

## Stored vs. computed

Commits, refs, PRs, and file-changes are **materialized** — emitted as records the pull writes
to the sink. **Diffs and file contents are computed on demand** from the git objects —
`diff_commits` runs a tree-diff + unified-diff, `file_at` reads a blob at a commit. No point
storing every PR's patch when git has the objects and the diff is cheap; this is why Entl can
show a PR's diff without ever having stored it.

Two notes on the materialized side:

- **`file_changes` is a derivation, not a git primitive.** Git stores no "which files changed"
  — the engine computes it by diffing each commit's tree against its first parent's, needing
  only blob *oids*, so it's independent of whether blob content is stored.
- **Trees and blob content are opt-in** (`--trees` / `--blobs`, off by default). Full history,
  the DAG, refs, and `file_changes` come without them; enabling them only *adds* records.

## Pull model

The engine **pulls continually** — forward and backward in time, at the user's choosing:

- **Forward** — tail new activity as it happens (the common case).
- **Backward (backfill)** — walk history back from now (the full initial load, and the way
  you recover).

It only ever *reads* the source — never writing back to git or the forge — so this is a
one-way pull, not a two-way sync.

The pull is **selective**: you pick which resources come down — the git side, the forge side,
or a subset (skip the event feed, leave trees/blobs off) — and what you don't pull never
reaches the change stream or a sink. Each [sink](./multidb.md) can then narrow again from there.

And it offers those changes in **two modes**:

- **Ephemeral stream** — subscribe to changes as they arrive; no history, no restart
  guarantees, just the hooks firing. Right when you only care about what's happening now.
- **Durable** — apply the stream to a [sink](./multidb.md); the sink *is* your durable store.

There is deliberately **no local durable queue**. The durable, replayable logs already exist
*upstream* — the git repo (the commit DAG + reflog) and the forge API (paginated history) —
so a local log would only re-log what the forge already keeps. If you lose state or want
history, you **backfill from source**. That collapses recovery into one primitive doing triple
duty: **initial load = restart recovery = gap repair**. (A durable queue isn't a problem yet;
revisit if it becomes one.)

**Git (incremental, append-only).** Read refs; for each, walk from the tip and **stop** at the
first oid already seen. New commits stream out as records; their file-changes are computed once
per new oid (oids are globally dedup'd). Only `refs` moves; commits are immutable. The heavy
per-commit work (decode + tree-diff + numstat) parallelizes across workers — each with its own
gix handle and caches — feeding one writer, while the ref walk itself is a cheap single pass.
The floor is decompression: zlib-inflating git objects is the irreducible cost. Two deliberate
diff choices: merge commits emit file-changes diffed against the first parent (a superset of
git's default), and renames follow gix's rename tracking (tunable).

**Forge (incremental, rate-limit aware).** GraphQL batches the PR graph (each PR carries
reviews + commits inline); REST for Actions (no GraphQL API). Conditional requests (ETags)
make idle polling nearly free: every pull starts by probing the **event feed**
(`/repos/{o}/{r}/events`) with `If-None-Match` — a `304` means nothing happened, skip every
resource for one request. PRs/issues are watermark-bounded (paginate newest-first, stop at
the first already-pulled).

### Progress & watermarks

Each sink records its own progress in `entl_progress`, keyed per **(repo, resource,
direction)**, so targets resume independently. The two directions ask *different* questions,
and only one of them is a timestamp:

- **Forward = freshness.** A high-watermark on the field the API *actually sorts by* (usually
  `updated_at`), kept a **lag margin** behind the newest thing seen and re-scanning an
  overlapping window each poll. That absorbs bounded reordering and the forge's own eventual
  consistency; the idempotent upserts make the overlap free. A bare "latest timestamp I saw"
  would silently drop anything that lands with a slightly-older key after you advanced.
- **Backfill = completeness of the set.** *Not* a timestamp — "did I reach the end of the
  enumeration," stored as a **resumable page/id cursor + a `complete` flag**. Completeness
  means the API said there are no more pages, not "everything before time *T*." That's immune
  to out-of-order arrival, because it asserts nothing about time.

The principle: **watermark on the iteration key the API guarantees, with a stable tiebreak
(id / opaque cursor) — never on a semantic timestamp you *hope* is monotonic.** A time-ordered
low-watermark is at best a derived convenience for resources that happen to enumerate in
stable chronological order; cursor + done-flag is the source of truth. Out-of-order arrival
never threatens the *data* — idempotent upsert-by-key converges regardless of order; it only
threatens the *bookkeeping*, and only if you'd encoded it as a time partition, which we don't.

**Delivery guarantee.** Advance the watermark only *after* the data is durably written.
Transactional sinks (DuckDB / Postgres / SQLite) write the rows and bump `entl_progress` in
the **same transaction**, so a crash can't leave the cursor ahead of the data; append-only
file sinks order it (data, then progress) and lean on idempotency → at-least-once that
converges on replay. That write-then-advance ordering plus idempotency *is* the recovery story
now that there's no queue. The one clean invariant: once a resource's backfill `complete = true`
**and** forward pull runs with a lag margin, you hold the full set and stay fresh — before
that, you honestly have only "what forward covers + however far backfill has paged."

## Streaming (events)

Three parts to purpose's "stream as events" goal:

1. The **event feed is stored** as a queryable activity log (`gh_events`) — the top-level
   "did anything happen?" signal and a bounded history (the forge caps it, ~90 days, complete
   going forward). It's the one resource backfill *can't* fully recover — the forge expires it
   — so forward-tailing is the only complete capture, the exception to "it's still in the forge."
2. **`watch` is the subscription surface** the whole model hangs on. A `notify` watcher on
   `.git` (filtered to ref changes) triggers incremental git ingest; a timer polls the forge.
   Each cycle reports what changed.
3. The built-in [sinks](./multidb.md) consume that surface, and **user code consumes the
   same stream on equal footing** — no private path for the sinks. A consumer that just
   reacts and forgets is the *ephemeral* mode; one that applies changes to a durable target is
   a *sink* (e.g. into PGlite, whose `live` queries then drive realtime). The pull loop owns
   writes to the default store; readers open read-only.

How that stream is actually moved out to consumers — across languages that don't share a
concurrency model — is its own design, below.

## The change stream — one primitive, every language

`watch` fans changes out to consumers, and consumers arrive in every language with a different
concurrency model — some have sync *and* async, some only threads, some only callbacks. The
engine can't assume any of it, so it exposes **one primitive** and lets each binding dress it up.

**The unit — batched Arrow change records.** Changes cross the FFI boundary as **Arrow record
batches**: columnar, near-zero-copy over Arrow's C Data Interface, and already the stack's
lingua franca (every target language has Arrow; DuckDB and `entl query` speak it natively).
Each batch carries a small envelope — which table, the op (`insert` / `update` / `delete`),
and the **cursor** it advances to. Batching is the speed: crossing FFI per row is death; per
batch it's one hop for thousands of rows.

**The transport — a bounded in-memory buffer per subscriber.** The pull loop writes each batch
into a bounded queue for every subscriber. Bounded means **backpressure**: a slow consumer
backs up *its own* queue and paces *its own* reads — never the engine, never the other
consumers, never unbounded memory. This buffer is transport, not a log; durability is the
sink's DB + `entl_progress` + backfill (see [Pull model](#pull-model)), so there's still no
durable queue.

**The core API — a synchronous, blocking, batched poll.** The one primitive every binding
builds on is cursor-shaped and sync:

```rust
fn poll(&mut self, timeout: Duration) -> Poll  // Batch(Arrow) | Idle | Closed
```

Blocking-with-timeout is the lowest common denominator — *every* language can call a blocking
function on a background thread and get data back. It needs none of the per-language "invoke a
managed function from a foreign thread" machinery (napi `ThreadsafeFunction`, Python's GIL,
JNI attach) that a **push** callback would, and it hands you backpressure for free because the
consumer pulls at its own rate. It's the "core stays sync; async lives in the binding" rule
([multilibrary](./multilibrary.md)) applied to streaming.

**The semantics per language — dress the primitive up.** Each binding maps `poll` to the
host's most natural idiom — async where the language has it, blocking where it doesn't:

| Language | Idiom over `poll` |
|---|---|
| Rust | `Iterator<Item = Batch>` / a channel `Receiver` — native, sync |
| Node/Bun | `for await (const b of entl.changes())` — `poll` wrapped in `AsyncTask` → `Promise`; or `.on('change', cb)` |
| Python | both `for b in changes()` and `async for b in changes()` (poll on an executor) |
| Go | `<-chan Batch`, fed by a polling goroutine |
| Ruby / Java / PHP | a blocking `each` / iterator, or a callback loop on a thread |

The **callback** shape (`watch(onChange)`) is just one of these dresses — the binding runs the
poll loop on a thread and calls back per batch. It's offered where it reads well, but it isn't
the core: a pull-based cursor is what ports everywhere and backpressures correctly, and it
never forces async onto a language that hasn't got it.

**Prior art.** This isn't a novel shape — it's how the established "one native core, many
language bindings" streaming systems work.

- **Pull-based cursor, wrapped per language.** Kafka's clients wrap the `librdkafka` C core and
  expose a `poll()` consume loop in every language
  ([librdkafka overview](https://docs.confluent.io/kafka-clients/librdkafka/current/overview.html),
  [Python client](https://docs.confluent.io/kafka-clients/python/current/overview.html)).
  [gRPC](https://grpc.io/blog/grpc-stacks/)'s bindings are thin wrappers over a shared C-core,
  and a response-streaming call surfaces as the host's native iterator (whose `next()` blocks)
  — sync *and* async per binding.
- **Resumable cursor + token — the closest whole-design match.**
  [MongoDB change streams](https://www.mongodb.com/docs/manual/changestreams/) are a resumable
  cursor keyed by a **resume token** (our envelope cursor), iterated as an iterator / `for await`
  / callback per driver.
- **Arrow batches across the boundary.** The transport follows Arrow's
  [C Data Interface](https://arrow.apache.org/docs/format/CDataInterface.html) and
  [ADBC](https://arrow.apache.org/adbc/current/faq.html), which exist precisely to move columnar
  batches zero-copy across languages.
- **Sync core, async only at the edge.** The documented napi-rs idiom:
  [`AsyncTask` → `Promise`](https://napi.rs/docs/concepts/async-task) wraps the blocking pull,
  while [`ThreadsafeFunction`](https://napi.rs/docs/concepts/threadsafe-function) is exactly the
  per-language "call JS from a foreign thread" machinery a **push** design would force on us.

The one deliberately less-common choice is using Arrow *record batches* as the change unit
rather than per-row JSON events — a throughput call, and a natural one since DuckDB already
speaks Arrow.

## Open risks

- **Blob/tree size** — the main size risk, hence both off by default.
- **First pull of a huge repo** is a full crawl — run once, in the background.
- **Deleted-user / bot semantics differ by transport** — REST `ghost` vs GraphQL null; bot
  logins normalized to `name[bot]`.
