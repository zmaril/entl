# Entl — multi-language design

Purpose's fifth goal: *expose all of the engine's functionality in every major programming
language.* This is how that's done without it exploding into per-language, per-database
maintenance. For the core, see [engine.md](./engine.md); for the map, [overall.md](./overall.md).

## The split that makes it tractable

Three kinds of work cross the language boundary, and each is handled completely differently:

1. **The control plane — call into the engine.** Pull git + forge, `watch`, switch on a
   [sink](./multidb.md), run an [analysis](./analysis.md). This is real engine code, and there
   is exactly **one** implementation (`entl-core`, Rust); each language gets a thin FFI binding
   that calls in. Nobody reimplements ingest, sinks, or analyses in Python — those are all
   Rust, switched on from the host language.
2. **The read plane — query where the data lives.** Reads don't cross the FFI boundary at all;
   they go straight to the [sink](./multidb.md) with its **own** driver — the local file for
   the default DuckDB, your Postgres connection for a Postgres sink, and so on. The **tables
   are the contract**, so on top of the driver we ship typed models for the ecosystem's usual
   ORM — a projection of the schema, not a reimplementation. DuckDB is only the default; the
   DAG-walk macros are a DuckDB convenience, but the tables themselves are portable everywhere.
3. **The stream plane — subscribe to changes.** The engine's change stream (its mechanism is
   in the change-stream section of [engine.md](./engine.md)) crosses the boundary as **one
   sync, blocking, batched `poll`** over Arrow record batches. Each binding dresses that single
   primitive in the host's natural idiom — async iterator, generator, channel, or callback —
   so live changes reach every language without per-language push-callback machinery, and
   async stays a per-binding wrapper, not a core requirement.

```
control plane  →  thin FFI binding        →  entl-core   (one Rust engine)
read plane     →  the sink's own driver    →  the store   (tables = the contract)
                  + typed ORM models (a projection of the schema)
stream plane   →  blocking poll (Arrow)    →  dressed per language
                  (async iterator / generator / channel / callback)
```

So "every language × every database" never becomes an N×M pile of code: the only per-language
pieces are a thin control-plane shim, a stream adapter that dresses one `poll`, and the ORM
model definitions — all small, all projections of one engine and one schema. Add a language by
adding a binding; the engine, the sinks, and the queries are unchanged.

## The Rust core is synchronous

`entl-core` is a plain **blocking** library — simpler to reason about, and async is a
per-binding concern (below). A binding is a thin shim over calls like:

```rust
let engine = entl_core::open(path)?;              // default store: a local DuckDB
engine.pull_git()?;
engine.pull_github()?;
engine.sink(Target::Postgres { url, tables })?;   // switch on a sink
```

## The first binding: Node/Bun (napi)

`entl-node` is the template for the rest:

- **In-process.** The engine runs inside the host runtime — no subprocess, no second service.
  For the default embedded (DuckDB) store it shares the one open handle, so the ingest and the
  reads see the same data with no cross-process lock.
- **Async at the edge.** Each heavy call offloads to libuv's threadpool via napi `AsyncTask`
  and returns a `Promise`, so the JS event loop never blocks — even on a Linux-kernel-scale
  query. The core stayed sync; the binding added the async.
- **Computed reads, exposed directly.** Some reads aren't SQL — `diffCommits`, `fileAt`, and
  the live git reads (`branchExists`, `commitBodies`, …) are computed from git objects on
  demand — so the binding exposes those engine functions too, run off-thread.
- **The change stream, dressed for JS.** An **async iterator** — `for await (const batch of
  entl.changes())` — over the blocking `poll`, wrapped in `AsyncTask` so the loop never stalls;
  a `watch(repo, onChange)` callback and an EventEmitter are the same stream in other clothes.
  Point it at a PGlite [sink](./multidb.md) and its `live` queries drive realtime for free.
- **Sinks, switched on.** `entl.sink({ target, url, tables })` enables any [sink](./multidb.md)
  — the data-moving is all Rust; the call just turns it on.

## The rest of the languages

Same shape, different FFI. The read ORM is whatever that ecosystem already reaches for against
a SQL store:

| Language | Binding | Read ORM |
|---|---|---|
| TypeScript / Node | napi-rs | Drizzle |
| Python | PyO3 / maturin | SQLAlchemy |
| Ruby | UniFFI | ActiveRecord |
| Java | UniFFI / JNI | jOOQ |
| Go | cgo / UniFFI | GORM |
| PHP | FFI | Doctrine |

And the **CLI is the zero-binding fallback**: anything that can run a subprocess and read the
store can use Entl via `entl` (see [cli.md](./cli.md)).

**Browser / WASM — eventual, read-plane only.** Native `entl-core` doesn't run in a browser
(the native `duckdb` crate doesn't compile to WASM), so the browser isn't a control-plane
target — it's a **read** surface: DuckDB-WASM querying an Entl-produced Parquet/DuckDB artifact.
Full client-side ingest would be a JS reimplementation of the git walk, a further reach.
Flagged, not scheduled — see [multidb](./multidb.md).

## Why this holds

- **One engine, thin edges.** Ingest, sinks, and analyses are all Rust in `entl-core`; the
  per-language code is just the shim, the stream adapter, and ORM models — and it depends on
  none of the ORMs, which live at the read edge and are swappable.
- **The contract is the schema.** The ORMs are thin typed views of stable tables; if an
  ecosystem's ORM isn't a fit, the native driver still works. Nothing about a language reaches
  into the engine.
