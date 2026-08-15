//! PDF extraction adapter for borax-tools: embedded-metadata and
//! text-layer identifier extraction behind a [`source::PdfSource`]
//! trait, over a pure-Rust engine ([`pure`]).
//!
//! Everything above the trait is offline and engine-free: identifier
//! scanning ([`scan`]) and the tiered orchestration ([`tiered`]) are
//! tested without a PDF engine, and the adapter is the only part that
//! needs real files.

pub mod pure;
pub mod scan;
pub mod source;
pub mod tiered;
