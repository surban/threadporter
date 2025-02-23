//! Threadporter — first aid kit for `!Send` + `!Sync` values ⛑️
//!
//! This crate provides help for working with types that are
//! `!Send` + `!Sync` as they usually occur when targeting
//! WebAssembly and working with JavaScript objects.
//!

mod thread_bound;
pub use thread_bound::{thread_bound, ThreadBound};
