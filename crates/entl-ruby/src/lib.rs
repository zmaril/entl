//! Ruby binding for the entl engine (Magnus) — the Rust sync engine in-process
//! in Ruby. Ruby's GVL serialises access; each call clones the DuckDB
//! connection for its work (same database).
//!
//! **The Magnus surface is GENERATED** (`generated.rs`, from the fluessig
//! catalog's op layer); the engine wiring is hand-written once in
//! `core_impl.rs` (the `GitCore`/`EntlCore` trait impls). No `@manual` ops
//! here (`watch` is offered by the node binding only).

mod core_impl;
mod generated;

pub use generated::*;

use magnus::{Error, Ruby};

#[magnus::init(name = "entl")]
fn init(ruby: &Ruby) -> Result<(), Error> {
    generated::register(ruby)
}
