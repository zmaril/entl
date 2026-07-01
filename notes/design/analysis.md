# Entl — analysis design

Purpose's fourth goal: *perform common and custom analysis of the data.* Analysis is the climb
from the primitives git and forge hand you to the structures git never stored. The engine
exposes it as a small **algebra**: a handful of **objects** (the nouns) and a handful of
**operations** (the verbs) that compose into everything from churn counts to burndown charts.
Every one is an **API call** — you name a composition over objects and get the result back,
computed on demand from the git objects (the same machinery the engine already uses for diffs,
blob reads, and conflict replay — see [engine.md](./engine.md)). For the *why*, see
[purpose.md](../purpose.md).

## The objects

Two families of nouns — the primitives git and the forge actually give you. Everything else
(`file_change`, conflicts, symbols, burndown, …) is what the **operations** below *produce*,
not a stored object.

**Git** — content-addressed objects, plus the refs that point into them:

| Object | Is | Keyed by |
|---|---|---|
| `commit` | author/committer, message, a tree, parent(s) — a node in the DAG | `oid` |
| `tree` | a directory snapshot (name → child oid) | `oid` |
| `blob` | file content, deduplicated | `oid` |
| `ref` | a named pointer (branch/tag) into the graph — not content-addressed | name |

**Forge** (`gh_*`) — the collaboration/automation layer, joined to git by commit oid:
`pull_request`, `review`, `comment`, `issue`, `check` / `workflow_run`, `event`, `user`.

## The operations

Six verbs over those objects. Analyses are chains of them.

- **walk** — follow edges through the commit graph from a starting set (a ref, an oid):
  parents, first-parent, ancestors, reachable set, commits-between, merge-base. Yields a
  sequence of commits in some order (topological, first-parent, …). The traversal primitive.
- **diff** — compare two snapshots: two `tree`s → changed paths (`file_change`); a commit vs
  its parent → line hunks; three trees → a 3-way merge (conflicts). Computed on demand.
- **map** — apply a function to each object independently: each `blob` → parsed symbols, each
  `commit` → metadata, each `pull_request` → a latency. Because git objects are
  content-addressed, a `blob` map is **parse-once, cache-forever** — embarrassingly parallel.
- **reduce** — combine a sequence into an accumulator. **Unordered** for counts and
  leaderboards; **ordered** for a topological fold over the DAG that carries state forward and
  reconciles it at merges (the hard kind).
- **join** — relate objects across families by shared keys: forge ↔ git by commit oid,
  `review` ↔ `pull_request`, and so on. Where git meets forge.
- **collect** — gather a chain's output and hand it back. (To keep a result rather than
  recompute it, the sink engine can persist it for you — see the last section.)

(Set ops — filter, intersect — round out `walk`: merge-base is `ancestors(A) ∩ ancestors(B)`.)

## Building analyses = composing operations

Everything is a chain of those verbs, and the layers stack from cheap to expensive:

| Analysis | Composition |
|---|---|
| symbols / metrics per file | **map** blobs |
| first-parent chain, ancestry, merge-base | **walk** |
| churn per file | **walk** commits → **diff** each vs parent → **reduce** by path |
| merge-conflict hot zones | **walk** merges → **diff** (3-way) → **reduce** by path |
| PR review latency, CI flakiness | **join** forge ↔ git → **map** → **reduce** |
| burndown / ownership over time | **walk** topologically → **diff** *each* commit → **ordered reduce** into a per-line age model, reconciled at merges |

Two things worth calling out:

- **`map` is content-keyed and incremental.** Keying a `map` by `blob_oid` (or `merge_oid`)
  means a result is valid forever and skippable once computed; each new object is just one more
  unit — the map extends, never recomputes, and dedups across repos for free.
- **the top layer is a `diff` *per commit*.** Burndown isn't a plain walk — at every commit, in
  order, it computes that commit's line diff (the same diff the engine does for `file_change` /
  `diff_commits`) and folds the hunks into a running per-file, per-era line model, snapshotting
  each tick. A diff per commit + strict ordering + merge reconciliation make it the heaviest
  analysis and the one you can't parallelize per unit.

## Common vs. custom

Common analyses ship as built-ins — Rust, in the engine, exactly like the sinks: the walk
macros, `analyze_conflicts` (walk + 3-way diff + reduce), code analysis (map over blobs), and a
burndown/ownership fold. **Custom** analysis is first-class: because the substrate is the
objects plus these six operations, "I want to know *X* about this repo" is just a different
chain — written in whatever language ([multilibrary](./multilibrary.md)). The engine's job is
to make the operations available and fast; the analyses are compositions on top.

## Standing analyses — automatic via the sink engine

Everything above is an on-demand API call. But because the engine **pulls continually**
([engine.md](./engine.md)) and the content-keyed compositions are incremental, an analysis can
also be **stood up as a [sink](./multidb.md)** — subscribed to the change stream so the engine
computes it *forward* and persists the results as new activity arrives, no re-running. So you
can have these maintained for you:

- **merge-conflict hot zones** kept current as merges land (each new `merge_oid` → 3-way diff → upsert),
- **leaderboards, review-latency, CI-flakiness** that update per new PR or commit,
- **burndown / ownership** series extended tick by tick as the fold advances with history,
- **code-symbol** tables that grow one row per new blob.

It's the same compositions, wired to the pull loop and a sink instead of returned once. And
once the results are materialized in a sink, plain **SQL** over them is a second surface —
DuckDB by default, the DAG walks packaged as macros (a DuckDB convenience; elsewhere a
recursive CTE).
