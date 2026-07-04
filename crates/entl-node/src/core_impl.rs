//! The hand-written half of the binding — one macro call. The actual
//! `GitCore`/`EntlCore` implementations live ONCE in entl-core
//! (`entl_core::binding_core_impls!`); the napi surface around them is
//! generated from the fluessig catalog.

use crate::generated::*;

entl_core::binding_core_impls!(rename_from = from);
