//! The filename/citation-key template engine.
//!
//! Templates use JabRef-style bracket syntax with Better-BibTeX-grade
//! semantics: literal text plus `[field:filter:filter...]` tokens, `||`
//! alternatives inside a token, and per-entry-type template tables.
//! Compilation ([`Template::compile`]) validates everything — fields,
//! filters, regex patterns — so rendering is infallible and
//! deterministic: the same record, template, and input always produce
//! the same string.
//!
//! # Grammar
//!
//! ```text
//! template  := ( literal | token )*
//! token     := "[" chain ( "||" chain )* "]"
//! chain     := field ( ":" filter )*
//! field     := "auth" | "authors" N? | "year" | "title"
//!            | "shorttitle" N? | "journal" | "volume" | "issue"
//!            | "pages" | "firstpage" | "publisher" | "doi"
//!            | "arxiv" | "sha1" | "entrytype"
//! filter    := "lower" | "upper" | "capitalize" | "titlecase"
//!            | "camel" | "slug" | "abbr" | "transliterate"
//!            | "trunc" N | regex-filter | affix-filter
//!            | lookup-filter
//! regex-filter := "regex(" qstring "," qstring ")"
//! affix-filter := ( "prefix" | "suffix" ) "(" qstring ")"
//! lookup-filter := "lookup(" qstring ")"
//! ```
//!
//! Literal text may contain any character except `[`, which always
//! opens a token (there is no escape for a literal bracket). A `]`
//! outside a token is literal. Whitespace around `||` is ignored; no
//! other whitespace is allowed inside a token. `N` is a positive
//! decimal integer with no leading zero. `qstring` is a double-quoted
//! string in which `\"` escapes a quote and `\\` a backslash.
//!
//! # Field semantics
//!
//! Every field renders the empty string when the record lacks it —
//! alternatives (`||`) exist to handle that.
//!
//! - `auth` — the first author's family name.
//! - `authors` / `authorsN` — all (or the first N) family names joined
//!   with `-`; when authors were dropped by N, `-etal` is appended
//!   (`authors2` on Smith, Doe, Roe → `Smith-Doe-etal`).
//! - `year` — the issued year, as the decimal digits of the stored
//!   value.
//! - `title` — the title, verbatim.
//! - `shorttitle` / `shorttitleN` — the first N (default 3)
//!   non-function words of the title, joined by single spaces. Function
//!   words, compared case-insensitively, are: a, an, the, of, on, in,
//!   for, and, or, to, with, at, by, from, into, upon.
//! - `journal` — the container title, verbatim.
//! - `volume` / `issue` / `publisher` — the record's values, verbatim.
//! - `pages` — the page value, verbatim (`1234-1245`).
//! - `firstpage` — the part of `pages` before the first `-`, `–` or
//!   `—`, trimmed; the whole value when it holds none, so an article
//!   number such as `e0123456` survives intact. A value opening with a
//!   dash has nothing before it and renders empty.
//! - `doi` — the normalized DOI.
//! - `arxiv` — the bare arXiv id (no version).
//! - `sha1` — the file hash supplied in [`RenderInput`].
//! - `entrytype` — the record's CSL type string (e.g.
//!   `article-journal`).
//!
//! # Filter semantics
//!
//! Filters apply left to right; "words" are maximal runs separated by
//! ASCII whitespace.
//!
//! - `lower` / `upper` — whole-string case mapping.
//! - `capitalize` — first character uppercased, the rest lowercased.
//! - `titlecase` — each word: first character uppercased, rest
//!   lowercased; single spaces preserved as given.
//! - `camel` — like `titlecase`, then the words are concatenated with
//!   the separators removed (`An Awesome Paper` → `AnAwesomePaper`).
//! - `slug` — transliterate (see below), lowercase, then every run of
//!   characters outside `a-z0-9` becomes a single `-`, with leading and
//!   trailing `-` trimmed.
//! - `abbr` — the first character of each word, concatenated, case
//!   preserved (`J. Chem. Ed.` → `JCE`).
//! - `truncN` — the first N characters (Unicode scalar values).
//! - `transliterate` — fold common Latin letters to ASCII: German
//!   ä→ae, ö→oe, ü→ue, ß→ss (uppercase Ä→Ae, Ö→Oe, Ü→Ue); æ→ae, Æ→Ae,
//!   ø→o, Ø→O, å→a, Å→A, đ→d, Đ→D, ł→l, Ł→L, ñ→n, Ñ→N, ç→c, Ç→C; the
//!   accented vowels à á â ã è é ê ë ì í î ï ò ó ô õ ù ú û ý (and
//!   uppercase forms) lose their accent. Characters without a folding
//!   pass through unchanged.
//! - `prefix("text")` / `suffix("text")` — put `text` before or after
//!   the value, and leave the empty string alone. Being the identity on
//!   empty is the point: it lets a separator belong to the optional
//!   segment it separates, so `[volume:prefix("-")]` contributes `-146`
//!   or nothing at all.
//! - `lookup("table")` — replace the value with what the named table
//!   holds for it, or with the empty string when the table holds no
//!   such key. Matching folds both sides through [`slug`], and a miss
//!   is recorded in [`Rendered::misses`] rather than failing. The
//!   table must be declared in configuration; see [`crate::tables`].
//! - `regex("pattern","replacement")` — replace every non-overlapping
//!   match of the pattern ([`regex`] crate syntax; `$1` group
//!   references work in the replacement). The pattern is compiled at
//!   template-compile time; a bad pattern is a compile error.

use std::collections::HashMap;
use std::fmt;

use regex::Regex;

use crate::record::{EntryType, Name, Record};
use crate::tables::LookupTables;

/// Why a template source failed to compile. Every variant names the
/// offending token so a configuration layer can report "template X,
/// token Y".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// Malformed template text: an unclosed `[`, an empty token or
    /// chain, stray whitespace inside a token, a malformed filter
    /// argument list. `position` is the byte offset of the construct in
    /// the source.
    Syntax { position: usize, message: String },
    /// A field name that is not in the vocabulary.
    UnknownField { token: String },
    /// A filter name that is not in the filter set.
    UnknownFilter { token: String },
    /// A `regex(...)` pattern rejected by the regex engine.
    BadRegex { pattern: String, message: String },
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::Syntax { position, message } => {
                write!(f, "template syntax error at byte {position}: {message}")
            }
            TemplateError::UnknownField { token } => write!(f, "unknown field {token:?}"),
            TemplateError::UnknownFilter { token } => write!(f, "unknown filter {token:?}"),
            TemplateError::BadRegex { pattern, message } => {
                write!(f, "bad regex {pattern:?}: {message}")
            }
        }
    }
}

impl std::error::Error for TemplateError {}

/// What a template renders from: the record plus per-file facts that
/// live outside it.
#[derive(Debug, Clone, Copy)]
pub struct RenderInput<'a> {
    pub record: &'a Record,
    /// Lowercase hex digest of the file's content, for the `sha1`
    /// field; `None` renders it empty.
    pub sha1: Option<&'a str>,
}

/// A lookup that found no row.
///
/// Carries what was asked rather than what folded, because the point of
/// a miss is to tell the user which line to add to their file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Miss {
    /// The table's configured name.
    pub table: String,
    /// The value looked up, as the record held it.
    pub input: String,
}

/// What rendering produced: the string, and every lookup that missed
/// while producing it.
///
/// Misses are returned rather than logged so that the obligation to
/// report them is visible at each call site. They are collected from
/// every chain evaluated, including chains whose output an alternative
/// replaced — a table lacking a journal is worth knowing about even
/// when a fallback covered for it — and appear in evaluation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub misses: Vec<Miss>,
}

/// A compiled template. Compilation performs all validation; rendering
/// cannot fail.
#[derive(Debug)]
pub struct Template {
    source: String,
    segments: Vec<Segment>,
}

impl Template {
    /// Compile template source, validating the whole grammar: bracket
    /// structure, field vocabulary, filter set, filter arguments, and
    /// regex patterns. Fail-fast contract: any error a template can
    /// produce is produced here, never at render time.
    pub fn compile(source: &str) -> Result<Template, TemplateError> {
        Ok(Template {
            source: source.to_string(),
            segments: Parser::new(source).parse()?,
        })
    }

    /// The source text this template was compiled from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The tables this template looks up, in order of first appearance
    /// and without repeats.
    ///
    /// What lets a configuration layer refuse a `lookup` naming no
    /// declared table before any file is processed, and what stops a
    /// table's own fragment from looking anything up. Rendering never
    /// consults it.
    pub fn tables(&self) -> Vec<&str> {
        todo!("Template::tables")
    }

    /// Render the template. Within a token, chains are tried left to
    /// right and the first non-empty result (after its filters) wins;
    /// a token whose chains all render empty contributes nothing.
    ///
    /// `tables` supplies what `lookup` consults; pass
    /// [`LookupTables::new`] when the template has none. Rendering
    /// cannot fail, and is a function of `self`, `input` and `tables`
    /// alone.
    pub fn render(&self, input: &RenderInput<'_>, tables: &LookupTables) -> Rendered {
        let mut text = String::new();
        let mut misses = Vec::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(literal) => text.push_str(literal),
                Segment::Token(chains) => {
                    for chain in chains {
                        let value = chain.render(input, tables, &mut misses);
                        if !value.is_empty() {
                            text.push_str(&value);
                            break;
                        }
                    }
                }
            }
        }
        Rendered { text, misses }
    }
}

// ---------------------------------------------------------------------
// Compiled representation
// ---------------------------------------------------------------------

#[derive(Debug)]
enum Segment {
    Literal(String),
    Token(Vec<Chain>),
}

#[derive(Debug)]
struct Chain {
    field: Field,
    filters: Vec<Filter>,
}

impl Chain {
    fn render(
        &self,
        input: &RenderInput<'_>,
        tables: &LookupTables,
        misses: &mut Vec<Miss>,
    ) -> String {
        let mut value = self.field.render(input);
        for filter in &self.filters {
            value = filter.apply(&value, tables, misses);
        }
        value
    }
}

#[derive(Debug, Clone, Copy)]
enum Field {
    Auth,
    /// `None` keeps every author; `Some(n)` keeps the first `n` and
    /// marks the remainder with `-etal`.
    Authors(Option<usize>),
    Year,
    Title,
    ShortTitle(usize),
    Journal,
    Volume,
    Issue,
    Pages,
    /// The page value up to the first dash of any width; the whole of
    /// it when there is none.
    FirstPage,
    Publisher,
    Doi,
    Arxiv,
    Sha1,
    EntryType,
}

#[derive(Debug)]
enum Filter {
    Lower,
    Upper,
    Capitalize,
    TitleCase,
    Camel,
    Slug,
    Abbr,
    Transliterate,
    Trunc(usize),
    /// Identity on the empty string; otherwise wraps its input.
    Prefix(String),
    /// Identity on the empty string; otherwise wraps its input.
    Suffix(String),
    /// The name of the table to consult.
    Lookup(String),
    Regex {
        regex: Regex,
        replacement: String,
    },
}

/// How many words `shorttitle` keeps when no count is given.
const DEFAULT_SHORT_TITLE_WORDS: usize = 3;

/// Words carrying no subject matter of their own, compared
/// case-insensitively.
///
/// `shorttitle` skips them when choosing the words that name a work.
/// Callers comparing two renderings of one title drop them for the same
/// reason: a shared `the` is not evidence that two titles agree.
pub const FUNCTION_WORDS: [&str; 16] = [
    "a", "an", "the", "of", "on", "in", "for", "and", "or", "to", "with", "at", "by", "from",
    "into", "upon",
];

// ---------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------

struct Parser<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Parser<'a> {
        Parser {
            source,
            position: 0,
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self, c: char) {
        self.position += c.len_utf8();
    }

    fn syntax<T>(&self, message: &str) -> Result<T, TemplateError> {
        Err(TemplateError::Syntax {
            position: self.position,
            message: message.to_string(),
        })
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if !c.is_whitespace() {
                break;
            }
            self.bump(c);
        }
    }

    fn parse(&mut self) -> Result<Vec<Segment>, TemplateError> {
        let mut segments = Vec::new();
        let mut literal = String::new();
        while let Some(c) = self.peek() {
            self.bump(c);
            if c == '[' {
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(Segment::Token(self.parse_token()?));
            } else {
                literal.push(c);
            }
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        Ok(segments)
    }

    /// Parse the chains of a token whose `[` has been consumed, through
    /// the closing `]`.
    fn parse_token(&mut self) -> Result<Vec<Chain>, TemplateError> {
        let mut chains = Vec::new();
        loop {
            self.skip_whitespace();
            chains.push(self.parse_chain()?);
            self.skip_whitespace();
            if self.rest().starts_with("||") {
                self.position += 2;
            } else if self.rest().starts_with(']') {
                self.position += 1;
                return Ok(chains);
            } else if self.rest().is_empty() {
                return self.syntax("unclosed '['");
            } else {
                return self.syntax("expected '||' or ']'");
            }
        }
    }

    fn parse_chain(&mut self) -> Result<Chain, TemplateError> {
        let field = self.parse_field()?;
        let mut filters = Vec::new();
        while self.peek() == Some(':') {
            self.bump(':');
            filters.push(self.parse_filter()?);
        }
        Ok(Chain { field, filters })
    }

    /// Consume a run of ASCII letters followed by a run of ASCII digits.
    /// Both fields and filters use this shape, so `sha1` and `trunc8`
    /// are single names rather than a name and a stray suffix.
    fn parse_name(&mut self) -> &'a str {
        let source = self.source;
        let start = self.position;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.position += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.position += 1;
        }
        &source[start..self.position]
    }

    fn parse_field(&mut self) -> Result<Field, TemplateError> {
        let name = self.parse_name();
        if name.is_empty() {
            return self.syntax("expected a field name");
        }
        match name {
            "auth" => Ok(Field::Auth),
            "authors" => Ok(Field::Authors(None)),
            "year" => Ok(Field::Year),
            "title" => Ok(Field::Title),
            "shorttitle" => Ok(Field::ShortTitle(DEFAULT_SHORT_TITLE_WORDS)),
            "journal" => Ok(Field::Journal),
            "volume" => Ok(Field::Volume),
            "issue" => Ok(Field::Issue),
            "pages" => Ok(Field::Pages),
            "firstpage" => Ok(Field::FirstPage),
            "publisher" => Ok(Field::Publisher),
            "doi" => Ok(Field::Doi),
            "arxiv" => Ok(Field::Arxiv),
            "sha1" => Ok(Field::Sha1),
            "entrytype" => Ok(Field::EntryType),
            _ => match (
                counted_name(name, "authors"),
                counted_name(name, "shorttitle"),
            ) {
                (Some(Some(count)), _) => Ok(Field::Authors(Some(count))),
                (_, Some(Some(count))) => Ok(Field::ShortTitle(count)),
                (Some(None), _) | (_, Some(None)) => {
                    self.syntax("a field count must be a positive integer")
                }
                (None, None) => Err(TemplateError::UnknownField {
                    token: name.to_string(),
                }),
            },
        }
    }

    fn parse_filter(&mut self) -> Result<Filter, TemplateError> {
        if self.rest().starts_with("regex(") {
            self.position += "regex(".len();
            return self.parse_regex();
        }
        for (name, build) in ONE_ARGUMENT_FILTERS {
            if self.rest().starts_with(name) {
                self.position += name.len();
                return Ok(build(self.parse_affix()?));
            }
        }
        let name = self.parse_name();
        if name.is_empty() {
            return self.syntax("expected a filter name");
        }
        match name {
            "lower" => Ok(Filter::Lower),
            "upper" => Ok(Filter::Upper),
            "capitalize" => Ok(Filter::Capitalize),
            "titlecase" => Ok(Filter::TitleCase),
            "camel" => Ok(Filter::Camel),
            "slug" => Ok(Filter::Slug),
            "abbr" => Ok(Filter::Abbr),
            "transliterate" => Ok(Filter::Transliterate),
            _ => match counted_name(name, "trunc") {
                Some(Some(count)) => Ok(Filter::Trunc(count)),
                Some(None) => self.syntax("truncN needs a positive count"),
                None => Err(TemplateError::UnknownFilter {
                    token: name.to_string(),
                }),
            },
        }
    }

    /// Parse the single quoted argument of a one-argument filter whose
    /// opening parenthesis has been consumed, through its closing
    /// parenthesis.
    fn parse_affix(&mut self) -> Result<String, TemplateError> {
        let text = self.parse_quoted()?;
        if self.peek() != Some(')') {
            return self.syntax("expected ')' closing the affix argument");
        }
        self.bump(')');
        Ok(text)
    }

    /// Parse the two quoted arguments of a `regex(` whose opening
    /// parenthesis has been consumed, and compile the pattern.
    fn parse_regex(&mut self) -> Result<Filter, TemplateError> {
        let pattern = self.parse_quoted()?;
        if self.peek() != Some(',') {
            return self.syntax("expected ',' between the regex arguments");
        }
        self.bump(',');
        let replacement = self.parse_quoted()?;
        if self.peek() != Some(')') {
            return self.syntax("expected ')' closing regex(...)");
        }
        self.bump(')');

        match Regex::new(&pattern) {
            Ok(regex) => Ok(Filter::Regex { regex, replacement }),
            Err(error) => Err(TemplateError::BadRegex {
                pattern,
                message: error.to_string(),
            }),
        }
    }

    fn parse_quoted(&mut self) -> Result<String, TemplateError> {
        if self.peek() != Some('"') {
            return self.syntax("expected a double-quoted argument");
        }
        self.bump('"');

        let mut value = String::new();
        loop {
            let Some(c) = self.peek() else {
                return self.syntax("unterminated quoted argument");
            };
            self.bump(c);
            match c {
                '"' => return Ok(value),
                '\\' => {
                    let Some(escaped) = self.peek() else {
                        return self.syntax("unterminated quoted argument");
                    };
                    self.bump(escaped);
                    // Only \" and \\ are escapes; every other backslash
                    // reaches the regex engine as written, so patterns
                    // such as \d need no doubling.
                    if escaped != '"' && escaped != '\\' {
                        value.push('\\');
                    }
                    value.push(escaped);
                }
                _ => value.push(c),
            }
        }
    }
}

/// Split `name` into `prefix` plus a decimal count. `None` when `name`
/// is not `prefix` followed by digits; `Some(None)` when the digits are
/// present but are not a positive integer without a leading zero.
fn counted_name(name: &str, prefix: &str) -> Option<Option<usize>> {
    let digits = name.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        // "trunc" alone, or a name that merely starts with the prefix.
        return if digits.is_empty() { Some(None) } else { None };
    }
    Some(digits.parse().ok().filter(|count| *count > 0))
}

// ---------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------

impl Field {
    fn render(self, input: &RenderInput<'_>) -> String {
        let record = input.record;
        match self {
            Field::Auth => record
                .authors
                .first()
                .map(|author| author.family.clone())
                .unwrap_or_default(),
            Field::Authors(limit) => render_authors(&record.authors, limit),
            Field::Year => record
                .issued
                .map(|issued| issued.year.to_string())
                .unwrap_or_default(),
            Field::Title => record.title.clone().unwrap_or_default(),
            Field::ShortTitle(words) => {
                short_title(record.title.as_deref().unwrap_or_default(), words)
            }
            Field::Journal => record.container_title.clone().unwrap_or_default(),
            Field::Volume => record.volume.clone().unwrap_or_default(),
            Field::Issue => record.issue.clone().unwrap_or_default(),
            Field::Pages => record.pages.clone().unwrap_or_default(),
            Field::FirstPage => first_page(record.pages.as_deref().unwrap_or_default()),
            Field::Publisher => record.publisher.clone().unwrap_or_default(),
            Field::Doi => record
                .doi
                .as_ref()
                .map(|doi| doi.as_str().to_string())
                .unwrap_or_default(),
            Field::Arxiv => record
                .borax
                .arxiv
                .as_ref()
                .map(|arxiv| arxiv.id().to_string())
                .unwrap_or_default(),
            Field::Sha1 => input.sha1.unwrap_or_default().to_string(),
            Field::EntryType => record.entry_type.csl().to_string(),
        }
    }
}

fn render_authors(authors: &[Name], limit: Option<usize>) -> String {
    let kept = limit.unwrap_or(authors.len()).min(authors.len());
    let mut families: Vec<&str> = authors[..kept]
        .iter()
        .map(|author| author.family.as_str())
        .collect();
    if kept < authors.len() {
        families.push("etal");
    }
    families.join("-")
}

/// Builds a filter from the single quoted argument parsed after its
/// opening parenthesis.
type ArgumentBuilder = fn(String) -> Filter;

/// The filters written as a name, one quoted argument and a closing
/// parenthesis, each with what to build from that argument.
const ONE_ARGUMENT_FILTERS: [(&str, ArgumentBuilder); 3] = [
    ("prefix(", Filter::Prefix),
    ("suffix(", Filter::Suffix),
    ("lookup(", Filter::Lookup),
];

/// What `table` holds for `value`, or the empty string, recording a
/// miss when the table holds no such key.
///
/// A table the run never declared is not consulted and not recorded:
/// configuration refuses that before the first file, so reaching it
/// here would mean the check was skipped, and inventing a miss would
/// report the wrong problem.
// The stub uses none of its parameters and pushes into none of its
// vectors; both expectations lapse once the body is written.
#[expect(unused_variables, clippy::ptr_arg, reason = "unimplemented stub")]
fn lookup(value: &str, table: &str, tables: &LookupTables, misses: &mut Vec<Miss>) -> String {
    todo!("lookup")
}

/// `pages` up to its first dash of any width, trimmed.
///
/// A value with no dash is its own first page, which is what keeps an
/// article number (`e0123456`, `045301`) whole. Trimming applies to
/// whatever is returned, so a dashless value is trimmed exactly as a
/// range's first page is.
///
/// A value opening with a dash has nothing before it and yields the
/// empty string, rather than being read as a range missing its start.
fn first_page(pages: &str) -> String {
    pages
        .split(['-', '–', '—'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// `value` wrapped in `before` and `after`, or the empty string
/// unchanged.
fn affix(value: &str, before: &str, after: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        format!("{before}{value}{after}")
    }
}

fn short_title(title: &str, words: usize) -> String {
    title
        .split_ascii_whitespace()
        .filter(|word| {
            !FUNCTION_WORDS
                .iter()
                .any(|function| word.eq_ignore_ascii_case(function))
        })
        .take(words)
        .collect::<Vec<&str>>()
        .join(" ")
}

impl Filter {
    fn apply(&self, value: &str, tables: &LookupTables, misses: &mut Vec<Miss>) -> String {
        match self {
            Filter::Lower => value.to_lowercase(),
            Filter::Upper => value.to_uppercase(),
            Filter::Capitalize => capitalize(value),
            Filter::TitleCase => title_case(value),
            Filter::Camel => title_case(value).split_ascii_whitespace().collect(),
            Filter::Slug => slug(value),
            Filter::Abbr => value
                .split_ascii_whitespace()
                .filter_map(|word| word.chars().next())
                .collect(),
            Filter::Transliterate => transliterate(value),
            Filter::Trunc(count) => value.chars().take(*count).collect(),
            Filter::Prefix(text) => affix(value, text, ""),
            Filter::Suffix(text) => affix(value, "", text),
            Filter::Lookup(table) => lookup(value, table, tables, misses),
            Filter::Regex { regex, replacement } => {
                regex.replace_all(value, replacement.as_str()).into_owned()
            }
        }
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

fn title_case(value: &str) -> String {
    let mut cased = String::with_capacity(value.len());
    let mut at_word_start = true;
    for c in value.chars() {
        if c.is_ascii_whitespace() {
            cased.push(c);
            at_word_start = true;
        } else if at_word_start {
            cased.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            cased.extend(c.to_lowercase());
        }
    }
    cased
}

/// The canonical matching key of `value`: the fold behind both the
/// `slug` filter and every comparison [`crate::tables`] makes.
///
/// Four steps, in this order and no others:
///
/// 1. [`transliterate`] — fold the Latin letters that table names to
///    ASCII, leaving every other character as it stands.
/// 2. Lowercase, by Unicode default case conversion.
/// 3. Replace every maximal run of characters outside `a-z0-9` with a
///    single `-`.
/// 4. Trim leading and trailing `-`.
///
/// So `Journal of the American Chemical Society.` and
/// `J. Am. Chem. Soc.` fold to `journal-of-the-american-chemical-society`
/// and `j-am-chem-soc`, and `Zeitschrift für Chemie` folds to
/// `zeitschrift-fuer-chemie`.
///
/// The result is empty when no character survives — a title written
/// entirely in a script step 1 does not cover. An empty fold is not a
/// key: [`crate::tables`] neither stores nor matches one, because
/// every such title would otherwise match every other.
///
/// These four steps are the contract another tool reimplements to
/// resolve the same curated table the same way, which is why they are
/// stated here rather than left to the implementation. Step 1 in
/// particular is a choice and not a standard: it expands `ä` to `ae`
/// in the German manner where Unicode NFKD would strip the diaeresis.
pub fn slug(value: &str) -> String {
    let folded = transliterate(value).to_lowercase();
    let mut slug = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// Fold the common Latin letters of `value` to ASCII.
///
/// The mapping is the one the `transliterate` filter documents: ä→ae,
/// ö→oe, ü→ue, ß→ss and their uppercase forms; æ→ae, ø→o, å→a, đ→d,
/// ł→l, ñ→n, ç→c; and the accented vowels lose their accents.
///
/// A character with no folding passes through unchanged, so the result
/// is not guaranteed to be ASCII: only the letters in the table above
/// are handled, and Greek, Cyrillic, and CJK are left as they were.
pub fn transliterate(value: &str) -> String {
    let mut folded = String::with_capacity(value.len());
    for c in value.chars() {
        match fold(c) {
            Some(ascii) => folded.push_str(ascii),
            None => folded.push(c),
        }
    }
    folded
}

/// The ASCII folding of `c`, or `None` when the character passes
/// through unchanged.
fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'ä' => "ae",
        'Ä' => "Ae",
        'ö' => "oe",
        'Ö' => "Oe",
        'ü' => "ue",
        'Ü' => "Ue",
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "Ae",
        'ø' => "o",
        'Ø' => "O",
        'å' => "a",
        'Å' => "A",
        'đ' => "d",
        'Đ' => "D",
        'ł' => "l",
        'Ł' => "L",
        'ñ' => "n",
        'Ñ' => "N",
        'ç' => "c",
        'Ç' => "C",
        'à' | 'á' | 'â' | 'ã' => "a",
        'À' | 'Á' | 'Â' | 'Ã' => "A",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'È' | 'É' | 'Ê' | 'Ë' => "E",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'Ì' | 'Í' | 'Î' | 'Ï' => "I",
        'ò' | 'ó' | 'ô' | 'õ' => "o",
        'Ò' | 'Ó' | 'Ô' | 'Õ' => "O",
        'ù' | 'ú' | 'û' => "u",
        'Ù' | 'Ú' | 'Û' => "U",
        'ý' => "y",
        'Ý' => "Y",
        _ => return None,
    })
}

/// Per-entry-type templates with a required default.
#[derive(Debug)]
pub struct TemplateTable {
    default: Template,
    by_type: HashMap<EntryType, Template>,
}

impl TemplateTable {
    /// A table that uses `default` for every entry type.
    pub fn new(default: Template) -> TemplateTable {
        TemplateTable {
            default,
            by_type: HashMap::new(),
        }
    }

    /// Set the template used for `entry_type`, replacing any previous
    /// one.
    pub fn insert(&mut self, entry_type: EntryType, template: Template) {
        self.by_type.insert(entry_type, template);
    }

    /// The template for `entry_type`: its specific template when set,
    /// the default otherwise.
    pub fn get(&self, entry_type: EntryType) -> &Template {
        self.by_type.get(&entry_type).unwrap_or(&self.default)
    }

    /// Render `record` (plus per-file input) with the template its
    /// entry type selects.
    pub fn render(&self, input: &RenderInput<'_>, tables: &LookupTables) -> Rendered {
        self.get(input.record.entry_type).render(input, tables)
    }
}
