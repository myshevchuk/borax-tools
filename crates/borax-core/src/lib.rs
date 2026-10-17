//! Pure bibliographic logic for borax-tools: the canonical record model,
//! the filename/key template engine, rename planning, bibliography
//! serialization, the library store, and the content index's entries.
//! This crate performs no I/O.

pub mod bib_output;
pub mod bibtex;
pub mod content;
pub mod identifier;
pub mod index;
pub mod library;
pub mod record;
pub mod rename;
pub mod sanitize;
pub mod tables;
pub mod template;
pub mod time;
