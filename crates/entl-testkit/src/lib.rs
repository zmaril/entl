#![recursion_limit = "512"]
//! entl-testkit — property-based round-trip testing for entl.
//!
//! Generate a [`World`] (a git history + forge state), materialize it into a real repo (+ a fake
//! forge), run it through the ingest → store → extract pipeline, reassemble the original forms,
//! and check they match. See the plan in notes / `hashed-wishing-rose`.

pub mod forge;
pub mod forge_gen;
pub mod generate;
pub mod materialize;
pub mod mock;
pub mod reassemble;
pub mod world;

pub use entl_core::gitwrite::{git, import, SnapCommit, SnapRef};
pub use forge::ForgeWorld;
pub use forge_gen::arb_forge_world;
pub use generate::arb_git_world;
pub use materialize::materialize;
pub use mock::MockForge;
pub use world::{GenBlob, GenCommit, GenRef, GenSig, GitWorld, Mode, World};
