# AGENTS.md

Guidance for coding agents working in this repo. For the *why*, see
[notes/purpose.md](./notes/purpose.md); for the *how*, the [design docs](./notes/design/) —
`overall`, `engine`, `analysis`, `cli`, `multilibrary`, `multidb`, `testing`, and `docs`.

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
                     (AsyncTask → Promise). sync.ts is a thin executor over the core driver
                     sink (entl.driverPlan()) — mirrors into PGlite/Postgres.
crates/entl-python   PyO3 bindings (built via maturin, mixed layout): the engine in-process in
                     CPython (`entl._entl`) + `entl.models` (generated SQLAlchemy read-plane —
                     read-only; create_all/drop_all are guarded, the sink owns the schema).
crates/entl-ruby     Magnus bindings: the engine in-process in Ruby (rb_sys/rake-compiler build).
schema/              entl's catalog: entl.tsp (TypeSpec, authored) + the emitted catalog.json /
                     api.json / schema_docs.json. The schema TOOL that lowers + generates from
                     these — fluessig — lives in its own repo (github.com/zmaril/fluessig); entl
                     invokes it at codegen time (scripts/gen.sh). See the schema convention below.
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
bun run gen                      # regenerate ALL generated artifacts from schema/entl.tsp (runs
                                 # scripts/gen.sh): schema_gen.rs, schema_docs.json, entl.models
                                 # (py), tables.gen.ts + schema.gen.ts, the 3 binding surfaces.
                                 # Needs the fluessig tool: a checkout at ../fluessig, or
                                 # FLUESSIG_DIR=<path>. CI's `node` job fails on any drift.
bun test                         # coverage test: the sink must cover every entl table

# the Python addon (PyO3 → maturin). Excluded from the default cargo set, like entl-node.
cd crates/entl-python
uv venv && uv pip install --group dev   # ALL build/test deps — declared once in pyproject's
                                        # [dependency-groups] (maturin/pytest/sqlalchemy/pyarrow;
                                        # entl itself ships no pyarrow — ChangeBatch speaks the
                                        # PyCapsule interface, consumers bring their own Arrow)
# Use the venv's own maturin/python (NOT `uv run maturin` — its editable install can load a
# stale .so after a rebuild).
.venv/bin/maturin develop
# entl.models is GENERATED from schema/entl.tsp — regenerate via `bun run gen`
# in crates/entl-node (one command regenerates every ORM artifact + the Rust schema).
.venv/bin/python -m pytest tests/   # sink/extract/rebuild/matrix/arrow + the SQLAlchemy models

# the Ruby addon (Magnus, rb_sys). Needs a Ruby 3.x/4.x + LIBCLANG_PATH → arm64 libclang.
cd crates/entl-ruby
bundle install                      # deps declared once in the Gemfile (rb_sys, minitest)
LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib bundle exec ruby extconf.rb && make
bundle exec ruby -I. -Itest test/test_entl.rb   # Entl.new / sink / query / extract / arrow ipc

# the CLI: pull a repo and sync into a target DB (sqlite / jsonl / postgres)
./target/release/entl sink ./some-repo --to sqlite   --dest out.db
./target/release/entl sink ./some-repo --to postgres --dest "postgres://user:pw@host/db" \
    --tables commits,gh_pull_requests --rename commits=git_commits --schema entl

# the CLI
./target/release/entl load ./some-repo --db data.duckdb
./target/release/entl query "SELECT * FROM gh_pull_requests LIMIT 5" --db data.duckdb
# rehydrate a repo from a store (needs `--objects` at sink time):
./target/release/entl sink ./some-repo --to sqlite --dest s.db --db :memory: --no-github --objects
./target/release/entl rebuild --from sqlite --dest s.db --out /tmp/rehydrated

# the round-trip property tests (notes/design/testing.md). Embedded stores always run;
# Postgres runs when ENTL_TEST_PG is set.
cargo test -p entl-testkit                                  # P1 store round-trip, P2 OID-exact, P3 forge
ENTL_TEST_PG=postgres://postgres:pg@localhost:55432/entl cargo test -p entl-testkit
# the cross-language matrix: generate a corpus, then each binding sinks + extracts it back.
cargo run -p entl-testkit --bin gen_corpus -- /tmp/entl-corpus
ENTL_CORPUS=/tmp/entl-corpus bun test matrix.test.ts        # (in crates/entl-node)
ENTL_CORPUS=/tmp/entl-corpus python -m pytest tests/test_matrix.py  # (in crates/entl-python)

# the docs site
cd site && bun install
bun run dev                      # dev server, localhost:3000
bun run build                    # static export → site/out  (prebuild runs `gen`)
bun run gen                      # regenerate the reference pages from source
bun test examples.test.ts        # RUN every code block in guides/cookbook.mdx against a fixture
                                 # repo, so docs can't drift. Needs the CLI (`cargo build --release`),
                                 # the napi addon, and the python venv built first.
```

**After changing `entl-core`**, rebuild the consumers to see the change: the napi addon
(`cd crates/entl-node && bun run build`), the Python addon (`cd crates/entl-python && uv run
maturin develop`), and the CLI (`cargo build --release`). The docs generator reads source
*files*, so it picks up changes without a rebuild.

## Conventions

- **Forge-namespacing.** GitHub tables are `gh_*`; git-generic tables (`commits`, `refs`,
  `file_changes`, …) are bare so a future forge reuses them. Keep new GitHub tables `gh_`.
- **One schema mechanism: the fluessig catalog, generated into code.** The schema's single source
  of truth is `schema/entl.tsp` (all tables, keys, relations, docs). The tool that lowers +
  generates it, **fluessig, lives in its own repo** (github.com/zmaril/fluessig); entl invokes it
  at codegen time via `scripts/gen.sh` (which locates fluessig via a `../fluessig` checkout or
  `FLUESSIG_DIR`). The chain: `schema/entl.tsp` → (fluessig emitter) → `schema/catalog.json` +
  `schema/api.json` → (`fluessig-gen`) → the COMMITTED `schema_gen.rs` (per-dialect
  `__table__`-templated DDL + PKs, consumed by `db.rs` and the sinks at zero runtime cost) +
  `schema/schema_docs.json` (feeds the docs site's schema reference) + the ORM/binding surfaces.
  Just run `bun run gen` in `crates/entl-node` — it drives the whole chain. The store is a
  **derived cache**: `db.rs` content-hashes the generated schema + `migrations/duckdb/extras.sql`
  (the one hand-written SQL left — macros + hex views) and on any change **drops every table and
  rebuilds**; the caller re-ingests. **Add a table** = edit `schema/entl.tsp`, run `bun run gen`,
  commit — CI's `node` job regenerates against fluessig and **fails on any drift** in a committed
  artifact. NB: DuckDB Appenders are positional — the generated column order is canonical; ingest
  appenders follow it.
- **Schema docs live in `schema/entl.tsp`.** TypeSpec doc comments (`/** … */`) on models and fields
  flow through `catalog.json` → `schema_docs.json` → the docs site's schema reference. Same idea
  for Rust (`///`), the napi bindings (JSDoc), and the CLI (clap `///` help) — the generator ports
  all of them.
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
- entl-core's **public** `arrow` (the `RecordBatch` in `ChangeBatch` / `entl_core::RecordBatch`)
  is the direct `arrow` crate and **floats independently** of the arrow the `duckdb` crate ships —
  two arrow majors in `Cargo.lock` is **expected**, not a breakage. DuckDB-produced batches get
  converted to entl's arrow at the bounded `query_arrow()` read sites via an Arrow-IPC round-trip in
  `crates/entl-core/src/arrow_bridge.rs` (no `unsafe`); the write path is row-based, so nothing
  crosses into duckdb. On a duckdb **major** bump, the one thing to bump is the bridge-local,
  package-renamed `arrow58` dep in `Cargo.toml` — it exists only to give the read-side IPC *writer*
  the `ipc` feature on duckdb's arrow, and should track duckdb's arrow major (NOT a whole-crate
  lockstep bump). See [notes/design/arrow-ipc.md](./notes/design/arrow-ipc.md) for the rationale.

## Working agreement

- **Do not commit, open PRs, or merge unless told.** Branch before committing on `main`.
- **Do not modify production unless told.**
- Report outcomes honestly — if tests fail, say so with the output.
