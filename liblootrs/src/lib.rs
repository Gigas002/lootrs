//! Domain logic for lootrs: data model, runners, prefixes and process control.
//!
//! The library never parses CLI arguments, initializes logging or reads
//! configuration files on its own; callers pass resolved values in.

pub mod diag;
pub mod launch;
pub mod model;
pub mod paths;
pub mod prefix;
pub mod process;
pub mod runlog;
mod runner;
pub mod store;
