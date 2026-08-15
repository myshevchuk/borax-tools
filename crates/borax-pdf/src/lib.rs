//! PDF extraction adapter for borax-tools: embedded-metadata and
//! text-layer identifier extraction behind a [`source::PdfSource`]
//! trait, with a statically linked pdfium backend.
//!
//! Everything above the trait is pure and offline: identifier scanning
//! ([`scan`]) and the tiered orchestration ([`tiered`]) are tested
//! without a PDF engine, and the engine-specific adapter is the only
//! part that needs real files.

pub mod scan;
pub mod source;
pub mod tiered;
