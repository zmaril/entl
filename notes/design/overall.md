# Entl — overall design

The architecture that ties everything together. For the *why*, see
[purpose.md](../purpose.md); the per-area designs are linked at the bottom.

## What we're building

> Entl turns git repositories and forge activity into streaming data you can work with in any major language, with any major database.

[purpose.md](../purpose.md) commits to five goals. This is how the system delivers them:

| Goal (from purpose) | Where it lives |
|---|---|
| Pull git + forge activity as data, continually | **the engine** — [engine.md](./engine.md) |
| Stream that data as events software can react to | **the engine** (the change stream + `watch`) |
| Store it in any major database | **multidb** — [multidb.md](./multidb.md) |
| Perform common and custom analysis of it | **analysis** — [analysis.md](./analysis.md) |
| Expose all of the above in every major language | **multilibrary** — [multilibrary.md](./multilibrary.md) + **cli** — [cli.md](./cli.md) |

Plus the [docs site](./docs.md), which is how all of it is explained.

## One engine, many surfaces

The system is one Rust engine and a set of thin surfaces over it. The trick that keeps
"every language, every database" from becoming an N×M maintenance explosion is a hard
separation into three layers — only one of which is per-language:

1. **The engine.** Pulls git + forge, computes the derivations, and emits a **change stream**
   — the pull loop, on-demand diffs, the analyses. *One* Rust implementation (`entl-core`).
   This is where all the real work is.
2. **The contract (the moat).** A stable **schema** — plain tables + a few macros. Once the
   bytes are in that shape, *the data itself is the interface*. Everything interoperates
   through it, in whatever store holds it.
3. **The surfaces.** The CLI, the language bindings, and the database **sinks** — thin
   consumers, none reimplementing the engine: they drive its control plane, subscribe to its
   change stream, and let reads ride the contract. The sinks are just subscribers — DuckDB by
   default, any target beside it, or none.

```
  .git  ─────►┌──────────────────────────┐  changes    ┌──────────────┐
  (live)      │         entl-core         │───────────►│    sinks     │  DuckDB (default),
  forge ─────►│  pull → records → emit    │ (subscribe) │ schema+macros │  Postgres, PGlite,
  (GitHub…)   │  + watch + analysis       │───────────►│              │  JSONL … — or none
              └──────────────────────────┘             └──────┬───────┘
                  ▲             ▲                              │ the contract (stable schema)
                  │             │                              ▼
              CLI (clap)   language bindings              reads
              a surface    (control plane + subscribe)  • any driver / ORM on the sink
                                                        • user code on the same stream
```

## What it is, concretely

- **Read-only over the source.** Entl observes — it never mutates the working repo or
  pushes to a forge. That single non-goal shrinks the surface area enormously (see
  purpose, "why it's possible").
- **A puller, not a database.** Entl continually *pulls* git + forge and lands the data in
  the sink you choose — a local DuckDB file by default, or a remote Postgres, object storage,
  wherever. Local is a convenient default, not the point: the engine runs where you run it,
  and the data lands where you send it.
- **Forge-general.** The git half of the data is universal; only the forge half is
  per-platform, and it's namespaced (`gh_*`) so the next forge slots in beside it.
- **Incremental + continuous.** Re-pulling is cheap; `watch` keeps sinks fresh — pulling
  forward (tail) or backward (backfill), your choice — and emits changes as they happen.
- **Deliberately low-tech.** Hand-written migrations, thin language bindings, no ORM or
  codegen. The schema is small and stable (git + forge shapes barely change), so plain,
  tested, and boring beats clever — most of Entl is glue, and that's the point.

## Workspace layout

```
entl/
  crates/
    entl-core    the engine: pull (gix + forge APIs), change stream, watch, analysis, sinks + migrations
    entl-cli     the single static binary  →  cli.md
    entl-node    the first language binding (napi)  →  multilibrary / multidb
  site/          the documentation site  →  docs.md
  notes/         purpose + these design docs
```

## The design docs

- [engine.md](./engine.md) — the core: git + forge ingest, the pull model, streaming.
- [analysis.md](./analysis.md) — composing operations over the objects into derived structures, on demand or as a standing sink.
- [cli.md](./cli.md) — the CLI surface.
- [multilibrary.md](./multilibrary.md) — exposing the engine in every language.
- [multidb.md](./multidb.md) — storing the data in any database.
- [docs.md](./docs.md) — the documentation site.


