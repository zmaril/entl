# translation — abstracting the language bindings

*Thoughts on: "exposing all the APIs feels like boilerplate; we abstracted the DDL — can we
abstract the bindings too?"* Context: entl-core (sync Rust) is exposed three times — napi (Node),
PyO3 (Python), Magnus (Ruby) — and fluessig's runtime will want the same. The same methods, retyped
in three idioms. This is the analog of the pre-fluessig world where the schema was retyped in three
SQL dialects.

## The key reframe: a binding is three surfaces, not one

The boilerplate feeling is real but *uneven*. A binding is really three different things glued
together, and they have wildly different "should we generate this?" answers:

1. **The control surface** — `open`, `load_git`, `query`, `sink`, `extract`, config structs,
   errors→exceptions. Dumb, mechanical, near-identical across languages. **This is the real
   boilerplate, and it is the generatable part.**
2. **The data surface** — Arrow `RecordBatch`es flowing in/out (query results, `changes()`,
   `driverPlan()`). **This is already abstracted — by Arrow itself** (see below). It needs *no*
   binding generation.
3. **The idiom shim** — the async/streaming dressing: napi `AsyncTask→Promise`, PyO3 GIL-release,
   Ruby GVL, the `poll` primitive dressed as a JS async-iterator / Python `__iter__` / etc. **This is
   the valuable 20% and the part generators do worst.** It's each language's personality.

The mistake would be to look for one tool that generates *all three*. The right move is: generate #1,
lean on Arrow for #2, hand-craft a thin #3.

## #2 is already solved: Arrow *is* a cross-language ABI

The biggest lever costs nothing because it already exists. The **Arrow C Data Interface**
(`ArrowSchema` / `ArrowArray` — a tiny stable C ABI) lets any Arrow implementation import a batch
**zero-copy** across an FFI boundary: pyarrow, arrow-js, arrow-rs, nanoarrow all speak it. So for the
data plane you don't generate or serialize anything — you hand each language a pointer and it wraps
it natively. entl's whole "the change stream is Arrow so it crosses FFI cheaply" bet *is* this. Any
binding-abstraction plan should route all bulk data through Arrow FFI and never through a generated
value-marshalling layer. (This also quietly rules out tools whose whole model is serializing values
across the boundary — see UniFFI's caveat.)

## The "describe once, generate glue" tools (#1)

The ecosystem's answer to control-surface boilerplate is IDL- or annotation-driven codegen — the
exact philosophy as fluessig, applied to *interfaces* instead of *schemas*:

- **UniFFI** (Mozilla) — the closest turnkey fit. Annotate Rust (`#[uniffi::export]`, no separate
  IDL needed — the Rust source is the single source of truth) → generates **Python, Ruby, Swift,
  Kotlin** (+ community Go/C#). Handles records, enums, methods, `Result`→exceptions, callbacks, and
  (recent) **async**. This would delete most of the PyO3 + Magnus hand-code.
  - **The catch for us: no first-class Node.** UniFFI's world is mobile (Swift/Kotlin) + Python/Ruby;
    server-side napi isn't a target (there's a React-Native/JSI backend, but that's not Bun/Node).
    Node is load-bearing for entl (the PGlite/`sync.ts` story), so UniFFI alone can't cover the set.
  - Second caveat: UniFFI's default boundary *serializes values* over a C ABI — which you'd bypass
    for Arrow anyway, so it only ever carries the small control-surface types. Fine, as long as big
    data never goes through it.
- **Diplomat** (ICU4X/Unicode) — Rust → C/C++/**JS(WASM)**/Python from a "diplomatic" safe subset.
  Broader language reach incl. JS, but JS is via WASM (wrong runtime for us — see below), and its
  async story is thinner than UniFFI's.
- **WIT + the WASM Component Model** (`wit-bindgen`) — the *cleanest* idea: WIT is a real
  cross-language interface IDL; generate guest+host bindings for Rust/JS/Python/Go/C. This is the
  spiritual twin of what we want. **But it's WASM-gated**, and entl is a native, IO-heavy engine
  (libgit2, DuckDB, the filesystem) — WASM sandboxing + the perf/marshalling tax make it the wrong
  runtime *today*. Revisit only if a WASM build ever makes sense (mostly it doesn't here).
- **flutter_rust_bridge** (Rust↔Dart) — worth noting as *evidence*: it generates genuinely idiomatic
  async **and streams** (Rust `Stream` → Dart `Stream`). Dart-only, so not for us, but it proves the
  streaming-shim (#3) *can* be generated well when a tool commits to one target's idioms. The reason
  we'd still hand-write #3 is that no tool does this well across *our* three targets at once.
- **SWIG / cbindgen** — the old guard. SWIG (C/C++ → many langs) produces un-idiomatic glue and
  fights Rust idioms; cbindgen only emits the C header (you still bind by hand). Skip.

## Why Rust "being good at bindings" is the setup, not the answer

Rust's macro system is *why* the annotation-driven tools (UniFFI's `#[export]`, napi's `#[napi]`)
are ergonomic — the language is good at *generating* bindings, not just writing them. So the honest
statement is: Rust already lets us generate a lot; the question is whether we adopt a
*cross-language* generator (UniFFI-shaped) instead of *N per-language* generators (napi + PyO3 +
Magnus macros), so the surface is declared **once**.

## The entl-specific tension

The two things that make our bindings *good* are exactly the two things generators handle worst:

- **Idiomatic async** — a real JS `Promise`, a Python awaitable/iterator, Ruby GVL-aware blocking.
  Generic generators tend to emit a lowest-common-denominator async that feels foreign in each host.
- **The `poll` stream dressed per language** — `changes()`/`driverPlan()` as an async iterator is a
  deliberate design choice ("one sync primitive, every binding dresses it in its own idiom"). That's
  a *feature*, and it's hand-craft by nature.

So a naive "generate everything" would produce bindings that are *worse* than the ones we have, and
would bypass Arrow. The value isn't in generating 100% — it's in generating the boring 80% while
protecting the 20% that is each language's soul.

## Recommendation (the fluessig-shaped answer)

Same trade as the DDL: **generate the mechanical part, keep the escape hatches, don't boil the
ocean.**

1. **Arrow FFI for all bulk data** (#2). Non-negotiable, already true, biggest lever, zero codegen.
2. **Generate the control surface** (#1) from a single declaration. Two routes:
   - *Adopt UniFFI* for Python + Ruby (+ free Swift/Kotlin if entl ever goes mobile). Deletes the
     most boilerplate, but leaves Node out and means migrating the existing hand-bindings.
   - *Or a small entl/fluessig-specific generator*: declare the API once (an IDL, or proc-macro
     annotations on the core) and emit napi + PyO3 + Magnus skeletons tailored to our exact
     needs — including Node and including the Arrow-FFI handoff. More work, but no Node gap and no
     un-idiomatic output. This is "uniffi-lite for our surface."
3. **Hand-write the thin idiom shim** (#3) over the one sync `poll` primitive — it's small precisely
   *because* the core is sync and Arrow carries the data. Keep it. It's the 20% worth owning.
4. **Node stays special** regardless — napi + the JS async-iterator idiom + the PGlite ecosystem are
   load-bearing and poorly served by the cross-language tools.

Net: realistically **60–80% of the control-surface boilerplate is abstractable**; the data plane is
*already* abstracted by Arrow; the residual is the idiomatic async we *want* to keep hand-made. Not
one magic button — but the same shape of win as fluessig, and a real one.

## The pleasing symmetry — SPIKED AND PROVEN (crates/fluessig/spike/)

fluessig uses TypeSpec's **data** layer (`model`/`scalar`) and pointedly *not* its `interface`/`op`
API layer. The binding problem is the mirror image: it's an **interface/op** problem. So: describe
the API surface in TypeSpec's `op` layer and emit per-language binding skeletons, the same way the
fluessig emitter emits `catalog.json`.

**This is no longer a note — it works.** The spike declares the entl surface once
(`interface Entl { @ctor open(…); loadGit(…): GitStats; query(…): string; @stream changes(…): ChangeBatch }`)
and generates napi + PyO3 + Magnus Rust bindings **that all pass `cargo check`** against a stub core
trait. The move that answers the "generators kill idiom" objection (#3 above):

> **Every op has a SHAPE (`@ctor` | unary | `@stream` | `@manual`). The idiom is hand-written once
> per (language × shape) as a template — AsyncTask→Promise, `allow_threads`, GVL-plain, and the
> poll-stream dressed as async-iterator / `__iter__` / `.next`-with-nil — and the generator applies
> it mechanically.** N ops × M languages collapses to 4×3 templates; the idiom survives *by
> construction*; `@manual` is the escape hatch for the truly bespoke.

All three stream dressings poll the same core primitive (`PollStream::poll`), so entl's "one sync
primitive, every binding dresses it" design is preserved — the generated `NextChangesTask` is
near line-for-line the hand-written `NextChangeTask` in entl-node. Remaining to prove: wire to the
*real* entl-core and pass the existing three test suites; extend types (options bags, enums,
optional params); route bulk data via Arrow C-FFI instead of JSON strings. See the spike README.

This changes the recommendation above: the "small entl/fluessig-specific generator" route (#2's
second option) is now the *demonstrated* one, and it has no Node gap — napi is a first-class
target, which UniFFI can't offer.

**Where the generator lives:** the spike's `gen.mjs` is JS for spike speed only — it has zero
TypeSpec dependency (its input is `api.json`). The real one is a **Rust back-end in the fluessig
crate** (`fluessig bindgen`, committed generated source per entl convention), for the same reason
fluessig colocates DDL + marshalling: the generated `core.rs` trait must agree with the Rust
IR/marshalling types (PollStream bounds, the Arrow-FFI handoff, error conventions), and colocating
makes that agreement hold by construction. Only the checked-program walk (`extract.mjs`) stays
JS/Node — authoring time only. Binding codegen is simply one more codec-family back-end: schema
codecs read `catalog.json`, binding codegen reads `api.json`, one binary emits both.

## Cost / when to actually do this

Adopting a cross-language generator is a **rewrite of working bindings** and **constrains the core
API** to the generator's expressible types. So the trigger is honest boilerplate pain exceeding
migration cost — which is more likely to be true for **fluessig** (bindings don't exist yet; green
field) than for **entl** (bindings exist and work). Concrete cheap next step: **design fluessig's
runtime API to be UniFFI-shaped from day one** (records/enums/`Result`/simple methods; Arrow FFI for
data; sync core), and **prototype UniFFI on one target (Python)** to *measure* the reduction before
committing. If it deletes 70% of the glue and the shim stays hand-written, that's the answer for the
new code — and entl can migrate later, or not.

## Open questions

- Does entl actually need mobile (Swift/Kotlin)? If yes, UniFFI's value jumps (it's the mobile
  answer) and the Node gap becomes the only hole to fill by hand.
- Is the Node async-iterator idiom generatable *for Node specifically* via a napi-side template, so
  even #3 shrinks? (napi-rs has strong codegen; worth a spike.)
- Should the "single API declaration" live as Rust proc-macro annotations (single source of truth =
  the code) or an IDL file (language-neutral, but a second thing to keep in sync)? Annotations lean
  "no boilerplate"; an IDL leans "shared with non-Rust cores." Lean annotations.
