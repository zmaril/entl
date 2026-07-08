# Entl — design docs

An index into the per-area design docs under [`notes/design/`](./design/). For the
*why* Entl exists, see [purpose.md](./purpose.md); each doc below covers one part of
the *how*.

- [overall.md](./design/overall.md) — the architecture that ties everything together;
  the map the per-area docs hang off.
- [engine.md](./design/engine.md) — `entl-core`: pulling git + forge activity, keeping
  it fresh, streaming changes, and running the built-in analyses.
- [analysis.md](./design/analysis.md) — the analysis algebra: objects and operations
  that compose into everything from churn counts to burndown charts, computed on demand.
- [cli.md](./design/cli.md) — `entl`, the single static binary: point it at a repo, pull
  git + forge into a store, query with any client.
- [multilibrary.md](./design/multilibrary.md) — exposing the engine in every major
  language without per-language, per-database maintenance exploding.
- [multidb.md](./design/multidb.md) — DuckDB as one sink among many: the ingest and the
  stable schema are the value, not where the data lands.
- [testing.md](./design/testing.md) — proving fidelity by generating arbitrary git+forge
  worlds, round-tripping them through every store and language, and checking they match.
- [docs.md](./design/docs.md) — how the documentation site under `site/` is built.
