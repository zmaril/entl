//! PyO3 binding for the entl engine. The Rust sync engine runs in-process
//! inside CPython; heavy calls release the GIL (`detach`).
//!
//! **The PyO3 surface is GENERATED** (`generated.rs`, from the fluessig
//! catalog's op layer — pyclasses, kwargs-flattened methods, iterator
//! dressing); the engine wiring is hand-written once in `core_impl.rs`
//! (the `GitCore`/`EntlCore` trait impls). No `@manual` ops here yet
//! (`watch` is offered by the node binding only).

mod core_impl;
mod generated;

pub use generated::*;

use pyo3::prelude::*;

/// The compiled module: `entl._entl` (the `entl` package re-exports it).
#[pymodule]
fn _entl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    generated::register(m)
}
