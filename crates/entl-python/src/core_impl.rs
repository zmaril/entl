//! The hand-written half of the binding — one macro call. The actual
//! `GitCore`/`EntlCore` implementations live ONCE in entl-core
//! (`entl_core::binding_core_impls!`); the PyO3 surface around them is
//! generated from the fluessig catalog. (`from_`: Python renames
//! `TableRename.from` — a Python keyword.)

use crate::generated::*;

entl_core::binding_core_impls!(rename_from = from_);
