# Decoupling entl's arrow version from duckdb's (the IPC read boundary)

Status: exploration (branch `explore/arrow-decouple`). Proves a design; not yet the default.

## The problem — the lockstep constraint

Today AGENTS.md carries this rule:

> entl-core's direct `arrow` dependency must stay in **version-lockstep with the arrow that
> the duckdb crate depends on** (cargo then unifies them into one crate, keeping `RecordBatch`
> a single type). On a duckdb bump, bump `arrow` to duckdb's arrow major — two arrows in
> Cargo.lock means the Arrow handoff stops compiling.

That rule exists because entl carries its change stream in Arrow (`ChangeBatch.batch:
RecordBatch`, re-exported as `entl_core::RecordBatch`), and the same `RecordBatch` type is what
DuckDB hands back from `query_arrow(...)`. When entl's own `arrow` dep and duckdb's bundled
`arrow` are the **same major**, cargo unifies them into one crate instance and the two sides
speak the identical `RecordBatch` — no conversion, everything just compiles. The price is
coupling: entl's **public** Arrow type — the one every binding (node/python/ruby) names — cannot
move except when duckdb moves, and a duckdb bump forces a whole-crate arrow bump in lockstep or
nothing compiles.

We want entl's public Arrow to **float independently**: pick its own arrow major, upgrade on its
own schedule, and not have a duckdb bump ripple into the binding surface.

## The finding — the coupling is tiny

Mapping every place Arrow crosses a boundary in `entl-core` shows the lockstep is load-bearing in
only a handful of spots:

- **The write path never touches Arrow.** Ingest writes to DuckDB row-by-row through the
  `Appender` (`append_row(params![…])`), not via `appender-arrow`. No Arrow crosses *into* duckdb.
- **The in-memory builders own their batches.** `ingest.rs` and `objects.rs` build change-stream
  batches from plain Rust row structs (`BinaryBuilder`/`StringBuilder`/…). These never touch
  duckdb-owned memory — they can build in *any* arrow version natively, with zero conversion.
- **Arrow only crosses OUT of duckdb at the read sites** — `stmt.query_arrow(...)` in
  `driver.rs` (backfill), `github/mod.rs` (the delta/all/keys/subject emitters), `extract.rs`
  (snapshotting), and `db.rs` (`query_table` pretty-print + `query_arrow_ipc`). These, plus the
  one linchpin type (`entl_core::RecordBatch` / `ChangeBatch.batch`), are the *entire* coupling.
- **The bindings and testkit already name only entl's types** (`entl_core::RecordBatch`,
  `batch_ipc`, `batch_to_ffi`) — none has an arrow dep. So flipping the linchpin type inside
  entl-core is invisible to them.

So "decouple" = give entl its own arrow for the linchpin type + the native builders, and convert
duckdb's batches to it at those few read sites.

## The mechanism — Arrow IPC at the read boundary

entl-core now declares **two** arrow crates in `Cargo.toml`:

```toml
# entl's OWN arrow — the public change-stream type + every in-memory builder.
arrow   = { version = "59", default-features = false, features = ["ipc", "ffi", "prettyprint"] }
# The bridge's read-side codec, major-pinned to whatever arrow duckdb bundles, renamed so it
# can coexist. `ipc` here is unioned (via cargo feature-unification) onto duckdb's shared arrow.
arrow58 = { package = "arrow", version = "58", default-features = false, features = ["ipc"] }
```

`entl_core::RecordBatch` and `ChangeBatch.batch` are now `arrow::record_batch::RecordBatch`
(v59). The native builders and the row-readers (`sink::cell_json`/`batch_to_json`,
`ChangeBatch::pretty`) all speak v59 — they build/read entl-owned memory, so nothing converts.

At each read site, DuckDB hands back a `duckdb::arrow` (v58) batch; the new
`crate::arrow_bridge::duckdb_batches_to_entl` converts a `Vec` of them to v59 via a single **Arrow
IPC round-trip**: serialize with the v58 IPC `StreamWriter`, read back with the v59 IPC
`StreamReader`. The Arrow IPC stream format is stable across major versions, so this is a
well-defined, `unsafe`-free conversion. `db.rs::query_arrow_ipc` (whose batches are duckdb's)
simply uses the v58 writer directly; `query_table`'s pretty-print stays entirely on duckdb's
arrow (it never becomes a `ChangeBatch`).

### Why the `arrow58` rename is needed

duckdb's bundled arrow requests only `["prettyprint", "ffi"]` — **no `ipc`**. Today
`arrow::ipc::…` works on duckdb batches only because the single unified arrow crate unions
entl's `ipc` feature onto it. Once entl's arrow is a *different major* (59), duckdb's arrow (58)
is a separate crate instance and loses `ipc`, so there'd be no writer on the duckdb side of the
bridge. Declaring `arrow58` (same major as duckdb, `package = "arrow"`, `features = ["ipc"]`)
makes cargo unify it with duckdb's arrow-58 and union `ipc` in — giving the bridge a v58 writer
without any change to duckdb. The v58 surface is confined to the bridge; everything public is v59.

### Alternatives considered

| Approach | Correctness | Copy cost | Complexity |
|---|---|---|---|
| **Stay locked** (today) | trivially correct | zero | zero — but public type can't float; duckdb bumps ripple to bindings |
| **IPC at read boundary** (chosen) | format is version-stable; no `unsafe` | one buffer copy per read batch | low — one helper + a bridge-side v58 rename |
| **Arrow C Data Interface** (`to_ffi`/`from_ffi`, zero-copy) | correct only if the two crates' `FFI_ArrowArray`/`FFI_ArrowSchema` are transmuted between — they are `#[repr(C)]` ABI mirrors, but the transmute is `unsafe` and fragile across arrow revisions | zero-copy | higher — `unsafe` pointer reinterpretation between two crate versions |

The read sites are all **bounded, one-shot** (a table backfill, a delta emit, a snapshot), so
IPC's single copy is negligible; and the **hot path** — the streaming in-memory builders — never
enters the bridge (it builds v59 natively). IPC buys version-independence with no `unsafe` and one
cheap copy exactly where copies don't matter. The C Data Interface is the fallback if a future
zero-copy hot path ever needs it.

## Proof

Two arrow majors coexist in `Cargo.lock` and everything compiles + passes:

```
$ cargo tree -p entl-core -i arrow@58.3.0
arrow v58.3.0
├── duckdb v1.10504.0
│   └── entl-core
└── entl-core            # the arrow58 rename (bridge read-side)

$ cargo tree -p entl-core -i arrow@59.1.0
arrow v59.1.0
└── entl-core            # entl's OWN public type
```

- `cargo build -p entl-core` — clean (arrow 58.3.0 **and** 59.1.0 both compiled).
- `cargo test -p entl-core` — **14 passed** (incl. `arrow_bridge::round_trips_a_duckdb_batch_into_entl_arrow`,
  `db::query_arrow_ipc_round_trips_and_covers_zero_rows`, the stream IPC/FFI tests, and the
  ingest→poll streaming test).
- `cargo test -p entl-testkit` — **P1 store round-trip, P2 git reassembly, P3 forge** all pass
  (7 tests across `roundtrip`/`forge`/`materialize`), the arrow-type-agnostic end-to-end suite.
- `cargo fmt --all --check` clean; `cargo clippy -p entl-core --all-targets -- -D warnings` clean.
- The napi node binding rebuilds against the flipped core with no source change (it names only
  `entl_core::RecordBatch`/`batch_ipc`).

> Environment note: this sandbox's stable `rustc 1.94.1` cannot compile `libsqlite3-sys 0.38.1`
> (its build script uses the still-unstable `cfg_select!`); pristine `main` fails identically, so
> the build was run on `nightly 1.99.0`. This is unrelated to the arrow change.

## Migration cost & caveats

- **Cost is small**: two dep lines, one `arrow_bridge` module (~40 lines), and mechanical import
  flips from `duckdb::arrow::…` to `arrow::…` at the builder/reader sites. The public API and the
  bindings are unchanged.
- **`appender-arrow` caveat**: the `duckdb` feature `appender-arrow` is enabled but **unused**
  today (the write path is row-based). If a future write path ever appends *Arrow* into duckdb,
  that batch must be in duckdb's arrow — it would need the **reverse** bridge (v59 → v58 via the
  same IPC round-trip). The bridge is symmetric; only the direction helper is missing.
- **On a duckdb bump**: bump the `arrow58` rename's major to duckdb's new arrow (a one-line,
  bridge-local change). entl's public `arrow` stays put — that is the decoupling.

## Recommendation

Adopt it. The lockstep rule couples entl's *entire public Arrow surface* to duckdb's release
cadence for the sake of a zero-copy handoff that only matters on bounded read paths. Trading one
cheap IPC copy at those bounded sites for an independently-versioned public Arrow — with no
`unsafe`, no binding changes, and the streaming hot path untouched — is the right call. Replace
the AGENTS.md "must stay in version-lockstep" gotcha with a pointer to this note and the
bridge-local `arrow58`-bump procedure.
