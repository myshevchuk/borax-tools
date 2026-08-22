//! Pure bibliographic logic for borax-tools: the canonical record model,
//! the filename/key template engine, rename planning, bibliography
//! serialization, and the collection ledger. This crate performs no I/O.

pub mod bib_output;
pub mod bibtex;
pub mod content;
pub mod identifier;
pub mod ledger;
pub mod record;
pub mod rename;
pub mod sanitize;
pub mod template;
pub mod time;
