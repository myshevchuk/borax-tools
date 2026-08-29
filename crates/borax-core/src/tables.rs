//! Externally supplied lookup tables.
//!
//! A table is a named map from a string to a value, read from a
//! tab-separated file the user maintains outside borax. Configuration
//! declares which columns supply the keys and which supplies the
//! values; the [`lookup`](crate::template) filter consults one by name.
//!
//! A table holds no power beyond substitution. Its values are literal
//! text, or — when the declaration says so — template fragments, which
//! are compiled when the table loads and may not themselves look
//! anything up. Rendering therefore stays a total, deterministic
//! function of the record, the template, and a fixed set of compiled
//! templates.
//!
//! This module performs no I/O: [`Table::load`] takes the file's text.
//! Reading the file is the adapter's job.
//!
//! # Matching
//!
//! Keys are compared after folding, by
//! [`crate::template::slug`]. Two spellings of one journal — `J. Am.
//! Chem. Soc.` and `J Am Chem Soc` — therefore reach the same row, and
//! the fold is normative so that another tool reading the same file
//! resolves it the same way.

use std::collections::BTreeMap;
use std::fmt;

use crate::template::{RenderInput, Template, TemplateError};

/// What a table's value column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueKind {
    /// Literal text, substituted verbatim. The default, so a value
    /// containing brackets is data rather than a template.
    #[default]
    Text,
    /// Template source, compiled when the table loads.
    Template,
}

/// A table declaration: which columns to read and what the values are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSpec {
    /// The columns supplying keys. A row contributes one key per
    /// column; a column whose cell is empty contributes none.
    pub key_columns: Vec<String>,
    /// The column supplying values.
    pub value_column: String,
    /// Whether values are literal text or template fragments.
    pub values: ValueKind,
}

/// What a row substitutes for the key that matched it.
#[derive(Debug)]
pub enum Value {
    /// Literal text.
    Text(String),
    /// A compiled fragment, rendered against the same input as the
    /// template that reached it.
    Fragment(Template),
}

impl Value {
    /// The source text this value was built from: the literal itself,
    /// or the fragment's template source.
    ///
    /// Two values are the same value exactly when their sources are
    /// equal, which is what conflict detection compares.
    pub fn source(&self) -> &str {
        todo!("Value::source")
    }

    /// Render this value against `input`.
    ///
    /// A literal renders itself. A fragment renders as a template,
    /// which cannot look anything up, so it produces no misses.
    pub fn render(&self, input: &RenderInput<'_>) -> String {
        todo!("Value::render")
    }
}

/// A row skipped, or a key dropped, while loading a table. Loading
/// continues: a malformed row is not a reason to refuse the file.
///
/// Warnings are ordered by the line they concern. A row producing
/// several — no key cell for two named columns, say — yields one
/// warning per cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableWarning {
    /// The 1-based line of the file the warning is about.
    pub line: usize,
    /// What was skipped and why, for a diagnostic. Prose, not a
    /// contract: callers match on [`TableWarning::line`].
    pub message: String,
}

/// Why a table would not load.
///
/// Every variant is a configuration error: it is a property of the
/// declaration or the file, so it is the same for every input file and
/// is reported before any of them is processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// The file held no header row.
    NoHeader,
    /// The declaration named a column the header does not have.
    MissingColumn { column: String },
    /// The declaration named no key column at all.
    NoKeyColumn,
    /// Two rows fold to one key but substitute different values.
    ///
    /// `first` is the value already held when the clash was found and
    /// `second` the one that clashed with it, so `line` is the later
    /// row's — the one a reader edits to resolve it. Values are
    /// compared by [`Value::source`], so two fragments differ exactly
    /// when their source text does.
    Conflict {
        key: String,
        first: String,
        second: String,
        line: usize,
    },
    /// A template-valued cell would not compile.
    BadFragment {
        line: usize,
        source: String,
        error: TemplateError,
    },
    /// A template-valued cell looks a table up. Indirection stops at
    /// one level, so this is refused at load rather than guarded at
    /// render.
    NestedLookup {
        line: usize,
        source: String,
        table: String,
    },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("TableError::fmt")
    }
}

impl std::error::Error for TableError {}

/// One loaded table: folded keys to the values they substitute.
#[derive(Debug)]
pub struct Table {
    entries: BTreeMap<String, Value>,
}

impl Table {
    /// Load a table from the text of a tab-separated file.
    ///
    /// The first non-blank line is the header; every column `spec`
    /// names must appear in it, and columns it does not name are
    /// ignored. Each subsequent non-blank line is a row contributing
    /// one entry per key column, keyed by
    /// [`crate::template::slug`] of that column's cell.
    ///
    /// A leading byte-order mark is ignored, blank lines are skipped,
    /// and CRLF and LF endings are both accepted.
    ///
    /// Returns the table and the warnings loading produced: a row with
    /// no value cell, a row with no cell for a key column, and a key
    /// folding to the empty string are each skipped and warned about
    /// rather than refused. Two rows folding to one key are an error
    /// when their values differ and neither an error nor a warning when
    /// they agree, since a curated file may legitimately repeat one
    /// form in two columns.
    ///
    /// When `spec.values` is [`ValueKind::Template`], every value cell
    /// is compiled; a cell that will not compile, or that uses the
    /// `lookup` filter, fails the load.
    pub fn load(text: &str, spec: &TableSpec) -> Result<(Table, Vec<TableWarning>), TableError> {
        todo!("Table::load")
    }

    /// The value `input` matches, or `None` when no row folds to the
    /// same key.
    ///
    /// `input` is folded before lookup, so the caller passes the record
    /// value as it stands. An input folding to the empty string matches
    /// nothing.
    pub fn get(&self, input: &str) -> Option<&Value> {
        todo!("Table::get")
    }

    /// How many keys the table holds. Rows contributing several keys
    /// count once per key.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The tables a run may look up, by the names configuration gave them.
#[derive(Debug, Default)]
pub struct LookupTables {
    tables: BTreeMap<String, Table>,
}

impl LookupTables {
    /// No tables at all: what a configuration declaring none resolves
    /// to, and what a template with no `lookup` renders against.
    pub fn new() -> LookupTables {
        LookupTables {
            tables: BTreeMap::new(),
        }
    }

    /// Add `table` under `name`, replacing any table already there.
    pub fn insert(&mut self, name: String, table: Table) {
        self.tables.insert(name, table);
    }

    /// The table called `name`, or `None`.
    pub fn get(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Whether a table called `name` was declared.
    ///
    /// What validates a compiled template's `lookup` tokens before any
    /// file is processed.
    pub fn contains(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// The declared names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }
}

/// Parse the text of a tab-separated file into a header and its rows.
///
/// The first non-blank line is the header, split on tabs. Each
/// subsequent non-blank line is split the same way and zipped with the
/// header: a row with fewer cells than the header has empty cells for
/// the rest, and cells beyond the header's width are dropped.
///
/// A leading byte-order mark is stripped from the header's first
/// column. Blank lines are skipped everywhere and CRLF endings are
/// accepted. Rows carry the 1-based line they came from, for
/// diagnostics.
///
/// Fails only when there is no header row at all; a malformed row is
/// the caller's problem to warn about.
pub fn parse_tsv(text: &str) -> Result<(Vec<String>, Vec<(usize, Vec<String>)>), TableError> {
    todo!("parse_tsv")
}
