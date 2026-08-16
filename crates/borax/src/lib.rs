//! The pipeline behind the `borax` binary: configuration, the event
//! stream, and the subcommands that wire `borax-core`, `borax-pdf`, and
//! `borax-sources` together.
//!
//! The binary is a thin shell over this library so that everything it
//! does is reachable from integration tests without spawning a process.

pub mod bib;
pub mod cache;
pub mod config;
pub mod event;
pub mod journal;
pub mod pipeline;
pub mod renaming;
pub mod session;
