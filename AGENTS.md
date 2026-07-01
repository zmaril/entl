# AGENTS.md

Guidance for coding agents working in this repo. For the *why*, see
[notes/purpose.md](./notes/purpose.md); for the *how*, the [design docs](./notes/design/) —
`overall`, `engine`, `analysis`, `cli`, `multilibrary`, `multidb`, and `docs`.

## What Entl is

A local engine that ingests a repo's **git history + GitHub activity** and lands it in the
**store of your choice** — DuckDB by default, but Postgres, SQLite, JSONL/Parquet and more are
all valid **sinks** — then exposes it for query via a CLI, a Rust crate, and in-process
Node/Bun bindings. DuckDB is the *default sink*, not a requirement: git reads come live from
the repo, and the sinks are subscribers to the engine's change stream (see
[notes/design/multidb.md](./notes/design/multidb.md)). Entl is read-only and never mutates the working repo or pushes to GitHub.

## Layout

```
crates/entl-core     the engine (Rust). Write path uses the raw duckdb crate; schema is
                     hand-written SQL migrations. Sync, no async.
crates/entl-cli      the CLI (init/load/watch/analysis/query/tables).
crates/entl-node     napi bindings: the engine in-process in Node/Bun. Async lives here
                     (AsyncTask → Promise). Also the PGlite/Postgres sink (sync.ts).
site/                the docs site (Fumadocs — Next.js + MDX, static export). See notes/design/docs.md.
notes/               design docs.
```

## Build & test

```sh
# core + CLI (entl-node is excluded from the default set — it builds via napi)
cargo build --release
cargo test                       # Rust tests (e.g. crates/entl-core/src/github/mod.rs)

# the napi addon → .node + index.js + index.d.ts
cd crates/entl-node && bun run build
bun run gen                      # regenerate the PGlite-sink table types (tables.gen.ts)
bun test                         # coverage test: the sink must cover every entl table

# the CLI
./target/release/entl load ./some-repo --db data.duckdb
./target/release/entl query "SELECT * FROM gh_pull_requests LIMIT 5" --db data.duckdb

# the docs site
cd site && bun install
bun run dev                      # dev server, localhost:3000
bun run build                    # static export → site/out  (prebuild runs `gen`)
bun run gen                      # regenerate the reference pages from source
```

**After changing `entl-core`**, rebuild the consumers to see the change: the napi addon
(`cd crates/entl-node && bun run build`) and the CLI (`cargo build --release`). The docs
generator reads source *files*, so it picks up changes without a rebuild.

## Conventions

- **Forge-namespacing.** GitHub tables are `gh_*`; git-generic tables (`commits`, `refs`,
  `file_changes`, …) are bare so a future forge reuses them. Keep new GitHub tables `gh_`.
- **Migrations are append-only SQL.** Editing an existing migration does **not** re-run on
  an existing DB — add a new `000N_*.sql`, or delete the (derived, rebuildable) `.duckdb`
  cache in dev. A fresh DB applies all migrations in order.
- **Schema docs live in the migrations.** A `--` comment block above `CREATE TABLE`
  documents the table; a trailing `-- …` on a column documents the column. These are inert
  SQL and flow into the generated schema reference. Same idea for Rust (`///`), the napi
  bindings (JSDoc), and the CLI (clap `///` help) — the generator ports all of them.
- **entl-core stays synchronous.** Async is a per-binding concern; the napi layer offloads
  to a threadpool and returns Promises. Don't make the core async.
- **In-process means one DB.** The napi binding shares the DuckDB connection via
  `try_clone()` — no second process, no cross-process file lock.
- **Stored vs computed.** Commits/refs/PRs/file-changes are materialized; diffs and blob
  contents are computed on demand from git objects (`diff_commits`, `file_at`).
- **Don't edit generated files.** The docs `reference/*` pages and `tables.gen.ts` are
  generated — edit the source (migrations / Rust / napi types) and run the generator.

## Gotchas

- `crates/entl-node/index.d.ts` and `sync.ts` are **gitignored** (built/source-of-truth),
  so they aren't on a docs-only build host. The docs generator is tolerant: it skips a page
  whose source is missing and keeps the committed copy.
- The docs reference generator escapes MDX-special chars in ported prose (`{`/`}`/`|`); see
  notes/design/docs.md if a schema comment breaks the build.

## Working agreement

- **Do not commit, open PRs, or merge unless told.** Branch before committing on `main`.
- **Do not modify production unless told.**
- Report outcomes honestly — if tests fail, say so with the output.
