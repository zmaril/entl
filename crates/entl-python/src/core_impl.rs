//! The hand-written half of the binding — one macro call. The actual
//! `GitCore`/`EntlCore` implementations live ONCE in entl-core
//! (`entl_core::binding_core_impls!`); the PyO3 surface around them is
//! generated from the fluessig catalog. (`from_`: Python renames
//! `TableRename.from` — a Python keyword.)

use crate::generated::*;
// The shared streaming contract (`Poll`/`PollStream`) the `binding_core_impls!`
// macro references bare at the expansion site; generated.rs imports it privately,
// so the glob above doesn't re-export it — bring it into scope explicitly.
use fluessig_runtime::{Poll, PollStream};

entl_core::binding_core_impls!(rename_from = from_);
