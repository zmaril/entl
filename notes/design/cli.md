# Entl — CLI design

`entl`, the single static binary (`crates/entl-cli`). It's one of the language surfaces
over [the engine](./engine.md) — the one that needs *no* language at all. Point it at a repo,
let it **pull** git + forge into a store (a local DuckDB by default, any [sink](./multidb.md)
on request), and query it with whatever client you like. For the *why*, see
[purpose.md](../purpose.md).

## Role

For most people, the CLI is the whole product: a self-contained binary that turns a checkout
into a queryable store (a `.duckdb` file by default) and keeps it fresh. It's also the
universal fallback for the multilibrary goal — anything that can run a subprocess and read the
store can use Entl through the CLI, no binding required (see [multilibrary](./multilibrary.md)).

Built with `clap`, shipped as a single `cargo build --release` static binary.

## Surface

```
entl init   [path]                    create the default store + apply its migrations
entl load   [path] [flags]            pull git history + forge once (one-way, incr.)
            --git-only / --github-only   one side only
            --blobs / --trees            also store blob content / tree structure
entl watch  [path] [--interval 60]    continually: pull git, poll forge
            --stream                     emit change batches as NDJSON on stdout
entl sink   [path] --to <url>         push into another target (Postgres, JSONL, Kafka, …)
            --tables … / --exclude …     which tables; --live keeps it tailing
entl analysis merge-conflicts [path]  replay merges → conflict hot zones
            analysis symbols / churn / …   tree-sitter code analysis
entl query  "SQL" [path]              run a query, pretty-print (Arrow table)
entl tables [path]                    list tables
entl path   [path]                    print the resolved .duckdb path
entl status [path]                    show entl_progress: last pull + watermarks
```

`path` defaults to the current directory's git root. Forge auth is auto-detected from `gh`.
`entl path` is the glue that makes `duckdb $(entl path)` "just work."

## How it wraps the engine

The CLI is a thin clap front-end over `entl-core`:

- **`load`** opens the store, migrates, then pulls git (with a live progress spinner off the
  worker counter) and/or the forge once, printing a one-line summary each.
- **`watch`** runs the engine's loop directly: a `notify` watcher on `.git` filtered to ref
  changes triggers an incremental git pull; a timer polls the forge. Pull errors are logged,
  not fatal — the loop keeps running. Single-writer, so a separate reader can query the store
  live while `watch` updates it. It's also the CLI's take on the **stream plane** (see
  [multilibrary](./multilibrary.md)): with `--stream` it drains the engine's change stream to
  **line-delimited JSON on stdout**, so any subprocess in any language can consume the live
  feed with no binding.
- **`sink`** turns on a database [sink](./multidb.md) from the CLI — the zero-binding way to
  push into Postgres, JSONL, Kafka, … with the same table selection the bindings expose.
  One-shot by default; `--live` keeps it tailing the change stream.
- **`analysis`** runs an analysis (a composition over the objects, [analysis.md](./analysis.md))
  and prints the headline result (e.g. top conflict hot zones); the full result is returned, and
  can be persisted — or stood up to recompute forward — into a [sink](./multidb.md).
- **`query` / `tables`** are the read conveniences; everything else reads straight from the
  store — a DuckDB client against the default file, or your sink's own driver.

## Design notes

- **Reads don't go through the CLI by design.** `query` exists for convenience, but the
  intended read path is "open the store with whatever you already use." The CLI owns the
  *control plane* (init / load / watch / sink / analyze), not query dialects.
- **No daemon.** `watch` is an ordinary foreground side-process; nothing is installed or
  backgrounded for you.
- **Stable exit/output** so the CLI is scriptable as the cross-language fallback — summaries
  to stderr, query results to stdout.

