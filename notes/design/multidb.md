# Entl — multi-database design

Purpose's third goal: *store the data in any major database.* Entl's value is the ingest and
the stable schema — **not** where the data lands. DuckDB was the original home and is still
the default, but it's now **one sink among many**, and a fully optional one. For the engine
that feeds the sinks, see [engine.md](./engine.md); for the map, [overall.md](./overall.md).

## The shift: DuckDB is a sink, not a mandatory store

Early Entl had a mandatory DuckDB "working store" that everything wrote to and other
databases were copied from. That's no longer the model. There is **one universal sink
abstraction**, and DuckDB is simply its **default target** — first among equals for local
analysis, and skippable. If you'd rather stream straight to Postgres, write nothing but
JSONL, or ship Parquet to object storage and never touch DuckDB, those are all supported
paths.

What makes this safe is the same thing the [multilibrary](./multilibrary.md) story leans on:
the schema is **the contract**. It's stable, plain, and described by the migrations — so any
target that can hold ordinary tables (or rows, or records) can hold Entl's data.

## Sinks are subscribers

The engine ([engine.md](./engine.md)) continually pulls git + forge and emits changes on
a subscription surface. A sink is just a built-in subscriber — a `(cursor, apply)` pair
that reads the change stream (the same blocking `poll` over Arrow record batches every consumer
uses — see the change-stream section of [engine.md](./engine.md)) and writes each batch into
its target. Crucially, user code subscribes to the exact same surface, on equal footing;
the DB sinks have no private back door. Adding a target is writing a new subscriber, not
editing the engine. A subscriber can also compute as it writes — a standing
[analysis](./analysis.md) (merge-conflict hot zones, burndown, …), maintained forward, is just
another thing a sink receives.

Two things fall out of that:

- **Any target, either domain.** The git-derived tables (`commits`, `refs`, `file_changes`,
  …) and the forge tables (`gh_*`) both flow through the same sink. git's source of truth is
  the local repo, so a sink for git data is *optional* — an acceleration; the forge has no
  local source of truth, so a sink is *required* if you want that data to persist. But the
  sink mechanism is identical for both.
- **Independent and resumable, per sink.** Each sink tracks its own progress (`entl_progress`,
  see [engine.md](./engine.md)), so targets run at their own pace and one added months later
  just backfills itself up — no central coordinator, no shared queue.

## Written once in Rust, enabled per language

Every sink adapter — Postgres, SQLite, ClickHouse, JSONL, Parquet, … — is **Rust, inside
`entl-core`**. There is no Python Postgres sink and no Ruby ClickHouse sink; the sink is the
*same* subscriber code no matter which language called it. What each language gets is a thin
**control-plane API to switch a sink on** and configure it — `entl.sink({ target: 'postgres',
url, tables })` in Node, the equivalent call elsewhere — exactly the "one Rust engine, thin
bindings" split from [multilibrary](./multilibrary.md): enabling a sink is a control-plane
call, the data-moving is all Rust. Two payoffs fall out: a new **target** is written once (one
Rust adapter) and lights up in every language at once; a new **language** gets every existing
sink for free.

## Targets

Anything that holds tables, rows, or records is fair game — relational, columnar, streaming,
document, and file formats alike:

| Target | Kind |
|---|---|
| DuckDB | embedded columnar |
| Postgres | relational |
| PGlite | Postgres-in-WASM |
| SQLite | embedded relational |
| MySQL | relational |
| ClickHouse | columnar |
| Kafka | streaming log |
| MongoDB | document |
| generic SQL | any SQL driver |
| JSONL / Arrow / Parquet | files / object storage |
| DuckDB-WASM (browser) | in-browser columnar |

**DuckDB-WASM / browser is a later target, read-sink first.** The native `duckdb` crate the
engine writes through doesn't compile to WASM, so the browser isn't a place `entl-core` runs —
it's a place that *reads*. The low-effort shape is: native Entl produces a Parquet / DuckDB
artifact, and DuckDB-WASM queries it in the browser (`httpfs` range reads, or OPFS for a
persistent local copy) — the browser as just another reader of the contract. Full client-side
ingest (isomorphic-git → DuckDB-WASM, a JS reimplementation of the git walk) is a further
reach, viable for small/medium repos. Flagged so the plan isn't lost; not scheduled.

DuckDB earns "default" on merit — embedded, columnar, fast bulk ingest, OLAP + point lookups,
single file, no server, and the natural home for materialized data and the SQL side of
analysis. But it's **strictly a sink for now**: no federation / query-engine role over the
other targets.

## Schemas & migrations

Each target's tables are created and kept current by **hand-written migrations** — plain,
simple DDL — applied by a **small Rust runner** that records each applied migration in a tiny
**`entl_migrations`** table (in the target itself) and applies whatever's pending. There's no
ORM, no schema-generation, no clever cross-dialect abstraction.

It's deliberately low-tech:

- **Hand-written, per database.** We write the DDL ourselves. It's mostly simple and largely
  common across targets, but where a dialect genuinely differs we just write that DB's variant
  rather than abstract it away.
- **Tested against each DB.** For every target we support, we run its migrations and confirm
  they apply cleanly and produce the schema we expect. That per-DB test is the whole quality
  bar — no framework promising portability, just green tests.
- **Append-only.** An applied migration is never edited; you add the next one. A fresh target
  runs them all in order, an existing one only what's new.
- **Object-ids as hex text off DuckDB.** The DDL types oid columns as hex text in the portable
  targets so they stay readable and driver-agnostic (the local DuckDB keeps them as raw bytes).

Why that's enough: the schema is **small and stable** — git and forge shapes barely change
(see [purpose.md](../purpose.md)) — so hand-writing and testing a handful of tables per DB is
cheap, gives total control, and adds zero magic. Adding a target's schema is writing its
migrations and testing them. That's that.

## Selecting tables — pull and sink

Two independent knobs, and you set both:

- **What the engine pulls.** Choose which resources come down at all — the git side, the forge
  side, or a subset: skip the event feed, leave trees/blobs off (the default), add them when
  you want. What you don't pull never enters the change stream. (Engine flags like `--git-only`
  / `--github-only` and `--trees` / `--blobs`; see [engine.md](./engine.md).)
- **What each sink writes.** A sink can materialize *fewer* tables than you pulled, and a
  *different* subset per target — `commits` + `gh_pull_requests` into Postgres for your app,
  everything into a local DuckDB for analysis, only `gh_events` into Kafka. Include or exclude
  per target, remap names, pick the target schema.

The two compose: the pull is the upper bound — you can only sink what you pulled — and each
sink narrows from there. Both are add/exclude lists, so every target holds exactly the tables
you asked for, nothing more.
