//! Pure bibliographic logic for borax-tools: the canonical record model,
//! the filename/key template engine, rename planning, and bibliography
//! serialization. This crate performs no I/O.

pub mod bib_output;
pub mod bibtex;
pub mod identifier;
pub mod record;
pub mod rename;
pub mod sanitize;
pub mod template;
