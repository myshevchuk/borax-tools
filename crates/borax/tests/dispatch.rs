#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use borax::bib::{BibFiles, citation_key, sidecar_path};
use borax::cache::{cleared_event, inspect, status_event};
use borax::cli::{Cli, Command};
use borax::config::{
    BibLayer, Effective, KeyColumns, Layer, NetworkLayer, Origin, RenameLayer, TableDeclaration,
    ValueKindName, resolve,
};
use borax::event::{
    Admission, Adoption, Condition, Event, Extraction, Level, LibraryAnswer, Overridden, Repair,
    SCHEMA, SkipReason, human_line,
};
use borax::library::{self, ARTIFACT_STORE, ITEM_STORE, STATE_DIR};
use borax::pipeline::Documents;
use borax::renaming::{Filesystem, RealFilesystem, RenameError, counts_for};
use borax::run::{Adapters, Configs, Streams, dispatch, entry_type, events_for, templates};
use borax::runlog::RUNS_DIR;
use borax::session::{Answer, Asker, Outcome, Question, Session, TextPrompt};
use borax_core::bib_output::{DuplicatePolicy, MergeOutcome, merge};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::library::{
    ArtifactId, ArtifactRecord, DuplicateReason, HashEntry, Item, ItemId, RunId as LibraryRunId,
};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record, Source as FieldSource};
use borax_core::tables::{LookupTables, Lookups, NoTables, Table, TableSpec, ValueKind};
use borax_core::template::RenderInput;
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_sources::cache::{CacheWrite, MemoryCache};
use borax_sources::source::{Fetched, Retrieval, Source, SourceError, SourceName};
use borax_sources::store::ContentIndex;
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`PdfSource`] fake driven by data supplied through its builder
/// methods, following the shape of the one in `pipeline.rs`.
#[derive(Clone)]
struct FakePdf {
    pages: Vec<Result<String, ExtractionError>>,
    info: InfoMetadata,
    xmp: Option<String>,
}

impl FakePdf {
    fn new() -> FakePdf {
        FakePdf {
            pages: Vec::new(),
            info: InfoMetadata::default(),
            xmp: None,
        }
    }

    fn with_xmp(mut self, xmp: impl Into<String>) -> FakePdf {
        self.xmp = Some(xmp.into());
        self
    }

    fn with_pages(mut self, pages: Vec<Result<String, ExtractionError>>) -> FakePdf {
        self.pages = pages;
        self
    }

    fn with_title(mut self, title: impl Into<String>) -> FakePdf {
        self.info.title = Some(title.into());
        self
    }
}

impl PdfSource for FakePdf {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn info_metadata(&self) -> &InfoMetadata {
        &self.info
    }

    fn xmp(&self) -> Option<&str> {
        self.xmp.as_deref()
    }

    fn page_text(&self, index: usize) -> Result<String, ExtractionError> {
        self.pages[index].clone()
    }
}

/// A PDF carrying `value` as a DOI in its XMP packet, resolved on the
/// embedded-metadata pass.
fn pdf_with_embedded_doi(value: &str) -> FakePdf {
    FakePdf::new().with_xmp(format!("<prism:doi>{value}</prism:doi>"))
}

/// A PDF with a page of ordinary prose holding no identifier.
fn pdf_with_no_identifier() -> FakePdf {
    FakePdf::new().with_pages(vec![Ok("just some prose, no identifiers here".to_string())])
}

/// A PDF whose only page is blank and whose metadata carries a title
/// holding no identifier — the "no text layer" controlled case design
/// D2 fixes the boundary with.
fn pdf_blank_with_title() -> FakePdf {
    FakePdf::new()
        .with_pages(vec![Ok(" \n".to_string())])
        .with_title("A Title")
}

/// A PDF with a page of readable prose and no identifier, whose
/// metadata also carries a title holding no identifier — the "text
/// without identifier" controlled case beside [`pdf_blank_with_title`].
fn pdf_prose_with_title() -> FakePdf {
    pdf_with_no_identifier().with_title("A Title")
}

/// The blank-page fake whose Info title itself holds a DOI.
fn pdf_blank_with_doi_title(value: &str) -> FakePdf {
    FakePdf::new()
        .with_pages(vec![Ok(" \n".to_string())])
        .with_title(value)
}

/// A PDF carrying `value` as an arXiv identifier in its first page's
/// text, resolved on the text-layer pass, with no identifier anywhere
/// in its metadata.
fn pdf_with_text_arxiv(value: &str) -> FakePdf {
    FakePdf::new().with_pages(vec![Ok(format!("see arXiv:{value} for details"))])
}

/// What [`FakeDocuments`] answers for one path.
struct LibraryEntry {
    hash: Result<ContentHash, ExtractionError>,
    pdf: Result<FakePdf, ExtractionError>,
}

/// A [`Documents`] fake backed by a map from path to a fixed `(hash, PDF
/// content or error)` pair, following the shape of the one in
/// `pipeline.rs`.
struct FakeDocuments {
    entries: BTreeMap<PathBuf, LibraryEntry>,
}

impl FakeDocuments {
    fn new() -> FakeDocuments {
        FakeDocuments {
            entries: BTreeMap::new(),
        }
    }

    /// A readable file: hashing and opening both succeed.
    fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        pdf: FakePdf,
    ) -> FakeDocuments {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Ok(hash),
                pdf: Ok(pdf),
            },
        );
        self
    }

    /// A file whose hash succeeds but whose open fails loudly, so a
    /// content-index hit that opened it anyway shows up as a skip
    /// rather than a silently-live resolution.
    fn with_open_error(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        error: ExtractionError,
    ) -> FakeDocuments {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Ok(hash),
                pdf: Err(error),
            },
        );
        self
    }
}

impl Documents for FakeDocuments {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.entries.get(path).map_or_else(
            || {
                Err(ExtractionError::Unreadable {
                    message: format!("no fake entry for {}", path.display()),
                })
            },
            |entry| entry.hash.clone(),
        )
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        match self.entries.get(path) {
            Some(entry) => entry
                .pdf
                .clone()
                .map(|pdf| Box::new(pdf) as Box<dyn PdfSource>),
            None => Err(ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            }),
        }
    }
}

/// A [`Source`] whose name and canned response are fixed at
/// construction, following the shape of the one in `pipeline.rs`.
struct FakeSource {
    name: SourceName,
    response: Result<Record, SourceError>,
}

impl Source for FakeSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Fetched, SourceError> {
        self.response.clone().map(Fetched::network)
    }
}

fn fake_source(name: SourceName, response: Result<Record, SourceError>) -> FakeSource {
    FakeSource { name, response }
}

/// A [`Source`] answering per identifier, for a batch whose files must
/// resolve to records that differ.
struct KeyedSource {
    name: SourceName,
    answers: BTreeMap<String, Record>,
}

impl KeyedSource {
    fn new(name: SourceName) -> KeyedSource {
        KeyedSource {
            name,
            answers: BTreeMap::new(),
        }
    }

    fn answering(mut self, identifier: &str, record: Record) -> KeyedSource {
        self.answers.insert(identifier.to_string(), record);
        self
    }
}

impl Source for KeyedSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, identifier: &Identifier) -> Result<Fetched, SourceError> {
        match self.answers.get(&identifier.to_string()) {
            Some(record) => Ok(Fetched::network(record.clone())),
            None => Err(SourceError::NotFound),
        }
    }
}

/// A [`Filesystem`] fake backed by a map from directory to the names
/// present there, following the shape of the one in `renaming.rs`.
/// Every [`Filesystem::rename`] call is recorded in order, so a test can
/// assert exactly which moves happened — including that none did.
struct FakeFilesystem {
    existing: BTreeMap<PathBuf, BTreeMap<String, Option<String>>>,
    renames: RefCell<Vec<(PathBuf, PathBuf)>>,
}

impl FakeFilesystem {
    fn new() -> FakeFilesystem {
        FakeFilesystem {
            existing: BTreeMap::new(),
            renames: RefCell::new(Vec::new()),
        }
    }

    fn renames(&self) -> Vec<(PathBuf, PathBuf)> {
        self.renames.borrow().clone()
    }

    /// Populate `directory` with `names`, each paired with the content
    /// hash known for it (`None` when unknown), following the shape of
    /// the one in `renaming.rs`.
    fn with_existing(
        mut self,
        directory: impl Into<PathBuf>,
        names: impl IntoIterator<Item = (&'static str, Option<&'static str>)>,
    ) -> FakeFilesystem {
        self.existing.insert(
            directory.into(),
            names
                .into_iter()
                .map(|(name, hash)| (name.to_string(), hash.map(str::to_string)))
                .collect(),
        );
        self
    }
}

impl Filesystem for FakeFilesystem {
    fn existing(&self, directory: &Path) -> BTreeMap<String, Option<String>> {
        self.existing.get(directory).cloned().unwrap_or_default()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError> {
        self.renames
            .borrow_mut()
            .push((from.to_path_buf(), to.to_path_buf()));
        Ok(())
    }
}

/// A [`BibFiles`] fake backed by an initial map of path to content,
/// following the shape of the one in `bib.rs`.
struct FakeBibFiles {
    initial: Vec<(PathBuf, String)>,
    write_failures: std::collections::BTreeSet<PathBuf>,
    writes: RefCell<Vec<(PathBuf, String)>>,
}

impl FakeBibFiles {
    fn new() -> FakeBibFiles {
        FakeBibFiles {
            initial: Vec::new(),
            write_failures: std::collections::BTreeSet::new(),
            writes: RefCell::new(Vec::new()),
        }
    }

    /// Make every `write` to `path` fail, following the shape of the
    /// one in `tests/bib.rs`.
    fn with_write_failure(mut self, path: impl Into<PathBuf>) -> FakeBibFiles {
        self.write_failures.insert(path.into());
        self
    }

    fn writes(&self) -> Vec<(PathBuf, String)> {
        self.writes.borrow().clone()
    }
}

impl BibFiles for FakeBibFiles {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        Ok(self
            .initial
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, content)| content.clone())
            .unwrap_or_default())
    }

    fn write(&self, path: &Path, content: &str) -> std::io::Result<()> {
        if self.write_failures.contains(path) {
            return Err(std::io::Error::other("fake write failure"));
        }
        self.writes
            .borrow_mut()
            .push((path.to_path_buf(), content.to_string()));
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

/// A [`LookupTables`] holding one empty table under each of `names`,
/// for the checks that only ask which names were declared.
fn declaring(names: &[&str]) -> LookupTables {
    let spec = TableSpec {
        key_columns: vec!["title".to_string()],
        value_column: "abbreviation".to_string(),
        values: ValueKind::Text,
    };
    let mut tables = LookupTables::new();
    for name in names {
        let (table, _) = Table::load("title\tabbreviation\n", &spec).unwrap();
        tables.insert((*name).to_string(), table);
    }
    tables
}

/// A [`Lookups`] over no tables: nothing here declares one, and no
/// template here looks one up.
fn no_tables() -> Lookups<'static> {
    static NONE: OnceLock<NoTables> = OnceLock::new();
    NONE.get_or_init(NoTables::default).lookups()
}

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
}

fn hash_for(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// An `Article` by one author in the given year, carrying `doi_value`.
/// Renders as `"{family}{year}"` under the `[auth][year]` template used
/// throughout this file.
fn record_by(family: &str, year: i32, doi_value: &str) -> Record {
    Record {
        title: None,
        authors: vec![Name {
            family: family.to_string(),
            given: None,
        }],
        issued: Some(DateParts {
            year,
            month: None,
            day: None,
        }),
        doi: Some(doi(doi_value)),
        ..Record::new(EntryType::Article)
    }
}

fn resolved_event(
    path: &Path,
    identifier: &str,
    record: &Record,
    source: &str,
    tier: Option<&str>,
    cached: bool,
) -> Event {
    Event::Resolved {
        path: path.to_path_buf(),
        identifier: identifier.to_string(),
        record: Box::new(record.clone()),
        source: source.to_string(),
        found: identifier.to_string(),

        claims: Vec::new(),

        tier: tier.map(str::to_string),
        cached,
        overrode: None,
        library: None,
    }
}

fn bib_entry_event(path: &Path, outcome: &MergeOutcome) -> Event {
    let (key, name) = match outcome {
        MergeOutcome::Added { key } => (key.clone(), "added"),
        MergeOutcome::AlreadyPresent { existing_key } => (existing_key.clone(), "already-present"),
        MergeOutcome::Updated { key } => (key.clone(), "updated"),
    };
    Event::BibEntry {
        path: path.to_path_buf(),
        key,
        outcome: name.to_string(),
    }
}

/// The `now` every fixture in this file uses: a fixed string, so a
/// run's timestamp and identifier are pinned rather than depending
/// on the clock.
fn fixed_now() -> String {
    "2024-01-01T00:00:00Z".to_string()
}

/// An [`Effective`] built from a single layer, for tests that need to
/// steer one or two settings away from the built-in defaults.
fn effective_with(customize: impl FnOnce(&mut Layer)) -> Effective {
    let mut layer = Layer::default();
    customize(&mut layer);
    resolve(vec![(Origin::Flag("test".to_string()), layer)]).unwrap()
}

/// An [`Effective`] whose default template is `template` and which is
/// otherwise the built-in defaults.
fn effective_with_default_template(template: &str) -> Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
    })
}

/// An [`Effective`] whose default citation-key template is `template` and
/// which is otherwise the built-in defaults.
fn effective_with_default_citation_key_template(template: &str) -> Effective {
    effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
    })
}

fn cli(command: Command, json: bool) -> Cli {
    Cli { command, json }
}

// ---------------------------------------------------------------------
// entry_type
// ---------------------------------------------------------------------

#[test]
fn each_variant_name_maps_to_its_own_entry_type() {
    let pairs = [
        ("article", EntryType::Article),
        ("preprint", EntryType::Preprint),
        ("book", EntryType::Book),
        ("chapter", EntryType::Chapter),
        ("thesis", EntryType::Thesis),
        ("report", EntryType::Report),
        ("patent", EntryType::Patent),
        ("standard", EntryType::Standard),
    ];

    for (name, expected) in pairs {
        assert_eq!(entry_type(name), Some(expected), "for {name:?}");
    }
}

/// The whole point of the docstring: `"article"` is the borax name for a
/// preprint's CSL sibling, not the CSL string a journal article
/// serializes to.
#[test]
fn article_maps_to_article_and_not_to_preprint() {
    assert_eq!(entry_type("article"), Some(EntryType::Article));
    assert_ne!(entry_type("article"), Some(EntryType::Preprint));
}

#[test]
fn default_is_not_an_entry_type() {
    assert_eq!(entry_type("default"), None);
}

#[test]
fn an_unknown_name_is_none() {
    assert_eq!(entry_type("not-a-real-entry-type"), None);
}

#[test]
fn matching_is_case_sensitive() {
    assert_eq!(entry_type("Article"), None);
    assert_eq!(entry_type("ARTICLE"), None);
}

// ---------------------------------------------------------------------
// templates
// ---------------------------------------------------------------------

#[test]
fn a_config_with_only_a_default_template_compiles_to_a_table_whose_default_renders_it() {
    let config = borax::config::Config {
        templates: BTreeMap::from([("default".to_string(), "[auth][year]".to_string())]),
        ..borax::config::Config::default()
    };

    let table = templates(&config.templates, "templates", &LookupTables::new()).unwrap();
    let record = record_by("Smith", 2024, "10.1000/templates-default");
    let rendered = table
        .render(
            &RenderInput {
                record: &record,
                sha1: None,
            },
            &LookupTables::new(),
        )
        .text;

    assert_eq!(rendered, "Smith2024");
}

#[test]
fn a_specific_entry_type_overrides_the_default_and_other_types_still_use_it() {
    let config = borax::config::Config {
        templates: BTreeMap::from([
            ("default".to_string(), "[auth][year]".to_string()),
            ("thesis".to_string(), "[title]".to_string()),
        ]),
        ..borax::config::Config::default()
    };

    let table = templates(&config.templates, "templates", &LookupTables::new()).unwrap();

    let mut thesis = record_by("Jones", 2020, "10.1000/templates-thesis");
    thesis.entry_type = EntryType::Thesis;
    thesis.title = Some("A Study of Borax".to_string());
    let thesis_rendered = table
        .render(
            &RenderInput {
                record: &thesis,
                sha1: None,
            },
            &LookupTables::new(),
        )
        .text;
    assert_eq!(thesis_rendered, "A Study of Borax");

    let mut book = record_by("Jones", 2020, "10.1000/templates-book");
    book.entry_type = EntryType::Book;
    let book_rendered = table
        .render(
            &RenderInput {
                record: &book,
                sha1: None,
            },
            &LookupTables::new(),
        )
        .text;
    assert_eq!(
        book_rendered, "Jones2020",
        "a type with no override still falls back to the default"
    );
}

#[test]
fn a_key_naming_no_entry_type_is_an_error_naming_the_offending_key() {
    let config = borax::config::Config {
        templates: BTreeMap::from([
            ("default".to_string(), "[auth][year]".to_string()),
            // Not one of the eight variant names entry_type recognises.
            ("journal-article".to_string(), "[title]".to_string()),
        ]),
        ..borax::config::Config::default()
    };

    let error = templates(&config.templates, "templates", &LookupTables::new()).unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(error.message.contains("journal-article"), "got {error:?}");
}

/// `"[nonexistentfield]"` is confirmed against `borax_core::template` to
/// fail `Template::compile` with `TemplateError::UnknownField`: no field
/// name, nor any `authorsN`/`shorttitleN` counted variant, matches it.
#[test]
fn a_template_that_will_not_compile_is_an_error_mentioning_the_problem() {
    let config = borax::config::Config {
        templates: BTreeMap::from([("default".to_string(), "[nonexistentfield]".to_string())]),
        ..borax::config::Config::default()
    };

    let error = templates(&config.templates, "templates", &LookupTables::new()).unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(error.message.contains("nonexistentfield"), "got {error:?}");
}

// ---------------------------------------------------------------------
// templates: the citation-key table, compiled against its own prefix
// ---------------------------------------------------------------------

#[test]
fn a_config_with_only_a_default_citation_key_template_compiles_to_a_table_whose_default_renders_it()
{
    let config = borax::config::Config {
        citation_keys: BTreeMap::from([("default".to_string(), "[auth:lower][year]".to_string())]),
        ..borax::config::Config::default()
    };

    let table = templates(&config.citation_keys, "citation-keys", &LookupTables::new()).unwrap();
    let record = record_by("Smith", 2024, "10.1000/citation-keys-default");

    let key = citation_key(&record, None, &table, &mut no_tables());

    assert_eq!(key, Some("smith2024".to_string()));
}

#[test]
fn a_citation_key_override_for_one_entry_type_leaves_others_on_the_default() {
    let config = borax::config::Config {
        citation_keys: BTreeMap::from([
            ("default".to_string(), "[auth:lower][year]".to_string()),
            ("thesis".to_string(), "[title]".to_string()),
        ]),
        ..borax::config::Config::default()
    };

    let table = templates(&config.citation_keys, "citation-keys", &LookupTables::new()).unwrap();

    let mut thesis = record_by("Jones", 2020, "10.1000/citation-keys-thesis");
    thesis.entry_type = EntryType::Thesis;
    thesis.title = Some("A Study of Borax".to_string());
    let thesis_rendered = table
        .render(
            &RenderInput {
                record: &thesis,
                sha1: None,
            },
            &LookupTables::new(),
        )
        .text;
    assert_eq!(thesis_rendered, "A Study of Borax");

    let mut book = record_by("Jones", 2020, "10.1000/citation-keys-book");
    book.entry_type = EntryType::Book;
    let book_rendered = table
        .render(
            &RenderInput {
                record: &book,
                sha1: None,
            },
            &LookupTables::new(),
        )
        .text;
    assert_eq!(
        book_rendered, "jones2020",
        "a type with no citation-key override still falls back to the default"
    );
}

#[test]
fn a_citation_key_naming_no_entry_type_is_an_error_naming_the_prefixed_key() {
    let config = borax::config::Config {
        citation_keys: BTreeMap::from([
            ("default".to_string(), "[auth:lower][year]".to_string()),
            // Not one of the eight variant names entry_type recognises.
            ("journal-article".to_string(), "[title]".to_string()),
        ]),
        ..borax::config::Config::default()
    };

    let error =
        templates(&config.citation_keys, "citation-keys", &LookupTables::new()).unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(
        error.message.contains("citation-keys.journal-article"),
        "expected the citation-keys prefix on the offending key, got {error:?}"
    );
}

#[test]
fn an_uncompilable_citation_key_template_is_an_error_naming_the_prefixed_key() {
    let config = borax::config::Config {
        citation_keys: BTreeMap::from([("default".to_string(), "[nonexistentfield]".to_string())]),
        ..borax::config::Config::default()
    };

    let error =
        templates(&config.citation_keys, "citation-keys", &LookupTables::new()).unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(
        error.message.contains("citation-keys.default"),
        "expected the citation-keys prefix on the offending key, got {error:?}"
    );
    assert!(error.message.contains("nonexistentfield"), "got {error:?}");
}

/// Spec scenario: "Lookup names no declared table".
#[test]
fn a_template_looking_up_an_undeclared_table_is_an_error_naming_the_key_and_the_table() {
    let config = borax::config::Config {
        templates: BTreeMap::from([(
            "default".to_string(),
            "[journal:lookup(\"jcode\")]".to_string(),
        )]),
        ..borax::config::Config::default()
    };

    let error = templates(&config.templates, "templates", &LookupTables::new()).unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert_eq!(
        error.message, "templates.default: unknown table \"jcode\"",
        "got {error:?}"
    );
}

#[test]
fn a_citation_key_looking_up_an_undeclared_table_is_an_error_naming_the_prefixed_key() {
    let config = borax::config::Config {
        citation_keys: BTreeMap::from([
            ("default".to_string(), "[auth:lower][year]".to_string()),
            (
                "article".to_string(),
                "[journal:lookup(\"pubcodes\")]".to_string(),
            ),
        ]),
        ..borax::config::Config::default()
    };

    let error = templates(
        &config.citation_keys,
        "citation-keys",
        &declaring(&["jcode"]),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert_eq!(
        error.message, "citation-keys.article: unknown table \"pubcodes\"",
        "got {error:?}"
    );
}

#[test]
fn a_template_looking_up_a_declared_table_compiles() {
    let config = borax::config::Config {
        templates: BTreeMap::from([(
            "default".to_string(),
            "[journal:lookup(\"jcode\")]".to_string(),
        )]),
        ..borax::config::Config::default()
    };

    assert!(
        templates(&config.templates, "templates", &declaring(&["jcode"])).is_ok(),
        "a declared table should satisfy the lookup"
    );
}

/// The regression this change exists to prevent, at the level the real
/// pipeline compiles tables: a `templates.default` override reaches only
/// the filename table, never the citation-key table compiled from
/// `citation_keys`.
#[test]
fn changing_templates_default_does_not_change_the_compiled_citation_key_table() {
    let config = borax::config::Config {
        templates: BTreeMap::from([("default".to_string(), "[year]-[auth]-long-form".to_string())]),
        ..borax::config::Config::default()
    };

    let citation_table =
        templates(&config.citation_keys, "citation-keys", &LookupTables::new()).unwrap();
    let record = record_by("Smith", 2024, "10.1000/templates-independent");

    let key = citation_key(&record, None, &citation_table, &mut no_tables());

    assert_eq!(key, Some("smith2024".to_string()));
}

// ---------------------------------------------------------------------
// events_for: Command::Config
// ---------------------------------------------------------------------

#[test]
fn config_emits_one_config_setting_event_per_setting_matching_effective_events() {
    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::config(),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events, effective.events(), "got {events:?}");
}

// ---------------------------------------------------------------------
// events_for: Command::Cache
// ---------------------------------------------------------------------

#[test]
fn cache_status_without_clear_emits_a_single_cache_status_event() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("entry.bin"), vec![0u8; 5]).unwrap();
    let stats_before = inspect(&root).unwrap();

    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(root.clone()),
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::cache(false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events, vec![status_event(&stats_before)], "got {events:?}");
    assert!(root.exists(), "a status check must not remove anything");
}

#[test]
fn cache_clear_emits_a_single_cache_cleared_event_and_empties_the_directory() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("entry.bin"), vec![0u8; 5]).unwrap();
    let stats_before = inspect(&root).unwrap();

    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(root.clone()),
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::cache(true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events, vec![cleared_event(&stats_before)], "got {events:?}");
    assert!(!root.exists(), "clearing must empty the cache directory");
}

/// Open question (see the report handed back with these tests): neither
/// `Adapters::cache_root`'s nor `events_for`'s doc comment says what a
/// cache command should do when the environment names no cache
/// directory. This pins the safe reading — refuse with a [`Diagnostic`]
/// rather than silently reporting an empty cache the run never looked
/// at.
#[test]
fn cache_with_no_cache_root_is_a_diagnostic() {
    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::cache(false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
}

// ---------------------------------------------------------------------
// events_for: Command::Resolve
// ---------------------------------------------------------------------

#[test]
fn resolve_emits_resolved_then_skipped_for_a_mixed_batch() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &good,
            hash_for("events-for-resolve-good"),
            pdf_with_embedded_doi("10.1000/events-for-good"),
        )
        .with_file(
            &bad,
            hash_for("events-for-resolve-bad"),
            pdf_with_no_identifier(),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/events-for-good")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![good.clone(), bad.clone()]),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            resolved_event(
                &good,
                "doi:10.1000/events-for-good",
                &record_by("Smith", 2024, "10.1000/events-for-good"),
                "crossref",
                Some("embedded-metadata"),
                false,
            ),
            Event::Skipped {
                path: bad,
                reason: SkipReason::NoIdentifier,
                library: None,
            },
        ],
        "got {events:?}"
    );
}

// ---------------------------------------------------------------------
// events_for: Command::Rename, preview
// ---------------------------------------------------------------------

#[test]
fn rename_preview_emits_resolved_and_planned_and_moves_nothing() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-rename-preview"),
        pdf_with_embedded_doi("10.1000/rename-preview"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/rename-preview")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            resolved_event(
                &path,
                "doi:10.1000/rename-preview",
                &record_by("Smith", 2024, "10.1000/rename-preview"),
                "crossref",
                Some("embedded-metadata"),
                false,
            ),
            Event::Planned {
                path: path.clone(),
                target: PathBuf::from("/lib/Smith2024.pdf"),
            },
        ],
        "got {events:?}"
    );
    assert!(
        filesystem.renames().is_empty(),
        "a preview run must not move any file"
    );
}

#[test]
fn rename_preview_with_no_journal_succeeds() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-rename-preview-no-journal"),
        pdf_with_embedded_doi("10.1000/rename-preview-no-journal"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/rename-preview-no-journal",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            resolved_event(
                &path,
                "doi:10.1000/rename-preview-no-journal",
                &record_by("Smith", 2024, "10.1000/rename-preview-no-journal"),
                "crossref",
                Some("embedded-metadata"),
                false,
            ),
            Event::Planned {
                path,
                target: PathBuf::from("/lib/Smith2024.pdf"),
            },
        ],
        "a preview needs no journal"
    );
}

// ---------------------------------------------------------------------
// events_for: Command::Rename, applying
// ---------------------------------------------------------------------

#[test]
fn rename_apply_emits_renamed_carrying_the_hash_and_moves_the_file() {
    let path = PathBuf::from("/lib/original.pdf");
    let hash = hash_for("events-for-rename-apply");
    let documents = library_with_resolvable(&path, hash.clone(), "10.1000/rename-apply");
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/rename-apply")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let target = PathBuf::from("/lib/Smith2024.pdf");

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            resolved_event(
                &path,
                "doi:10.1000/rename-apply",
                &record_by("Smith", 2024, "10.1000/rename-apply"),
                "crossref",
                Some("embedded-metadata"),
                false,
            ),
            Event::Renamed {
                path: path.clone(),
                target: target.clone(),
                hash,
            },
        ],
        "got {events:?}"
    );
    assert_eq!(filesystem.renames(), vec![(path.clone(), target.clone())]);
}

/// A [`FakeDocuments`] with one file whose embedded DOI is `doi_value`,
/// factored out because the apply test needs the hash again to build its
/// expected `Renamed` event.
fn library_with_resolvable(path: &Path, hash: ContentHash, doi_value: &str) -> FakeDocuments {
    FakeDocuments::new().with_file(path, hash, pdf_with_embedded_doi(doi_value))
}

// ---------------------------------------------------------------------
// events_for: Command::Bib
// ---------------------------------------------------------------------

#[test]
fn bib_emits_resolved_then_the_bib_events_and_the_fake_bib_files_received_the_writes() {
    let path = PathBuf::from("/lib/paper.pdf");
    let record = record_by("Smith", 2024, "10.1000/bib");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-bib"),
        pdf_with_embedded_doi("10.1000/bib"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(false),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::bib(vec![path.clone()]),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let expected_merge = merge("", &[("Smith2024", &record)], DuplicatePolicy::Skip);
    let mut expected = vec![resolved_event(
        &path,
        "doi:10.1000/bib",
        &record,
        "crossref",
        Some("embedded-metadata"),
        false,
    )];
    expected.extend(
        expected_merge
            .outcomes
            .iter()
            .map(|outcome| bib_entry_event(&path, outcome)),
    );
    assert_eq!(events, expected, "got {events:?}");
    assert_eq!(
        bib_files.writes(),
        vec![(PathBuf::from("refs.bib"), expected_merge.content)]
    );
}

/// documents spec: "No route from an artifactless item to a
/// bibliography" — `borax bib` takes files, so a library holding an
/// item with no artifact contributes nothing to its output. The item
/// is written to the item store and confirmed there through
/// [`borax::library::ItemStore`] before `bib` runs, so the absence of
/// its entry is a fact about `bib` and not an accident of the item
/// never having existed.
#[test]
fn bib_over_a_library_holding_an_artifactless_item_emits_no_entry_for_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let items = root.join("items");
    fs::create_dir_all(&items).unwrap();

    let artifactless = borax_core::library::Item {
        id: borax_core::library::ItemId::from_uuid(
            uuid::Uuid::parse_str("018f2b36-7f21-7abc-8def-0123456789ab").unwrap(),
        ),
        record: record_by("Jones", 2020, "10.1000/no-file"),
    };
    fs::write(
        items.join(format!("jones2020.{}.toml", artifactless.id)),
        artifactless.to_toml(),
    )
    .unwrap();

    let store = borax::library::ItemStore::read(root);
    assert_eq!(
        store.len(),
        1,
        "setup: the item store must hold the artifactless item"
    );
    assert!(store.by_id(&artifactless.id).is_some());

    let path = root.join("paper.pdf");
    let record = record_by("Smith", 2024, "10.1000/bib");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("bib-over-artifactless-item"),
        pdf_with_embedded_doi("10.1000/bib"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(false),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::bib(vec![path.clone()]),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let expected_merge = merge("", &[("Smith2024", &record)], DuplicatePolicy::Skip);
    let mut expected = vec![resolved_event(
        &path,
        "doi:10.1000/bib",
        &record,
        "crossref",
        Some("embedded-metadata"),
        false,
    )];
    expected.extend(
        expected_merge
            .outcomes
            .iter()
            .map(|outcome| bib_entry_event(&path, outcome)),
    );
    // The assertion that matters: exactly the events the one given file
    // produces, with nothing added for `jones2020` — no `BibEntry` for a
    // work the run was never given a file for.
    assert_eq!(events, expected, "got {events:?}");
    let entries: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::BibEntry { .. }))
        .collect();
    assert_eq!(entries.len(), 1, "got {events:?}");
}

// ---------------------------------------------------------------------
// events_for: an uncompilable filename template propagates as a
// Diagnostic, for the commands that render a filename
// ---------------------------------------------------------------------

#[test]
fn rename_with_an_uncompilable_template_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-rename-bad-template"),
        pdf_with_embedded_doi("10.1000/rename-bad-template"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/rename-bad-template")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[nonexistentfield]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
}

/// The other side of the cut: `bib` renders no filename, so a filename
/// template that will not compile is a value this run never reads and
/// cannot be ended by. Its citation-key templates still compile, which
/// is what `bib_with_an_uncompilable_citation_key_template_…` pins.
#[test]
fn bib_with_an_uncompilable_filename_template_runs_anyway() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-bib-bad-template"),
        pdf_with_embedded_doi("10.1000/bib-bad-template"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/bib-bad-template")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[nonexistentfield]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::bib(vec![path]),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .expect("a filename template bib never renders cannot end it");

    assert!(!events.is_empty(), "got {events:?}");
}

// ---------------------------------------------------------------------
// events_for: an uncompilable citation-key template propagates as a
// Diagnostic, before any file is processed
// ---------------------------------------------------------------------

#[test]
fn rename_with_an_uncompilable_citation_key_template_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-rename-bad-citation-key-template"),
        pdf_with_embedded_doi("10.1000/rename-bad-citation-key-template"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/rename-bad-citation-key-template",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_citation_key_template("[nonexistentfield]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(
        error.message.contains("citation-keys"),
        "expected the citation-keys prefix, got {error:?}"
    );
}

#[test]
fn bib_with_an_uncompilable_citation_key_template_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-bib-bad-citation-key-template"),
        pdf_with_embedded_doi("10.1000/bib-bad-citation-key-template"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/bib-bad-citation-key-template",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_citation_key_template("[nonexistentfield]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::bib(vec![path]),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(
        error.message.contains("citation-keys"),
        "expected the citation-keys prefix, got {error:?}"
    );
}

/// cli spec scenario "Citation-key key names no entry type": a
/// `citation-keys` key matching no entry type aborts the run as a
/// configuration error naming the key, before any file is processed —
/// the filename `templates` table stays fine, so this failure can only
/// come from the citation-key table also being compiled during preflight.
#[test]
fn bib_with_a_citation_key_naming_no_entry_type_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("events-for-bib-citation-key-bad-entry-type"),
        pdf_with_embedded_doi("10.1000/bib-citation-key-bad-entry-type"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/bib-citation-key-bad-entry-type",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([
            ("default".to_string(), "[auth:lower][year]".to_string()),
            ("journal-article".to_string(), "[title]".to_string()),
        ]));
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::bib(vec![path]),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(
        error.message.contains("citation-keys.journal-article"),
        "got {error:?}"
    );
}

// ---------------------------------------------------------------------
// dispatch: the envelope
// ---------------------------------------------------------------------

#[test]
fn json_format_opens_with_run_started_and_closes_with_run_finished_and_every_line_carries_schema() {
    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::config(), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("{line:?} did not parse: {error}"))
        })
        .collect();

    assert!(!lines.is_empty(), "expected at least two events");
    assert_eq!(
        lines.first().unwrap()["event"],
        "run-started",
        "got {lines:?}"
    );
    assert_eq!(
        lines.last().unwrap()["event"],
        "run-finished",
        "got {lines:?}"
    );
    for line in &lines {
        assert!(
            line.get("schema").is_some(),
            "every line must carry schema: {line:?}"
        );
    }
}

#[test]
fn json_stdout_is_entirely_well_formed_json_lines_and_nothing_else() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &good,
            hash_for("dispatch-json-good"),
            pdf_with_embedded_doi("10.1000/dispatch-json-good"),
        )
        .with_file(
            &bad,
            hash_for("dispatch-json-bad"),
            pdf_with_no_identifier(),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-json-good")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![good, bad]), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert!(
        err.is_empty(),
        "no diagnostic expected on a clean dispatch: {:?}",
        String::from_utf8_lossy(&err)
    );
    let text = String::from_utf8(out).unwrap();
    for line in text.lines() {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|error| panic!("{line:?} did not parse as JSON: {error}"));
    }
    assert!(
        text.is_empty() || text.ends_with('\n'),
        "expected stdout to consist only of complete lines, got {text:?}"
    );
}

#[test]
fn human_format_omits_run_started_but_still_ends_with_the_summary_line() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("dispatch-human"),
        pdf_with_embedded_doi("10.1000/dispatch-human"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-human")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![path.clone()]), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first(),
        Some(&"/lib/paper.pdf: resolved doi:10.1000/dispatch-human via crossref"),
        "RunStarted must be omitted in human format, got {lines:?}"
    );
    assert_eq!(
        lines.last(),
        Some(&"1 resolved, 0 skipped"),
        "design D4: resolve is Summary::Resolution and drops renamed, got {lines:?}"
    );
}

/// design D4: `resolve` names its skip in the summary — the total that
/// decides its exit is never hidden.
#[test]
fn human_format_of_resolve_with_a_skip_names_it_and_the_outcome_is_partial() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &good,
            hash_for("dispatch-human-skip-good"),
            pdf_with_embedded_doi("10.1000/dispatch-human-skip"),
        )
        .with_file(
            &bad,
            hash_for("dispatch-human-skip-bad"),
            pdf_with_no_identifier(),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-human-skip")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::resolve(vec![good.clone(), bad.clone()]), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.last(),
        Some(&"1 resolved, 1 skipped"),
        "got {lines:?}"
    );
    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

#[test]
fn run_finished_counts_match_counts_for_over_the_body_events() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &good,
            hash_for("dispatch-counts-good"),
            pdf_with_embedded_doi("10.1000/dispatch-counts-good"),
        )
        .with_file(
            &bad,
            hash_for("dispatch-counts-bad"),
            pdf_with_no_identifier(),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-counts-good")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![good, bad]), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let all_events: Vec<Event> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let (last, body) = all_events.split_last().unwrap();
    let Event::RunFinished { counts } = last else {
        panic!("expected the last event to be RunFinished, got {last:?}")
    };

    assert_eq!(*counts, counts_for(body), "got {counts:?}");
}

// ---------------------------------------------------------------------
// dispatch: Outcome
// ---------------------------------------------------------------------

#[test]
fn a_clean_run_returns_success() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("dispatch-success"),
        pdf_with_embedded_doi("10.1000/dispatch-success"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-success")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::resolve(vec![path]), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

#[test]
fn a_run_with_a_skip_returns_partial() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &good,
            hash_for("dispatch-partial-good"),
            pdf_with_embedded_doi("10.1000/dispatch-partial-good"),
        )
        .with_file(
            &bad,
            hash_for("dispatch-partial-bad"),
            pdf_with_no_identifier(),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-partial-good")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::resolve(vec![good, bad]), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

// ---------------------------------------------------------------------
// dispatch: a refusal that only dispatch can make
// ---------------------------------------------------------------------

/// An applying rename with nowhere to record itself is refused by
/// `dispatch` rather than by `events_for`: the gate is the run log's,
/// and `events_for` never opens one. The refusal still has to behave
/// like every other one — nothing on stdout, the reason on stderr, and
/// no file moved.
#[test]
fn a_refusal_dispatch_alone_makes_is_fatal_and_writes_nothing_to_stdout() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("dispatch-fatal"),
        pdf_with_embedded_doi("10.1000/dispatch-fatal"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dispatch-fatal")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let command = Command::rename(vec![path], true);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(command, false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(
        out.is_empty(),
        "a fatal run must write no event stream: {:?}",
        String::from_utf8_lossy(&out)
    );
    let err_text = String::from_utf8(err).unwrap();
    assert!(
        err_text.starts_with("error: "),
        "expected a refusal on stderr, got {err_text:?}"
    );
    assert!(
        err_text.contains("record what it moves"),
        "expected {err_text:?} to say why the run was refused"
    );
    assert!(filesystem.renames().is_empty());
}

#[test]
fn diagnostics_never_appear_on_stdout_regardless_of_which_check_produced_them() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    // A citation-key template, because that is the table `bib` renders
    // from: the check has to be one this command actually makes.
    let effective = effective_with_default_citation_key_template("[nonexistentfield]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::bib(vec![path]), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(
        out.is_empty(),
        "diagnostics must never appear on stdout: {:?}",
        String::from_utf8_lossy(&out)
    );
    assert!(!err.is_empty(), "expected the diagnostic on stderr");
}

// ---------------------------------------------------------------------
// external tables: loading, misses, and the run's opening event
// ---------------------------------------------------------------------

/// The fixture table every test below declares: two journals, keyed on
/// their title, valued by their abbreviation.
const JOURNALS: &str = "\
title\tabbreviation
Amino Acids\tAA
Journal of the American Chemical Society\tJACS
";

/// An [`Effective`] declaring a `jcode` table over `path`, with `[auth]
/// [year]-[journal:lookup("jcode")]` as its filename template — a name
/// a miss still produces, so a run is not curtailed by one.
fn effective_looking_up(path: &Path) -> Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]-[journal:lookup(\"jcode\")]".to_string(),
        )]));
        layer.tables = Some(BTreeMap::from([(
            "jcode".to_string(),
            TableDeclaration {
                path: path.to_path_buf(),
                key: KeyColumns::One("title".to_string()),
                value: "abbreviation".to_string(),
                values: ValueKindName::Text,
            },
        )]));
    })
}

/// An article in `journal`: [`record_by`]'s record with a container
/// title, which is what a `lookup` on `journal` reads.
fn article_in(journal: &str, doi_value: &str) -> Record {
    Record {
        container_title: Some(journal.to_string()),
        ..record_by("Smith", 2024, doi_value)
    }
}

#[test]
fn a_lookup_that_hits_names_the_file_with_the_table_value() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journals.tsv");
    fs::write(&table, JOURNALS).unwrap();

    let path = PathBuf::from("/lib/paper.pdf");
    let record = article_in("Amino Acids", "10.1000/lookup-hit");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("lookup-hit"),
        pdf_with_embedded_doi("10.1000/lookup-hit"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events.last().unwrap(),
        &Event::Planned {
            path,
            target: PathBuf::from("/lib/Smith2024-AA.pdf"),
        },
        "got {events:?}"
    );
}

/// Spec scenario: "An unmatched journal is named once".
#[test]
fn two_files_in_one_unlisted_journal_produce_one_lookup_missed_event() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journals.tsv");
    fs::write(&table, JOURNALS).unwrap();

    let first = PathBuf::from("/lib/one.pdf");
    let second = PathBuf::from("/lib/two.pdf");
    let record = article_in("Journal of Unlisted Results", "10.1000/unlisted");
    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_for("unlisted-one"),
            pdf_with_embedded_doi("10.1000/unlisted"),
        )
        .with_file(
            &second,
            hash_for("unlisted-two"),
            pdf_with_embedded_doi("10.1000/unlisted"),
        );
    let crossref = fake_source(SourceName::Crossref, Ok(record.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![first, second], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let missed: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::LookupMissed { .. }))
        .collect();
    assert_eq!(
        missed,
        vec![&Event::LookupMissed {
            table: "jcode".to_string(),
            input: "Journal of Unlisted Results".to_string(),
        }],
        "got {events:?}"
    );
    assert!(
        matches!(events.last(), Some(Event::LookupMissed { .. })),
        "misses follow the per-file events, got {events:?}"
    );
}

#[test]
fn two_unlisted_journals_produce_one_event_each_in_input_order() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journals.tsv");
    fs::write(&table, JOURNALS).unwrap();

    let first = PathBuf::from("/lib/one.pdf");
    let second = PathBuf::from("/lib/two.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_for("two-unlisted-one"),
            pdf_with_embedded_doi("10.1000/unlisted-one"),
        )
        .with_file(
            &second,
            hash_for("two-unlisted-two"),
            pdf_with_embedded_doi("10.1000/unlisted-two"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/unlisted-one",
            article_in("Acta Obscura", "10.1000/unlisted-one"),
        )
        .answering(
            "doi:10.1000/unlisted-two",
            article_in("Zeitschrift Obskur", "10.1000/unlisted-two"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![first, second], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let missed: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::LookupMissed { .. }))
        .collect();
    assert_eq!(
        missed,
        vec![
            &Event::LookupMissed {
                table: "jcode".to_string(),
                input: "Acta Obscura".to_string(),
            },
            &Event::LookupMissed {
                table: "jcode".to_string(),
                input: "Zeitschrift Obskur".to_string(),
            },
        ],
        "got {events:?}"
    );
}

/// Spec scenario: "The run log identifies the table".
#[test]
fn run_started_names_each_table_read_by_path_and_digest() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journals.tsv");
    fs::write(&table, JOURNALS).unwrap();

    let path = PathBuf::from("/lib/paper.pdf");
    let record = article_in("Amino Acids", "10.1000/run-started-tables");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("run-started-tables"),
        pdf_with_embedded_doi("10.1000/run-started-tables"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![path], false), true),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let started: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();

    assert_eq!(started["event"], "run-started");
    assert_eq!(
        started["tables"],
        serde_json::json!([{
            "name": "jcode",
            "path": table,
            "digest": hash_bytes(JOURNALS.as_bytes()).as_str(),
        }]),
        "got {started}"
    );
}

#[test]
fn a_run_that_reads_no_table_opens_with_an_empty_table_list() {
    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::config(), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let started: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();

    assert_eq!(started["tables"], serde_json::json!([]));
}

/// Spec scenario: "Declared table file is missing".
#[test]
fn a_declared_table_that_cannot_be_read_ends_the_run_naming_the_table_and_the_path() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("absent.tsv");

    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("table-missing"),
        pdf_with_embedded_doi("10.1000/table-missing"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(error.message.contains("tables.jcode"), "got {error:?}");
    assert!(
        error.message.contains(&table.display().to_string()),
        "got {error:?}"
    );
}

#[test]
fn a_header_without_the_declared_value_column_ends_the_run_naming_the_table() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journals.tsv");
    fs::write(&table, "title\tshorttitle\nAmino Acids\tAmino Acids\n").unwrap();

    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("table-column"),
        pdf_with_embedded_doi("10.1000/table-column"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
    assert!(error.message.contains("tables.jcode"), "got {error:?}");
    assert!(error.message.contains("abbreviation"), "got {error:?}");
}

// ---------------------------------------------------------------------
// the target pattern, end to end
// ---------------------------------------------------------------------

/// The curated journal file: the header the other tool already reads —
/// `abbreviation`, `title`, `shorttitle` — plus the `code` column borax
/// adds beside it, so what this exercises is one shared file and not a
/// format of borax's own.
const JOURNAL_TITLES: &str = include_str!("journal_titles.tsv");

/// An [`Effective`] declaring `jcode` over the curated file as a
/// fragment-valued table keyed on both title columns, with the pattern
/// this whole change exists to render as its filename template.
fn effective_for_the_target_pattern(path: &Path) -> Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[year]-[journal:lookup(\"jcode\")]-[firstpage]".to_string(),
        )]));
        layer.tables = Some(BTreeMap::from([(
            "jcode".to_string(),
            TableDeclaration {
                path: path.to_path_buf(),
                key: KeyColumns::Many(vec!["title".to_string(), "shorttitle".to_string()]),
                value: "code".to_string(),
                values: ValueKindName::Template,
            },
        )]));
    })
}

/// A 2024 article in `journal` on pages `1234-1245`, in `volume` when
/// the record has one.
fn article_on_pages(journal: &str, volume: Option<&str>, doi_value: &str) -> Record {
    Record {
        volume: volume.map(str::to_string),
        pages: Some("1234-1245".to_string()),
        ..article_in(journal, doi_value)
    }
}

/// One run, one template, four journals: the flag that decides whether
/// a volume belongs in the name lives in the curated file, so
/// `[year]-[journal:lookup("jcode")]-[firstpage]` renders every shape
/// the pattern has without knowing which journal it is naming.
#[test]
fn the_target_pattern_names_every_journal_shape_from_one_template() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journal_titles.tsv");
    fs::write(&table, JOURNAL_TITLES).unwrap();

    // Two flagged journals with a volume and without — one of them
    // reached by the abbreviated spelling its `shorttitle` column holds
    // — and an unflagged row in the same table.
    let batch = [
        (
            "/lib/jacs.pdf",
            "10.1000/jacs",
            "Journal of the American Chemical Society",
            Some("146"),
            "/lib/2024-JACS-146-1234.pdf",
        ),
        (
            "/lib/jacs-no-volume.pdf",
            "10.1000/jacs-no-volume",
            "Journal of the American Chemical Society",
            None,
            "/lib/2024-JACS-1234.pdf",
        ),
        (
            "/lib/abb.pdf",
            "10.1000/abb",
            "Arch. Biochem. Biophys.",
            Some("146"),
            "/lib/2024-ABB-146-1234.pdf",
        ),
        (
            "/lib/abb-no-volume.pdf",
            "10.1000/abb-no-volume",
            "Archives of Biochemistry and Biophysics",
            None,
            "/lib/2024-ABB-1234.pdf",
        ),
        (
            "/lib/aa.pdf",
            "10.1000/aa",
            "Amino Acids",
            Some("46"),
            "/lib/2024-AA-1234.pdf",
        ),
    ];

    let mut documents = FakeDocuments::new();
    let mut crossref = KeyedSource::new(SourceName::Crossref);
    for (path, doi_value, journal, volume, _) in &batch {
        documents = documents.with_file(
            PathBuf::from(path),
            hash_for(doi_value),
            pdf_with_embedded_doi(doi_value),
        );
        crossref = crossref.answering(
            &format!("doi:{doi_value}"),
            article_on_pages(journal, *volume, doi_value),
        );
    }
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(
            batch.iter().map(|(path, ..)| PathBuf::from(path)).collect(),
            false,
        ),
        &Configs::uniform(effective_for_the_target_pattern(&table)),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let planned: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Planned { .. }))
        .collect();
    let expected: Vec<Event> = batch
        .iter()
        .map(|(path, _, _, _, target)| Event::Planned {
            path: PathBuf::from(path),
            target: PathBuf::from(target),
        })
        .collect();

    assert_eq!(
        planned,
        expected.iter().collect::<Vec<&Event>>(),
        "got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::LookupMissed { .. })),
        "every journal is in the table, got {events:?}"
    );
}

// ---------------------------------------------------------------------
// interactive rename: tasks 3.2-3.4, through a scripted Asker
//
// D9 settles the seam: no second rename entry point, `rename_events` is
// the one driver and `Session` decides whether it asks. Every test here
// drives it through the same `events_for`/`dispatch` a batch run uses,
// with `Session::interactive(&mut asker)` in place of `Session::batch()`
// and `apply: false` on the command — in an interactive run the answer
// is the gate, not the flag.
//
// `rename_events` currently opens with
// `if session.mode == Mode::Interactive { todo!(...) }`, so every test
// below fails on that panic today; the assertions below are what they
// fail against once the driver exists.
// ---------------------------------------------------------------------

/// A scripted [`Asker`]: answers a fixed list of choices and a fixed
/// list of text answers, each in order, records every [`Question`] and
/// [`TextPrompt`] it was asked, and panics with a clear message if asked
/// for more of either than it was given — so a driver that asks about a
/// file it should not have asked about, or that asks for text nobody
/// scripted, fails loudly rather than silently consuming the wrong
/// answer.
///
/// `texts` is empty by default ([`ScriptedAsker::new`]); a test that
/// never expects a text prompt gets the same loud failure the old,
/// choices-only double gave, and a test that supplies a `supply`
/// answer scripts what comes back from it with
/// [`ScriptedAsker::with_texts`].
struct ScriptedAsker {
    answers: std::vec::IntoIter<Answer>,
    texts: std::vec::IntoIter<Option<String>>,
    asked: RefCell<Vec<Question>>,
    texts_asked: RefCell<Vec<TextPrompt>>,
}

impl ScriptedAsker {
    fn new(answers: Vec<Answer>) -> ScriptedAsker {
        ScriptedAsker {
            answers: answers.into_iter(),
            texts: Vec::new().into_iter(),
            asked: RefCell::new(Vec::new()),
            texts_asked: RefCell::new(Vec::new()),
        }
    }

    /// Script what [`Asker::text`] returns, in order: `Some(input)` for
    /// a line the operator typed, `None` for Esc or an empty line.
    fn with_texts(mut self, texts: Vec<Option<String>>) -> ScriptedAsker {
        self.texts = texts.into_iter();
        self
    }

    fn questions_asked(&self) -> Vec<Question> {
        self.asked.borrow().clone()
    }

    fn texts_asked(&self) -> Vec<TextPrompt> {
        self.texts_asked.borrow().clone()
    }
}

impl Asker for ScriptedAsker {
    fn choose(&mut self, question: &Question) -> Answer {
        self.asked.borrow_mut().push(question.clone());
        self.answers.next().unwrap_or_else(|| {
            panic!(
                "asked more questions than were scripted; question was {question:?}, \
                 already asked {:?}",
                self.asked.borrow()
            )
        })
    }

    fn text(&mut self, prompt: &TextPrompt) -> Option<String> {
        self.texts_asked.borrow_mut().push(prompt.clone());
        self.texts.next().unwrap_or_else(|| {
            panic!(
                "asked for more text than was scripted; prompt was {prompt:?}, \
                 already asked {:?}",
                self.texts_asked.borrow()
            )
        })
    }
}

/// design D5/"An interactive run asks before each move": a rename
/// answer carries out the move exactly as an applying batch run would,
/// and a skip answer leaves the file untouched and reports it skipped
/// with reason `declined`.
#[test]
fn a_rename_answer_moves_the_file_and_a_skip_answer_declines_it() {
    let a = PathBuf::from("/lib/a.pdf");
    let b = PathBuf::from("/lib/b.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash_for("interactive-rename-a"),
            pdf_with_embedded_doi("10.1000/interactive-rename-a"),
        )
        .with_file(
            &b,
            hash_for("interactive-rename-b"),
            pdf_with_embedded_doi("10.1000/interactive-rename-b"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/interactive-rename-a",
            record_by("Smith", 2024, "10.1000/interactive-rename-a"),
        )
        .answering(
            "doi:10.1000/interactive-rename-b",
            record_by("Doe", 2023, "10.1000/interactive-rename-b"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename, Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert_eq!(
        filesystem.renames(),
        vec![(a.clone(), PathBuf::from("/lib/Smith2024.pdf"))],
        "only the accepted file must move: got {:?}",
        filesystem.renames()
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path, target, .. }
                if *path == a && target.as_path() == Path::new("/lib/Smith2024.pdf")
        )),
        "the accepted file must report renamed exactly as an applying batch run would: \
         got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Skipped { path, reason: SkipReason::Declined, .. } if *path == b)),
        "the declined file must be reported skipped with reason declined: got {events:?}"
    );
    assert_eq!(
        asker.questions_asked().len(),
        2,
        "got {:?}",
        asker.questions_asked()
    );
}

/// design "A file with nothing to decide": a file already carrying the
/// name its record implies is reported already-named and no question is
/// put about it, while a file with a move to decide still gets one.
#[test]
fn a_file_with_nothing_to_decide_is_never_asked_about() {
    let already_named = PathBuf::from("/lib/Smith2024.pdf");
    let needs_a_decision = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &already_named,
            hash_for("interactive-already-named"),
            pdf_with_embedded_doi("10.1000/interactive-already-named"),
        )
        .with_file(
            &needs_a_decision,
            hash_for("interactive-needs-decision"),
            pdf_with_embedded_doi("10.1000/interactive-needs-decision"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/interactive-already-named",
            record_by("Smith", 2024, "10.1000/interactive-already-named"),
        )
        .answering(
            "doi:10.1000/interactive-needs-decision",
            record_by("Doe", 2023, "10.1000/interactive-needs-decision"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![already_named.clone(), needs_a_decision.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::AlreadyNamed { path } if *path == already_named
        )),
        "got {events:?}"
    );
    assert_eq!(
        asker.questions_asked().len(),
        1,
        "the already-named file must never be asked about: got {:?}",
        asker.questions_asked()
    );
    assert_eq!(
        asker.questions_asked()[0].path,
        needs_a_decision,
        "got {:?}",
        asker.questions_asked()
    );
}

/// design "A declared name stays free" / "the split lets the driver ask
/// before claiming": a declined proposal leaves its target free for the
/// next file that wants it, and an accepted one takes it, exactly as
/// `Planner::propose`/`claim` state.
#[test]
fn a_declined_proposal_leaves_the_next_files_target_unsuffixed() {
    let a = PathBuf::from("/lib/a.pdf");
    let b = PathBuf::from("/lib/b.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash_for("interactive-decline-a"),
            pdf_with_embedded_doi("10.1000/interactive-decline-a"),
        )
        .with_file(
            &b,
            hash_for("interactive-decline-b"),
            pdf_with_embedded_doi("10.1000/interactive-decline-b"),
        );
    // Both render "Smith2024.pdf" under `[auth][year]`, so they collide.
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/interactive-decline-a",
            record_by("Smith", 2024, "10.1000/interactive-decline-a"),
        )
        .answering(
            "doi:10.1000/interactive-decline-b",
            record_by("Smith", 2024, "10.1000/interactive-decline-b"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path, target, .. }
                if *path == b && target.as_path() == Path::new("/lib/Smith2024.pdf")
        )),
        "the second file's target must be unsuffixed once the first declines it: got {events:?}"
    );
}

#[test]
fn an_accepted_proposal_suffixes_the_next_files_colliding_target() {
    let a = PathBuf::from("/lib/a.pdf");
    let b = PathBuf::from("/lib/b.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash_for("interactive-accept-a"),
            pdf_with_embedded_doi("10.1000/interactive-accept-a"),
        )
        .with_file(
            &b,
            hash_for("interactive-accept-b"),
            pdf_with_embedded_doi("10.1000/interactive-accept-b"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/interactive-accept-a",
            record_by("Smith", 2024, "10.1000/interactive-accept-a"),
        )
        .answering(
            "doi:10.1000/interactive-accept-b",
            record_by("Smith", 2024, "10.1000/interactive-accept-b"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path, target, .. }
                if *path == b && target.as_path() == Path::new("/lib/Smith2024a.pdf")
        )),
        "the second file's target must be suffixed once the first claims it: got {events:?}"
    );
}

/// The one path where `Planning::accept` reconstructs the planner key
/// by stripping the group directory: a template rendering into a
/// subdirectory, accepted through the interactive driver. Only the
/// batch tests (`renaming.rs`) covered this before — `plan`/`propose`
/// widen into the subdirectory the same way in both paths, but only
/// `accept`, not `claim`, has to turn a full `sub/Name.pdf` target back
/// into the relative key the planner claimed it under.
#[test]
fn an_interactive_proposal_into_a_subdirectory_claims_the_right_key_and_suffixes_a_second() {
    let a = PathBuf::from("/lib/a.pdf");
    let b = PathBuf::from("/lib/b.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash_for("interactive-subdir-a"),
            pdf_with_embedded_doi("10.1000/interactive-subdir-a"),
        )
        .with_file(
            &b,
            hash_for("interactive-subdir-b"),
            pdf_with_embedded_doi("10.1000/interactive-subdir-b"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/interactive-subdir-a",
            record_by("Smith", 2024, "10.1000/interactive-subdir-a"),
        )
        .answering(
            "doi:10.1000/interactive-subdir-b",
            record_by("Smith", 2024, "10.1000/interactive-subdir-b"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("sub/[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path, target, .. }
                if *path == a && target.as_path() == Path::new("/lib/sub/Smith2024.pdf")
        )),
        "the accepted file must land in the subdirectory the template named: got {events:?}"
    );
    assert!(
        filesystem
            .renames()
            .contains(&(a, PathBuf::from("/lib/sub/Smith2024.pdf"))),
        "got {:?}",
        filesystem.renames()
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path, target, .. }
                if *path == b && target.as_path() == Path::new("/lib/sub/Smith2024a.pdf")
        )),
        "a second file targeting the same subdirectory name must be suffixed, proving \
         `accept` claimed the first under the subdirectory-relative key `propose` widened \
         into: got {events:?}"
    );
}

/// design "each file's resolution, question, fate and sidecar SHALL
/// remain adjacent": with no sidecars or master file configured there is
/// nothing but the resolved/fate pair for each file, but adjacency and
/// input order are exactly what this pins — a driver that resolved a
/// later file to decide an earlier one's suffix would interleave them.
#[test]
fn each_files_events_stay_adjacent_and_in_input_order() {
    let a = PathBuf::from("/lib/a.pdf");
    let b = PathBuf::from("/lib/b.pdf");
    let c = PathBuf::from("/lib/c.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash_for("interactive-adjacency-a"),
            pdf_with_embedded_doi("10.1000/interactive-adjacency-a"),
        )
        .with_file(
            &b,
            hash_for("interactive-adjacency-b"),
            pdf_with_embedded_doi("10.1000/interactive-adjacency-b"),
        )
        .with_file(
            &c,
            hash_for("interactive-adjacency-c"),
            pdf_with_embedded_doi("10.1000/interactive-adjacency-c"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/interactive-adjacency-a",
            record_by("Smith", 2024, "10.1000/interactive-adjacency-a"),
        )
        .answering(
            "doi:10.1000/interactive-adjacency-b",
            record_by("Doe", 2023, "10.1000/interactive-adjacency-b"),
        )
        .answering(
            "doi:10.1000/interactive-adjacency-c",
            record_by("Roe", 2022, "10.1000/interactive-adjacency-c"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename, Answer::Skip, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone(), c.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let paths: Vec<PathBuf> = events
        .iter()
        .map(|event| match event {
            Event::Resolved { path, .. }
            | Event::Renamed { path, .. }
            | Event::Skipped { path, .. } => path.clone(),
            other => panic!("unexpected event in an adjacency test: {other:?}"),
        })
        .collect();

    assert_eq!(
        paths,
        vec![a.clone(), a, b.clone(), b, c.clone(), c],
        "each file's resolved event must be immediately followed by its own fate, in input order"
    );
}

/// Runs one rename over a fresh real library holding a single file and
/// hands back what the library recorded: every artifact record it holds
/// afterwards, and how many items it minted. `answers` empty runs the
/// batch path under `--apply`; anything else runs the interactive one,
/// which carries its accepted moves out without `--apply`.
fn recorded_by_a_rename(answers: Vec<Answer>) -> (Vec<ArtifactRecord>, usize) {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"interactive-and-apply bytes";
    let path = write_real_file(&root, "original.pdf", bytes);
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(bytes),
        pdf_with_embedded_doi("10.1000/interactive-admission"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/interactive-admission")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let interactive = !answers.is_empty();
    let mut asker = ScriptedAsker::new(answers);
    let mut session = match interactive {
        true => Session::interactive(&mut asker),
        false => Session::batch(),
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], !interactive),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut session,
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: renamed, .. } if *renamed == path
        )),
        "setup: the file must move: got {events:?}"
    );
    (
        library::ArtifactStore::read(&root)
            .iter()
            .cloned()
            .collect(),
        library::ItemStore::read(&root).len(),
    )
}

/// design D8/"Everything after the decision is the batch path": an
/// accepted interactive rename is recorded in the library exactly as an
/// applying batch run records it — same library-relative path, same
/// hash history, an item of its own either way — because what happens
/// after the decision does not know or care whether the decision came
/// from `--apply` or from a yes.
///
/// The identities and the modification times are the two things that
/// cannot match: a UUID is minted per record and the two runs write
/// their files at different moments.
#[test]
fn an_accepted_interactive_rename_is_recorded_exactly_as_an_apply_run_records_it() {
    let (accepted, accepted_items) = recorded_by_a_rename(vec![Answer::Rename]);
    let (applied, applied_items) = recorded_by_a_rename(Vec::new());

    assert_eq!(applied.len(), 1, "setup: got {applied:?}");
    assert_eq!(accepted.len(), applied.len(), "got {accepted:?}");
    assert_eq!(accepted_items, 1, "got {accepted_items}");
    assert_eq!(applied_items, accepted_items);

    let accepted = &accepted[0];
    let applied = &applied[0];
    assert_eq!(accepted.path, "Smith2024.pdf", "got {accepted:?}");
    assert_eq!(accepted.path, applied.path);
    assert_eq!(accepted.size, applied.size);
    assert_eq!(
        accepted.history, applied.history,
        "an accepted interactive rename must record exactly the history an apply run records"
    );
    assert_eq!(
        accepted.history[0].run,
        LibraryRunId::new(fixed_now()),
        "the hash entry is stamped with the run, got {:?}",
        accepted.history
    );
    assert!(
        accepted.item.is_some() && applied.item.is_some(),
        "both must link the record to the item minted for the work"
    );
}

/// design "Quitting an interactive run leaves the rest untouched": a
/// quit at the third of five files leaves the first two moved and the
/// rest untouched and unresolved; `run-finished` counts the file quit
/// at and every file after it as unreached, and the master `.bib`
/// merge still runs for the files that were visited.
#[test]
fn quitting_counts_unreached_and_still_merges_the_bib_for_the_visited_files() {
    let paths: Vec<PathBuf> = (1..=5)
        .map(|n| PathBuf::from(format!("/lib/{n}.pdf")))
        .collect();
    let mut documents = FakeDocuments::new();
    let mut crossref = KeyedSource::new(SourceName::Crossref);
    for (n, path) in paths.iter().enumerate() {
        let doi_value = format!("10.1000/interactive-quit-{n}");
        documents = documents.with_file(
            path,
            hash_for(&format!("interactive-quit-{n}")),
            pdf_with_embedded_doi(&doi_value),
        );
        crossref = crossref.answering(
            &format!("doi:{doi_value}"),
            record_by(&format!("Author{n}"), 2020 + n as i32, &doi_value),
        );
    }
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    // An interactive run is an applying run for its log (design D7), so
    // it is refused outright unless there is somewhere to write one.
    let state = tempdir().unwrap();
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(false),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename, Answer::Rename, Answer::Quit]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::rename(paths.clone(), false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    // Unreached files did not succeed either: the exit code is the
    // partial-success one even though nothing was declined here.
    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
    assert_eq!(
        filesystem.renames().len(),
        2,
        "only the first two accepted files must move: got {:?}",
        filesystem.renames()
    );
    assert!(
        !lines.iter().any(|line| line["event"] == "resolved"
            && (line["path"] == "/lib/4.pdf" || line["path"] == "/lib/5.pdf")),
        "a file after the quit must never be resolved: got {lines:?}"
    );

    let finished = lines
        .last()
        .unwrap_or_else(|| panic!("expected at least run-started and run-finished"));
    assert_eq!(finished["event"], "run-finished", "got {lines:?}");
    assert_eq!(
        finished["counts"]["unreached"], 3,
        "the file quit at and both after it must count as unreached: got {lines:?}"
    );

    assert!(
        lines.iter().any(|line| line["event"] == "bib-entry"),
        "the master .bib merge must still run for the files visited before the quit: \
         got {lines:?}"
    );
}

// ---------------------------------------------------------------------
// design D4/D5: passing over already-named files in an interactive run
// ---------------------------------------------------------------------

/// [`effective_with_default_template`], additionally setting
/// `rename.skip-named` to `skip_named` rather than leaving it at its
/// built-in default.
fn effective_skipping_named(template: &str, skip_named: bool) -> Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
        layer.rename = Some(RenameLayer {
            collision: None,
            batch: None,
            skip_named: Some(skip_named),
        });
    })
}

/// design "An interactive run passes over already-named files": with
/// `rename.skip-named` on (the default), two already-named files among
/// a directory of three render nothing at all — no resolution line, no
/// outcome line, no question — while the third, which needs a decision,
/// is reported and asked about exactly as it would be with the setting
/// off. The closing summary says how many were passed over.
#[test]
fn an_interactive_run_with_skip_named_renders_nothing_for_already_named_files() {
    let smith = PathBuf::from("/lib/Smith2024.pdf");
    let doe = PathBuf::from("/lib/Doe2023.pdf");
    let original = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &smith,
            hash_for("skip-named-smith"),
            pdf_with_embedded_doi("10.1000/skip-named-smith"),
        )
        .with_file(
            &doe,
            hash_for("skip-named-doe"),
            pdf_with_embedded_doi("10.1000/skip-named-doe"),
        )
        .with_file(
            &original,
            hash_for("skip-named-roe"),
            pdf_with_embedded_doi("10.1000/skip-named-roe"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/skip-named-smith",
            record_by("Smith", 2024, "10.1000/skip-named-smith"),
        )
        .answering(
            "doi:10.1000/skip-named-doe",
            record_by("Doe", 2023, "10.1000/skip-named-doe"),
        )
        .answering(
            "doi:10.1000/skip-named-roe",
            record_by("Roe", 2022, "10.1000/skip-named-roe"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_skipping_named("[auth][year]", true);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(
            Command::rename(vec![smith.clone(), doe.clone(), original.clone()], false),
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();

    assert!(
        !text.contains("Smith2024.pdf"),
        "a passed-over file must render nothing at all: {text}"
    );
    assert!(
        !text.contains("Doe2023.pdf"),
        "a passed-over file must render nothing at all: {text}"
    );
    assert!(
        text.contains("original.pdf: resolved"),
        "the file needing a decision must still be reported: {text}"
    );
    assert!(
        text.contains("Roe2022.pdf"),
        "the accepted file's own new name must be reported: {text}"
    );
    assert_eq!(
        asker.questions_asked().len(),
        1,
        "only the file needing a decision may be asked about: {:?}",
        asker.questions_asked()
    );
    assert_eq!(asker.questions_asked()[0].path, original);
    assert!(
        text.contains("2 already named (not shown)"),
        "the summary must say how many were passed over: {text}"
    );
    assert_eq!(outcome, Outcome::Success, "got stdout {text}");
}

/// The setting's other half: `--no-skip-named` shows an already-named
/// file exactly as a batch run does.
///
/// It is now also asked about — `supply-identifiers-interactively`
/// amended the requirement this test was written against, because a
/// file named from the wrong record is exactly what `--no-skip-named`
/// is for and the operator had no way to say so. The keep/supply
/// question it gets is pinned by
/// [`an_already_named_file_under_no_skip_named_offers_to_keep_or_supply`];
/// what this test still holds is that the batch rendering is
/// unchanged.
#[test]
fn an_interactive_run_with_no_skip_named_renders_already_named_files_as_batch_does() {
    let smith = PathBuf::from("/lib/Smith2024.pdf");
    let original = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &smith,
            hash_for("no-skip-named-smith"),
            pdf_with_embedded_doi("10.1000/no-skip-named-smith"),
        )
        .with_file(
            &original,
            hash_for("no-skip-named-roe"),
            pdf_with_embedded_doi("10.1000/no-skip-named-roe"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/no-skip-named-smith",
            record_by("Smith", 2024, "10.1000/no-skip-named-smith"),
        )
        .answering(
            "doi:10.1000/no-skip-named-roe",
            record_by("Roe", 2022, "10.1000/no-skip-named-roe"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_skipping_named("[auth][year]", false);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    // The already-named file is asked first and kept; the other is
    // renamed.
    let mut asker = ScriptedAsker::new(vec![Answer::Keep, Answer::Rename]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::rename(vec![smith.clone(), original.clone()], false),
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();

    assert!(
        text.contains("Smith2024.pdf: resolved"),
        "with the setting off, an already-named file's resolution line must show: {text}"
    );
    assert!(
        text.contains("Smith2024.pdf: already named"),
        "with the setting off, an already-named file's outcome line must show: {text}"
    );
    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        2,
        "with the setting off, both files are asked about: {questions:?}"
    );
    assert_eq!(questions[0].path, smith, "got {questions:?}");
    assert_eq!(
        questions[0].choices.first().copied(),
        Some(Answer::Keep),
        "keeping it is what Enter does: {questions:?}"
    );
}

/// design "The run log and any `--json` rendering are unaffected": the
/// JSON stream carries a passed-over file's `resolved` and
/// `already-named` events in full, whatever the human terminal shows.
#[test]
fn the_json_stream_carries_a_passed_over_files_events_in_full() {
    let smith = PathBuf::from("/lib/Smith2024.pdf");
    let original = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &smith,
            hash_for("json-skip-named-smith"),
            pdf_with_embedded_doi("10.1000/json-skip-named-smith"),
        )
        .with_file(
            &original,
            hash_for("json-skip-named-roe"),
            pdf_with_embedded_doi("10.1000/json-skip-named-roe"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/json-skip-named-smith",
            record_by("Smith", 2024, "10.1000/json-skip-named-smith"),
        )
        .answering(
            "doi:10.1000/json-skip-named-roe",
            record_by("Roe", 2022, "10.1000/json-skip-named-roe"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_skipping_named("[auth][year]", true);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::rename(vec![smith.clone(), original.clone()], false),
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert!(
        lines
            .iter()
            .any(|line| line["event"] == "resolved" && line["path"] == "/lib/Smith2024.pdf"),
        "got {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line["event"] == "already-named" && line["path"] == "/lib/Smith2024.pdf"),
        "got {lines:?}"
    );
}

/// design D5: "the hold covers one file, ends before that file's
/// question is put". An [`Asker`] that can see what the terminal has
/// been given proves both halves of the promise at once: the resolved
/// line of the file it is about to be asked about is already there, and
/// nothing about an earlier, passed-over file ever leaked in — neither
/// before the question nor, since this is the only question the run
/// puts, after it.
struct SharedBuffer(Rc<RefCell<Vec<u8>>>);

impl std::io::Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct ObservingAsker {
    buffer: Rc<RefCell<Vec<u8>>>,
    answers: std::vec::IntoIter<Answer>,
    seen_at_first_question: RefCell<Option<String>>,
    first_question: RefCell<Option<Question>>,
}

impl ObservingAsker {
    fn new(buffer: Rc<RefCell<Vec<u8>>>, answers: Vec<Answer>) -> ObservingAsker {
        ObservingAsker {
            buffer,
            answers: answers.into_iter(),
            seen_at_first_question: RefCell::new(None),
            first_question: RefCell::new(None),
        }
    }

    fn first_question(&self) -> Question {
        self.first_question
            .borrow()
            .clone()
            .expect("must have been asked at least once")
    }

    fn seen_at_first_question(&self) -> String {
        self.seen_at_first_question
            .borrow()
            .clone()
            .expect("must have been asked at least once")
    }
}

impl Asker for ObservingAsker {
    fn choose(&mut self, question: &Question) -> Answer {
        if self.seen_at_first_question.borrow().is_none() {
            let snapshot = String::from_utf8(self.buffer.borrow().clone()).unwrap();
            *self.seen_at_first_question.borrow_mut() = Some(snapshot);
            *self.first_question.borrow_mut() = Some(question.clone());
        }
        self.answers.next().unwrap_or_else(|| {
            panic!("asked more questions than were scripted; question was {question:?}")
        })
    }

    /// This change puts no text prompt; a double that is asked for one
    /// fails the test rather than inventing an answer.
    fn text(&mut self, prompt: &TextPrompt) -> Option<String> {
        panic!("asked for text, which no question here puts: {prompt:?}")
    }
}

#[test]
fn the_hold_ends_before_the_question_and_never_leaks_a_passed_over_files_lines() {
    let smith = PathBuf::from("/lib/Smith2024.pdf");
    let original = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &smith,
            hash_for("hold-smith"),
            pdf_with_embedded_doi("10.1000/hold-smith"),
        )
        .with_file(
            &original,
            hash_for("hold-roe"),
            pdf_with_embedded_doi("10.1000/hold-roe"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/hold-smith",
            record_by("Smith", 2024, "10.1000/hold-smith"),
        )
        .answering(
            "doi:10.1000/hold-roe",
            record_by("Roe", 2022, "10.1000/hold-roe"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_skipping_named("[auth][year]", true);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let buffer = Rc::new(RefCell::new(Vec::new()));
    let mut asker = ObservingAsker::new(buffer.clone(), vec![Answer::Rename]);
    let mut shared_out = SharedBuffer(buffer.clone());
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut shared_out,
        err: &mut err,
    };

    let _ = dispatch(
        &cli(
            Command::rename(vec![smith.clone(), original.clone()], false),
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let seen = asker.seen_at_first_question();
    // The hold now lasts until the file's fate is settled, which is
    // later than this change's predecessor allowed: a file the operator
    // re-identifies must not have had its first record reported
    // already. What reaches the operator before they decide is the
    // question's description, which is not the stream — so that is
    // where the file's own resolution has to be legible.
    let asked = asker.first_question();
    assert_eq!(asked.path, original, "got {asked:?}");
    assert!(
        asked
            .description
            .iter()
            .any(|line| line.contains("original.pdf")),
        "the file's own resolution must be in front of the operator before \
         they are asked about it: {asked:?}"
    );
    assert!(
        !seen.contains("Smith2024.pdf"),
        "a passed-over file's lines must never reach the terminal, before the question \
         asked about a later file or after: {seen}"
    );
}

// ---------------------------------------------------------------------
// design D5: bibliography output is not held and is not suppressed
// ---------------------------------------------------------------------

/// A sidecar written beside a passed-over file is reported even though
/// nothing else about that file is: the setting hides a file that needs
/// no decision, not what a run actually did.
#[test]
fn a_sidecar_for_a_passed_over_file_is_still_reported() {
    let smith = PathBuf::from("/lib/Smith2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &smith,
        hash_for("sidecar-passed-over"),
        pdf_with_embedded_doi("10.1000/sidecar-passed-over"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/sidecar-passed-over")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.rename = Some(RenameLayer {
            collision: None,
            batch: None,
            skip_named: Some(true),
        });
        layer.bib = Some(BibLayer {
            path: None,
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![smith.clone()], false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    assert!(
        !text.contains("Smith2024.pdf: resolved"),
        "the passed-over file's resolution line must not show: {text}"
    );
    assert!(
        !text.contains("Smith2024.pdf: already named"),
        "the passed-over file's outcome line must not show: {text}"
    );
    assert!(
        text.contains("sidecar written to"),
        "a sidecar written beside a passed-over file must still be reported: {text}"
    );
}

/// The same for a sidecar that failed to write: the failure is a fact
/// about the run and is reported whether or not the file it happened
/// beside was shown.
#[test]
fn a_sidecar_write_failure_for_a_passed_over_file_is_still_reported() {
    let smith = PathBuf::from("/lib/Smith2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &smith,
        hash_for("sidecar-failure-passed-over"),
        pdf_with_embedded_doi("10.1000/sidecar-failure-passed-over"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/sidecar-failure-passed-over",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new().with_write_failure(sidecar_path(&smith));
    let state = tempdir().unwrap();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.rename = Some(RenameLayer {
            collision: None,
            batch: None,
            skip_named: Some(true),
        });
        layer.bib = Some(BibLayer {
            path: None,
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![smith.clone()], false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    assert!(
        !text.contains("Smith2024.pdf: resolved"),
        "the passed-over file's resolution line must not show: {text}"
    );
    assert!(
        !text.contains("Smith2024.pdf: already named"),
        "the passed-over file's outcome line must not show: {text}"
    );
    assert!(
        text.contains("skipped"),
        "a sidecar write failure beside a passed-over file must still be reported: {text}"
    );
}

// ---------------------------------------------------------------------
// Task 3.1: the description reaches the question, and both streams keep
// their shape (design D1's "written where the question is written";
// design D4's batch wording)
// ---------------------------------------------------------------------

/// design "The description is part of the question, so it goes where
/// the question goes": the evidence a rename question rests on reaches
/// the `Question` the asker is given, and the `resolved` line the run
/// always reports still reaches stdout untouched.
#[test]
fn the_description_reaches_the_question_and_stdout_still_carries_resolved() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("description-reaches-question"),
        pdf_with_embedded_doi("10.1000/description-reaches-question"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/description-reaches-question",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![path.clone()], false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let asked = asker.questions_asked();
    assert_eq!(asked.len(), 1, "got {asked:?}");
    assert!(
        !asked[0].description.is_empty(),
        "a question about a resolved file must carry the evidence it rests on \
         (crate::describe::describe), got an empty description: {asked:?}"
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line["event"] == "resolved" && line["path"] == "/lib/paper.pdf"),
        "the resolved event must still reach stdout, unreplaced by the \
         description: got {lines:?}"
    );
}

/// design "It is written to standard error beside the menu, not into
/// the event stream on standard output": the description itself never
/// appears in the JSON stream — only the `resolved` event's own fields
/// do, and `Question::description` carries no field of its own there.
#[test]
fn the_description_does_not_reach_the_json_event_stream() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("description-not-in-stream"),
        pdf_with_embedded_doi("10.1000/description-not-in-stream"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by(
            "Smith",
            2024,
            "10.1000/description-not-in-stream",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![path.clone()], false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let asked = asker.questions_asked();
    let description = asked[0].description.join("\n");
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let resolved = lines
        .iter()
        .find(|line| line["event"] == "resolved")
        .unwrap_or_else(|| panic!("no resolved event in {lines:?}"));

    assert!(
        resolved.get("description").is_none(),
        "the resolved event must carry no description field of its own: {resolved:?}"
    );
    assert!(
        !description.is_empty(),
        "the description shown to the asker must not be empty just because \
         it stays off stdout"
    );
}

/// design D1's `file` line: "names the file as the rest of the run
/// names it — relative to where the run was started, and whole when it
/// lies outside". Two directories under one run both holding
/// `paper.pdf` are two different moves, and two questions naming both
/// of them `paper.pdf` would take one answer for the other.
#[test]
fn two_files_of_one_name_in_two_directories_are_asked_about_distinguishably() {
    let one = PathBuf::from("/lib/tree-a/paper.pdf");
    let other = PathBuf::from("/lib/tree-b/paper.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &one,
            hash_for("tree-a-paper"),
            pdf_with_embedded_doi("10.1000/tree-a"),
        )
        .with_file(
            &other,
            hash_for("tree-b-paper"),
            pdf_with_embedded_doi("10.1000/tree-b"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/tree-a",
            record_by("Adams", 2024, "10.1000/tree-a"),
        )
        .answering(
            "doi:10.1000/tree-b",
            record_by("Brown", 2023, "10.1000/tree-b"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip, Answer::Skip]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![one, other], false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker).started_in(PathBuf::from("/lib")),
        &mut streams,
    );

    let asked = asker.questions_asked();
    assert_eq!(asked.len(), 2, "got {asked:?}");
    let named: Vec<String> = asked
        .iter()
        .map(|question| {
            question
                .description
                .iter()
                .find(|line| line.starts_with("file "))
                .unwrap_or_else(|| panic!("no file line in {:?}", question.description))
                .clone()
        })
        .collect();

    // Built with the platform's own separator rather than written with
    // a slash: what this test is about is that the two files are told
    // apart by the directory above them, and Windows spells that
    // `tree-a\paper.pdf`.
    let named_as = |tree: &str| {
        format!(
            "file        {}",
            Path::new(tree).join("paper.pdf").display()
        )
    };
    assert_eq!(
        named,
        vec![named_as("tree-a"), named_as("tree-b"),],
        "each question must name its own file: got {asked:?}"
    );
}

/// design D4: "the batch line changes in one word: a content-index
/// answer reads `via crossref (cached)` rather than `via cache
/// (cached)`". A batch run's human output for a cached hit whose record
/// was made by Crossref names Crossref, not the content index.
#[test]
fn a_batch_cached_resolution_names_its_provenance_not_the_cache() {
    let path = PathBuf::from("/lib/paper.pdf");
    let hash = hash_for("batch-cached-provenance");
    // The documents would fail loudly if opened, so an accidental open
    // (rather than a content-index hit) shows up as a failure, not a
    // silently-live resolution.
    let documents = FakeDocuments::new().with_open_error(
        &path,
        hash.clone(),
        ExtractionError::Unreadable {
            message: "must never be opened".to_string(),
        },
    );
    let mut record = record_by("Smith", 2024, "10.1000/batch-cached-provenance");
    record.borax = BoraxExt {
        provenance: [("title".to_string(), FieldSource::Crossref)]
            .into_iter()
            .collect(),
        ..BoraxExt::default()
    };
    let index = ContentIndex::new(MemoryCache::new());
    index.put(&hash, &record);
    let sources: Vec<&dyn Source> = Vec::new();
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![path.clone()]), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("via crossref (cached)"),
        "a cached hit whose provenance names Crossref must say so, not \
         \"via cache\": got {text:?}"
    );
    assert!(!text.contains("via cache (cached)"), "got {text:?}");
}

// ---------------------------------------------------------------------
// interactive rename: supplied identifiers (design D1/D2/D2a/D3/D5/D7,
// tasks 3.2, 3.2a, 3.3, 3.4, 3.5, 4.1, 4.2)
//
// `decided` currently ends with
// `Answer::Override | Answer::Keep | Answer::Supply | Answer::Retry => todo!(...)`,
// so every test below that scripts one of those answers fails on that
// panic today. Tests that never reach that arm (an ordinary rename or
// skip) instead fail on their own assertion, most often because a menu
// does not yet offer `Supply` at all — pinning what the finished driver
// owes before the `Supply` loop exists to prove it.
// ---------------------------------------------------------------------

/// [`record_by`], with `title` set — the shape a conflict check needs.
fn record_by_with_title(family: &str, year: i32, doi_value: &str, title: &str) -> Record {
    Record {
        title: Some(title.to_string()),
        ..record_by(family, year, doi_value)
    }
}

/// `question.choices` names exactly `default` followed by `rest`, in any
/// order within `rest`: the first choice is the default an unadorned
/// Enter would take, and every situation offers a fixed set with no
/// choice that does not belong.
fn assert_choices(question: &Question, default: Answer, rest: &[Answer]) {
    assert_eq!(
        question.choices.first().copied(),
        Some(default),
        "the first choice is the default and must be {default:?}: got {:?}",
        question.choices
    );
    assert_eq!(
        question.choices.len(),
        rest.len() + 1,
        "got {:?}",
        question.choices
    );
    for answer in rest {
        assert!(
            question.choices.contains(answer),
            "expected {answer:?} among the choices, got {:?}",
            question.choices
        );
    }
}

/// A one-file [`Adapters`] rig shared by the tests below: a single PDF
/// at `/lib/paper.pdf`, an empty content index, no ledger, no
/// collection. `documents` and `sources` are supplied by the caller since
/// every test needs its own identifiers.
struct SupplyFixture {
    path: PathBuf,
    documents: FakeDocuments,
    index: ContentIndex<MemoryCache>,
    filesystem: FakeFilesystem,
    bib_files: FakeBibFiles,
}

impl SupplyFixture {
    fn new(documents: FakeDocuments) -> SupplyFixture {
        SupplyFixture {
            path: PathBuf::from("/lib/paper.pdf"),
            documents,
            index: ContentIndex::new(MemoryCache::new()),
            filesystem: FakeFilesystem::new(),
            bib_files: FakeBibFiles::new(),
        }
    }

    fn adapters<'a>(&'a self, sources: &'a [&'a dyn Source]) -> Adapters<'a, MemoryCache> {
        Adapters {
            documents: &self.documents,
            sources,
            index: &self.index,
            filesystem: &self.filesystem,
            bib_files: &self.bib_files,
            cache_root: None,
            now: fixed_now,
            collection_root: None,
            state_root: None,
        }
    }
}

// ---------------------------------------------------------------------
// D1, first table: which files get which question
// ---------------------------------------------------------------------

/// "resolves, proposal is a move": rename · supply a different
/// identifier · skip · quit, defaulting to rename.
#[test]
fn a_resolved_move_offers_supplying_a_different_identifier_alongside_rename() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-resolved-move"),
        pdf_with_embedded_doi("10.1000/d1-resolved-move"),
    ));
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d1-resolved-move",
        record_by("Smith", 2024, "10.1000/d1-resolved-move"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(
        &questions[0],
        Answer::Rename,
        &[Answer::Supply, Answer::Skip, Answer::Quit],
    );
}

/// "no identifier found": supply an identifier · skip · quit.
#[test]
fn a_file_with_no_identifier_offers_to_supply_one() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-no-identifier"),
        pdf_with_no_identifier(),
    ));
    let sources: Vec<&dyn Source> = Vec::new();
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(&questions[0], Answer::Supply, &[Answer::Skip, Answer::Quit]);
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::NoIdentifier,
                ..
            } if *path == fixture.path
        )),
        "a plain skip leaves the reason a batch run would have given: got {events:?}"
    );
}

/// "identifier found, no service holds it", the conclusive case: every
/// source answered `NotFound`, so nothing offers a retry.
#[test]
fn an_identifier_no_service_holds_offers_no_retry_when_the_answer_is_conclusive() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-conclusive-unresolvable"),
        pdf_with_embedded_doi("10.1000/d1-conclusive-unresolvable"),
    ));
    let crossref = fake_source(SourceName::Crossref, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(&questions[0], Answer::Supply, &[Answer::Skip, Answer::Quit]);
    assert!(
        !questions[0].choices.contains(&Answer::Retry),
        "a conclusive miss must not offer a retry: got {:?}",
        questions[0].choices
    );
}

/// The same row, inconclusive this time: a source that could not be
/// reached offers a retry, first.
#[test]
fn an_unreachable_service_offers_a_retry_before_supplying_an_identifier() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-inconclusive-unresolvable"),
        pdf_with_embedded_doi("10.1000/d1-inconclusive-unresolvable"),
    ));
    let crossref = fake_source(
        SourceName::Crossref,
        Err(SourceError::Unavailable {
            message: "503".to_string(),
        }),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_eq!(
        questions[0].choices.first().copied(),
        Some(Answer::Retry),
        "an inconclusive miss offers a retry first: got {:?}",
        questions[0].choices
    );
    assert!(
        questions[0].choices.contains(&Answer::Supply),
        "got {:?}",
        questions[0].choices
    );
}

/// "conflict, proposal is a move": rename anyway (never the default) ·
/// supply a different identifier · skip · quit.
#[test]
fn a_conflict_with_a_free_target_offers_overriding_but_never_defaults_to_it() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-conflict-move"),
        pdf_with_embedded_doi("10.1000/d1-conflict-move")
            .with_title("Old Title Extracted from the PDF"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d1-conflict-move",
        record_by_with_title(
            "Smith",
            2024,
            "10.1000/d1-conflict-move",
            "A Completely Different Title About Something Else",
        ),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(
        &questions[0],
        Answer::Skip,
        &[Answer::Override, Answer::Supply, Answer::Quit],
    );
}

/// "conflict, proposal is not a move": no override is offered, and a
/// plain skip is reported with the reason a batch run would have given
/// — the conflict, not `declined`.
#[test]
fn a_conflict_whose_target_is_not_free_offers_no_override() {
    // The file already sits at the name its (conflicting) record would
    // render, so the proposal computed from it is not a move.
    let path = PathBuf::from("/lib/Smith2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-conflict-not-move"),
        pdf_with_embedded_doi("10.1000/d1-conflict-not-move")
            .with_title("Old Title Extracted from the PDF"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d1-conflict-not-move",
        record_by_with_title(
            "Smith",
            2024,
            "10.1000/d1-conflict-not-move",
            "A Completely Different Title About Something Else",
        ),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(&questions[0], Answer::Skip, &[Answer::Supply, Answer::Quit]);
    assert!(
        !questions[0].choices.contains(&Answer::Override),
        "nothing to override when the proposal is not a move: got {:?}",
        questions[0].choices
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path: skipped_path,
                reason: SkipReason::Conflict { .. },
                ..
            } if *skipped_path == path
        )),
        "a batch run reports every conflict this way, whether or not it \
         would have been a move: got {events:?}"
    );
}

/// "unreadable or encrypted, hash known": supply an identifier · skip ·
/// quit.
#[test]
fn an_unreadable_file_with_a_known_hash_offers_to_supply_an_identifier() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_open_error(
        "/lib/paper.pdf",
        hash_for("d1-unreadable"),
        ExtractionError::Encrypted,
    ));
    let sources: Vec<&dyn Source> = Vec::new();
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(&questions[0], Answer::Supply, &[Answer::Skip, Answer::Quit]);
}

/// "already named, `--no-skip-named`": keep · supply a different
/// identifier · quit, defaulting to keep, and keeping emits
/// `already-named` exactly as a batch run's outcome would.
#[test]
fn an_already_named_file_under_no_skip_named_offers_to_keep_or_supply() {
    let path = PathBuf::from("/lib/Smith2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-already-named"),
        pdf_with_embedded_doi("10.1000/d1-already-named"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d1-already-named",
        record_by("Smith", 2024, "10.1000/d1-already-named"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_skipping_named("[auth][year]", false);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Keep]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(&questions[0], Answer::Keep, &[Answer::Supply, Answer::Quit]);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::AlreadyNamed { path: named } if *named == path)),
        "keeping an already-named file must emit already-named: got {events:?}"
    );
    assert!(
        filesystem.renames().is_empty(),
        "keeping must move nothing: got {:?}",
        filesystem.renames()
    );
}

// ---------------------------------------------------------------------
// D1, second table: where a supplied identifier leads
// ---------------------------------------------------------------------

/// A supplied identifier nothing holds leaves the file's original record
/// still on offer: the operator may accept it, supply another, or skip.
#[test]
fn a_supplied_identifier_that_does_not_resolve_leaves_the_original_record_on_offer() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-transition-unresolvable"),
        pdf_with_embedded_doi("10.1000/original-still-good"),
    ));
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/original-still-good",
        record_by("Smith", 2024, "10.1000/original-still-good"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.9999/nothing-holds-this".to_string())]);

    let events = events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        2,
        "the menu is put again after the failed supply: got {questions:?}"
    );
    assert_eq!(
        questions[1].target, questions[0].target,
        "the file's original proposal is what is offered again: got {questions:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path, target, .. }
                if *path == fixture.path && target == &PathBuf::from("/lib/Smith2024.pdf")
        )),
        "the original record is what ends up renamed: got {events:?}"
    );
}

/// Escaping the text prompt leaves the question exactly as it was.
#[test]
fn abandoning_the_supply_prompt_changes_nothing() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-transition-abandoned"),
        pdf_with_embedded_doi("10.1000/abandoned-supply"),
    ));
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/abandoned-supply",
        record_by("Smith", 2024, "10.1000/abandoned-supply"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip]).with_texts(vec![None]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 2, "got {questions:?}");
    assert_eq!(
        questions[1], questions[0],
        "an abandoned input must leave the question exactly as it was: got {questions:?}"
    );
}

/// Input that names no identifier is refused in place, without ever
/// reaching a service, and the operator is asked again.
#[test]
fn refused_input_is_asked_again_rather_than_reopening_the_menu() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-transition-refused"),
        pdf_with_embedded_doi("10.1000/refused-input"),
    ));
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/refused-input",
        record_by("Smith", 2024, "10.1000/refused-input"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("see email from Anna".to_string()), None]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert_eq!(
        asker.texts_asked().len(),
        2,
        "unparseable input is asked again, not folded back into the choice \
         menu: got {:?}",
        asker.texts_asked()
    );
    assert!(
        asker.texts_asked()[1].refused.is_some(),
        "the second prompt must say what was wrong with the first: got {:?}",
        asker.texts_asked()
    );
    // Two, not one: the menu that offered `Supply`, and the menu the
    // Esc returned to — which is what consumes the scripted `Skip` and
    // gives the file a fate. What "does not reopen the menu" forbids is
    // a third, between the two text prompts; an implementation that put
    // one would run out of scripted answers here.
    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        2,
        "refused input must not reopen the choice menu: got {questions:?}"
    );
    assert_eq!(
        questions[1], questions[0],
        "an abandoned input leaves the question exactly as it was: got {questions:?}"
    );
}

/// design "A reference's DOI caught": supplying the file's own DOI over
/// a proposal built from a citation's DOI produces a fresh proposal, and
/// the first name proposed is never claimed.
#[test]
fn supplying_a_different_identifier_produces_a_fresh_proposal_leaving_the_first_name_unclaimed() {
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash_for("d1-reference-doi"),
        pdf_with_embedded_doi("10.1000/reference-doi"),
    ));
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/reference-doi",
            record_by("Wrong", 2020, "10.1000/reference-doi"),
        )
        .answering(
            "doi:10.1000/the-files-own-doi",
            record_by("Right", 2021, "10.1000/the-files-own-doi"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/the-files-own-doi".to_string())]);

    let events = events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { target, .. } if target == &PathBuf::from("/lib/Right2021.pdf")
        )),
        "got {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            Event::Renamed { target, .. } | Event::Planned { target, .. }
                if target == &PathBuf::from("/lib/Wrong2020.pdf")
        )),
        "the first proposal's name must never be claimed: got {events:?}"
    );
}

/// design "Re-identifying a named file": with `--no-skip-named`, an
/// already-named file whose record was wrong can be supplied a better
/// identifier and renamed under it.
#[test]
fn no_skip_named_supplying_and_renaming_re_identifies_a_wrongly_named_file() {
    let path = PathBuf::from("/lib/Wrong2020.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-reidentify"),
        pdf_with_embedded_doi("10.1000/reidentify-wrong"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/reidentify-wrong",
            record_by("Wrong", 2020, "10.1000/reidentify-wrong"),
        )
        .answering(
            "doi:10.1000/reidentify-right",
            record_by("Right", 2021, "10.1000/reidentify-right"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_skipping_named("[auth][year]", false);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/reidentify-right".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 2, "got {questions:?}");
    assert_choices(&questions[0], Answer::Keep, &[Answer::Supply, Answer::Quit]);
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: renamed, target, .. }
                if *renamed == path && target == &PathBuf::from("/lib/Right2021.pdf")
        )),
        "got {events:?}"
    );
}

// ---------------------------------------------------------------------
// D5: a file's verdict follows the operator's decision
// ---------------------------------------------------------------------

/// A conflict overridden by a fresh supplied identifier is reported
/// once: one `resolved` (tier `supplied`), one `renamed`, and no
/// `skipped` for the same file.
#[test]
fn a_file_settled_by_a_supplied_identifier_is_reported_once() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d5-one-report"),
        pdf_with_embedded_doi("10.1000/d5-original").with_title("Old Title Extracted from the PDF"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/d5-original",
            record_by_with_title(
                "Smith",
                2024,
                "10.1000/d5-original",
                "A Completely Different Title About Something Else",
            ),
        )
        .answering(
            "doi:10.1000/d5-supplied",
            record_by("Doe", 2023, "10.1000/d5-supplied"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/d5-supplied".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let resolved: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
        .collect();
    let renamed: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Renamed { path: p, .. } if *p == path))
        .collect();
    let skipped: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
        .collect();

    assert_eq!(resolved.len(), 1, "got {events:?}");
    assert_eq!(renamed.len(), 1, "got {events:?}");
    assert!(skipped.is_empty(), "got {events:?}");
    match resolved[0] {
        Event::Resolved { tier, .. } => {
            assert_eq!(
                tier.as_deref(),
                Some("supplied"),
                "the tier reported must say the identifier was supplied: got {resolved:?}"
            );
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// D7: an operator's answer is remembered in the content index, on
// rename only
// ---------------------------------------------------------------------

/// A rename made from a supplied identifier writes its record to the
/// content index under the file's hash, so a later run can resolve it
/// from there without asking again.
#[test]
fn a_rename_from_a_supplied_identifier_is_written_to_the_content_index() {
    let hash = hash_for("d7-write-on-rename");
    let fixture = SupplyFixture::new(FakeDocuments::new().with_file(
        "/lib/paper.pdf",
        hash.clone(),
        pdf_with_no_identifier(),
    ));
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d7-write-on-rename",
        record_by("Smith", 2024, "10.1000/d7-write-on-rename"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/d7-write-on-rename".to_string())]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert_eq!(
        fixture.index.get(&hash),
        Some(record_by("Smith", 2024, "10.1000/d7-write-on-rename")),
        "the accepted record must be written under the file's hash"
    );
}

/// An identifier supplied and then abandoned by skipping is never
/// written to the content index, and a record the file already carried
/// there is left untouched.
#[test]
fn skipping_after_a_supplied_identifier_leaves_the_content_index_as_it_was() {
    let hash = hash_for("d7-abandoned-not-written");
    // The documents would fail loudly if opened: an already-indexed file
    // is answered from the index and never touches the file at all,
    // which is what this test needs to hold while the operator's
    // candidate is abandoned.
    let documents = FakeDocuments::new().with_open_error(
        "/lib/paper.pdf",
        hash.clone(),
        ExtractionError::Unreadable {
            message: "must never be opened".to_string(),
        },
    );
    let fixture = SupplyFixture::new(documents);
    let already_held = record_by("Roe", 2019, "10.1000/d7-already-held");
    fixture.index.put(&hash, &already_held);
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d7-wrong-candidate",
        record_by("Doe", 2023, "10.1000/d7-wrong-candidate"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("10.1000/d7-wrong-candidate".to_string())]);

    let events = events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Declined,
                ..
            } if *path == fixture.path
        )),
        "a move that was on offer and then abandoned is declined: got {events:?}"
    );
    assert_eq!(
        fixture.index.get(&hash),
        Some(already_held),
        "the record the file already carried must be left exactly as it was"
    );
}

// ---------------------------------------------------------------------
// Second red pass: overrode, the three "resolves, not a move" supply
// transitions, the remaining 4.2 abandonment cases, citation (4.2a) and
// a failing content-index write (4.2b)
// ---------------------------------------------------------------------

/// design D5/D6, task 3.6: accepting a conflict reports what was
/// overridden on the record's own `resolved` event, in the vocabulary
/// the skip would have used, followed by its `renamed` — never a
/// `skipped` for the file.
#[test]
fn overriding_a_conflict_reports_what_was_overridden_and_renames() {
    let path = PathBuf::from("/lib/original.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("overrode-reports"),
        pdf_with_embedded_doi("10.1000/overrode-reports")
            .with_title("Old Title Extracted from the PDF"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/overrode-reports",
        record_by_with_title(
            "Smith",
            2024,
            "10.1000/overrode-reports",
            "A Completely Different Title About Something Else",
        ),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Override]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let resolved: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
        .collect();
    let renamed: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Renamed { path: p, .. } if *p == path))
        .collect();
    let skipped: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
        .collect();

    assert_eq!(resolved.len(), 1, "got {events:?}");
    assert_eq!(renamed.len(), 1, "got {events:?}");
    assert!(skipped.is_empty(), "got {events:?}");
    match resolved[0] {
        Event::Resolved { overrode, .. } => {
            assert_eq!(
                *overrode,
                Some(Overridden {
                    field: "title".to_string(),
                    extracted: "Old Title Extracted from the PDF".to_string(),
                    resolved: "A Completely Different Title About Something Else".to_string(),
                    similarity: 0.2,
                }),
                "must carry the same field, values and similarity the skip would have: \
                 got {resolved:?}"
            );
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}

/// A conflict the operator could have overridden but chose instead to
/// skip is abandoned exactly as any other candidate: nothing is written
/// to the content index for it.
#[test]
fn skipping_a_conflict_that_could_have_been_overridden_writes_nothing() {
    let path = PathBuf::from("/lib/original.pdf");
    let hash = hash_for("d7-override-declined");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/d7-override-declined")
            .with_title("Old Title Extracted from the PDF"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d7-override-declined",
        record_by_with_title(
            "Smith",
            2024,
            "10.1000/d7-override-declined",
            "A Completely Different Title About Something Else",
        ),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    // The free target means `Override` is on the menu; the operator
    // declines it.
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    // Proves the scenario actually put the conflict question with
    // `Override` on it, rather than this test passing vacuously because
    // today's driver skips a conflicting file before ever asking
    // anything.
    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        1,
        "the conflict must be asked about, with Override among its choices: got {questions:?}"
    );
    assert!(
        questions[0].choices.contains(&Answer::Override),
        "got {questions:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path: p,
                reason: SkipReason::Conflict { .. },
                ..
            } if *p == path
        )),
        "got {events:?}"
    );
    assert_eq!(
        index.get(&hash),
        None,
        "a conflict the operator declined to override must not be remembered"
    );
}

// ---------------------------------------------------------------------
// D1's second table: the three "resolves, proposal is not a move"
// outcomes a supplied identifier can lead to
// ---------------------------------------------------------------------

/// A supplied identifier that resolves into a taken target reports that
/// outcome and puts the file's own menu again, unchanged: the situation
/// the operator is asked about is the one their file was already in,
/// not a new one built from the failed candidate.
#[test]
fn a_supplied_identifier_resolving_into_a_taken_target_reports_and_reasks() {
    const CANDIDATE: &str = "10.1000/would-collide";
    const OUTCOME: &str = "name taken";

    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-transition-target-taken"),
        pdf_with_no_identifier(),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/would-collide",
        record_by("Doe", 2023, "10.1000/would-collide"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    // "Doe2023.pdf" — the name the supplied candidate would render — is
    // already occupied by an unrelated file, and the policy is `skip`
    // so a collision is a `TargetTaken`, not a suffix.
    let filesystem =
        FakeFilesystem::new().with_existing("/lib", [("Doe2023.pdf", Some("unrelated-hash"))]);
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.rename = Some(RenameLayer {
            collision: Some("skip".to_string()),
            batch: None,
            skip_named: None,
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("10.1000/would-collide".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        2,
        "the file's menu is put again: got {questions:?}"
    );
    assert_eq!(
        questions[1].choices, questions[0].choices,
        "the situation the operator is asked about is unchanged: got {questions:?}"
    );
    assert_eq!(questions[1].path, questions[0].path, "got {questions:?}");
    // The candidate's outcome is reported in the re-put question's
    // description, which is the only channel left for it: design D5
    // keeps an abandoned candidate out of the event stream entirely.
    // Asserting only that the description changed would pass on any
    // change at all, so the identifier tried and the outcome's own
    // label both have to be there.
    let reported = questions[1].description.join("\n");
    assert!(
        reported.contains(CANDIDATE) && reported.contains(OUTCOME),
        "the re-put question must report {CANDIDATE} and {OUTCOME}: got {reported}"
    );
    assert!(
        !questions[0].description.join("\n").contains(CANDIDATE),
        "and must not have reported it before it was tried: got {questions:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path: p,
                reason: SkipReason::NoIdentifier,
                ..
            } if *p == path
        )),
        "the file's own, undisturbed situation is what a plain skip reports: got {events:?}"
    );
    assert_eq!(
        index.get(&hash_for("d1-transition-target-taken")),
        None,
        "a candidate that led to a taken target is never remembered"
    );
}

/// A supplied identifier that resolves into a record the template
/// renders no usable name from reports that outcome and puts the file's
/// own menu again, unchanged.
#[test]
fn a_supplied_identifier_resolving_into_an_unnameable_record_reports_and_reasks() {
    const CANDIDATE: &str = "10.1000/renders-empty";
    const OUTCOME: &str = "no name";

    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-transition-unnameable"),
        pdf_with_no_identifier(),
    );
    // No author and no issued date: `[auth][year]` renders empty.
    let empty_record = Record {
        doi: Some(doi("10.1000/renders-empty")),
        ..Record::new(EntryType::Article)
    };
    let crossref =
        KeyedSource::new(SourceName::Crossref).answering("doi:10.1000/renders-empty", empty_record);
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("10.1000/renders-empty".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 2, "got {questions:?}");
    assert_eq!(
        questions[1].choices, questions[0].choices,
        "got {questions:?}"
    );
    assert_eq!(questions[1].path, questions[0].path, "got {questions:?}");
    // The candidate's outcome is reported in the re-put question's
    // description, which is the only channel left for it: design D5
    // keeps an abandoned candidate out of the event stream entirely.
    // Asserting only that the description changed would pass on any
    // change at all, so the identifier tried and the outcome's own
    // label both have to be there.
    let reported = questions[1].description.join("\n");
    assert!(
        reported.contains(CANDIDATE) && reported.contains(OUTCOME),
        "the re-put question must report {CANDIDATE} and {OUTCOME}: got {reported}"
    );
    assert!(
        !questions[0].description.join("\n").contains(CANDIDATE),
        "and must not have reported it before it was tried: got {questions:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path: p,
                reason: SkipReason::NoIdentifier,
                ..
            } if *p == path
        )),
        "got {events:?}"
    );
}

/// A supplied identifier that resolves into a record whose name is the
/// one the file already carries reports that outcome and puts the
/// file's own menu again, unchanged.
#[test]
fn a_supplied_identifier_resolving_into_the_files_own_current_name_reports_and_reasks() {
    const CANDIDATE: &str = "10.1000/renders-current-name";
    const OUTCOME: &str = "same name";

    let path = PathBuf::from("/lib/Smith2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d1-transition-already-named"),
        pdf_with_no_identifier(),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/renders-current-name",
        record_by("Smith", 2024, "10.1000/renders-current-name"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("10.1000/renders-current-name".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 2, "got {questions:?}");
    assert_eq!(
        questions[1].choices, questions[0].choices,
        "got {questions:?}"
    );
    assert_eq!(questions[1].path, questions[0].path, "got {questions:?}");
    // The candidate's outcome is reported in the re-put question's
    // description, which is the only channel left for it: design D5
    // keeps an abandoned candidate out of the event stream entirely.
    // Asserting only that the description changed would pass on any
    // change at all, so the identifier tried and the outcome's own
    // label both have to be there.
    let reported = questions[1].description.join("\n");
    assert!(
        reported.contains(CANDIDATE) && reported.contains(OUTCOME),
        "the re-put question must report {CANDIDATE} and {OUTCOME}: got {reported}"
    );
    assert!(
        !questions[0].description.join("\n").contains(CANDIDATE),
        "and must not have reported it before it was tried: got {questions:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path: p,
                reason: SkipReason::NoIdentifier,
                ..
            } if *p == path
        )),
        "got {events:?}"
    );
}

// ---------------------------------------------------------------------
// task 4.2: the remaining abandonment cases — quitting after a supply,
// and declining an override
// ---------------------------------------------------------------------

/// Quitting after a supplied identifier abandons it exactly as skipping
/// does: nothing is written, and what the file's hash already held in
/// the content index is untouched.
#[test]
fn quitting_after_a_supplied_identifier_leaves_the_content_index_as_it_was() {
    let hash = hash_for("d7-quit-not-written");
    // The documents would fail loudly if opened: an already-indexed file
    // is answered from the index and never touches the file, which is
    // what must hold while the candidate is abandoned.
    let documents = FakeDocuments::new().with_open_error(
        "/lib/paper.pdf",
        hash.clone(),
        ExtractionError::Unreadable {
            message: "must never be opened".to_string(),
        },
    );
    let fixture = SupplyFixture::new(documents);
    let already_held = record_by("Roe", 2019, "10.1000/d7-quit-already-held");
    fixture.index.put(&hash, &already_held);
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d7-quit-candidate",
        record_by("Doe", 2023, "10.1000/d7-quit-candidate"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let effective = effective_with_default_template("[auth][year]");
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Quit])
        .with_texts(vec![Some("10.1000/d7-quit-candidate".to_string())]);

    events_for(
        &Command::rename(vec![fixture.path.clone()], false),
        &Configs::uniform(effective),
        &fixture.adapters(&sources),
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert_eq!(
        fixture.index.get(&hash),
        Some(already_held),
        "the record the file already carried must be left exactly as it was"
    );
}

// ---------------------------------------------------------------------
// task 4.2a: an abandoned candidate is never cited
// ---------------------------------------------------------------------

/// design D5: "This is a rule the driver has to hold deliberately,
/// because the batch path cites every file it resolves, including one
/// whose move was declined." A record the operator supplied and then
/// skipped produces no sidecar and no master-bibliography entry.
#[test]
fn an_abandoned_supplied_candidate_is_never_cited() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents =
        FakeDocuments::new().with_file(&path, hash_for("cite-abandoned"), pdf_with_no_identifier());
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/cite-abandoned-candidate",
        record_by("Doe", 2023, "10.1000/cite-abandoned-candidate"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("10.1000/cite-abandoned-candidate".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    // Proves the scenario actually reached the supply loop, rather than
    // this test passing vacuously because today's driver skips a
    // no-identifier file before ever asking anything.
    assert_eq!(
        asker.texts_asked().len(),
        1,
        "the operator must actually have been asked for an identifier: got {:?}",
        asker.texts_asked()
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Sidecar { .. } | Event::BibEntry { .. })),
        "an abandoned candidate must produce no citation: got {events:?}"
    );
    assert!(
        bib_files.writes().is_empty(),
        "got {:?}",
        bib_files.writes()
    );
}

/// The other half of the same rule: a file whose own resolution stands
/// — even after a failed supply along the way — is cited from that
/// record exactly as a batch run cites it.
#[test]
fn a_files_own_resolution_is_still_cited_after_a_failed_supply() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("cite-own-record"),
        pdf_with_embedded_doi("10.1000/cite-own-record"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/cite-own-record",
        record_by("Smith", 2024, "10.1000/cite-own-record"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.9999/does-not-exist".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let renamed_to = PathBuf::from("/lib/Smith2024.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Sidecar { path: p, .. } if *p == renamed_to
        )),
        "got {events:?}"
    );
    assert!(
        bib_files.writes().iter().any(|(written_path, content)| {
            written_path == &sidecar_path(&renamed_to) && content.contains("Smith2024")
        }),
        "the sidecar must cite the file's own record: got {:?}",
        bib_files.writes()
    );
}

// ---------------------------------------------------------------------
// task 4.2b: a content-index write that fails leaves the rename
// standing
// ---------------------------------------------------------------------

/// A [`Cache`] whose every write fails, modelled on [`MemoryCache`] but
/// never keeping what it is given.
struct WriteFailingCache;

impl borax_sources::cache::Cache for WriteFailingCache {
    fn get(&self, _key: &str) -> Option<Record> {
        None
    }

    fn put(&self, _key: &str, _record: &Record) -> CacheWrite {
        CacheWrite::Failed {
            message: "write-failing cache".to_string(),
        }
    }
}

#[test]
fn a_content_index_write_that_fails_leaves_the_rename_standing_and_asks_again_next_time() {
    let path = PathBuf::from("/lib/paper.pdf");
    let hash = hash_for("d7-write-fails");
    let documents = FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_identifier());
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/d7-write-fails",
        record_by("Smith", 2024, "10.1000/d7-write-fails"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(WriteFailingCache);
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/d7-write-fails".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: p, .. } if *p == path
        )),
        "the rename must stand even though the index write failed: got {events:?}"
    );
    assert_eq!(
        index.get(&hash),
        None,
        "the write is not kept — this is the failure being modelled"
    );

    // The next run over the same file, with the same (still empty)
    // index, must ask about it again rather than being answered from
    // it.
    let mut asker_again = ScriptedAsker::new(vec![Answer::Skip]);
    let effective_again = effective_with_default_template("[auth][year]");
    events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective_again),
        &adapters,
        &mut Session::interactive(&mut asker_again),
    )
    .unwrap();

    assert_eq!(
        asker_again.questions_asked().len(),
        1,
        "an unremembered file must be asked about again: got {:?}",
        asker_again.questions_asked()
    );
}

/// The strongest form of the same rule, and the one D1's second table
/// states outright: a candidate that *resolved* but led nowhere — here
/// into a taken target — leaves the file's own record untouched, so
/// renaming afterwards renames and cites from that record and not from
/// the candidate.
///
/// The weaker case, a supply that never resolved, is covered by
/// [`a_files_own_resolution_is_still_cited_after_a_failed_supply`].
/// This one is harder for a driver to get right: it held a second,
/// complete record in hand and has to discard it.
#[test]
fn a_resolved_candidate_that_led_nowhere_leaves_the_files_own_record_to_cite() {
    let path = PathBuf::from("/lib/paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("resolved-candidate-discarded"),
        pdf_with_embedded_doi("10.1000/the-files-own"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/the-files-own",
            record_by("Smith", 2024, "10.1000/the-files-own"),
        )
        .answering(
            "doi:10.1000/would-collide",
            record_by("Doe", 2023, "10.1000/would-collide"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    // The candidate's name is taken; the file's own name is free.
    let filesystem =
        FakeFilesystem::new().with_existing("/lib", [("Doe2023.pdf", Some("unrelated-hash"))]);
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.rename = Some(RenameLayer {
            collision: Some("skip".to_string()),
            batch: None,
            skip_named: None,
        });
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/would-collide".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert_eq!(
        asker.texts_asked().len(),
        1,
        "the operator must actually have been asked for an identifier: got {:?}",
        asker.texts_asked()
    );
    let renamed_to = PathBuf::from("/lib/Smith2024.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: p, target, .. } if *p == path && *target == renamed_to
        )),
        "the file's own record is what renames it: got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Sidecar { path: p, .. } if *p == renamed_to
        )),
        "and what it is cited from: got {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            Event::Sidecar { path: p, .. } if p == Path::new("/lib/Doe2023.pdf")
        )),
        "the discarded candidate must be cited nowhere: got {events:?}"
    );
    assert_eq!(
        index.get(&hash_for("resolved-candidate-discarded")),
        Some(record_by("Smith", 2024, "10.1000/the-files-own")),
        "the file keeps the record its own resolution wrote, not the candidate's"
    );
}

/// Quitting an interactive run in human output still prints the run's
/// summary.
///
/// A hold is open while each file's question is answered, and quitting
/// leaves the per-file loop without a fate for that file. Nothing
/// after it drains the hold, so before this was fixed every line the
/// run had left to write — the summary, and any bibliography result —
/// was buffered and silently dropped. The existing quit test runs in
/// JSON, where a hold is a no-op, so it passed throughout.
#[test]
fn quitting_a_human_interactive_run_still_prints_the_summary() {
    let first = PathBuf::from("/lib/first.pdf");
    let second = PathBuf::from("/lib/second.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_for("quit-summary-first"),
            pdf_with_embedded_doi("10.1000/quit-summary-first"),
        )
        .with_file(
            &second,
            hash_for("quit-summary-second"),
            pdf_with_embedded_doi("10.1000/quit-summary-second"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/quit-summary-first",
            record_by("Smith", 2024, "10.1000/quit-summary-first"),
        )
        .answering(
            "doi:10.1000/quit-summary-second",
            record_by("Doe", 2023, "10.1000/quit-summary-second"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let state = tempdir().unwrap();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: Some(state.path().to_path_buf()),
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Quit]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::rename(vec![first.clone(), second.clone()], false),
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("2 not reached"),
        "the summary must reach the terminal after a quit: {text:?}"
    );
    assert!(
        !text.contains("first.pdf: resolved"),
        "the file the run stopped at was given no fate, so it has no \
         verdict to report: {text:?}"
    );
}

/// A record the operator reached is checked against the library before
/// it is admitted, exactly as one the run resolved on its own is.
///
/// [`standing`](borax::pipeline::standing) runs the work check on a
/// record it resolved, but a supplied one never went through it, and
/// neither did a record the conflict check refused and the operator
/// then accepted.
///
/// What the check produces here is not the filing question but a
/// statement: supplying the identifier is itself the operator saying
/// what the file is, so the run says what that identifier collided
/// with and puts the move question again. The second answer carries
/// the move out, and the file is admitted as another artifact of the
/// work it named. Only a file whose own resolution landed on the work
/// is asked whether to file it.
#[test]
fn a_supplied_record_the_library_already_holds_says_so_and_asks_again() {
    let library = real_library();
    let root = library.path().to_path_buf();
    // The library already holds this work, at a different file.
    let sibling = seed_sibling_artifact(
        &root,
        "sibling-original.pdf",
        "Smith",
        2024,
        "10.1000/already-in-the-library",
        b"the copy already admitted",
    );
    let items = library::ItemStore::read(&root);
    let item = items
        .iter()
        .next()
        .expect("setup: the sibling must have minted an item")
        .id
        .clone();
    let item_file = items
        .file_of(&item)
        .expect("setup: the item's file must be found")
        .to_path_buf();

    let path = write_real_file(&root, "incoming.pdf", b"supplied work duplicate bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"supplied work duplicate bytes"),
        pdf_with_no_identifier(),
    );
    let mut held = record_by("Smith", 2024, "10.1000/already-in-the-library");
    held.doi = Doi::parse("10.1000/already-in-the-library").ok();
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering("doi:10.1000/already-in-the-library", held);
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    // Supply the identifier, accept the move it offers, and accept it
    // again once the run has said what the identifier collided with.
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename, Answer::Rename])
        .with_texts(vec![Some("10.1000/already-in-the-library".to_string())]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        3,
        "the move question must be put again after the collision is reported: got {questions:?}"
    );
    assert_eq!(
        questions[2].choices,
        vec![Answer::Rename, Answer::Supply, Answer::Skip, Answer::Quit],
        "the question put again is the ordinary one about the move, not the filing question: \
         got {:?}",
        questions[2].choices
    );
    let description = questions[2].description.join("\n");
    for named in [&sibling, &item_file] {
        let name = named.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            description.contains(&name),
            "the operator must be told what the identifier collided with ({name}): \
             got {description:?}"
        );
    }
    assert!(
        !events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                reason: SkipReason::Duplicate { .. },
                ..
            }
        )),
        "a supplied identifier is the operator saying what the file is, so it is not \
         reported a duplicate: got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Renamed { path: p, .. } if p == &path)),
        "the second answer must carry the move out: got {events:?}"
    );

    let records = library::ArtifactStore::read(&root);
    assert_eq!(
        records.len(),
        2,
        "the supplied file must be recorded beside the sibling"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "and against the item it named, with no second item minted"
    );
    assert!(
        records
            .iter()
            .all(|record| record.item.as_ref() == Some(&item)),
        "both records must name the one item: got {records:?}"
    );
}

// ---------------------------------------------------------------------
// Library fixtures — shared by the `status` and `validate` tests below
// ---------------------------------------------------------------------

const LIB_UUID_A: &str = "018f2b36-7f21-7abc-8def-0123456789ab";
const LIB_UUID_B: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef0";

fn lib_item_id(text: &str) -> ItemId {
    ItemId::parse(text).unwrap()
}

fn lib_artifact_id(text: &str) -> ArtifactId {
    ArtifactId::parse(text).unwrap()
}

/// A minimal but valid record, enough for an item to round-trip —
/// following the shape of the one in `tests/library.rs`.
fn lib_minimal_record(entry_type: EntryType) -> Record {
    let mut record = Record::new(entry_type);
    record.title = Some("A Title".to_string());
    record
}

fn lib_hash_entry(seed: &str, run: &str) -> HashEntry {
    HashEntry {
        hash: hash_for(seed),
        run: LibraryRunId::new(run),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        tool_version: "0.6.0-test".to_string(),
    }
}

/// Writes `item` under `root`'s item store, named for its own identity
/// so the fixture carries no unrelated `NameDisagrees` finding.
fn write_lib_item(root: &Path, key: &str, item: &Item) {
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(
        items.join(format!("{key}.{}.toml", item.id)),
        item.to_toml(),
    )
    .unwrap();
}

/// Writes `record` under `root`'s artifact-record store, named for its
/// own identity as an applying run would name it.
fn write_lib_artifact_record(root: &Path, record: &ArtifactRecord) {
    let dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{}.toml", record.id)), record.to_toml()).unwrap();
}

/// An artifact record naming an item the library does not hold — the
/// one library-level fixture the `validate` outcome tests below need,
/// built once here rather than copied at each call site.
fn write_dangling_artifact_record(root: &Path) {
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: Some(lib_item_id(LIB_UUID_B)),
        path: "orphaned-link.pdf".to_string(),
        size: 1,
        modified_millis: 0,
        history: vec![lib_hash_entry("dangling-item-link", "run-1")],
    };
    write_lib_artifact_record(root, &record);
}

/// Every file under `root`, with its bytes, including whatever
/// `.borax/` holds — following the shape of the one in
/// `tests/library.rs`, duplicated here since test binaries share no
/// support module.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.is_file() {
                out.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }

    let mut found = BTreeMap::new();
    walk(root, &mut found);
    found
}

/// A [`Documents`] fake that records how many times [`Documents::open`]
/// was called, so `status`'s "opens no document" guarantee (design D5)
/// can be asserted directly rather than only inferred from an error
/// that never surfaced.
struct CountingDocuments {
    opens: std::sync::atomic::AtomicUsize,
}

impl CountingDocuments {
    fn new() -> CountingDocuments {
        CountingDocuments {
            opens: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn open_count(&self) -> usize {
        self.opens.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Documents for CountingDocuments {
    fn hash(&self, _path: &Path) -> Result<ContentHash, ExtractionError> {
        Err(ExtractionError::Unreadable {
            message: "status must not need a hash either".to_string(),
        })
    }

    fn open(&self, _path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(ExtractionError::Unreadable {
            message: "status must not open an artifact".to_string(),
        })
    }
}

/// A [`Source`] that panics if it is ever asked anything, for tests
/// that must prove a source was never touched — following the shape of
/// the one in `tests/pipeline.rs`.
struct PanicSource {
    name: SourceName,
}

impl Source for PanicSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        panic!(
            "{} was asked to support an identifier in an offline run",
            self.name
        )
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Fetched, SourceError> {
        panic!("{} was asked to fetch in an offline run", self.name)
    }
}

// ---------------------------------------------------------------------
// events_for: Command::Status — task 4.1, the Hyperbole test
// ---------------------------------------------------------------------

/// The Hyperbole test (spec scenario "Two hundred files borax has never
/// seen"), scaled down to a dozen: the scenario's number is about
/// ceremony and not arithmetic. Every artifact is wired to fail loudly
/// if it is ever opened, so a report of the right counts is proof
/// `status` never tried, on no preceding command at all.
#[test]
fn status_over_a_marked_directory_of_new_pdfs_reports_every_count_unopened() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let mut documents = FakeDocuments::new();
    for i in 0..12 {
        let path = root.join(format!("paper-{i:02}.pdf"));
        fs::write(&path, b"").unwrap();
        documents = documents.with_open_error(
            &path,
            hash_for(&format!("hyperbole-{i}")),
            ExtractionError::Unreadable {
                message: "status must never open an artifact".to_string(),
            },
        );
    }
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let mut expected: Vec<Event> = (0..12)
        .map(|i| Event::LibraryCondition {
            path: format!("paper-{i:02}.pdf"),
            condition: Condition::Orphan,
        })
        .collect();
    expected.push(Event::LibraryStatus {
        root: root.clone(),
        artifacts: 12,
        items: 0,
        records: 0,
        orphans: 12,
        nested: Vec::new(),
        identifiable: None,
    });
    assert_eq!(events, expected, "got {events:?}");
}

/// The stronger form of the same guarantee: a recording fake shows
/// `Documents::open` was never called at all, rather than only that
/// every call would have failed.
#[test]
fn status_without_identify_never_calls_documents_open() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("paper.pdf"), b"").unwrap();
    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let _ = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(documents.open_count(), 0, "got {}", documents.open_count());
}

// ---------------------------------------------------------------------
// events_for: Command::Status --identify — task 4.2
// ---------------------------------------------------------------------

/// Scenario "What is identifiable is asked for": `--identify` reports
/// how many artifacts yield an identifier, and queries no service — the
/// source it is given panics if it is ever asked anything. Each
/// artifact is reported with what extraction found (updated assertion:
/// a `library-extraction` event precedes `library-status` for each of
/// the two artifacts, naming its result, with `identifiable` agreeing
/// with how many of them were `found`).
#[test]
fn status_identify_counts_artifacts_yielding_an_identifier_and_queries_no_source() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let identifiable = root.join("has-doi.pdf");
    let not_identifiable = root.join("no-doi.pdf");
    fs::write(&identifiable, b"").unwrap();
    fs::write(&not_identifiable, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(
            &identifiable,
            hash_for("identify-yes"),
            pdf_with_embedded_doi("10.1000/identify"),
        )
        .with_file(
            &not_identifiable,
            hash_for("identify-no"),
            pdf_with_no_identifier(),
        );
    let panicking = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&panicking];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "has-doi.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryCondition {
                path: "no-doi.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryExtraction {
                path: "has-doi.pdf".to_string(),
                extraction: Extraction::Found {
                    identifier: "doi:10.1000/identify".to_string(),
                    tier: "embedded-metadata".to_string(),
                },
            },
            Event::LibraryExtraction {
                path: "no-doi.pdf".to_string(),
                extraction: Extraction::TextWithoutIdentifier,
            },
            Event::LibraryStatus {
                root: root.clone(),
                artifacts: 2,
                items: 0,
                records: 0,
                orphans: 2,
                nested: Vec::new(),
                identifiable: Some(1),
            },
        ],
        "got {events:?}"
    );
}

/// Scenario "Each artifact is reported with what extraction found":
/// `a.pdf` carries a DOI in its XMP packet, `sub/b.pdf` carries an
/// arXiv identifier on its first page and none in its metadata. Both
/// `library-extraction` events precede `library-status`, in survey
/// order, `sub/b.pdf` is named with a `/` separator, and `identifiable`
/// is `Some(2)`.
#[test]
fn status_identify_reports_each_artifact_in_survey_order_before_the_totals() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let a = root.join("a.pdf");
    fs::create_dir_all(root.join("sub")).unwrap();
    let b = root.join("sub").join("b.pdf");
    fs::write(&a, b"").unwrap();
    fs::write(&b, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash_for("survey-a"),
            pdf_with_embedded_doi("10.1000/survey-a"),
        )
        .with_file(&b, hash_for("survey-b"), pdf_with_text_arxiv("2401.00001"));
    let panicking = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&panicking];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "a.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryCondition {
                path: "sub/b.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryExtraction {
                path: "a.pdf".to_string(),
                extraction: Extraction::Found {
                    identifier: "doi:10.1000/survey-a".to_string(),
                    tier: "embedded-metadata".to_string(),
                },
            },
            Event::LibraryExtraction {
                path: "sub/b.pdf".to_string(),
                extraction: Extraction::Found {
                    identifier: "arXiv:2401.00001".to_string(),
                    tier: "text-layer".to_string(),
                },
            },
            Event::LibraryStatus {
                root: root.clone(),
                artifacts: 2,
                items: 0,
                records: 0,
                orphans: 2,
                nested: Vec::new(),
                identifiable: Some(2),
            },
        ],
        "got {events:?}"
    );
}

/// Scenario "A blank page with an embedded title has no text layer" and
/// "Readable text without an identifier is told apart": the two
/// controlled cases are told apart from each other, and neither counts
/// as identifiable.
#[test]
fn status_identify_tells_the_two_controlled_cases_apart() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let blank = root.join("blank.pdf");
    let prose = root.join("prose.pdf");
    fs::write(&blank, b"").unwrap();
    fs::write(&prose, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(&blank, hash_for("controlled-blank"), pdf_blank_with_title())
        .with_file(&prose, hash_for("controlled-prose"), pdf_prose_with_title());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "blank.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryCondition {
                path: "prose.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryExtraction {
                path: "blank.pdf".to_string(),
                extraction: Extraction::NoTextLayer,
            },
            Event::LibraryExtraction {
                path: "prose.pdf".to_string(),
                extraction: Extraction::TextWithoutIdentifier,
            },
            Event::LibraryStatus {
                root: root.clone(),
                artifacts: 2,
                items: 0,
                records: 0,
                orphans: 2,
                nested: Vec::new(),
                identifiable: Some(0),
            },
        ],
        "got {events:?}"
    );
}

/// Adding a third artifact whose title itself holds a DOI to the two
/// controlled cases gives it `found` with `embedded-metadata`, and
/// `identifiable` becomes `Some(1)`.
#[test]
fn status_identify_counts_a_title_holding_a_doi_as_identifiable() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let blank = root.join("blank.pdf");
    let prose = root.join("prose.pdf");
    let doi_title = root.join("doi-title.pdf");
    fs::write(&blank, b"").unwrap();
    fs::write(&prose, b"").unwrap();
    fs::write(&doi_title, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(&blank, hash_for("title-blank"), pdf_blank_with_title())
        .with_file(&prose, hash_for("title-prose"), pdf_prose_with_title())
        .with_file(
            &doi_title,
            hash_for("title-doi"),
            pdf_blank_with_doi_title("10.1234/example"),
        );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let doi_event = events
        .iter()
        .find(|event| matches!(event, Event::LibraryExtraction { path, .. } if path == "doi-title.pdf"))
        .unwrap_or_else(|| panic!("no library-extraction event for doi-title.pdf: {events:?}"));
    assert_eq!(
        doi_event,
        &Event::LibraryExtraction {
            path: "doi-title.pdf".to_string(),
            extraction: Extraction::Found {
                identifier: "doi:10.1234/example".to_string(),
                tier: "embedded-metadata".to_string(),
            },
        }
    );
    assert_eq!(
        events.last(),
        Some(&Event::LibraryStatus {
            root: root.clone(),
            artifacts: 3,
            items: 0,
            records: 0,
            orphans: 3,
            nested: Vec::new(),
            identifiable: Some(1),
        }),
        "got {events:?}"
    );
}

/// Scenario "Encrypted and unreadable artifacts are told apart":
/// neither is reported as a file without an identifier.
#[test]
fn status_identify_tells_encrypted_and_unreadable_apart() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let encrypted = root.join("encrypted.pdf");
    let unreadable = root.join("unreadable.pdf");
    fs::write(&encrypted, b"").unwrap();
    fs::write(&unreadable, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_open_error(
            &encrypted,
            hash_for("encrypted"),
            ExtractionError::Encrypted,
        )
        .with_open_error(
            &unreadable,
            hash_for("unreadable"),
            ExtractionError::Unreadable {
                message: "truncated stream".to_string(),
            },
        );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "encrypted.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryCondition {
                path: "unreadable.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryExtraction {
                path: "encrypted.pdf".to_string(),
                extraction: Extraction::Encrypted,
            },
            Event::LibraryExtraction {
                path: "unreadable.pdf".to_string(),
                extraction: Extraction::Unreadable {
                    message: "truncated stream".to_string(),
                },
            },
            Event::LibraryStatus {
                root: root.clone(),
                artifacts: 2,
                items: 0,
                records: 0,
                orphans: 2,
                nested: Vec::new(),
                identifiable: Some(0),
            },
        ],
        "got {events:?}"
    );
}

/// design D5: `identifiable` equals the number of `library-extraction`
/// events whose result `is_found()`, counted from the returned events
/// themselves rather than asserted to equal some other number — so the
/// totals and the per-file results cannot disagree by construction.
#[test]
fn status_identify_identifiable_agrees_with_the_found_extraction_events() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let found = root.join("found.pdf");
    let blank = root.join("blank.pdf");
    let prose = root.join("prose.pdf");
    fs::write(&found, b"").unwrap();
    fs::write(&blank, b"").unwrap();
    fs::write(&prose, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(
            &found,
            hash_for("agree-found"),
            pdf_with_embedded_doi("10.1000/agree"),
        )
        .with_file(&blank, hash_for("agree-blank"), pdf_blank_with_title())
        .with_file(&prose, hash_for("agree-prose"), pdf_prose_with_title());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let found_count = events
        .iter()
        .filter(|event| match event {
            Event::LibraryExtraction { extraction, .. } => extraction.is_found(),
            _ => false,
        })
        .count();
    let identifiable = events.iter().find_map(|event| match event {
        Event::LibraryStatus { identifiable, .. } => Some(*identifiable),
        _ => None,
    });

    assert_eq!(identifiable, Some(Some(found_count)), "got {events:?}");
    assert_eq!(found_count, 1, "got {events:?}");
}

/// Scenario "What is identifiable is asked for" / design D5: plain
/// `status` over the same library emits no `library-extraction` event
/// at all.
#[test]
fn status_without_identify_emits_no_library_extraction_event() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let path = root.join("has-doi.pdf");
    fs::write(&path, b"").unwrap();
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("no-identify-flag"),
        pdf_with_embedded_doi("10.1000/no-flag"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::LibraryExtraction { .. })),
        "got {events:?}"
    );
}

/// An unmarked directory (`collection_root: None`) names paths relative
/// to the directory given, the same as a marked one.
#[test]
fn status_identify_over_an_unmarked_directory_names_paths_relative_to_it() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    // Deliberately no `.borax.toml`: nobody has marked this directory.
    let path = root.join("has-doi.pdf");
    fs::write(&path, b"").unwrap();
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("unmarked"),
        pdf_with_embedded_doi("10.1000/unmarked"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events.first(),
        Some(&Event::LibraryCondition {
            path: "has-doi.pdf".to_string(),
            condition: Condition::Orphan,
        }),
        "got {events:?}"
    );
    let extraction = events
        .iter()
        .find(|event| matches!(event, Event::LibraryExtraction { .. }))
        .unwrap_or_else(|| panic!("no library-extraction event: {events:?}"));
    assert_eq!(
        extraction,
        &Event::LibraryExtraction {
            path: "has-doi.pdf".to_string(),
            extraction: Extraction::Found {
                identifier: "doi:10.1000/unmarked".to_string(),
                tier: "embedded-metadata".to_string(),
            },
        },
        "got {events:?}"
    );
}

// ---------------------------------------------------------------------
// events_for: Command::Status --identify — the selection boundary
// (task 3.2)
// ---------------------------------------------------------------------

/// A [`Documents`] fake that records every path passed to `open` and
/// `hash`, so a test can prove the selection `status --identify`
/// inspects is exactly `survey.artifacts` — no sidecar, item, `.borax/`
/// file, nested library or symlink target is ever touched.
struct RecordingDocuments {
    inner: FakeDocuments,
    opened: Mutex<Vec<PathBuf>>,
    hashed: Mutex<Vec<PathBuf>>,
}

impl RecordingDocuments {
    fn new(inner: FakeDocuments) -> RecordingDocuments {
        RecordingDocuments {
            inner,
            opened: Mutex::new(Vec::new()),
            hashed: Mutex::new(Vec::new()),
        }
    }

    fn opened(&self) -> Vec<PathBuf> {
        self.opened.lock().unwrap().clone()
    }

    fn hashed(&self) -> Vec<PathBuf> {
        self.hashed.lock().unwrap().clone()
    }
}

impl Documents for RecordingDocuments {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.hashed.lock().unwrap().push(path.to_path_buf());
        self.inner.hash(path)
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        self.opened.lock().unwrap().push(path.to_path_buf());
        self.inner.open(path)
    }
}

/// Scenario "Only the counted artifacts are inspected": `paper.pdf`'s
/// sidecar, a PDF under `items/`, a PDF under `.borax/`, a PDF in a
/// nested library and, on Unix, a symlink to a PDF outside the tree are
/// all left out of the selection `status --identify` inspects.
#[test]
fn status_identify_opens_only_the_surveyed_artifact() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let paper = root.join("paper.pdf");
    fs::write(&paper, b"").unwrap();
    fs::write(root.join("paper.pdf.bib"), b"").unwrap();
    fs::create_dir_all(root.join("items")).unwrap();
    fs::write(root.join("items").join("item.pdf"), b"").unwrap();
    fs::create_dir_all(root.join(".borax")).unwrap();
    fs::write(root.join(".borax").join("hidden.pdf"), b"").unwrap();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested").join(".borax.toml"), b"").unwrap();
    fs::write(root.join("nested").join("inner.pdf"), b"").unwrap();
    #[cfg(unix)]
    let elsewhere = tempdir().unwrap();
    #[cfg(unix)]
    {
        let outside = elsewhere.path().join("outside.pdf");
        fs::write(&outside, b"").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link.pdf")).unwrap();
    }

    let documents = RecordingDocuments::new(FakeDocuments::new().with_file(
        &paper,
        hash_for("selection-boundary"),
        pdf_with_embedded_doi("10.1000/selection"),
    ));
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let extractions: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::LibraryExtraction { .. }))
        .collect();
    assert_eq!(
        extractions,
        vec![&Event::LibraryExtraction {
            path: "paper.pdf".to_string(),
            extraction: Extraction::Found {
                identifier: "doi:10.1000/selection".to_string(),
                tier: "embedded-metadata".to_string(),
            },
        }],
        "got {events:?}"
    );
    assert_eq!(
        documents.opened(),
        vec![paper.clone()],
        "got {:?}",
        documents.opened()
    );
    assert!(
        documents.hashed().is_empty(),
        "extraction hashes nothing: got {:?}",
        documents.hashed()
    );
}

// ---------------------------------------------------------------------
// dispatch, human mode: status --identify (task 3.4)
// ---------------------------------------------------------------------

/// Scenario "Failed extraction is not a partial run": one D7 line per
/// artifact in path order, then the report line ending `0
/// identifiable` as the last line. No line contains `resolved,` or
/// `skipped`, and the outcome is `Outcome::Success`.
#[test]
fn status_identify_human_mode_lists_each_artifact_before_the_report_line() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let blank = root.join("a-blank.pdf");
    let prose = root.join("b-prose.pdf");
    let unreadable = root.join("c-unreadable.pdf");
    fs::write(&blank, b"").unwrap();
    fs::write(&prose, b"").unwrap();
    fs::write(&unreadable, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(&blank, hash_for("human-blank"), pdf_blank_with_title())
        .with_file(&prose, hash_for("human-prose"), pdf_prose_with_title())
        .with_open_error(
            &unreadable,
            hash_for("human-unreadable"),
            ExtractionError::Unreadable {
                message: "truncated stream".to_string(),
            },
        );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::status(Some(root.clone()), true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "a-blank.pdf: orphan; no artifact record names it",
            "b-prose.pdf: orphan; no artifact record names it",
            "c-unreadable.pdf: orphan; no artifact record names it",
            "a-blank.pdf: no identifier found; the pages read hold no text",
            "b-prose.pdf: no identifier found in its metadata or the pages read",
            "c-unreadable.pdf: unreadable (truncated stream)",
            &format!(
                "{}: 3 artifacts, 0 items, 0 records, 3 orphans, 0 identifiable",
                root.display()
            ),
        ],
        "got {lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("resolved,") || line.contains("skipped")),
        "got {lines:?}"
    );
}

/// The same run with `--json` ends on `run-finished` with all seven
/// counters zero: an extraction result is not a skip and not a finding.
#[test]
fn status_identify_json_run_finished_counts_nothing_for_extraction_results() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let blank = root.join("a-blank.pdf");
    let prose = root.join("b-prose.pdf");
    let unreadable = root.join("c-unreadable.pdf");
    fs::write(&blank, b"").unwrap();
    fs::write(&prose, b"").unwrap();
    fs::write(&unreadable, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(&blank, hash_for("json-blank"), pdf_blank_with_title())
        .with_file(&prose, hash_for("json-prose"), pdf_prose_with_title())
        .with_open_error(
            &unreadable,
            hash_for("json-unreadable"),
            ExtractionError::Unreadable {
                message: "truncated stream".to_string(),
            },
        );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::status(Some(root.clone()), true), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["event"], serde_json::Value::from("run-finished"));
    assert_eq!(
        last["counts"],
        serde_json::json!({
            "resolved": 0, "renamed": 0, "skipped": 0, "named": 0,
            "unmatched": 0, "unreached": 0, "findings": 0,
        }),
        "got {last}"
    );
}

/// An unreadable artifact whose message holds `\u{1b}` renders `\x1b`
/// on its line.
#[test]
fn status_identify_human_mode_escapes_an_escape_character_in_the_message() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let unreadable = root.join("paper.pdf");
    fs::write(&unreadable, b"").unwrap();
    let documents = FakeDocuments::new().with_open_error(
        &unreadable,
        hash_for("escape-message"),
        ExtractionError::Unreadable {
            message: "\u{1b}[2J".to_string(),
        },
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::status(Some(root.clone()), true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    assert!(
        text.lines()
            .any(|line| line == "paper.pdf: unreadable (\\x1b[2J)"),
        "got {text:?}"
    );
    assert!(!text.contains('\u{1b}'), "got {text:?}");
}

/// Scenario "A blank page with an embedded title has no text layer" /
/// "Only the counted artifacts are inspected": `status` run in human
/// mode over a library in which no artifact yields an identifier is a
/// success, not a partial run.
#[test]
fn status_identify_human_mode_over_all_failures_still_exits_success() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let path = root.join("blank.pdf");
    fs::write(&path, b"").unwrap();
    let documents =
        FakeDocuments::new().with_file(&path, hash_for("all-fail"), pdf_blank_with_title());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::status(Some(root.clone()), true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

/// Scenario "A library nobody marked": a directory with no
/// `.borax.toml` above it is reported on as given, and nothing is
/// written to it — `.borax/` included, and the tree byte-identical
/// throughout.
#[test]
fn status_over_an_unmarked_directory_is_reported_as_given_and_writes_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    // Deliberately no `.borax.toml`: nobody has marked this directory.
    fs::write(root.join("paper.pdf"), b"").unwrap();
    let before = snapshot(&root);

    let documents = FakeDocuments::new().with_open_error(
        root.join("paper.pdf"),
        hash_for("unmarked-status"),
        ExtractionError::Unreadable {
            message: "status must not open an artifact".to_string(),
        },
    );
    let panicking = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&panicking];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        // Nothing above this directory is marked, and nothing configured
        // a `library-root` either.
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "paper.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryStatus {
                root: root.clone(),
                artifacts: 1,
                items: 0,
                records: 0,
                orphans: 1,
                nested: Vec::new(),
                identifiable: None,
            },
        ],
        "got {events:?}"
    );
    assert!(
        !root.join(".borax").exists(),
        "status over an unmarked directory must write nothing under it"
    );
    assert_eq!(
        snapshot(&root),
        before,
        "the tree must be byte-identical after status"
    );
}

// ---------------------------------------------------------------------
// events_for: Command::Status — orphan conditions, task 3.2
// ---------------------------------------------------------------------

/// design D3/D4: plain `status` over a library holding one recorded
/// artifact and one orphan names the orphan alone, before the totals,
/// and opens no document — a `CountingDocuments` records no open.
#[test]
fn status_names_only_the_orphan_and_opens_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("kept.pdf"), b"").unwrap();
    fs::write(root.join("new.pdf"), b"").unwrap();
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "kept.pdf".to_string(),
        size: 0,
        modified_millis: 0,
        history: vec![lib_hash_entry("kept-bytes", "run-1")],
    };
    write_lib_artifact_record(&root, &record);

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "new.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryStatus {
                root: root.clone(),
                artifacts: 2,
                items: 0,
                records: 1,
                orphans: 1,
                nested: Vec::new(),
                identifiable: None,
            },
        ],
        "got {events:?}"
    );
    assert_eq!(documents.open_count(), 0, "got {}", documents.open_count());
}

/// design D3: a nested library's PDF is named `"sub/deeper/x.pdf"` with
/// `/` on every platform, and the orphan condition names it the same
/// way the orphan itself appears among the counted artifacts.
#[test]
fn status_names_an_orphan_two_directories_down_with_a_forward_slash() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    fs::write(root.join("sub/deeper/x.pdf"), b"").unwrap();

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events.first(),
        Some(&Event::LibraryCondition {
            path: "sub/deeper/x.pdf".to_string(),
            condition: Condition::Orphan,
        }),
        "got {events:?}"
    );
}

/// design D4: nothing beneath a nested `.borax.toml` directory, a
/// `.bib` sidecar, a PDF under `items/`, or (on Unix) a symlink to a
/// PDF outside the tree is ever named — the same selection boundary
/// `status --identify` already keeps for extraction.
#[test]
fn status_names_no_orphan_outside_the_selection() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("paper.pdf"), b"").unwrap();
    fs::write(root.join("paper.pdf.bib"), b"").unwrap();
    fs::create_dir_all(root.join("items")).unwrap();
    fs::write(root.join("items").join("item.pdf"), b"").unwrap();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested").join(".borax.toml"), b"").unwrap();
    fs::write(root.join("nested").join("inner.pdf"), b"").unwrap();
    #[cfg(unix)]
    let elsewhere = tempdir().unwrap();
    #[cfg(unix)]
    {
        let outside = elsewhere.path().join("outside.pdf");
        fs::write(&outside, b"").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link.pdf")).unwrap();
    }

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let conditions: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::LibraryCondition { .. }))
        .collect();
    assert_eq!(
        conditions,
        vec![&Event::LibraryCondition {
            path: "paper.pdf".to_string(),
            condition: Condition::Orphan,
        }],
        "got {events:?}"
    );
}

/// design D5: with `--identify`, every `library-condition` precedes
/// every `library-extraction`, and `library-status` is last.
#[test]
fn status_identify_names_every_orphan_before_every_extraction() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let a = root.join("a.pdf");
    let b = root.join("b.pdf");
    fs::write(&a, b"").unwrap();
    fs::write(&b, b"").unwrap();
    let documents = FakeDocuments::new()
        .with_file(&a, hash_for("order-a"), pdf_with_no_identifier())
        .with_file(&b, hash_for("order-b"), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let last_condition = events
        .iter()
        .rposition(|event| matches!(event, Event::LibraryCondition { .. }));
    let first_extraction = events
        .iter()
        .position(|event| matches!(event, Event::LibraryExtraction { .. }));
    match (last_condition, first_extraction) {
        (Some(last_condition), Some(first_extraction)) => {
            assert!(last_condition < first_extraction, "got {events:?}")
        }
        other => panic!("expected both kinds of event: {other:?} in {events:?}"),
    }
    assert!(
        matches!(events.last(), Some(Event::LibraryStatus { .. })),
        "got {events:?}"
    );
}

/// design D3/D4: `status`'s `orphans` total equals the number of
/// `library-condition` events of kind `orphan`, counted from the
/// returned events themselves (D5) — and no `library-condition` is
/// ever `missing` or `unlinked`, even over a library holding a record
/// whose path holds no file and an item no record links to.
#[test]
fn status_orphans_total_equals_its_own_orphan_condition_events() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("new.pdf"), b"").unwrap();
    let dangling = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "gone.pdf".to_string(),
        size: 0,
        modified_millis: 0,
        history: vec![lib_hash_entry("gone-bytes", "run-1")],
    };
    write_lib_artifact_record(&root, &dangling);
    let item = Item {
        id: lib_item_id(LIB_UUID_B),
        record: lib_minimal_record(EntryType::Article),
    };
    write_lib_item(&root, "unlinked-item", &item);

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let orphan_conditions = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::LibraryCondition {
                    condition: Condition::Orphan,
                    ..
                }
            )
        })
        .count();
    let other_conditions = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::LibraryCondition {
                    condition: Condition::Missing { .. } | Condition::Unlinked { .. },
                    ..
                }
            )
        })
        .count();
    let total = events.iter().find_map(|event| match event {
        Event::LibraryStatus { orphans, .. } => Some(*orphans),
        _ => None,
    });

    assert_eq!(other_conditions, 0, "got {events:?}");
    assert_eq!(total, Some(orphan_conditions), "got {events:?}");
    assert_eq!(orphan_conditions, 1, "got {events:?}");
}

// ---------------------------------------------------------------------
// dispatch, human mode: status — orphan conditions, task 3.3
// ---------------------------------------------------------------------

/// design D7: plain `status` over two orphans prints the two D7 orphan
/// lines in path order, then the report line as the last line. No line
/// contains `resolved,` or `skipped`, and the outcome is
/// `Outcome::Success`.
#[test]
fn status_human_mode_lists_each_orphan_before_the_report_line() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("a.pdf"), b"").unwrap();
    fs::write(root.join("b.pdf"), b"").unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::status(Some(root.clone()), false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "a.pdf: orphan; no artifact record names it",
            "b.pdf: orphan; no artifact record names it",
            &format!(
                "{}: 2 artifacts, 0 items, 0 records, 2 orphans",
                root.display()
            ),
        ],
        "got {lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("resolved,") || line.contains("skipped")),
        "got {lines:?}"
    );
}

/// design D7: an orphan whose name holds an escape character renders it
/// as `\x1b` on its human line, while the `--json` run carries the raw
/// path.
#[test]
fn status_human_mode_escapes_an_escape_character_in_an_orphan_s_name() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    let path = root.join("e\u{1b}[2J.pdf");
    fs::write(&path, b"").unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::status(Some(root.clone()), false), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    assert!(
        text.lines()
            .any(|line| line == "e\\x1b[2J.pdf: orphan; no artifact record names it"),
        "got {text:?}"
    );

    let mut json_out = Vec::new();
    let mut json_err = Vec::new();
    let mut json_streams = Streams {
        out: &mut json_out,
        err: &mut json_err,
    };
    dispatch(
        &cli(Command::status(Some(root.clone()), false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut json_streams,
    );
    let json_text = String::from_utf8(json_out).unwrap();
    let condition_line = json_text
        .lines()
        .find(|line| line.contains("\"event\":\"library-condition\""))
        .unwrap_or_else(|| panic!("no library-condition line: {json_text:?}"));
    let value: serde_json::Value = serde_json::from_str(condition_line).unwrap();
    assert_eq!(
        value["path"],
        serde_json::Value::from("e\u{1b}[2J.pdf"),
        "got {condition_line:?}"
    );
}

/// design D8: the `--json` run ends on `run-finished` with all seven
/// counters zero — an orphan condition is neither a skip nor a finding.
#[test]
fn status_json_run_finished_counts_nothing_for_orphan_conditions() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("a.pdf"), b"").unwrap();
    fs::write(root.join("b.pdf"), b"").unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::status(Some(root.clone()), false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["event"], serde_json::Value::from("run-finished"));
    assert_eq!(
        last["counts"],
        serde_json::json!({
            "resolved": 0, "renamed": 0, "skipped": 0, "named": 0,
            "unmatched": 0, "unreached": 0, "findings": 0,
        }),
        "got {last}"
    );
}

// ---------------------------------------------------------------------
// events_for / dispatch: Command::Validate — task 5.1/5.2 (exit codes)
// ---------------------------------------------------------------------

/// The finding, then the orphan and missing conditions, then the
/// totals ([`borax::library::validation_events`]'s contract, exercised
/// through the command). The fixture holds the dangling record (`id`
/// `LIB_UUID_A`, path `"orphaned-link.pdf"`, which holds no file) and
/// `orphan.pdf`, so the same run also carries one missing artifact — a
/// count and not a second finding — and one orphan.
#[test]
fn validate_emits_the_finding_then_the_orphan_and_missing_conditions_then_the_totals() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_dangling_artifact_record(&root);
    fs::write(root.join("orphan.pdf"), b"").unwrap();

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::validate(Some(root.clone())),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events.len(), 4, "got {events:?}");
    assert!(
        matches!(events[0], Event::LibraryFinding { .. }),
        "got {:?}",
        events[0]
    );
    assert_eq!(
        events[1],
        Event::LibraryCondition {
            path: "orphan.pdf".to_string(),
            condition: Condition::Orphan,
        },
        "got {:?}",
        events[1]
    );
    assert_eq!(
        events[2],
        Event::LibraryCondition {
            path: "orphaned-link.pdf".to_string(),
            condition: Condition::Missing {
                id: LIB_UUID_A.to_string(),
                record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{LIB_UUID_A}.toml"),
            },
        },
        "got {:?}",
        events[2]
    );
    assert_eq!(
        events[3],
        Event::LibraryValidated {
            root: root.clone(),
            findings: 1,
            orphans: 1,
            missing: 1,
            unlinked: 0,
        },
        "got {:?}",
        events[3]
    );
}

/// Exit codes: a `validate` reporting any finding ends in
/// `Outcome::Partial`.
#[test]
fn validate_with_a_finding_returns_partial_success() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_dangling_artifact_record(&root);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

/// Exit codes: a `validate` reporting no finding ends in
/// `Outcome::Success`, whatever the orphan and unlinked counts —
/// scenario "A clean library exits 0".
#[test]
fn validate_with_no_finding_returns_success_whatever_the_orphan_and_unlinked_counts() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join("orphan.pdf"), b"").unwrap();
    let unlinked = Item {
        id: lib_item_id(LIB_UUID_A),
        record: lib_minimal_record(EntryType::Article),
    };
    write_lib_item(&root, "unlinked", &unlinked);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

// ---------------------------------------------------------------------
// events_for / dispatch: Command::Validate — conditions, task 4.3
// ---------------------------------------------------------------------

/// `new.pdf`, which no record names (orphan); a record naming
/// `gone.pdf`, which holds no file (missing); and an item file no
/// record links (unlinked). No finding. Returns the item file's
/// library-relative, `/`-separated path.
fn write_three_condition_library(root: &Path) -> String {
    fs::write(root.join("new.pdf"), b"").unwrap();
    let missing_record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "gone.pdf".to_string(),
        size: 0,
        modified_millis: 0,
        history: vec![lib_hash_entry("gone-bytes", "run-1")],
    };
    write_lib_artifact_record(root, &missing_record);
    let item = Item {
        id: lib_item_id(LIB_UUID_B),
        record: lib_minimal_record(EntryType::Article),
    };
    write_lib_item(root, "milner1978", &item);
    format!("{ITEM_STORE}/milner1978.{LIB_UUID_B}.toml")
}

/// Scenario "Validation names each condition it counts", through the
/// command: the orphan, missing and unlinked conditions in that order,
/// then the totals, and `Outcome::Success` (scenario "A condition does
/// not fail a run").
#[test]
fn validate_names_all_three_conditions_in_kind_order_and_succeeds() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let item_path = write_three_condition_library(&root);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::validate(Some(root.clone())),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "new.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryCondition {
                path: "gone.pdf".to_string(),
                condition: Condition::Missing {
                    id: LIB_UUID_A.to_string(),
                    record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{LIB_UUID_A}.toml"),
                },
            },
            Event::LibraryCondition {
                path: item_path,
                condition: Condition::Unlinked {
                    id: LIB_UUID_B.to_string(),
                },
            },
            Event::LibraryValidated {
                root: root.clone(),
                findings: 0,
                orphans: 1,
                missing: 1,
                unlinked: 1,
            },
        ],
        "got {events:?}"
    );

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut Streams {
            out: &mut Vec::new(),
            err: &mut Vec::new(),
        },
    );
    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

/// Scenario "A missing artifact alone exits 0": a library holding only
/// the missing record exits `Outcome::Success`, and `run-finished`
/// counts zero findings.
#[test]
fn validate_over_only_a_missing_record_exits_success() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let missing_record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "gone.pdf".to_string(),
        size: 0,
        modified_millis: 0,
        history: vec![lib_hash_entry("gone-bytes", "run-1")],
    };
    write_lib_artifact_record(&root, &missing_record);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["event"], serde_json::Value::from("run-finished"));
    assert_eq!(last["counts"]["findings"], serde_json::Value::from(0));
}

/// Scenario "Conditions are named beside the findings": a dangling link
/// whose file is present, an orphan and an unlinked item. The events
/// are the finding, the orphan condition, the unlinked condition, then
/// the totals; the outcome is `Outcome::Partial` because of the finding
/// alone.
#[test]
fn validate_names_conditions_beside_a_finding_and_is_partial() {
    const DANGLING_TARGET_UUID: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef9";

    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join("dangling-link.pdf"), b"present").unwrap();
    let dangling = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        // Named for an item the library does not hold, so the record
        // carries a `DanglingItem` finding — deliberately distinct from
        // `LIB_UUID_B`, which the unlinked item fixture below uses.
        item: Some(lib_item_id(DANGLING_TARGET_UUID)),
        path: "dangling-link.pdf".to_string(),
        size: 7,
        modified_millis: 0,
        history: vec![lib_hash_entry("present", "run-1")],
    };
    write_lib_artifact_record(&root, &dangling);
    fs::write(root.join("orphan.pdf"), b"").unwrap();
    let unlinked = Item {
        id: lib_item_id(LIB_UUID_B),
        record: lib_minimal_record(EntryType::Article),
    };
    write_lib_item(&root, "unlinked", &unlinked);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::validate(Some(root.clone())),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events.len(), 4, "got {events:?}");
    assert!(
        matches!(events[0], Event::LibraryFinding { .. }),
        "got {:?}",
        events[0]
    );
    assert_eq!(
        events[1],
        Event::LibraryCondition {
            path: "orphan.pdf".to_string(),
            condition: Condition::Orphan,
        },
        "got {:?}",
        events[1]
    );
    assert_eq!(
        events[2],
        Event::LibraryCondition {
            path: format!("{ITEM_STORE}/unlinked.{LIB_UUID_B}.toml"),
            condition: Condition::Unlinked {
                id: LIB_UUID_B.to_string(),
            },
        },
        "got {:?}",
        events[2]
    );
    assert_eq!(
        events[3],
        Event::LibraryValidated {
            root: root.clone(),
            findings: 1,
            orphans: 1,
            missing: 0,
            unlinked: 1,
        },
        "got {:?}",
        events[3]
    );

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut Streams {
            out: &mut Vec::new(),
            err: &mut Vec::new(),
        },
    );
    assert_eq!(
        outcome,
        Outcome::Partial,
        "got {outcome:?}: a condition does not fail a run, but the finding does"
    );
}

/// In human mode over the three-condition library: the three D7 lines
/// in kind order, then the existing totals line as the last line, with
/// no summary line after it.
#[test]
fn validate_human_mode_lists_all_three_conditions_before_the_totals_line() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_three_condition_library(&root);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::validate(Some(root.clone())), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "new.pdf: orphan; no artifact record names it",
            &format!(
                "gone.pdf: missing; artifact record {LIB_UUID_A} \
                 ({STATE_DIR}/{ARTIFACT_STORE}/{LIB_UUID_A}.toml) names this path \
                 and the library has no artifact here"
            ),
            &format!(
                "{ITEM_STORE}/milner1978.{LIB_UUID_B}.toml: unlinked; no artifact \
                 record links item {LIB_UUID_B}"
            ),
            &format!(
                "{}: 0 findings, 1 orphans, 1 missing, 1 unlinked",
                root.display()
            ),
        ],
        "got {lines:?}"
    );
}

/// Scenario "Two records of one identity are both named missing",
/// through the command: the `DuplicateIdentity` finding, then two
/// `missing` conditions that differ in `record` alone, then the totals
/// with `missing: 2`. `Outcome::Partial`, because of the finding.
#[test]
fn validate_names_two_missing_conditions_for_two_records_of_one_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let dir_path = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir_path).unwrap();
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "gone.pdf".to_string(),
        size: 0,
        modified_millis: 0,
        history: vec![lib_hash_entry("g1", "run-1")],
    };
    let first = format!("a-first.{LIB_UUID_A}.toml");
    fs::write(dir_path.join(&first), record.to_toml()).unwrap();
    let other = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "gone.pdf".to_string(),
        size: 0,
        modified_millis: 0,
        history: vec![lib_hash_entry("g2", "run-1")],
    };
    let second = format!("b-second.{LIB_UUID_A}.toml");
    fs::write(dir_path.join(&second), other.to_toml()).unwrap();

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::validate(Some(root.clone())),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events.len(), 4, "got {events:?}");
    assert!(
        matches!(events[0], Event::LibraryFinding { .. }),
        "got {:?}",
        events[0]
    );
    assert_eq!(
        events[1],
        Event::LibraryCondition {
            path: "gone.pdf".to_string(),
            condition: Condition::Missing {
                id: LIB_UUID_A.to_string(),
                record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{first}"),
            },
        },
        "got {:?}",
        events[1]
    );
    assert_eq!(
        events[2],
        Event::LibraryCondition {
            path: "gone.pdf".to_string(),
            condition: Condition::Missing {
                id: LIB_UUID_A.to_string(),
                record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{second}"),
            },
        },
        "got {:?}",
        events[2]
    );
    assert_eq!(
        events[3],
        Event::LibraryValidated {
            root: root.clone(),
            findings: 1,
            orphans: 0,
            missing: 2,
            unlinked: 0,
        },
        "got {:?}",
        events[3]
    );

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut Streams {
            out: &mut Vec::new(),
            err: &mut Vec::new(),
        },
    );
    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

// ---------------------------------------------------------------------
// events_for / dispatch: Command::Reconcile — group 6
// ---------------------------------------------------------------------

/// `borax reconcile` through the command line: one `LibraryRepair` event
/// per record repaired, then the totals
/// ([`borax::library::reconciliation_events`]'s contract, exercised
/// through the command).
#[test]
fn reconcile_emits_one_repair_event_then_the_totals() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"moved bytes").unwrap();
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "old/paper.pdf".to_string(),
        size: 1,
        modified_millis: 0,
        history: vec![lib_hash_entry("moved bytes", "run-0")],
    };
    write_lib_artifact_record(&root, &record);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::reconcile(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(events.len(), 2, "got {events:?}");
    assert_eq!(
        events[0],
        Event::LibraryRepair {
            id: LIB_UUID_A.to_string(),
            path: "new/paper.pdf".to_string(),
            repair: Repair::Repaired {
                from: "old/paper.pdf".to_string()
            },
        },
        "got {:?}",
        events[0]
    );
    assert_eq!(
        events[1],
        Event::LibraryReconciled {
            root: root.clone(),
            records: 1,
            confirmed: 0,
            repaired: 1,
            changed: 0,
            ambiguous: 0,
            missing: 0,
            hashed: 1,
        },
        "got {:?}",
        events[1]
    );
}

/// A clean library — every record's own path already agrees with its
/// file — emits only the totals: confirmed and nothing else.
#[test]
fn reconcile_over_a_clean_library_emits_only_the_totals() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("paper.pdf");
    fs::write(&path, b"steady bytes").unwrap();
    let metadata = fs::metadata(&path).unwrap();
    let modified_millis = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "paper.pdf".to_string(),
        size: metadata.len(),
        modified_millis,
        history: vec![lib_hash_entry("steady bytes", "run-0")],
    };
    write_lib_artifact_record(&root, &record);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::reconcile(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::LibraryReconciled {
            root: root.clone(),
            records: 1,
            confirmed: 1,
            repaired: 0,
            changed: 0,
            ambiguous: 0,
            missing: 0,
            hashed: 0,
        }],
        "a clean library must emit nothing besides the totals, got {events:?}"
    );
}

/// `--rehash` reaches the library: over a fixture whose bytes changed
/// while its recorded size and modification time did not, the plain
/// command reports nothing and `--rehash` reports the change.
#[test]
fn reconcile_rehash_flag_reaches_the_library() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("paper.pdf");
    let original: &[u8] = b"original content, unedited!";
    fs::write(&path, original).unwrap();
    let metadata = fs::metadata(&path).unwrap();
    let modified = metadata.modified().unwrap();
    let modified_millis = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "paper.pdf".to_string(),
        size: metadata.len(),
        modified_millis,
        history: vec![lib_hash_entry("original content, unedited!", "run-0")],
    };
    write_lib_artifact_record(&root, &record);
    // Edit the bytes to the same length, put the mtime back exactly.
    let mut edited = original.to_vec();
    edited[0] = b'X';
    assert_eq!(edited.len(), original.len(), "fixture must keep the length");
    fs::write(&path, &edited).unwrap();
    std::fs::File::open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let plain = events_for(
        &Command::reconcile(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        plain,
        vec![Event::LibraryReconciled {
            root: root.clone(),
            records: 1,
            confirmed: 1,
            repaired: 0,
            changed: 0,
            ambiguous: 0,
            missing: 0,
            hashed: 0,
        }],
        "without --rehash the fast path must hide the change, got {plain:?}"
    );

    let rehashed = events_for(
        &Command::reconcile(Some(root.clone()), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let new_hash = hash_bytes(&edited);
    assert_eq!(rehashed.len(), 2, "got {rehashed:?}");
    assert_eq!(
        rehashed[0],
        Event::LibraryRepair {
            id: LIB_UUID_A.to_string(),
            path: "paper.pdf".to_string(),
            repair: Repair::Changed {
                hash: new_hash.to_string()
            },
        },
        "got {:?}",
        rehashed[0]
    );
    assert_eq!(
        rehashed[1],
        Event::LibraryReconciled {
            root: root.clone(),
            records: 1,
            confirmed: 0,
            repaired: 0,
            changed: 1,
            ambiguous: 0,
            missing: 0,
            hashed: 1,
        },
        "got {:?}",
        rehashed[1]
    );
}

/// Reconciliation is a routine library operation and not an error path:
/// a reconcile reporting an ambiguity or a missing artifact still
/// succeeds, unlike `validate`'s findings which end in `Outcome::Partial`.
#[test]
fn reconcile_reporting_ambiguity_or_a_missing_artifact_still_succeeds() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let record = ArtifactRecord {
        id: lib_artifact_id(LIB_UUID_A),
        item: None,
        path: "gone.pdf".to_string(),
        size: 1,
        modified_millis: 0,
        history: vec![lib_hash_entry("gone bytes", "run-0")],
    };
    write_lib_artifact_record(&root, &record);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::reconcile(Some(root.clone()), false), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(
        outcome,
        Outcome::Success,
        "a missing artifact is a routine report, not a failure: got {outcome:?}"
    );
}

// ---------------------------------------------------------------------
// dispatch: an applying rename over a library with a finding — task 5.3
// ---------------------------------------------------------------------

/// Scenario "Validation refuses nothing": an applying `rename` run made
/// over a library holding a finding proceeds rather than being refused.
///
/// Task 7, which teaches an applying run to write artifact records, is
/// not built yet, so this fixes only what is true today and will stay
/// true afterwards — the move happens and the pre-existing finding is
/// still there — and leaves the write half to the TODO below.
#[test]
fn an_applying_rename_over_a_library_with_a_finding_proceeds() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    // A pre-existing finding that nothing in this run can repair or
    // remove: a half-written artifact record.
    let store_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&store_dir).unwrap();
    fs::write(
        store_dir.join("half-written.toml"),
        b"this is not [ valid toml",
    )
    .unwrap();

    let path = root.join("original.pdf");
    let documents = library_with_resolvable(
        &path,
        hash_for("validate-refuses-nothing"),
        "10.1000/validate-refuses-nothing",
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/validate-refuses-nothing")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::rename(vec![path.clone()], true), true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_ne!(
        outcome,
        Outcome::Fatal,
        "a library holding a finding must not refuse the run: got {outcome:?}"
    );
    assert_eq!(
        filesystem.renames(),
        vec![(path.clone(), root.join("Smith2024.pdf"))],
        "the move must proceed despite the finding: got {:?}",
        filesystem.renames()
    );

    let after = library::validate(&root);
    assert!(
        after
            .findings
            .iter()
            .any(|(file, _)| file == &store_dir.join("half-written.toml")),
        "the pre-existing finding must still be reported afterwards: got {:?}",
        after.findings
    );

    // TODO(batch E): once task 7.x lands, this run should also write an
    // artifact record for `Smith2024.pdf` ("an applying run records what
    // it admits") — the write path does not exist yet, so there is
    // nothing to assert about it here.
}

// ---------------------------------------------------------------------
// group 7: the write path — real files, since the store is made of them
//
// Every test below turns the old ledger off (`ledger: None`), so its
// own — unrelated — duplicate detection can never interfere with what
// an admission to the *library* store does; `record` is the only
// setting any of these tests steers. None uses `FakeFilesystem`: the
// store is made of real files, and `admit`'s stat and `store_write`'s
// atomic replace both need real ones on disk, which is why this is the
// first `RealFilesystem` test in this file.
// ---------------------------------------------------------------------

const G7_UUID_A: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef5";
const G7_UUID_B: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef6";
const G7_UUID_C: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef7";

/// A fresh tempdir marked as a library root (`.borax.toml` at its top).
fn real_library() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".borax.toml"), b"").unwrap();
    dir
}

/// Write `bytes` at `relative` under `root`, creating whatever
/// directories it needs, and hand back the full path.
fn write_real_file(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

/// `path`'s size and modification time, in the form an artifact record
/// stores them.
fn real_stat(path: &Path) -> (u64, i64) {
    let metadata = fs::metadata(path).unwrap();
    let millis = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    (metadata.len(), millis)
}

/// Overwrite the file at `path` with `bytes` — which must be the same
/// length as what is there now — then restore its original
/// modification time, so the fast path (`size`, `mtime`) cannot see the
/// edit: only comparing hashes can, which is exactly the gap
/// `--no-record`'s pre-move check and `borax reconcile --rehash` exist
/// for (tasks 7.3a and 7.3b).
fn edit_in_place_same_length(path: &Path, bytes: &[u8]) {
    let before = fs::metadata(path).unwrap();
    assert_eq!(
        before.len(),
        bytes.len() as u64,
        "the replacement must be the same length, or the fast path alone would see the edit"
    );
    let original_modified = before.modified().unwrap();
    fs::write(path, bytes).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(original_modified).unwrap();
}

/// A [`HashEntry`] carrying exactly `hash`, for a fixture that needs a
/// specific content hash rather than one derived from a seed string.
fn hash_entry_for(hash: ContentHash, run: &str) -> HashEntry {
    HashEntry {
        hash,
        run: LibraryRunId::new(run),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        tool_version: "0.6.0-test".to_string(),
    }
}

/// task 7.1: an applying run over a library writes an artifact record
/// for a file it moved — the new library-relative path, the file's
/// hash as the history's first entry, its size and modification time,
/// and an item link — minting the item since the library holds none.
#[test]
fn an_applying_run_writes_an_artifact_record_for_the_file_it_moved() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"task-7.1 bytes");
    let hash = hash_bytes(b"task-7.1 bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.1"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/task-7.1")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("Smith2024.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "the file must actually move: got {events:?}"
    );

    let items = library::ItemStore::read(&root);
    assert_eq!(items.len(), 1, "exactly one item must be minted");
    let records = library::ArtifactStore::read(&root);
    assert_eq!(
        records.len(),
        1,
        "exactly one artifact record must be written"
    );
    let record = records.iter().next().unwrap();
    assert_eq!(record.path, "Smith2024.pdf");
    assert_eq!(record.history.last().map(|entry| &entry.hash), Some(&hash));
    assert_eq!(record.item, Some(items.iter().next().unwrap().id.clone()));
    let (size, modified_millis) = real_stat(&target);
    assert_eq!(record.size, size);
    assert_eq!(record.modified_millis, modified_millis);
}

/// task 7.2a: an artifact that already has a record keeps its item
/// link across three ordinary applying re-runs of a file resolving to
/// no identifier — one repetition would not show the accumulation
/// design D4 rules out, and no item may be left with nothing pointing
/// at it.
#[test]
fn a_recorded_files_item_link_survives_three_applying_reruns() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let original = write_real_file(&root, "steady-original.pdf", b"task-7.2a bytes");
    let target = root.join("Steady2020.pdf");
    let hash = hash_bytes(b"task-7.2a bytes");
    let documents = FakeDocuments::new()
        .with_file(
            &original,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/steady"),
        )
        .with_file(
            &target,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/steady"),
        );
    let record = Record {
        doi: None,
        ..record_by("Steady", 2020, "10.1000/unused")
    };
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    events_for(
        &Command::rename(vec![original.clone()], true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let item_after_first = library::ItemStore::read(&root)
        .iter()
        .next()
        .expect("the first run must mint an item")
        .id
        .clone();

    for run in 2..=3 {
        events_for(
            &Command::rename(vec![target.clone()], true),
            &Configs::uniform(effective.clone()),
            &adapters,
            &mut Session::batch(),
        )
        .unwrap();

        assert_eq!(
            library::ItemStore::read(&root).len(),
            1,
            "run {run}: no item may accumulate behind a file resolving to no identifier"
        );
        let records = library::ArtifactStore::read(&root);
        assert_eq!(
            records.len(),
            1,
            "run {run}: no second artifact record may be minted for the same file"
        );
        assert_eq!(
            records.iter().next().unwrap().item,
            Some(item_after_first.clone()),
            "run {run}: the artifact's item link must not move"
        );
    }
}

/// task 7.2b: an operator's re-identification, and only an operator's,
/// re-links a recorded artifact to the item for the record they
/// settled on. A first, ordinary run names and records a file under
/// the wrong record; a second, interactive run with `--no-skip-named`
/// supplies the right identifier, renames, and re-links the artifact —
/// keeping its own identity, reporting both items, and leaving the item
/// it came from an ordinary state `borax validate` does not report.
#[test]
fn an_operators_reidentification_relinks_the_artifact_to_the_supplied_records_item() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let original = write_real_file(&root, "original.pdf", b"task-7.2b bytes");
    let hash = hash_bytes(b"task-7.2b bytes");
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.rename = Some(RenameLayer {
            collision: None,
            batch: None,
            skip_named: Some(false),
        });
    });

    let documents_1 = FakeDocuments::new().with_file(
        &original,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.2b-wrong"),
    );
    let crossref_1 = fake_source(
        SourceName::Crossref,
        Ok(record_by("Wrong", 2020, "10.1000/task-7.2b-wrong")),
    );
    let sources_1: Vec<&dyn Source> = vec![&crossref_1];
    let index_1 = ContentIndex::new(MemoryCache::new());
    let bib_files_1 = FakeBibFiles::new();
    let adapters_1 = Adapters {
        documents: &documents_1,
        sources: &sources_1,
        index: &index_1,
        filesystem: &RealFilesystem,
        bib_files: &bib_files_1,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    events_for(
        &Command::rename(vec![original.clone()], true),
        &Configs::uniform(effective.clone()),
        &adapters_1,
        &mut Session::batch(),
    )
    .unwrap();

    let wrong_target = root.join("Wrong2020.pdf");
    let wrong_item = library::ItemStore::read(&root)
        .iter()
        .next()
        .expect("the first run must mint the wrong item")
        .id
        .clone();
    let artifact_id_before = library::ArtifactStore::read(&root)
        .iter()
        .next()
        .expect("the first run must mint an artifact record")
        .id
        .clone();

    let documents_2 = FakeDocuments::new().with_file(
        &wrong_target,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.2b-wrong"),
    );
    let crossref_2 = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/task-7.2b-wrong",
            record_by("Wrong", 2020, "10.1000/task-7.2b-wrong"),
        )
        .answering(
            "doi:10.1000/task-7.2b-right",
            record_by("Right", 2021, "10.1000/task-7.2b-right"),
        );
    let sources_2: Vec<&dyn Source> = vec![&crossref_2];
    let index_2 = ContentIndex::new(MemoryCache::new());
    let bib_files_2 = FakeBibFiles::new();
    let adapters_2 = Adapters {
        documents: &documents_2,
        sources: &sources_2,
        index: &index_2,
        filesystem: &RealFilesystem,
        bib_files: &bib_files_2,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Rename])
        .with_texts(vec![Some("10.1000/task-7.2b-right".to_string())]);

    let events = events_for(
        &Command::rename(vec![wrong_target.clone()], false),
        &Configs::uniform(effective),
        &adapters_2,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let right_target = root.join("Right2021.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. }
                if from == &wrong_target && to == &right_target
        )),
        "got {events:?}"
    );

    let records_after = library::ArtifactStore::read(&root);
    assert_eq!(
        records_after.len(),
        1,
        "the artifact record must not duplicate: got {:?}",
        records_after.iter().collect::<Vec<_>>()
    );
    let record_after = records_after
        .by_id(&artifact_id_before)
        .expect("the record must keep its own artifact identity across the relink");

    let items_after = library::ItemStore::read(&root);
    assert_eq!(items_after.len(), 2, "both items must still exist");
    let right_item = items_after
        .iter()
        .find(|item| item.id != wrong_item)
        .expect("a second item for the right record must now exist")
        .id
        .clone();
    assert_eq!(record_after.item, Some(right_item));

    let validation = library::validate(&root);
    assert!(
        validation.findings.is_empty(),
        "an item nothing links to is an ordinary state, not a finding: got {:?}",
        validation.findings
    );
}

/// task 7.2c: an item with no artifact takes the incoming file as its
/// first artifact — not a duplicate, no second item minted.
#[test]
fn an_item_with_no_artifact_takes_the_incoming_file_as_its_first_artifact() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let existing_item = Item {
        id: lib_item_id(G7_UUID_A),
        record: record_by("Cited", 2019, "10.1000/task-7.2c-cited"),
    };
    write_lib_item(&root, "cited2019", &existing_item);

    let path = write_real_file(&root, "original.pdf", b"task-7.2c bytes");
    let hash = hash_bytes(b"task-7.2c bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.2c-cited"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Cited", 2019, "10.1000/task-7.2c-cited")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Skipped { .. })),
        "the file that finally supplies an artifactless item must never be treated as a \
         duplicate: got {events:?}"
    );

    let items = library::ItemStore::read(&root);
    assert_eq!(
        items.len(),
        1,
        "no second item may be minted for a work already cited"
    );
    let records = library::ArtifactStore::read(&root);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records.iter().next().unwrap().item,
        Some(existing_item.id.clone())
    );
}

/// task 7.2c: an item whose every artifact record names a path holding
/// no file is admitted the same way — the absent artifact's own record
/// is left exactly as it is, and no second item is minted.
#[test]
fn an_item_whose_only_artifact_is_gone_still_takes_a_new_artifact() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let record = record_by("Gone", 2018, "10.1000/task-7.2c-gone");
    let item = Item {
        id: lib_item_id(G7_UUID_B),
        record: record.clone(),
    };
    write_lib_item(&root, "gone2018", &item);
    let absent = ArtifactRecord {
        id: lib_artifact_id(G7_UUID_C),
        item: Some(item.id.clone()),
        path: "vanished.pdf".to_string(),
        size: 1,
        modified_millis: 0,
        history: vec![lib_hash_entry("vanished bytes", "run-0")],
    };
    write_lib_artifact_record(&root, &absent);
    let store_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    let absent_file = store_dir.join(format!("{}.toml", absent.id));
    let before = fs::read(&absent_file).unwrap();

    let path = write_real_file(&root, "original.pdf", b"task-7.2c-gone bytes");
    let hash = hash_bytes(b"task-7.2c-gone bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.2c-gone"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let records = library::ArtifactStore::read(&root);
    assert_eq!(
        records.len(),
        2,
        "the absent artifact's record is left, and a new one is minted"
    );
    let new_record = records
        .iter()
        .find(|record| record.id != absent.id)
        .expect("a new artifact record must exist");
    assert_eq!(new_record.item, Some(item.id.clone()));
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "no second item may be minted"
    );

    let after = fs::read(&absent_file).unwrap();
    assert_eq!(
        before, after,
        "the absent artifact's own record must be left exactly as it is"
    );
}

/// task 7.3: an already-named file inside the library is recorded by an
/// applying run — the run holds the record and agrees with the name,
/// so a library already in good order is recorded rather than passed
/// over.
#[test]
fn an_already_named_file_inside_the_library_is_recorded_by_an_applying_run() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "Smith2024.pdf", b"task-7.3 already-named bytes");
    let hash = hash_bytes(b"task-7.3 already-named bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/task-7.3-already-named"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/task-7.3-already-named")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::AlreadyNamed { path: p } if p == &path)),
        "got {events:?}"
    );
    let records = library::ArtifactStore::read(&root);
    assert_eq!(
        records.len(),
        1,
        "an already-named file must still be recorded"
    );
    assert_eq!(records.iter().next().unwrap().path, "Smith2024.pdf");
}

/// task 7.3: a preview writes no artifact record and no item, while an
/// applying run over the same file does — proven by running both over
/// the same library rather than asserting the preview's emptiness on
/// its own, which a run that writes nothing at all, ever, would also
/// satisfy.
#[test]
fn a_preview_writes_nothing_but_an_applying_run_over_the_same_file_does() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"task-7.3 preview bytes");
    let hash = hash_bytes(b"task-7.3 preview bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/task-7.3-preview"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Preview", 2022, "10.1000/task-7.3-preview")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        library::ItemStore::read(&root).is_empty(),
        "a preview must mint no item"
    );
    assert!(
        library::ArtifactStore::read(&root).is_empty(),
        "a preview must write no artifact record"
    );
    assert!(path.exists(), "a preview must not move the file");

    events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "an applying run over the same file must mint what the preview did not"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "an applying run over the same file must record what the preview did not"
    );
}

/// task 7.3: `--no-record` writes neither an artifact record nor an
/// item, even though the move itself still happens — proven against a
/// run with the setting on for the same file, since "writes nothing"
/// alone cannot distinguish a gate that is honoured from one that was
/// never wired in.
#[test]
fn no_record_moves_the_file_but_writes_nothing_unlike_an_ordinary_run() {
    // With the account on, the same kind of file gets a record.
    let recording = real_library();
    let recording_root = recording.path().to_path_buf();
    let recording_path =
        write_real_file(&recording_root, "original.pdf", b"task-7.3 recording bytes");
    let recording_documents = FakeDocuments::new().with_file(
        &recording_path,
        hash_bytes(b"task-7.3 recording bytes"),
        pdf_with_embedded_doi("10.1000/task-7.3-recording"),
    );
    let recording_crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Recording", 2022, "10.1000/task-7.3-recording")),
    );
    let recording_sources: Vec<&dyn Source> = vec![&recording_crossref];
    let recording_index = ContentIndex::new(MemoryCache::new());
    let recording_bib_files = FakeBibFiles::new();
    let recording_adapters = Adapters {
        documents: &recording_documents,
        sources: &recording_sources,
        index: &recording_index,
        filesystem: &RealFilesystem,
        bib_files: &recording_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(recording_root.clone()),
        state_root: None,
    };
    events_for(
        &Command::rename(vec![recording_path.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &recording_adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(
        library::ArtifactStore::read(&recording_root).len(),
        1,
        "setup: an ordinary applying run must record the file it moved"
    );

    // With `--no-record`, the move still happens, but nothing is
    // written.
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"task-7.3 no-record bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"task-7.3 no-record bytes"),
        pdf_with_embedded_doi("10.1000/task-7.3-no-record"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("NoRecord", 2022, "10.1000/task-7.3-no-record")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.record = Some(false);
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("NoRecord2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "the move must still happen under --no-record: got {events:?}"
    );
    assert!(
        library::ItemStore::read(&root).is_empty(),
        "--no-record must mint no item, unlike the ordinary run above"
    );
    assert!(
        library::ArtifactStore::read(&root).is_empty(),
        "--no-record must write no artifact record, unlike the ordinary run above"
    );
}

/// task 7.3: a file renamed outside the library gets no record and is
/// reported as outside it — borax brings no file into a library.
#[test]
fn a_file_renamed_outside_the_library_gets_no_record_and_is_reported_outside_it() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let elsewhere = tempdir().unwrap();
    let path = write_real_file(elsewhere.path(), "original.pdf", b"task-7.3 outside bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"task-7.3 outside bytes"),
        pdf_with_embedded_doi("10.1000/task-7.3-outside"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Outside", 2022, "10.1000/task-7.3-outside")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = elsewhere.path().join("Outside2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "the file is renamed where it sits: got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::LibraryAdmission { path: p, admission: Admission::Outside } if p == &target
        )),
        "the run must report the file as outside the library: got {events:?}"
    );
    assert!(library::ItemStore::read(&root).is_empty());
    assert!(library::ArtifactStore::read(&root).is_empty());
}

/// task 2.5's admissions half: a file under a nested `.borax.toml` is
/// excluded from the enclosing library's admissions exactly as it is
/// from its artifact walk, its orphan count and its reconciliation —
/// the same rule, asked by a fourth operation. The enclosing library's
/// applying run still renames the file where it sits, reports it
/// `Outside`, and records nothing for it; the nested library's own
/// store is where such a file would be recorded, and this run is not
/// that library's.
#[test]
fn an_applying_run_does_not_record_a_file_under_a_nested_library_against_the_enclosing_one() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let nested = root.join("nested-project");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join(".borax.toml"), b"").unwrap();
    let path = write_real_file(&nested, "original.pdf", b"task-2.5 nested bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"task-2.5 nested bytes"),
        pdf_with_embedded_doi("10.1000/task-2.5-nested"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Nested", 2022, "10.1000/task-2.5-nested")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        // The *enclosing* library is what the run is over: the nested
        // directory's own marker does not change what root the run was
        // given.
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = nested.join("Nested2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "the enclosing run still renames the file where it sits: got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::LibraryAdmission { path: p, admission: Admission::Outside } if p == &target
        )),
        "a file under a nested library must be reported outside the enclosing one: \
         got {events:?}"
    );
    assert!(
        library::ItemStore::read(&root).is_empty(),
        "the enclosing library's item store must gain nothing from a nested library's file"
    );
    assert!(
        library::ArtifactStore::read(&root).is_empty(),
        "the enclosing library's artifact store must gain nothing from a nested library's file"
    );
}

/// task 7.3a: under `--no-record`, an artifact whose record's history
/// holds its hash is moved, leaving the record with a stale path, and a
/// `borax reconcile` afterwards repairs that path by the hash already
/// recorded — identity and item link unchanged throughout. Proven
/// against a companion run with the account on for the same kind of
/// file, which updates the record directly and needs no reconcile at
/// all: "the move proceeds and reconcile can fix the path" is not by
/// itself distinguishable from a run that writes nothing, ever.
#[test]
fn no_record_moves_an_artifact_whose_history_holds_its_hash_leaving_a_stale_path() {
    // Companion: the same file, the same held record, with the account
    // on. The move must update the record directly, with no reconcile
    // needed.
    let recording = real_library();
    let recording_root = recording.path().to_path_buf();
    let recording_seed = "task-7.3a-recording bytes";
    let recording_path =
        write_real_file(&recording_root, "original.pdf", recording_seed.as_bytes());
    let recording_hash = hash_for(recording_seed);
    let recording_record = record_by("Recording", 2022, "10.1000/task-7.3a-recording");
    let recording_item = Item {
        id: lib_item_id(G7_UUID_A),
        record: recording_record.clone(),
    };
    write_lib_item(&recording_root, "recording2022", &recording_item);
    let (recording_size, _) = real_stat(&recording_path);
    let recording_held = ArtifactRecord {
        id: lib_artifact_id(G7_UUID_B),
        item: Some(recording_item.id.clone()),
        path: "original.pdf".to_string(),
        size: recording_size,
        modified_millis: 0,
        history: vec![hash_entry_for(recording_hash.clone(), "run-0")],
    };
    write_lib_artifact_record(&recording_root, &recording_held);
    let recording_documents = FakeDocuments::new().with_file(
        &recording_path,
        recording_hash,
        pdf_with_embedded_doi("10.1000/task-7.3a-recording"),
    );
    let recording_crossref = fake_source(SourceName::Crossref, Ok(recording_record));
    let recording_sources: Vec<&dyn Source> = vec![&recording_crossref];
    let recording_index = ContentIndex::new(MemoryCache::new());
    let recording_bib_files = FakeBibFiles::new();
    let recording_adapters = Adapters {
        documents: &recording_documents,
        sources: &recording_sources,
        index: &recording_index,
        filesystem: &RealFilesystem,
        bib_files: &recording_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(recording_root.clone()),
        state_root: None,
    };
    events_for(
        &Command::rename(vec![recording_path.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &recording_adapters,
        &mut Session::batch(),
    )
    .unwrap();
    let recorded = library::ArtifactStore::read(&recording_root)
        .by_id(&recording_held.id)
        .unwrap()
        .clone();
    assert_eq!(
        recorded.path, "Recording2022.pdf",
        "setup: an ordinary applying run must update the record's path directly"
    );

    // The case under test: the same setup, but `--no-record`.
    let library = real_library();
    let root = library.path().to_path_buf();
    let seed = "task-7.3a-history bytes";
    let path = write_real_file(&root, "original.pdf", seed.as_bytes());
    let hash = hash_for(seed);
    let record = record_by("History", 2022, "10.1000/task-7.3a-history");
    let item = Item {
        id: lib_item_id(G7_UUID_A),
        record: record.clone(),
    };
    write_lib_item(&root, "history2022", &item);
    let (size, _) = real_stat(&path);
    let held = ArtifactRecord {
        id: lib_artifact_id(G7_UUID_B),
        item: Some(item.id.clone()),
        path: "original.pdf".to_string(),
        size,
        modified_millis: 0,
        history: vec![hash_entry_for(hash.clone(), "run-0")],
    };
    write_lib_artifact_record(&root, &held);

    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.3a-history"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.record = Some(false);
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("History2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "the hash already in history must let the move proceed under --no-record: \
         got {events:?}"
    );

    let stale = library::ArtifactStore::read(&root)
        .by_id(&held.id)
        .unwrap()
        .clone();
    assert_eq!(
        stale.path, "original.pdf",
        "unlike the recording run above, --no-record must leave the path stale"
    );
    assert_eq!(stale.item, Some(item.id.clone()));
    assert_eq!(stale.history, held.history);

    library::reconcile(
        &root,
        false,
        LibraryRunId::new("reconcile-1"),
        "2026-01-01T00:00:00Z",
        "0.6.0-test",
    );

    let repaired = library::ArtifactStore::read(&root)
        .by_id(&held.id)
        .unwrap()
        .clone();
    assert_eq!(
        repaired.path, "History2022.pdf",
        "reconcile must repair the stale path by the hash already recorded"
    );
    assert_eq!(repaired.item, Some(item.id.clone()));
    assert_eq!(
        repaired.history, held.history,
        "the identity and item link must be unchanged throughout"
    );
}

/// task 7.3a, the critical counterexample: an artifact edited in place
/// to the same length with its modification time preserved has a
/// record whose history does not hold its current hash, so the fast
/// path alone would confirm a record that cannot survive the move.
/// `--no-record` must refuse this one move — a run without the flag
/// moving the same file is [`no_record_moves_an_artifact_whose_history_holds_its_hash_leaving_a_stale_path`]'s
/// counterpart and is exercised at the end of this test.
#[test]
fn no_record_refuses_a_move_when_the_edit_in_place_hash_is_stale() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let original: &[u8] = b"stranding-content-A";
    let edited: &[u8] = b"stranding-content-B";
    assert_eq!(original.len(), edited.len());

    let path = write_real_file(&root, "original.pdf", original);
    let recorded_hash = hash_bytes(original);
    let (size, modified_millis) = real_stat(&path);
    let held = ArtifactRecord {
        id: lib_artifact_id(G7_UUID_A),
        item: None,
        path: "original.pdf".to_string(),
        size,
        modified_millis,
        history: vec![hash_entry_for(recorded_hash.clone(), "run-0")],
    };
    write_lib_artifact_record(&root, &held);

    edit_in_place_same_length(&path, edited);
    let current_hash = hash_bytes(edited);
    assert_ne!(current_hash, recorded_hash);

    let documents = FakeDocuments::new().with_file(
        &path,
        current_hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.3a-refusal"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Refusal", 2022, "10.1000/task-7.3a-refusal")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.record = Some(false);
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped { path: p, reason: SkipReason::Stranding { id }, .. }
                if p == &path && id == &held.id.to_string()
        )),
        "the move must be refused because no recorded hash would survive it: got {events:?}"
    );
    assert!(path.exists(), "the file must not have moved");
    let after = library::ArtifactStore::read(&root)
        .by_id(&held.id)
        .unwrap()
        .clone();
    assert_eq!(
        after, held,
        "the record must be untouched by a refused move"
    );

    // The counterpart: the same file, the same edit, without the flag —
    // the move happens and the file's hash is written into the history,
    // so the refusal above is the flag's and not the file's.
    let effective_recording = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
    });
    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective_recording),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("Refusal2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "without the flag the same move must proceed: got {events:?}"
    );
    let recorded = library::ArtifactStore::read(&root)
        .by_id(&held.id)
        .unwrap()
        .clone();
    assert_eq!(recorded.path, "Refusal2022.pdf");
    assert_eq!(
        recorded.history.last().map(|entry| &entry.hash),
        Some(&current_hash),
        "the file's current hash must be written into the history"
    );
}

/// task 7.3b: after an applying run moves an artifact edited in place
/// to the same length with its modification time preserved, its
/// record's newest hash is the file's, the earlier hash is still
/// there, and the path, size and modification time are the file's —
/// so the next reconcile settles it on the fast path.
#[test]
fn an_applying_run_writes_the_edited_files_current_hash_into_the_update() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let original: &[u8] = b"update-content-value-A";
    let edited: &[u8] = b"update-content-value-B";
    assert_eq!(original.len(), edited.len());

    let path = write_real_file(&root, "original.pdf", original);
    let recorded_hash = hash_bytes(original);
    let record = record_by("Update", 2022, "10.1000/task-7.3b");
    let item = Item {
        id: lib_item_id(G7_UUID_A),
        record: record.clone(),
    };
    write_lib_item(&root, "update2022", &item);
    let (size, modified_millis) = real_stat(&path);
    let held = ArtifactRecord {
        id: lib_artifact_id(G7_UUID_B),
        item: Some(item.id.clone()),
        path: "original.pdf".to_string(),
        size,
        modified_millis,
        history: vec![hash_entry_for(recorded_hash.clone(), "run-0")],
    };
    write_lib_artifact_record(&root, &held);

    edit_in_place_same_length(&path, edited);
    let current_hash = hash_bytes(edited);

    let documents = FakeDocuments::new().with_file(
        &path,
        current_hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.3b"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("Update2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "got {events:?}"
    );

    let updated = library::ArtifactStore::read(&root)
        .by_id(&held.id)
        .unwrap()
        .clone();
    assert_eq!(updated.path, "Update2022.pdf");
    assert_eq!(
        updated.history.last().map(|entry| &entry.hash),
        Some(&current_hash),
        "the newest hash must be the file's current bytes, not its stale recorded one"
    );
    assert!(
        updated
            .history
            .iter()
            .any(|entry| entry.hash == recorded_hash),
        "the earlier hash must still be there: got {:?}",
        updated.history
    );
    let (size, modified_millis) = real_stat(&target);
    assert_eq!(updated.size, size);
    assert_eq!(
        updated.modified_millis, modified_millis,
        "so the next reconcile settles this record on the fast path"
    );
}

/// task 7.4: a store write that fails leaves the rename standing and
/// reported, and the next applying run over the file records it again.
/// `.borax/artifacts` is a plain file here, so the store's own write
/// cannot even create its directory.
#[test]
fn a_failed_store_write_leaves_the_rename_standing_and_the_next_run_records_it() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"task-7.4 durability bytes");
    let hash = hash_bytes(b"task-7.4 durability bytes");
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(root.join(STATE_DIR).join(ARTIFACT_STORE), b"blocking").unwrap();

    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.4"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Durable", 2022, "10.1000/task-7.4")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("Durable2022.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if from == &path && to == &target
        )),
        "the rename must stand even though the store write behind it failed: got {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::LibraryAdmission { path: p, admission: Admission::Unwritten { .. } }
                if p == &target
        )),
        "the failed write must be reported: got {events:?}"
    );
    assert!(target.exists(), "the file must have actually moved");
    assert!(
        library::ArtifactStore::read(&root).is_empty(),
        "no record may exist when its own write failed"
    );

    fs::remove_file(root.join(STATE_DIR).join(ARTIFACT_STORE)).unwrap();
    let documents_2 =
        FakeDocuments::new().with_file(&target, hash, pdf_with_embedded_doi("10.1000/task-7.4"));
    let sources_2: Vec<&dyn Source> = vec![&crossref];
    let index_2 = ContentIndex::new(MemoryCache::new());
    let bib_files_2 = FakeBibFiles::new();
    let adapters_2 = Adapters {
        documents: &documents_2,
        sources: &sources_2,
        index: &index_2,
        filesystem: &RealFilesystem,
        bib_files: &bib_files_2,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    events_for(
        &Command::rename(vec![target.clone()], true),
        &Configs::uniform(effective),
        &adapters_2,
        &mut Session::batch(),
    )
    .unwrap();

    let records = library::ArtifactStore::read(&root);
    assert_eq!(
        records.len(),
        1,
        "the next applying run over the file must record it"
    );
    assert_eq!(records.iter().next().unwrap().path, "Durable2022.pdf");
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "the item the interrupted admission left behind must be reused, not duplicated"
    );
}

// ---------------------------------------------------------------------
// group 7.6/7.6a/7.6b: duplicate detection against the library store
//
// The account this batch checks against is `ledger: None` throughout,
// exactly as group 7 above: the *old* ledger-based duplicate detection
// (`crate::pipeline::content_duplicate`/`work_duplicate`, gated on
// `adapters.ledger`) is still wired into `rename_events` and still
// runs, but it finds nothing here since it is never given a ledger.
// `Account::content_duplicate`/`work_duplicate` are `todo!()` in
// `library.rs` and are not yet reached from `run.rs` at all, so a
// duplicate the *library* store holds is, today, simply never found:
// every assertion below that expects one to be caught fails on its own
// terms rather than panicking.
// ---------------------------------------------------------------------

/// Runs an ordinary applying rename over a freshly written PDF at
/// `relative`, resolving to `doi_value`, so the library already holds
/// an item and a live artifact for that identifier before the test
/// asks about a second PDF of the same work. Hands back the path the
/// file moved to.
fn seed_sibling_artifact(
    root: &Path,
    relative: &str,
    family: &str,
    year: i32,
    doi_value: &str,
    bytes: &[u8],
) -> PathBuf {
    let original = write_real_file(root, relative, bytes);
    let documents = FakeDocuments::new().with_file(
        &original,
        hash_bytes(bytes),
        pdf_with_embedded_doi(doi_value),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record_by(family, year, doi_value)));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.to_path_buf()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![original.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { target, .. } => Some(target.clone()),
            _ => None,
        })
        .expect("setup: the sibling file must have moved");
    assert!(target.exists(), "setup: the sibling file must be on disk");
    target
}

/// task 7.6a: a batch run over a file whose identifier an item already
/// carries — with a live sibling artifact recorded against it — skips
/// the file with the work-duplicate reason naming the recorded path,
/// writes no record and moves nothing, including under `--apply`
/// (design D16).
#[test]
fn a_batch_run_skips_a_file_whose_identifier_an_item_already_carries_as_a_work_duplicate() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let sibling_target = seed_sibling_artifact(
        &root,
        "sibling-original.pdf",
        "Sibling",
        2020,
        "10.1000/task-7.6a-batch",
        b"task-7.6a sibling bytes",
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "setup: exactly one item must be minted"
    );

    let incoming = write_real_file(&root, "incoming.pdf", b"task-7.6a incoming bytes");
    let documents = FakeDocuments::new().with_file(
        &incoming,
        hash_bytes(b"task-7.6a incoming bytes"),
        pdf_with_embedded_doi("10.1000/task-7.6a-batch"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Sibling", 2020, "10.1000/task-7.6a-batch")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![incoming.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Work,
                    existing_path,
                },
                ..
            } if path == &incoming && existing_path == &sibling_target
        )),
        "a second PDF of an already-archived work must be skipped as a work duplicate \
         naming the sibling's path: got {events:?}"
    );
    assert!(
        incoming.exists(),
        "a work duplicate must move nothing, including under --apply"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "a work duplicate must write no record"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "a work duplicate must mint no second item"
    );
}

/// task 7.6a: an interactive run over the same shape of file puts the
/// question, offering exactly file-as-another-artifact, skip and quit
/// with skip as the default — filing must never be — and naming both
/// the item's file and the sibling artifact's path in its description.
/// Declining reports the work-duplicate reason and not `declined`.
#[test]
fn an_interactive_run_offers_filing_a_work_duplicate_as_another_artifact() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let sibling_target = seed_sibling_artifact(
        &root,
        "sibling-original.pdf",
        "Sibling",
        2021,
        "10.1000/task-7.6a-interactive",
        b"task-7.6a-i sibling bytes",
    );
    let items = library::ItemStore::read(&root);
    let item = items.iter().next().expect("setup: an item must be minted");
    let item_file = items
        .file_of(&item.id)
        .expect("setup: the item's file must be found")
        .to_path_buf();

    let incoming = write_real_file(&root, "incoming.pdf", b"task-7.6a-i incoming bytes");
    let documents = FakeDocuments::new().with_file(
        &incoming,
        hash_bytes(b"task-7.6a-i incoming bytes"),
        pdf_with_embedded_doi("10.1000/task-7.6a-interactive"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Sibling", 2021, "10.1000/task-7.6a-interactive")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![incoming.clone()], false),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_eq!(
        questions[0].choices,
        vec![Answer::Skip, Answer::File, Answer::Quit],
        "skip must be the default and filing must never be: got {:?}",
        questions[0].choices
    );
    // Named by file name rather than by full path: the requirement is
    // that the operator is shown which work and which file they are
    // deciding against, and whether a description renders a path in
    // full, relative to the library, or relative to the working
    // directory is a rendering decision this does not pin.
    let description = questions[0].description.join("\n");
    let item_named = item_file.file_name().unwrap().to_string_lossy().to_string();
    let sibling_named = sibling_target
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        description.contains(&item_named),
        "the description must name the item ({item_named}): got {description:?}"
    );
    assert!(
        description.contains(&sibling_named),
        "the description must name the recorded artifact ({sibling_named}): got {description:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Work,
                    existing_path,
                },
                ..
            } if path == &incoming && existing_path == &sibling_target
        )),
        "declining must report the work-duplicate reason and not `declined`: got {events:?}"
    );
}

/// task 7.6a: accepting returns the file to planning, where the
/// ordinary question about its move is put — only then is it moved and
/// recorded, as a second artifact of the item it matched with no
/// second item minted.
///
/// This one goes red on the `todo!()` in `run.rs`'s `Answer::File`
/// arm rather than on an assertion, there being no way to reach the
/// accepting path without answering it. Its assertions are what the
/// green stage is held to.
#[test]
fn filing_a_work_duplicate_moves_it_and_records_it_against_the_same_item() {
    let library = real_library();
    let root = library.path().to_path_buf();
    seed_sibling_artifact(
        &root,
        "sibling-original.pdf",
        "Filed",
        2022,
        "10.1000/task-7.6a-filed",
        b"task-7.6a-f sibling bytes",
    );
    let item = library::ItemStore::read(&root)
        .iter()
        .next()
        .expect("setup: an item must be minted")
        .id
        .clone();

    let incoming = write_real_file(&root, "incoming.pdf", b"task-7.6a-f incoming bytes");
    let documents = FakeDocuments::new().with_file(
        &incoming,
        hash_bytes(b"task-7.6a-f incoming bytes"),
        pdf_with_embedded_doi("10.1000/task-7.6a-filed"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Filed", 2022, "10.1000/task-7.6a-filed")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::File, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![incoming.clone()], false),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(
        questions.len(),
        2,
        "accepting must be followed by the ordinary move question: got {questions:?}"
    );
    assert_eq!(
        questions[1].choices,
        vec![Answer::Rename, Answer::Supply, Answer::Skip, Answer::Quit],
        "the second question must be the ordinary one about the move: got {:?}",
        questions[1].choices
    );
    let moved = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { path, target, .. } if path == &incoming => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the accepted file must be moved: got {events:?}"));

    let records = library::ArtifactStore::read(&root);
    assert_eq!(
        records.len(),
        2,
        "filing must write a second artifact record"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "filing must mint no second item: it is another artifact of the one it matched"
    );
    let filed = records
        .by_path(&library::library_relative(&root, &moved).unwrap())
        .unwrap_or_else(|| panic!("the moved file must have a record: got {records:?}"));
    assert_eq!(
        filed.item.as_ref(),
        Some(&item),
        "the second artifact must be linked to the item it was filed against"
    );
}

/// task 7.6a/normative "A file reported as a content duplicate ... SHALL
/// NOT be asked about": an interactive run over a byte-identical file
/// puts no question at all and reports it a content duplicate.
#[test]
fn a_content_duplicate_is_never_asked_about_interactively() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let shared_hash = hash_bytes(b"task-7.6a-c shared bytes");

    let sibling_original = write_real_file(
        &root,
        "sibling-original.pdf",
        b"task-7.6a-c sibling bytes on disk",
    );
    let sibling_documents = FakeDocuments::new().with_file(
        &sibling_original,
        shared_hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.6a-c-sibling"),
    );
    let sibling_crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Sibling", 2019, "10.1000/task-7.6a-c-sibling")),
    );
    let sibling_sources: Vec<&dyn Source> = vec![&sibling_crossref];
    let sibling_index = ContentIndex::new(MemoryCache::new());
    let sibling_bib_files = FakeBibFiles::new();
    let sibling_adapters = Adapters {
        documents: &sibling_documents,
        sources: &sibling_sources,
        index: &sibling_index,
        filesystem: &RealFilesystem,
        bib_files: &sibling_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let setup_events = events_for(
        &Command::rename(vec![sibling_original.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &sibling_adapters,
        &mut Session::batch(),
    )
    .unwrap();
    let sibling_target = setup_events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { target, .. } => Some(target.clone()),
            _ => None,
        })
        .expect("setup: the sibling file must have moved");

    let incoming = write_real_file(&root, "incoming.pdf", b"task-7.6a-c incoming bytes on disk");
    let documents = FakeDocuments::new().with_file(
        &incoming,
        shared_hash,
        pdf_with_embedded_doi("10.1000/task-7.6a-c-incoming"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Incoming", 2022, "10.1000/task-7.6a-c-incoming")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    // One spare answer in case the file is still put to a question
    // today; the assertion below is what must fail, not the asker
    // running out of scripted answers.
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![incoming.clone()], false),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert!(
        asker.questions_asked().is_empty(),
        "a content duplicate must never be asked about: got {:?}",
        asker.questions_asked()
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Content,
                    existing_path,
                },
                ..
            } if path == &incoming && existing_path == &sibling_target
        )),
        "got {events:?}"
    );
}

/// task 7.6: with the account on, a byte-identical second file is a
/// content duplicate and must be skipped rather than admitted; with
/// `--no-record`, the same shape of duplicate is admitted silently —
/// the check is off, not merely non-blocking, which is what proves
/// `--no-record` suppresses the check itself and not only the writes
/// behind it.
#[test]
fn recording_catches_a_content_duplicate_but_no_record_admits_it_silently() {
    // With the account on, a genuine recorded duplicate is caught.
    let recording = real_library();
    let recording_root = recording.path().to_path_buf();
    let recording_shared_hash = hash_bytes(b"task-7.6-r shared bytes");
    let recording_sibling = write_real_file(
        &recording_root,
        "sibling-original.pdf",
        b"task-7.6-r sibling bytes on disk",
    );
    let recording_sibling_documents = FakeDocuments::new().with_file(
        &recording_sibling,
        recording_shared_hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.6-r-sibling"),
    );
    let recording_sibling_crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Sibling", 2018, "10.1000/task-7.6-r-sibling")),
    );
    let recording_sibling_sources: Vec<&dyn Source> = vec![&recording_sibling_crossref];
    let recording_sibling_index = ContentIndex::new(MemoryCache::new());
    let recording_sibling_bib_files = FakeBibFiles::new();
    let recording_sibling_adapters = Adapters {
        documents: &recording_sibling_documents,
        sources: &recording_sibling_sources,
        index: &recording_sibling_index,
        filesystem: &RealFilesystem,
        bib_files: &recording_sibling_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(recording_root.clone()),
        state_root: None,
    };
    let recording_setup_events = events_for(
        &Command::rename(vec![recording_sibling.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &recording_sibling_adapters,
        &mut Session::batch(),
    )
    .unwrap();
    let recording_sibling_target = recording_setup_events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { target, .. } => Some(target.clone()),
            _ => None,
        })
        .expect("setup: the sibling file must have moved");

    let recording_incoming = write_real_file(
        &recording_root,
        "incoming.pdf",
        b"task-7.6-r incoming bytes on disk",
    );
    let recording_documents = FakeDocuments::new().with_file(
        &recording_incoming,
        recording_shared_hash,
        pdf_with_embedded_doi("10.1000/task-7.6-r-incoming"),
    );
    let recording_crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Incoming", 2023, "10.1000/task-7.6-r-incoming")),
    );
    let recording_sources: Vec<&dyn Source> = vec![&recording_crossref];
    let recording_index = ContentIndex::new(MemoryCache::new());
    let recording_bib_files = FakeBibFiles::new();
    let recording_adapters = Adapters {
        documents: &recording_documents,
        sources: &recording_sources,
        index: &recording_index,
        filesystem: &RealFilesystem,
        bib_files: &recording_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(recording_root.clone()),
        state_root: None,
    };

    let recording_events = events_for(
        &Command::rename(vec![recording_incoming.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &recording_adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        recording_events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Content,
                    existing_path,
                },
                ..
            } if path == &recording_incoming && existing_path == &recording_sibling_target
        )),
        "with the account on, a byte-identical file must be skipped as a content \
         duplicate: got {recording_events:?}"
    );
    assert!(
        recording_incoming.exists(),
        "a content duplicate must never be moved"
    );
    assert_eq!(
        library::ArtifactStore::read(&recording_root).len(),
        1,
        "a content duplicate must write no second record"
    );

    // With `--no-record`, the same shape of duplicate is admitted
    // silently.
    let library = real_library();
    let root = library.path().to_path_buf();
    let shared_hash = hash_bytes(b"task-7.6-n shared bytes");

    let sibling = write_real_file(
        &root,
        "sibling-original.pdf",
        b"task-7.6-n sibling bytes on disk",
    );
    let sibling_documents = FakeDocuments::new().with_file(
        &sibling,
        shared_hash.clone(),
        pdf_with_embedded_doi("10.1000/task-7.6-n-sibling"),
    );
    let sibling_crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Sibling", 2018, "10.1000/task-7.6-n-sibling")),
    );
    let sibling_sources: Vec<&dyn Source> = vec![&sibling_crossref];
    let sibling_index = ContentIndex::new(MemoryCache::new());
    let sibling_bib_files = FakeBibFiles::new();
    let sibling_adapters = Adapters {
        documents: &sibling_documents,
        sources: &sibling_sources,
        index: &sibling_index,
        filesystem: &RealFilesystem,
        bib_files: &sibling_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    events_for(
        &Command::rename(vec![sibling.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &sibling_adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "setup: the sibling must be recorded, so a genuine duplicate exists to check against"
    );

    let incoming = write_real_file(&root, "incoming.pdf", b"task-7.6-n incoming bytes on disk");
    let documents = FakeDocuments::new().with_file(
        &incoming,
        shared_hash,
        pdf_with_embedded_doi("10.1000/task-7.6-n-incoming"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Incoming", 2024, "10.1000/task-7.6-n-incoming")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.record = Some(false);
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![incoming.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, .. } if from == &incoming
        )),
        "--no-record must silently admit a recorded duplicate rather than checking for one: \
         got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Skipped { .. })),
        "--no-record must skip nothing: got {events:?}"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "--no-record must write no record for the incoming file, leaving only the sibling's"
    );
}

// ---------------------------------------------------------------------
// group 7.6: a stale recorded path never blocks, and is said once
//
// "Stale entries never block re-admission": disk is the source of
// truth, so a record whose last-known path holds no file vetoes
// nothing — and what the run has to say about it is one fact about the
// library, said once however many files revealed it.
// ---------------------------------------------------------------------

const STALE_UUID_A: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdefa";
const STALE_UUID_B: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdefb";

/// Writes an artifact record for `relative` holding `seed`'s hash,
/// whatever stands at that path — the shape a stale record has: the
/// library says a file is there and nothing is.
fn write_record_for(root: &Path, id: &str, relative: &str, seed: &str) {
    write_lib_artifact_record(
        root,
        &ArtifactRecord {
            id: lib_artifact_id(id),
            item: None,
            path: relative.to_string(),
            size: 100,
            modified_millis: 0,
            history: vec![lib_hash_entry(seed, "run-earlier")],
        },
    );
}

/// Runs an applying rename over `paths` through `dispatch`, so what the
/// run writes to standard error can be read: `events_for` reports the
/// events and drops the diagnostics.
fn stderr_of_rename(
    root: &Path,
    paths: Vec<PathBuf>,
    documents: &FakeDocuments,
    sources: &[&dyn Source],
) -> (String, String) {
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents,
        sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.to_path_buf()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();

    dispatch(
        &cli(Command::rename(paths, true), true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
        &mut Streams {
            out: &mut out,
            err: &mut err,
        },
    );

    (
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// A file whose hash a record holds, at a path holding no file, is
/// admitted normally — and the run says once that the library holds
/// paths to reconcile, naming the command that repairs them.
#[test]
fn a_stale_recorded_path_is_admitted_and_warns_that_the_library_needs_reconciling() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"stale-path bytes";
    let path = write_real_file(&root, "original.pdf", bytes);
    write_record_for(&root, STALE_UUID_A, "gone.pdf", "stale-path bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(bytes),
        pdf_with_embedded_doi("10.1000/stale-path"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/stale-path")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    let (out, err) = stderr_of_rename(&root, vec![path], &documents, &sources);

    assert!(
        out.contains("\"event\":\"renamed\""),
        "a stale recorded path must never block an admission: got {out:?}"
    );
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 1, "expected exactly one warning, got {err:?}");
    assert!(lines[0].starts_with("warning:"), "got {err:?}");
    assert!(
        lines[0].contains("reconcile"),
        "the warning must name the command that repairs the paths, got {err:?}"
    );
}

/// One fact about the library, said once: two files each matching a
/// stale record still produce a single warning.
#[test]
fn two_files_hitting_stale_recorded_paths_still_warn_once() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let first = write_real_file(&root, "first.pdf", b"stale-first bytes");
    let second = write_real_file(&root, "second.pdf", b"stale-second bytes");
    write_record_for(&root, STALE_UUID_A, "gone-one.pdf", "stale-first bytes");
    write_record_for(&root, STALE_UUID_B, "gone-two.pdf", "stale-second bytes");
    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_bytes(b"stale-first bytes"),
            pdf_with_embedded_doi("10.1000/stale-first"),
        )
        .with_file(
            &second,
            hash_bytes(b"stale-second bytes"),
            pdf_with_embedded_doi("10.1000/stale-second"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/stale-first",
            record_by("First", 2024, "10.1000/stale-first"),
        )
        .answering(
            "doi:10.1000/stale-second",
            record_by("Second", 2024, "10.1000/stale-second"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];

    let (_, err) = stderr_of_rename(&root, vec![first, second], &documents, &sources);

    assert_eq!(
        err.lines().count(),
        1,
        "two stale matches must still warn once, got {err:?}"
    );
}

/// A run that matches nothing is not a run with stale paths: a miss is
/// a miss, and nothing is said.
#[test]
fn a_run_matching_no_record_says_nothing_about_stale_paths() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"no-match bytes");
    write_record_for(&root, STALE_UUID_A, "unrelated.pdf", "other bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"no-match bytes"),
        pdf_with_embedded_doi("10.1000/no-match"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-match")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    let (_, err) = stderr_of_rename(&root, vec![path], &documents, &sources);

    assert!(err.is_empty(), "a miss is not staleness, got {err:?}");
}

/// The path an artifact record stores is relative to the library root
/// and `/`-separated — not relative to the directory the file happened
/// to be renamed within, which is what would make a record unreadable
/// from anywhere else in the library.
#[test]
fn an_applied_rename_records_the_files_new_path_relative_to_the_library_root() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"library-relative bytes";
    let path = write_real_file(&root, "sub/original.pdf", bytes);
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(bytes),
        pdf_with_embedded_doi("10.1000/library-relative"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/library-relative")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    let (out, _) = stderr_of_rename(&root, vec![path], &documents, &sources);

    assert!(
        out.contains("\"event\":\"renamed\""),
        "setup: the file must move: got {out:?}"
    );
    let records = library::ArtifactStore::read(&root);
    let recorded: Vec<&str> = records.iter().map(|record| record.path.as_str()).collect();
    assert_eq!(recorded, vec!["sub/Smith2024.pdf"], "got {recorded:?}");
}

// ---------------------------------------------------------------------
// group 7.8: the retired ledger is left alone
// ---------------------------------------------------------------------

/// task 7.8: no library command reads, writes or deletes
/// `.borax/ledger.jsonl`, borax's own retired accounting — a library
/// holding one is reported, validated, reconciled and adopted with the
/// file left byte-identical, and an applying run appends nothing to it.
/// The file sits inside the state directory every library command
/// writes to, so it is left alone deliberately rather than by being out
/// of reach.
#[test]
fn no_library_command_touches_the_retired_ledger_file() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let ledger_path = root.join(STATE_DIR).join("ledger.jsonl");
    fs::create_dir_all(ledger_path.parent().unwrap()).unwrap();
    let original: &[u8] =
        b"{\"schema\":1,\"path\":\"old.pdf\",\"hash\":\"deadbeef\",\"run\":\"run-0\"}\n";
    fs::write(&ledger_path, original).unwrap();

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    events_for(
        &Command::status(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(
        fs::read(&ledger_path).unwrap(),
        original,
        "status must not touch the retired ledger file"
    );

    events_for(
        &Command::validate(Some(root.clone())),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(
        fs::read(&ledger_path).unwrap(),
        original,
        "validate must not touch the retired ledger file"
    );

    events_for(
        &Command::reconcile(Some(root.clone()), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(
        fs::read(&ledger_path).unwrap(),
        original,
        "reconcile must not touch the retired ledger file"
    );

    events_for(
        &Command::adopt(Some(root.clone())),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(
        fs::read(&ledger_path).unwrap(),
        original,
        "adopt must not touch the retired ledger file"
    );

    // An applying rename over the same library, which writes the state
    // directory the retired file sits in.
    let path = write_real_file(&root, "original.pdf", b"task-7.8 bytes");
    let hash = hash_bytes(b"task-7.8 bytes");
    let rename_documents =
        FakeDocuments::new().with_file(&path, hash, pdf_with_embedded_doi("10.1000/task-7.8"));
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Ledgerless", 2022, "10.1000/task-7.8")),
    );
    let rename_sources: Vec<&dyn Source> = vec![&crossref];
    let rename_index = ContentIndex::new(MemoryCache::new());
    let rename_bib_files = FakeBibFiles::new();
    let rename_adapters = Adapters {
        documents: &rename_documents,
        sources: &rename_sources,
        index: &rename_index,
        filesystem: &RealFilesystem,
        bib_files: &rename_bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &rename_adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Renamed { .. })),
        "setup: the file must actually move: got {events:?}"
    );

    assert_eq!(
        fs::read(&ledger_path).unwrap(),
        original,
        "an applying run must write nothing to the retired ledger file: got {:?}",
        fs::read(&ledger_path).unwrap()
    );
}

/// task 7.6b: each of the three recorded artifacts of one item,
/// reached in turn by a batch run, is reported neither a duplicate nor
/// skipped — the check passes over the item the incoming file's own
/// record links to. Without that rule every artifact of a
/// multi-artifact item meets its siblings as a duplicate of the item
/// all three belong to, and a batch run skips all three.
///
/// The three sit in directories of their own, each already carrying
/// the name its record renders, so the run reaches each of them with
/// its own record in hand and nothing about the move confuses what is
/// being asserted.
#[test]
fn a_batch_run_over_three_artifacts_of_one_item_skips_none_of_them() {
    const G7_UUID_D: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef8";

    let library = real_library();
    let root = library.path().to_path_buf();
    let item = Item {
        id: lib_item_id(G7_UUID_A),
        record: record_by("Sibling", 2020, "10.1000/task-7.6b"),
    };
    write_lib_item(&root, "sibling2020", &item);

    let mut documents = FakeDocuments::new();
    let mut paths = Vec::new();
    for (uuid, directory, bytes) in [
        (G7_UUID_B, "one", &b"task-7.6b one"[..]),
        (G7_UUID_C, "two", &b"task-7.6b two"[..]),
        (G7_UUID_D, "three", &b"task-7.6b three"[..]),
    ] {
        let relative = format!("{directory}/Sibling2020.pdf");
        let path = write_real_file(&root, &relative, bytes);
        let (size, modified_millis) = real_stat(&path);
        write_lib_artifact_record(
            &root,
            &ArtifactRecord {
                id: lib_artifact_id(uuid),
                item: Some(item.id.clone()),
                path: relative,
                size,
                modified_millis,
                history: vec![hash_entry_for(hash_bytes(bytes), "run-0")],
            },
        );
        documents = documents.with_file(
            &path,
            hash_bytes(bytes),
            pdf_with_embedded_doi("10.1000/task-7.6b"),
        );
        paths.push(path);
    }

    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Sibling", 2020, "10.1000/task-7.6b")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(paths.clone(), true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    // Each file was resolved, so each of them reached the work check
    // rather than being passed over before it: without this the test
    // could hold with no check having run at all.
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Resolved { .. }))
            .count(),
        3,
        "each of the three must be resolved, or nothing was checked: got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Skipped { .. })),
        "no artifact of an item may be a duplicate of the item it belongs to: got {events:?}"
    );
    for path in &paths {
        assert!(path.exists(), "nothing may move: {}", path.display());
    }
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "no second item may be minted"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        3,
        "the three records must still be three"
    );
}

// ---------------------------------------------------------------------
// group 8: borax adopt — recording from the content index, offline
// ---------------------------------------------------------------------

/// Every file under `root` outside the two stores, with its bytes: the
/// library's artifacts and everything a person put beside them, which
/// is exactly what adoption must leave alone.
fn outside_the_stores(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    snapshot(root)
        .into_iter()
        .filter(|(path, _)| {
            !path.starts_with(root.join(STATE_DIR).join(ARTIFACT_STORE))
                && !path.starts_with(root.join(ITEM_STORE))
        })
        .collect()
}

/// The adoption events of `events`, as `(path, adoption)` pairs.
fn adoptions(events: &[Event]) -> Vec<(String, Adoption)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::LibraryAdoption { path, adoption } => Some((path.clone(), adoption.clone())),
            _ => None,
        })
        .collect()
}

/// The closing totals of an adoption run, as `(adopted, orphans)`.
fn adopted_totals(events: &[Event]) -> (usize, usize) {
    match events.last() {
        Some(Event::LibraryAdopted {
            adopted, orphans, ..
        }) => (*adopted, *orphans),
        other => panic!("an adoption run must end with its totals: got {other:?}"),
    }
}

/// design D6: every orphan the walk found before the run gets exactly
/// one `library-adoption` event, and the orphans the totals leave equal
/// the adoption events that did not record one. `orphans_before` is
/// `library::survey(root).orphans.len()`, taken before `adopt_over`
/// runs. An addition to an existing test's assertions, never a
/// replacement of them.
fn assert_every_orphan_is_accounted_for(orphans_before: usize, events: &[Event]) {
    let adoption_events = events
        .iter()
        .filter(|event| matches!(event, Event::LibraryAdoption { .. }))
        .count();
    assert_eq!(
        adoption_events, orphans_before,
        "every orphan the run reached must get exactly one adoption event: got {events:?}"
    );
    let left = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::LibraryAdoption { adoption, .. }
                    if !matches!(adoption, Adoption::Recorded { .. })
            )
        })
        .count();
    assert_eq!(
        adopted_totals(events).1,
        left,
        "the orphans left must equal the adoption events that did not record one: \
         got {events:?}"
    );
}

/// Run `borax adopt` over the library at `root`, answering from
/// `index`, with a document reader that counts every open and sources
/// that panic when asked anything — so a run that succeeds is a run
/// that neither extracted nor queried.
fn adopt_over(root: &Path, index: &ContentIndex<MemoryCache>) -> Vec<Event> {
    let documents = CountingDocuments::new();
    let crossref = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&crossref];
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.to_path_buf()),
        state_root: None,
    };

    let events = events_for(
        &Command::adopt(Some(root.to_path_buf())),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert_eq!(documents.open_count(), 0, "adopt must open no document");
    assert!(
        filesystem.renames().is_empty(),
        "adopt must rename nothing: got {:?}",
        filesystem.renames()
    );
    events
}

/// task 8.1, scenario "A library borax has seen before": each orphan
/// the content index answers for gains an artifact record carrying its
/// hash, library-relative path, size and modification time, linked to
/// an item holding the cached record. An item the library already holds
/// for one of that record's identifiers is reused — whether it was
/// there before the run or minted earlier in it — rather than minted
/// again.
#[test]
fn adopt_records_each_orphan_the_content_index_answers_for() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let first = write_real_file(&root, "first.pdf", b"task-8.1 first");
    let second = write_real_file(&root, "sub/second.pdf", b"task-8.1 second");
    // Another file of the first one's work: different bytes, same DOI.
    let copy = write_real_file(&root, "sub/deeper/copy.pdf", b"task-8.1 copy");

    let held = Item {
        id: lib_item_id(G7_UUID_A),
        record: record_by("Held", 2019, "10.1000/task-8.1-held"),
    };
    write_lib_item(&root, "held2019", &held);

    let index = ContentIndex::new(MemoryCache::new());
    let cached = record_by("Adopted", 2020, "10.1000/task-8.1");
    index.put(&hash_bytes(b"task-8.1 first"), &cached);
    index.put(
        &hash_bytes(b"task-8.1 second"),
        &record_by("Held", 2019, "10.1000/task-8.1-held"),
    );
    index.put(&hash_bytes(b"task-8.1 copy"), &cached);
    let untouched = outside_the_stores(&root);
    let orphans_before = library::survey(&root).orphans.len();

    let events = adopt_over(&root, &index);

    assert_every_orphan_is_accounted_for(orphans_before, &events);
    assert_eq!(adopted_totals(&events), (3, 0), "got {events:?}");
    let reported: Vec<String> = adoptions(&events)
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(
        reported,
        vec!["first.pdf", "sub/deeper/copy.pdf", "sub/second.pdf"],
        "one adoption per orphan, in the walk's order"
    );

    let records = library::ArtifactStore::read(&root);
    assert_eq!(records.len(), 3);
    for (path, bytes) in [
        (&first, &b"task-8.1 first"[..]),
        (&second, &b"task-8.1 second"[..]),
        (&copy, &b"task-8.1 copy"[..]),
    ] {
        let relative = library::library_relative(&root, path).unwrap();
        let record = records.by_path(&relative).unwrap();
        let (size, modified_millis) = real_stat(path);
        assert_eq!(
            (record.size, record.modified_millis),
            (size, modified_millis)
        );
        assert_eq!(record.history.len(), 1, "one hash, the file's own");
        assert_eq!(record.history[0].hash, hash_bytes(bytes));
        assert_eq!(record.history[0].run, LibraryRunId::new(fixed_now()));
        assert_eq!(record.history[0].tool_version, env!("CARGO_PKG_VERSION"));
    }

    let items = library::ItemStore::read(&root);
    assert_eq!(
        items.len(),
        2,
        "the held item reused, one minted for the other work and reused for its copy"
    );
    let link = |path: &Path| {
        records
            .by_path(&library::library_relative(&root, path).unwrap())
            .unwrap()
            .item
            .clone()
            .unwrap()
    };
    assert_eq!(link(&second), held.id, "the item already held is reused");
    assert_eq!(
        link(&first),
        link(&copy),
        "two artifacts of one work name one item"
    );
    let minted = items.by_id(&link(&first)).unwrap();
    assert_eq!(minted.record, cached, "the item holds the cached record");

    assert_eq!(
        outside_the_stores(&root),
        untouched,
        "nothing outside the two stores may be written, moved or deleted"
    );
}

/// task 8.2, scenario "What adoption leaves alone": an artifact the
/// index cannot answer for is still an orphan and counted as one; an
/// artifact that already has a record is byte-identical afterwards,
/// item link included, even though the index answers for it with a
/// different work; and a second run writes no library state.
#[test]
fn adopt_leaves_the_unknown_and_the_recorded_alone_and_is_idempotent() {
    let library = real_library();
    let root = library.path().to_path_buf();
    write_real_file(&root, "known.pdf", b"task-8.2 known");
    write_real_file(&root, "unknown.pdf", b"task-8.2 unknown");
    let recorded = write_real_file(&root, "recorded.pdf", b"task-8.2 recorded");

    let item = Item {
        id: lib_item_id(G7_UUID_A),
        record: record_by("Recorded", 2018, "10.1000/task-8.2-recorded"),
    };
    write_lib_item(&root, "recorded2018", &item);
    let (size, modified_millis) = real_stat(&recorded);
    write_lib_artifact_record(
        &root,
        &ArtifactRecord {
            id: lib_artifact_id(G7_UUID_B),
            item: Some(item.id.clone()),
            path: "recorded.pdf".to_string(),
            size,
            modified_millis,
            history: vec![hash_entry_for(hash_bytes(b"task-8.2 recorded"), "run-0")],
        },
    );
    let record_file = root
        .join(STATE_DIR)
        .join(ARTIFACT_STORE)
        .join(format!("{G7_UUID_B}.toml"));
    let record_bytes = fs::read(&record_file).unwrap();

    let index = ContentIndex::new(MemoryCache::new());
    index.put(
        &hash_bytes(b"task-8.2 known"),
        &record_by("Known", 2021, "10.1000/task-8.2-known"),
    );
    // What adopting the recorded artifact would take: a different work.
    index.put(
        &hash_bytes(b"task-8.2 recorded"),
        &record_by("Other", 2022, "10.1000/task-8.2-other"),
    );

    let orphans_before = library::survey(&root).orphans.len();
    let events = adopt_over(&root, &index);

    assert_every_orphan_is_accounted_for(orphans_before, &events);
    assert_eq!(
        adopted_totals(&events),
        (1, 1),
        "known adopted, unknown still an orphan: got {events:?}"
    );
    let reported = adoptions(&events);
    assert!(
        reported
            .iter()
            .any(|(path, adoption)| path == "unknown.pdf" && adoption == &Adoption::Unindexed),
        "the content index holds nothing for `unknown.pdf`, so it must be reported \
         `unindexed`, not left out: got {reported:?}"
    );
    assert!(
        reported
            .iter()
            .any(|(path, adoption)| path == "known.pdf"
                && matches!(adoption, Adoption::Recorded { .. })),
        "got {reported:?}"
    );
    assert_eq!(
        fs::read(&record_file).unwrap(),
        record_bytes,
        "a recorded artifact's record is left byte-identical"
    );
    let survey = library::survey(&root);
    assert_eq!(
        survey.orphans,
        vec![root.join("unknown.pdf")],
        "the unknown artifact is still an orphan"
    );
    assert_eq!(library::ItemStore::read(&root).len(), 2);

    let before = snapshot(&root);
    let stamps: Vec<(PathBuf, (u64, i64))> = before
        .keys()
        .map(|path| (path.clone(), real_stat(path)))
        .collect();

    let orphans_before_again = library::survey(&root).orphans.len();
    let again = adopt_over(&root, &index);

    assert_every_orphan_is_accounted_for(orphans_before_again, &again);
    assert_eq!(adopted_totals(&again), (0, 1), "got {again:?}");
    assert_eq!(
        adoptions(&again),
        vec![("unknown.pdf".to_string(), Adoption::Unindexed)],
        "the unknown orphan is reported again, every run, since it is still \
         an orphan: got {:?}",
        adoptions(&again)
    );
    assert_eq!(snapshot(&root), before, "a second run writes nothing");
    let restamped: Vec<(PathBuf, (u64, i64))> = before
        .keys()
        .map(|path| (path.clone(), real_stat(path)))
        .collect();
    assert_eq!(restamped, stamps, "not even rewritten with the same bytes");
}

/// task 8.2a, scenario "A recorded artifact moved out of band is not
/// adopted": an orphan whose bytes a record's history already holds is
/// that record's artifact moved, so adopt reports it held and writes
/// nothing for it, and the reconcile after repairs the existing record.
/// A second orphan carrying the bytes an earlier orphan of the same run
/// was adopted with is held by the record just written.
#[test]
fn adopt_holds_an_orphan_whose_bytes_a_record_already_holds() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let item = Item {
        id: lib_item_id(G7_UUID_A),
        record: record_by("Moved", 2017, "10.1000/task-8.2a-moved"),
    };
    write_lib_item(&root, "moved2017", &item);
    let recorded = write_real_file(&root, "before/moved.pdf", b"task-8.2a moved");
    let (size, modified_millis) = real_stat(&recorded);
    write_lib_artifact_record(
        &root,
        &ArtifactRecord {
            id: lib_artifact_id(G7_UUID_B),
            item: Some(item.id.clone()),
            path: "before/moved.pdf".to_string(),
            size,
            modified_millis,
            history: vec![hash_entry_for(hash_bytes(b"task-8.2a moved"), "run-0")],
        },
    );
    // A file manager's move: the record still names the old path.
    fs::create_dir_all(root.join("after")).unwrap();
    fs::rename(&recorded, root.join("after/moved.pdf")).unwrap();
    write_real_file(&root, "twin-a.pdf", b"task-8.2a twin");
    write_real_file(&root, "twin-b.pdf", b"task-8.2a twin");

    let index = ContentIndex::new(MemoryCache::new());
    index.put(
        &hash_bytes(b"task-8.2a moved"),
        &record_by("Other", 2022, "10.1000/task-8.2a-other"),
    );
    index.put(
        &hash_bytes(b"task-8.2a twin"),
        &record_by("Twin", 2023, "10.1000/task-8.2a-twin"),
    );
    let orphans_before = library::survey(&root).orphans.len();

    let events = adopt_over(&root, &index);

    assert_every_orphan_is_accounted_for(orphans_before, &events);
    assert_eq!(adopted_totals(&events), (1, 2), "got {events:?}");
    let reported = adoptions(&events);
    assert_eq!(
        reported[0],
        (
            "after/moved.pdf".to_string(),
            Adoption::Held {
                id: G7_UUID_B.to_string()
            }
        ),
        "got {reported:?}"
    );
    let Adoption::Recorded { id: twin, .. } = &reported[1].1 else {
        panic!("the first twin is adopted: got {reported:?}");
    };
    assert_eq!(reported[1].0, "twin-a.pdf");
    assert_eq!(
        reported[2],
        (
            "twin-b.pdf".to_string(),
            Adoption::Held { id: twin.clone() }
        ),
        "the second twin is held by the record the run just wrote"
    );
    assert_eq!(library::ArtifactStore::read(&root).len(), 2);
    assert_eq!(
        library::ItemStore::read(&root).len(),
        2,
        "no item is minted for the moved artifact's cached record"
    );

    library::reconcile(
        &root,
        false,
        LibraryRunId::new("run-reconcile"),
        "2026-01-02T00:00:00Z",
        "0.6.0-test",
    );
    let records = library::ArtifactStore::read(&root);
    let repaired = records.by_id(&lib_artifact_id(G7_UUID_B)).unwrap();
    assert_eq!(repaired.path, "after/moved.pdf");
    assert_eq!(repaired.item, Some(item.id.clone()));
}

/// Scenario "Every orphan is accounted for": three orphans — one the
/// index answers for, one whose bytes an existing record holds, and one
/// the index holds nothing for. In walk order the adoptions are
/// `Recorded`, `Held` and `Unindexed`, one each, and `adopted_totals`
/// is `(1, 2)`.
#[test]
fn adopt_every_orphan_is_accounted_for() {
    let library = real_library();
    let root = library.path().to_path_buf();
    write_real_file(&root, "a-recorded.pdf", b"task-5.1 recorded");
    let held_path = write_real_file(&root, "b-held.pdf", b"task-5.1 held");
    write_real_file(&root, "c-unindexed.pdf", b"task-5.1 unindexed");
    let (size, modified_millis) = real_stat(&held_path);
    write_lib_artifact_record(
        &root,
        &ArtifactRecord {
            id: lib_artifact_id(G7_UUID_A),
            item: None,
            path: "elsewhere/b-held.pdf".to_string(),
            size,
            modified_millis,
            history: vec![hash_entry_for(hash_bytes(b"task-5.1 held"), "run-0")],
        },
    );

    let index = ContentIndex::new(MemoryCache::new());
    index.put(
        &hash_bytes(b"task-5.1 recorded"),
        &record_by("Recorded", 2020, "10.1000/task-5.1-recorded"),
    );
    let orphans_before = library::survey(&root).orphans.len();

    let events = adopt_over(&root, &index);

    assert_every_orphan_is_accounted_for(orphans_before, &events);
    assert_eq!(adopted_totals(&events), (1, 2), "got {events:?}");
    let reported = adoptions(&events);
    assert_eq!(reported.len(), 3, "got {reported:?}");
    assert!(
        matches!(
            reported.iter().find(|(path, _)| path == "a-recorded.pdf"),
            Some((_, Adoption::Recorded { .. }))
        ),
        "got {reported:?}"
    );
    assert!(
        matches!(
            reported.iter().find(|(path, _)| path == "b-held.pdf"),
            Some((_, Adoption::Held { .. }))
        ),
        "got {reported:?}"
    );
    assert_eq!(
        reported.iter().find(|(path, _)| path == "c-unindexed.pdf"),
        Some(&("c-unindexed.pdf".to_string(), Adoption::Unindexed)),
        "got {reported:?}"
    );
}

/// task 8.3, scenario "Adoption reads neither the sidecars nor the old
/// ledger": over a library whose file has a lossless sidecar and which
/// holds a ledger naming that file's hash, with the content index
/// empty, nothing is adopted, every artifact stays an orphan, and both
/// files are byte-identical afterwards.
#[test]
fn adopt_reads_neither_a_sidecar_nor_the_retired_ledger() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "Sidecar2020.pdf", b"task-8.3 bytes");
    let record = record_by("Sidecar", 2020, "10.1000/task-8.3");
    let sidecar = sidecar_path(&path);
    fs::write(
        &sidecar,
        borax_core::bib_output::sidecar(&record, "Sidecar2020"),
    )
    .unwrap();
    let ledger = root.join(STATE_DIR).join("ledger.jsonl");
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    let ledger_line = format!(
        "{{\"schema\":1,\"path\":\"Sidecar2020.pdf\",\"hash\":\"{}\",\"run\":\"run-0\"}}\n",
        hash_bytes(b"task-8.3 bytes")
    );
    fs::write(&ledger, &ledger_line).unwrap();
    let sidecar_bytes = fs::read(&sidecar).unwrap();
    let orphans_before = library::survey(&root).orphans.len();

    let events = adopt_over(&root, &ContentIndex::new(MemoryCache::new()));

    assert_every_orphan_is_accounted_for(orphans_before, &events);
    assert_eq!(adopted_totals(&events), (0, 1), "got {events:?}");
    assert!(library::ArtifactStore::read(&root).is_empty());
    assert!(library::ItemStore::read(&root).is_empty());
    assert_eq!(library::survey(&root).orphans, vec![path]);
    assert_eq!(fs::read(&sidecar).unwrap(), sidecar_bytes);
    assert_eq!(fs::read_to_string(&ledger).unwrap(), ledger_line);
}

/// task 8.4, scenario "Adoption after the cache is cleared": the
/// content index is a cache, so after `borax cache --clear` adoption
/// records nothing and reports every artifact an orphan — and the run
/// succeeds, since an empty cache is not a failure.
#[test]
fn adopt_after_the_cache_is_cleared_adopts_nothing_and_succeeds() {
    use borax_sources::store::FileCache;

    let library = real_library();
    let root = library.path().to_path_buf();
    write_real_file(&root, "one.pdf", b"task-8.4 one");
    write_real_file(&root, "two.pdf", b"task-8.4 two");
    let cache_dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(cache_dir.path()));
    index.put(
        &hash_bytes(b"task-8.4 one"),
        &record_by("One", 2020, "10.1000/task-8.4-one"),
    );
    index.put(
        &hash_bytes(b"task-8.4 two"),
        &record_by("Two", 2020, "10.1000/task-8.4-two"),
    );

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(cache_dir.path().to_path_buf()),
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let cleared = events_for(
        &Command::cache(true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();
    assert!(
        matches!(cleared.as_slice(), [Event::CacheCleared { entries: 2, .. }]),
        "setup: the clear must remove both index entries: got {cleared:?}"
    );

    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };
    let outcome = dispatch(
        &cli(Command::adopt(Some(root.clone())), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "stderr: {err:?}");
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("\"event\":\"library-adopted\"")
            && out.contains("\"adopted\":0")
            && out.contains("\"orphans\":2"),
        "got {out:?}"
    );
    for path in ["one.pdf", "two.pdf"] {
        assert!(
            out.lines().any(|line| {
                line.contains("\"event\":\"library-adoption\"")
                    && line.contains(&format!("\"path\":\"{path}\""))
                    && line.contains("\"kind\":\"unindexed\"")
            }),
            "an emptied cache must still report {path} as `unindexed`, not just \
             left out: got {out:?}"
        );
    }
    assert!(library::ArtifactStore::read(&root).is_empty());
    assert!(library::ItemStore::read(&root).is_empty());
}

/// New ("Adoption after the cache is cleared does not make every orphan
/// `unindexed`", D6): a readable orphan no record holds is `unindexed`,
/// an orphan whose bytes an existing record holds is `held`, and (on
/// Unix) an orphan that cannot be read is `unreadable` — clearing the
/// cache changes none of that.
#[test]
fn adopt_after_the_cache_is_cleared_still_tells_unindexed_held_and_unreadable_apart() {
    use borax_sources::store::FileCache;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let library = real_library();
    let root = library.path().to_path_buf();
    write_real_file(&root, "a-unindexed.pdf", b"task-5.1b unindexed");
    let held_path = write_real_file(&root, "b-held.pdf", b"task-5.1b held");
    let (size, modified_millis) = real_stat(&held_path);
    write_lib_artifact_record(
        &root,
        &ArtifactRecord {
            id: lib_artifact_id(G7_UUID_A),
            item: None,
            path: "elsewhere/b-held.pdf".to_string(),
            size,
            modified_millis,
            history: vec![hash_entry_for(hash_bytes(b"task-5.1b held"), "run-0")],
        },
    );
    #[cfg(unix)]
    let unreadable_path = write_real_file(&root, "c-unreadable.pdf", b"task-5.1b unreadable");

    let cache_dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(cache_dir.path()));
    index.put(
        &hash_bytes(b"task-5.1b unindexed"),
        &record_by("Unindexed", 2020, "10.1000/task-5.1b-unindexed"),
    );

    #[cfg(unix)]
    {
        fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&unreadable_path).is_ok() {
            fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o644)).unwrap();
            eprintln!(
                "skipping adopt_after_the_cache_is_cleared_still_tells_unindexed_held_and_unreadable_apart: \
                 file permissions were not enforced (running as root?)"
            );
            return;
        }
    }

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(cache_dir.path().to_path_buf()),
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    // Clear the cache the index was just primed through.
    events_for(
        &Command::cache(true),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };
    dispatch(
        &cli(Command::adopt(Some(root.clone())), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    #[cfg(unix)]
    fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o644)).unwrap();

    let out = String::from_utf8(out).unwrap();
    let adoption_kind = |path: &str| -> Option<String> {
        out.lines().find_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            if value["event"] == "library-adoption" && value["path"] == path {
                Some(value["adoption"]["kind"].as_str().unwrap().to_string())
            } else {
                None
            }
        })
    };

    assert_eq!(
        adoption_kind("a-unindexed.pdf"),
        Some("unindexed".to_string()),
        "got {out:?}"
    );
    assert_eq!(
        adoption_kind("b-held.pdf"),
        Some("held".to_string()),
        "got {out:?}"
    );
    #[cfg(unix)]
    assert_eq!(
        adoption_kind("c-unreadable.pdf"),
        Some("unreadable".to_string()),
        "clearing the cache must not turn an unreadable orphan into `unindexed`: got {out:?}"
    );
}

/// Adoption writes library state, and a directory nobody marked holds
/// none: `borax adopt` outside any library is refused before it starts,
/// and leaves the directory exactly as it was.
#[test]
fn adopt_outside_any_library_is_refused_and_writes_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_real_file(&root, "unmarked.pdf", b"adopt unmarked");
    let index = ContentIndex::new(MemoryCache::new());
    index.put(
        &hash_bytes(b"adopt unmarked"),
        &record_by("Unmarked", 2020, "10.1000/adopt-unmarked"),
    );
    let before = snapshot(&root);

    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let refused = events_for(
        &Command::adopt(Some(root.clone())),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
    );

    assert!(
        matches!(&refused, Err(diagnostic) if diagnostic.level == Level::Error),
        "got {refused:?}"
    );
    assert_eq!(snapshot(&root), before);
}

// ---------------------------------------------------------------------
// group 9.5: the checks answer from the library as the run has left it
// so far
//
// The account `rename_events` builds is checked exactly once, at the
// start of the run — a snapshot that stays one. The fix task 9.6 owns
// is making that account learn each admission (and, in a preview, each
// plan) as the run makes it, so a file reached later in the same run is
// checked against what the run has already done rather than against
// what the library held before it started.
// ---------------------------------------------------------------------

const G95_UUID_A: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdefc";

/// task 9.5, scenario "A byte-identical pair in one run": an applying
/// batch run over two byte-identical files, neither recorded before the
/// run, records the first and reports the second a content duplicate
/// naming the path the first now has. `b.pdf`'s document would fail
/// loudly if opened, so a wrongly-resolved second file shows up as a
/// distinct failure rather than a silent admission: the content check
/// SHALL run before resolution, and a duplicate reached only because the
/// account learned of the first file's admission must still be found
/// before any source or extractor is touched for the second.
#[test]
fn a_byte_identical_pair_neither_recorded_is_one_admission_and_one_content_duplicate() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"task-9.5-pair bytes";
    let a = write_real_file(&root, "a.pdf", bytes);
    let b = write_real_file(&root, "b.pdf", bytes);
    let hash = hash_bytes(bytes);
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/task-9.5-pair"),
        )
        .with_open_error(
            &b,
            hash.clone(),
            ExtractionError::Unreadable {
                message: "must never be opened: the content check runs before resolution"
                    .to_string(),
            },
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Pair", 2024, "10.1000/task-9.5-pair")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let a_target = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { path, target, .. } if path == &a => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the first of the pair must be renamed: got {events:?}"));

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Content,
                    existing_path,
                },
                ..
            } if path == &b && existing_path == &a_target
        )),
        "the second of a byte-identical pair reached in one run must be a content \
         duplicate naming the first's new path: got {events:?}"
    );
    assert!(b.exists(), "a content duplicate must never be moved");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Resolved { path, .. } if path == &b)),
        "a content duplicate is found before resolution, so it is never resolved: \
         got {events:?}"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "the pair must leave one artifact record, not two"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "the pair must mint one item, not two"
    );
}

/// task 9.5, scenario "A byte-identical pair in one run": a preview of
/// the same pair reports the same outcome — a plan for the first and a
/// content duplicate for the second naming the path the first would now
/// have — and writes nothing to either store.
#[test]
fn a_preview_of_a_byte_identical_pair_reports_the_same_duplicate_and_writes_nothing() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"task-9.5-pair-preview bytes";
    let a = write_real_file(&root, "a.pdf", bytes);
    let b = write_real_file(&root, "b.pdf", bytes);
    let hash = hash_bytes(bytes);
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/task-9.5-pair-preview"),
        )
        .with_open_error(
            &b,
            hash.clone(),
            ExtractionError::Unreadable {
                message: "must never be opened: the content check runs before resolution"
                    .to_string(),
            },
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Preview", 2024, "10.1000/task-9.5-pair-preview")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], false),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let a_target = events
        .iter()
        .find_map(|event| match event {
            Event::Planned { path, target } if path == &a => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the first of the pair must be planned: got {events:?}"));

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Content,
                    existing_path,
                },
                ..
            } if path == &b && existing_path == &a_target
        )),
        "a preview must learn what it would admit the same way an applying run does: \
         got {events:?}"
    );
    assert!(a.exists(), "a preview must move nothing");
    assert!(b.exists(), "a preview must move nothing");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Resolved { path, .. } if path == &b)),
        "a content duplicate is found before resolution, so it is never resolved: \
         got {events:?}"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        0,
        "a preview must write nothing to the artifact store"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        0,
        "a preview must write nothing to the item store"
    );
}

/// task 9.5, scenario "A copy of an artifact the run has just moved":
/// an applying run moves a recorded artifact and then reaches a
/// byte-identical copy of it. The copy is reported a content duplicate
/// naming the moved artifact's new path, not its stale recorded one —
/// and the run does not report that the library has paths to reconcile,
/// since a record whose artifact this run moved is not a stale path for
/// having moved.
#[test]
fn a_copy_of_an_artifact_the_run_has_just_moved_names_its_new_path() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"task-9.5-moved-copy bytes";
    let old = write_real_file(&root, "old.pdf", bytes);
    let hash = hash_bytes(bytes);
    let record = record_by("Moved", 2024, "10.1000/task-9.5-moved-copy");
    let item = Item {
        id: lib_item_id(G95_UUID_A),
        record: record.clone(),
    };
    write_lib_item(&root, "moved2024", &item);
    let (size, modified_millis) = real_stat(&old);
    let held = ArtifactRecord {
        id: lib_artifact_id(G95_UUID_A),
        item: Some(item.id.clone()),
        path: "old.pdf".to_string(),
        size,
        modified_millis,
        history: vec![hash_entry_for(hash.clone(), "run-0")],
    };
    write_lib_artifact_record(&root, &held);

    let copy = write_real_file(&root, "copy.pdf", bytes);
    let documents = FakeDocuments::new()
        .with_file(
            &old,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/task-9.5-moved-copy"),
        )
        .with_open_error(
            &copy,
            hash.clone(),
            ExtractionError::Unreadable {
                message: "must never be opened: matched as a content duplicate before \
                          resolution"
                    .to_string(),
            },
        );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![old.clone(), copy.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let old_target = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { path, target, .. } if path == &old => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the recorded artifact must be renamed: got {events:?}"));

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Content,
                    existing_path,
                },
                ..
            } if path == &copy && existing_path == &old_target
        )),
        "the copy must name the artifact's new path, not the stale one its record held \
         when the run began: got {events:?}"
    );
    assert!(copy.exists(), "a content duplicate must never be moved");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Resolved { path, .. } if path == &copy)),
        "a content duplicate is found before resolution, so it is never resolved: \
         got {events:?}"
    );
    let updated = library::ArtifactStore::read(&root)
        .by_id(&held.id)
        .unwrap_or_else(|| panic!("the moved artifact's record must still be there"))
        .clone();
    assert_eq!(
        updated.path,
        library::library_relative(&root, &old_target).unwrap(),
        "the record must name where the run put the file, not where it used to be"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "the copy must write no second record"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "the copy must mint no second item"
    );
}

/// task 9.5, scenario "A copy of an artifact the run has just moved":
/// the run does not report that the library has paths to reconcile. A
/// record whose artifact this run moved is not a stale path for having
/// moved — the false "paths to reconcile" warning is exactly the defect
/// the 149-PDF run turned up.
#[test]
fn a_copy_of_an_artifact_the_run_has_just_moved_warns_of_nothing_to_reconcile() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"task-9.5-moved-copy-warn bytes";
    let old = write_real_file(&root, "old.pdf", bytes);
    let hash = hash_bytes(bytes);
    let record = record_by("Warn", 2024, "10.1000/task-9.5-moved-copy-warn");
    let item = Item {
        id: lib_item_id(G95_UUID_A),
        record: record.clone(),
    };
    write_lib_item(&root, "warn2024", &item);
    let (size, modified_millis) = real_stat(&old);
    let held = ArtifactRecord {
        id: lib_artifact_id(G95_UUID_A),
        item: Some(item.id.clone()),
        path: "old.pdf".to_string(),
        size,
        modified_millis,
        history: vec![hash_entry_for(hash.clone(), "run-0")],
    };
    write_lib_artifact_record(&root, &held);

    let copy = write_real_file(&root, "copy.pdf", bytes);
    let documents = FakeDocuments::new()
        .with_file(
            &old,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/task-9.5-moved-copy-warn"),
        )
        .with_open_error(
            &copy,
            hash.clone(),
            ExtractionError::Unreadable {
                message: "must never be opened: matched as a content duplicate before \
                          resolution"
                    .to_string(),
            },
        );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];

    let (_, err) = stderr_of_rename(&root, vec![old, copy], &documents, &sources);

    assert!(
        err.is_empty(),
        "a record this run moved must never be reported as a path to reconcile: got {err:?}"
    );
}

/// task 9.5, scenario "Two files of one work in one batch": a batch
/// run reaches two different files resolving to one DOI that no item
/// carried before the run. The first is recorded against a new item and
/// the second is reported a work duplicate naming the first's new path.
#[test]
fn two_files_of_one_work_in_one_batch_are_one_admission_and_one_work_duplicate() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let first = write_real_file(&root, "first.pdf", b"task-9.5-work first bytes");
    let second = write_real_file(&root, "second.pdf", b"task-9.5-work second bytes");
    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_bytes(b"task-9.5-work first bytes"),
            pdf_with_embedded_doi("10.1000/task-9.5-work"),
        )
        .with_file(
            &second,
            hash_bytes(b"task-9.5-work second bytes"),
            pdf_with_embedded_doi("10.1000/task-9.5-work"),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Work", 2024, "10.1000/task-9.5-work")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![first.clone(), second.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let first_target = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { path, target, .. } if path == &first => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the first file must be renamed: got {events:?}"));

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                path,
                reason: SkipReason::Duplicate {
                    reason: DuplicateReason::Work,
                    existing_path,
                },
                ..
            } if path == &second && existing_path == &first_target
        )),
        "the second file of one work reached in one batch must be a work duplicate \
         naming the first's new path: got {events:?}"
    );
    assert!(second.exists(), "a work duplicate must never be moved");
    assert_eq!(
        library::ItemStore::read(&root).len(),
        1,
        "one work in one batch must mint one item, not two"
    );
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        1,
        "one work in one batch must leave one artifact record, not two"
    );
}

/// task 9.5, "Plus: under `--no-record`...": with the checks off, the
/// same byte-identical pair from
/// [`a_byte_identical_pair_neither_recorded_is_one_admission_and_one_content_duplicate`]
/// is moved in full — one to a collision-suffixed name, since both
/// resolve to the same template target — neither reported a duplicate,
/// and nothing is written to either store. This pins that the fix does
/// not turn the checks on under `--no-record`; it may already pass.
#[test]
fn no_record_moves_both_of_a_byte_identical_pair_and_checks_neither() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let bytes = b"task-9.5-no-record bytes";
    let a = write_real_file(&root, "a.pdf", bytes);
    let b = write_real_file(&root, "b.pdf", bytes);
    let hash = hash_bytes(bytes);
    let documents = FakeDocuments::new()
        .with_file(
            &a,
            hash.clone(),
            pdf_with_embedded_doi("10.1000/task-9.5-no-record"),
        )
        .with_file(
            &b,
            hash,
            pdf_with_embedded_doi("10.1000/task-9.5-no-record"),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("NoRecord", 2024, "10.1000/task-9.5-no-record")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.record = Some(false);
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &RealFilesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![a.clone(), b.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let a_target = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { path, target, .. } if path == &a => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the first file must be renamed: got {events:?}"));
    let b_target = events
        .iter()
        .find_map(|event| match event {
            Event::Renamed { path, target, .. } if path == &b => Some(target.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("--no-record must move both files of the pair, checking neither: got {events:?}")
        });

    assert_ne!(
        a_target, b_target,
        "the second must take a collision-suffixed name, distinct from the first's: \
         got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Skipped { .. })),
        "--no-record must skip nothing: got {events:?}"
    );
    assert!(a_target.exists());
    assert!(b_target.exists());
    assert_eq!(
        library::ArtifactStore::read(&root).len(),
        0,
        "--no-record must write no records"
    );
    assert_eq!(
        library::ItemStore::read(&root).len(),
        0,
        "--no-record must write no items"
    );
}

// ---------------------------------------------------------------------
// dispatch, human mode: each command's summary shape — design D4,
// tasks 2.1, 2.3, 2.4, 2.5
// ---------------------------------------------------------------------

/// design D4: `status` is `Silent`. It ends on the `library-status`
/// report and never on a `resolved,` line.
#[test]
fn status_human_mode_ends_on_the_library_status_line_with_no_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::status(Some(root.clone()), false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_last = format!(
        "{}: 0 artifacts, 0 items, 0 records, 0 orphans",
        root.display()
    );

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D4: `status --identify` is `Silent` too, the same as plain
/// `status`.
#[test]
fn status_identify_human_mode_ends_on_the_library_status_line_with_no_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::status(Some(root.clone()), true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_last = format!(
        "{}: 0 artifacts, 0 items, 0 records, 0 orphans, 0 identifiable",
        root.display()
    );

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D4: `validate` is `Validation`. It ends on the
/// `library-validated` line, which already carries the findings count,
/// and a finding still decides `Outcome::Partial`.
#[test]
fn validate_human_mode_ends_on_the_library_validated_line_and_is_partial() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_dangling_artifact_record(&root);

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_last = format!(
        "{}: 1 findings, 0 orphans, 1 missing, 0 unlinked",
        root.display()
    );

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

/// design D4: `reconcile` is `Silent`. It ends on the
/// `library-reconciled` totals and never on a `resolved,` line.
#[test]
fn reconcile_human_mode_ends_on_its_own_totals_line_with_no_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::reconcile(Some(root.clone()), false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_last = format!(
        "{}: 0 records, 0 confirmed, 0 repaired, 0 changed, 0 ambiguous, 0 missing, 0 hashed",
        root.display()
    );

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D4: `adopt` is `Silent`, so it ends on its own totals line.
/// That it stays silent when a lookup misses follows from `adopt`
/// mapping to `Silent` and `Silent` ignoring `unmatched`, both pinned
/// by unit tests.
#[test]
fn adopt_human_mode_ends_on_its_own_totals_line_with_no_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::adopt(Some(root.clone())), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_last = format!("{}: 0 adopted, 0 orphans", root.display());

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D7: in human mode, a library with one unindexed orphan prints
/// its D7 line, then the `library-adopted` line as the last line, with
/// no summary line.
#[test]
fn adopt_human_mode_lists_an_unindexed_orphan_before_the_totals_line() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::write(root.join(".borax.toml"), b"").unwrap();
    fs::write(root.join("new.pdf"), b"unindexed bytes").unwrap();
    let documents = CountingDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::adopt(Some(root.clone())), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "new.pdf: the content index holds no record of its bytes, so it is \
             still an orphan",
            &format!("{}: 0 adopted, 1 orphans", root.display()),
        ],
        "got {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D4: `config` is `Silent`. It ends on its own last
/// `config-setting` line, matching what `human_line` renders for it,
/// and never on a `resolved,` line.
#[test]
fn config_human_mode_ends_on_its_own_setting_line_with_no_summary() {
    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::config(), false),
        &Configs::uniform(effective.clone()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_events = effective.events();
    let expected_last =
        human_line(expected_events.last().unwrap()).expect("a config setting always renders");

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D4: `cache` is `Silent`. It ends on its own `cache-status`
/// line and never on a `resolved,` line.
#[test]
fn cache_human_mode_ends_on_its_own_status_line_with_no_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    fs::create_dir_all(&root).unwrap();

    let effective = resolve(Vec::new()).unwrap();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(root.clone()),
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::cache(false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected_last = format!("{}: 0 entries, 0 bytes", root.display());

    assert_eq!(lines.last(), Some(&expected_last.as_str()), "got {lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("resolved,")),
        "got {lines:?}"
    );
}

/// design D4: `bib` shares `Resolution` with `resolve`. Its summary has
/// no `renamed` clause, whatever the counts.
#[test]
fn bib_human_mode_summary_has_no_renamed_clause() {
    let path = PathBuf::from("/lib/paper.pdf");
    let record = record_by("Smith", 2024, "10.1000/bib-human-summary");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("bib-human-summary"),
        pdf_with_embedded_doi("10.1000/bib-human-summary"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(false),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::bib(vec![path]), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.last(),
        Some(&"1 resolved, 0 skipped"),
        "got {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("renamed")),
        "got {lines:?}"
    );
}

/// An [`Effective`] declaring a `jcode` table over `path`, consulted by
/// the citation-key template rather than the filename one — the table
/// `bib` reads.
fn effective_looking_up_for_bib(path: &Path) -> Effective {
    effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]-[journal:lookup(\"jcode\")]".to_string(),
        )]));
        layer.tables = Some(BTreeMap::from([(
            "jcode".to_string(),
            TableDeclaration {
                path: path.to_path_buf(),
                key: KeyColumns::One("title".to_string()),
                value: "abbreviation".to_string(),
                values: ValueKindName::Text,
            },
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(false),
        });
    })
}

/// design D4: `bib`'s `Resolution` summary names an unmatched lookup the
/// same way `resolve`'s does — `bib` is the only other command
/// `external-tables`'s restated requirement names as counting misses in
/// its summary.
#[test]
fn bib_human_mode_summary_names_an_unmatched_lookup() {
    let directory = tempdir().unwrap();
    let table = directory.path().join("journals.tsv");
    fs::write(&table, JOURNALS).unwrap();

    let path = PathBuf::from("/lib/paper.pdf");
    let record = article_in("Journal of Unlisted Results", "10.1000/bib-unmatched");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("bib-unmatched"),
        pdf_with_embedded_doi("10.1000/bib-unmatched"),
    );
    let crossref = fake_source(SourceName::Crossref, Ok(record));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_looking_up_for_bib(&table);
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::bib(vec![path]), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.last(),
        Some(&"1 resolved, 0 skipped, 1 unmatched"),
        "got {lines:?}"
    );
}

/// design D4/D3a, the regression guard: a batch `rename` over one
/// already-named file still ends `1 resolved, 0 renamed, 0 skipped, 1
/// already named` — `Renaming`'s wording is unchanged by this change.
/// The existing interactive tests asserting `2 already named (not
/// shown)` and `2 not reached` are untouched and must stay green.
#[test]
fn batch_rename_over_one_already_named_file_keeps_the_renaming_summary_line() {
    let already_named = PathBuf::from("/lib/Smith2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &already_named,
        hash_for("regression-already-named"),
        pdf_with_embedded_doi("10.1000/regression-already-named"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/regression-already-named")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![already_named], false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.last(),
        Some(&"1 resolved, 0 renamed, 0 skipped, 1 already named"),
        "got {lines:?}"
    );
}

/// design D5: `status --json` is unaffected by the summary shape —
/// JSON always closes with `run-finished` carrying all seven counters
/// and schema 3, whatever the human rendering does.
#[test]
fn status_json_still_ends_with_run_finished_and_all_seven_counters() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::status(Some(root), false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();

    assert_eq!(last["event"], serde_json::Value::from("run-finished"));
    assert_eq!(last["schema"], serde_json::Value::from(SCHEMA));
    let counts = last["counts"].as_object().unwrap();
    let mut keys: Vec<&str> = counts.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "findings",
            "named",
            "renamed",
            "resolved",
            "skipped",
            "unmatched",
            "unreached",
        ],
        "got {last:?}"
    );
}

/// design D5: a human-mode `status` inside a library with the run log on
/// (the default) writes a log whose last line is the same
/// `run-finished` event, though stdout showed no summary.
#[test]
fn status_human_mode_run_log_still_ends_with_run_finished_though_stdout_showed_no_summary() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::status(Some(root.clone()), false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let stdout = String::from_utf8(out).unwrap();
    assert!(
        !stdout.lines().any(|line| line.contains("resolved,")),
        "stdout must show no summary: got {stdout:?}"
    );

    let runs_dir = root.join(STATE_DIR).join(RUNS_DIR);
    let mut logs: Vec<PathBuf> = fs::read_dir(&runs_dir)
        .unwrap_or_else(|error| panic!("expected a run log under {runs_dir:?}: {error}"))
        .map(|entry| entry.unwrap().path())
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 1, "expected exactly one run log: got {logs:?}");

    let contents = fs::read_to_string(&logs[0]).unwrap();
    let last: serde_json::Value = serde_json::from_str(contents.lines().last().unwrap()).unwrap();

    assert_eq!(last["event"], serde_json::Value::from("run-finished"));
}

// ---------------------------------------------------------------------
// consult-library-first, task 4.1: `resolve` reads the run's library
// (design D1, D3, D5, D7, D8)
// ---------------------------------------------------------------------

/// Writes an item carrying `record` under `root`, and an artifact
/// record linking to it at `relative`, holding `hash`. Hands back the
/// item's identity.
fn seed_tracked_file(root: &Path, relative: &str, hash: ContentHash, record: Record) -> ItemId {
    let item = Item {
        id: ItemId::from_uuid(uuid::Uuid::now_v7()),
        record,
    };
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join(format!("{}.toml", item.id)), item.to_toml()).unwrap();

    let artifact = ArtifactRecord {
        id: ArtifactId::from_uuid(uuid::Uuid::now_v7()),
        item: Some(item.id.clone()),
        path: relative.to_string(),
        size: 100,
        modified_millis: 0,
        history: vec![HashEntry {
            hash,
            run: LibraryRunId::new("earlier-run"),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            tool_version: "0.6.0-test".to_string(),
        }],
    };
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::write(
        records.join(format!("{}.toml", artifact.id)),
        artifact.to_toml(),
    )
    .unwrap();

    item.id
}

/// The review scenario (proposal "Why"): an item corrected after
/// admission is served over a stale content-index entry sharing the
/// file's hash. `resolve --json` must read the library first: the
/// corrected title, `tier: "library"`, `cached: false`, no claims, a
/// `library` object of kind `tracked`, schema 3 — and the index entry
/// left byte-identical.
#[test]
fn resolve_reports_the_librarys_corrected_item_over_a_stale_index_entry() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "klykov.pdf", b"klykov bytes");
    let hash = hash_bytes(b"klykov bytes");
    let mut corrected = record_by("Klykov", 2020, "10.1000/klykov");
    corrected.title = Some("REVIEW CORRECTION: Klykov et al.".to_string());
    seed_tracked_file(&root, "klykov.pdf", hash.clone(), corrected.clone());

    let stale = record_by("Klykov", 2020, "10.1000/klykov");
    let documents = FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    index.put(&hash, &stale);
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![path.clone()]),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let resolved = events
        .iter()
        .find(|event| matches!(event, Event::Resolved { .. }))
        .unwrap_or_else(|| panic!("expected a Resolved event: got {events:?}"));
    match resolved {
        Event::Resolved {
            record,
            tier,
            cached,
            claims,
            library,
            ..
        } => {
            assert_eq!(record.title, corrected.title);
            assert_eq!(tier.as_deref(), Some("library"));
            assert!(!cached);
            assert_eq!(claims, &Vec::new());
            assert!(matches!(library, Some(LibraryAnswer::Tracked { .. })));
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
    assert_eq!(
        index.get(&hash),
        Some(stale),
        "a library answer must leave the content index exactly as it was (design D6)"
    );
}

/// The same, under `--no-cache`: no source is asked (there is none to
/// ask, and none may be), and nothing is written to the index — the
/// library is not the cache `--no-cache` bypasses (design D7).
#[test]
fn resolve_no_cache_still_reads_the_library_and_writes_nothing_to_the_index() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "klykov.pdf", b"klykov bytes 2");
    let hash = hash_bytes(b"klykov bytes 2");
    let corrected = record_by("Klykov", 2020, "10.1000/klykov-2");
    seed_tracked_file(&root, "klykov.pdf", hash.clone(), corrected.clone());

    let documents = FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.network = Some(NetworkLayer {
            cache: Some(false),
            ..NetworkLayer::default()
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![path]),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let resolved = events
        .iter()
        .find(|event| matches!(event, Event::Resolved { .. }))
        .unwrap_or_else(|| panic!("expected a Resolved event: got {events:?}"));
    match resolved {
        Event::Resolved { record, tier, .. } => {
            assert_eq!(record.title, corrected.title);
            assert_eq!(tier.as_deref(), Some("library"));
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
    assert_eq!(
        index.get(&hash),
        None,
        "nothing must be written to the index"
    );
}

/// An untracked PDF with a content-index entry: served from the index
/// as today, with `library` naming the `untracked` kind rather than
/// `null` — it was consulted, and the library said it does not track
/// this file.
#[test]
fn resolve_reports_untracked_for_an_unrecorded_file_inside_a_library() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "unrecorded.pdf", b"unrecorded bytes");
    let hash = hash_bytes(b"unrecorded bytes");
    let cached_record = record_by("Doe", 2023, "10.1000/unrecorded");
    let documents = FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    index.put(&hash, &cached_record);
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![path]),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { .. }))
    {
        Some(Event::Resolved {
            cached, library, ..
        }) => {
            assert!(cached);
            assert_eq!(library, &Some(LibraryAnswer::Untracked));
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// `collection_root: None` — a run outside any library — leaves
/// `library: null` and every other field exactly as it is today.
#[test]
fn resolve_outside_any_library_reports_library_null() {
    let path = PathBuf::from("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("no-library-run"),
        pdf_with_embedded_doi("10.1000/no-library-run"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-library-run")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![path]),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { .. }))
    {
        Some(Event::Resolved { library, .. }) => assert_eq!(library, &None),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// A file under a nested `.borax.toml` is not consulted either:
/// `library: null` (design D8).
#[test]
fn resolve_under_a_nested_library_reports_library_null() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let nested = root.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join(".borax.toml"), b"").unwrap();
    let path = write_real_file(&root, "nested/paper.pdf", b"nested bytes");
    seed_tracked_file(
        &root,
        "nested/paper.pdf",
        hash_bytes(b"nested bytes"),
        record_by("Smith", 2024, "10.1000/nested"),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"nested bytes"),
        pdf_with_no_identifier(),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![path]),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { .. } | Event::Skipped { .. }))
    {
        Some(Event::Resolved { library, .. }) => assert_eq!(library, &None),
        Some(Event::Skipped { library, .. }) => assert_eq!(library, &None),
        other => panic!("expected a per-file event, got {other:?}"),
    }
}

/// One unparsable artifact-record file: the library's other tracked
/// files still resolve from their items, an unrecorded file reports
/// `unreadable-records`, and stderr carries design D5's one-record
/// warning exactly once.
#[test]
fn resolve_with_one_unreadable_record_warns_once_and_still_tracks_the_rest() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let good_path = write_real_file(&root, "good.pdf", b"good bytes");
    seed_tracked_file(
        &root,
        "good.pdf",
        hash_bytes(b"good bytes"),
        record_by("Smith", 2024, "10.1000/good"),
    );
    let unrecorded_path = write_real_file(&root, "unrecorded.pdf", b"unrecorded bytes 2");
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::write(records.join("broken.toml"), "not valid toml {{{").unwrap();

    let documents = FakeDocuments::new()
        .with_file(
            &good_path,
            hash_bytes(b"good bytes"),
            pdf_with_no_identifier(),
        )
        .with_file(
            &unrecorded_path,
            hash_bytes(b"unrecorded bytes 2"),
            pdf_with_no_identifier(),
        );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::resolve(vec![good_path.clone(), unrecorded_path.clone()]),
            true,
        ),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let stderr = String::from_utf8(err).unwrap();
    assert_eq!(
        stderr.matches("artifact record could not be read").count(),
        1,
        "the warning must appear exactly once: {stderr:?}"
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let good_event = lines
        .iter()
        .find(|line| line["path"].as_str().unwrap_or("").ends_with("good.pdf"))
        .unwrap_or_else(|| panic!("no event for good.pdf: {lines:?}"));
    assert_eq!(good_event["library"]["kind"], "tracked");
    let unrecorded_event = lines
        .iter()
        .find(|line| {
            line["path"]
                .as_str()
                .unwrap_or("")
                .ends_with("unrecorded.pdf")
        })
        .unwrap_or_else(|| panic!("no event for unrecorded.pdf: {lines:?}"));
    assert_eq!(unrecorded_event["library"]["kind"], "unreadable-records");
}

/// A library with no store faults writes no artifact-store warning.
#[test]
fn resolve_over_a_clean_library_writes_no_artifact_store_warning() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "good.pdf", b"clean bytes");
    seed_tracked_file(
        &root,
        "good.pdf",
        hash_bytes(b"clean bytes"),
        record_by("Smith", 2024, "10.1000/clean"),
    );
    let documents =
        FakeDocuments::new().with_file(&path, hash_bytes(b"clean bytes"), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![path]), true),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    assert_eq!(String::from_utf8(err).unwrap(), String::new());
}

/// Human mode: the tracked file's line ends ` (from the library)`, and
/// the closing line is `1 resolved, 0 skipped` — unaffected by the
/// library answer (design D5).
#[test]
fn resolve_human_mode_line_ends_with_from_the_library() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "smith.pdf", b"smith bytes");
    seed_tracked_file(
        &root,
        "smith.pdf",
        hash_bytes(b"smith bytes"),
        record_by("Smith", 2024, "10.1000/human-mode"),
    );
    let documents =
        FakeDocuments::new().with_file(&path, hash_bytes(b"smith bytes"), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![path]), false),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("(from the library)")),
        "got {lines:?}"
    );
    assert_eq!(
        lines.last(),
        Some(&"1 resolved, 0 skipped"),
        "got {lines:?}"
    );
}

// ---------------------------------------------------------------------
// consult-library-first, task 5.1/5.3: `rename` reads the library under
// every `record` setting, and the consultation travels through a
// rename (design D7, D8, D11)
// ---------------------------------------------------------------------

/// Writes an artifact record under `root` naming `relative`, holding
/// `hash`, linked to `item` — `None` for a dangling link the item
/// store holds no file for.
fn seed_artifact_record(root: &Path, relative: &str, hash: ContentHash, item: Option<ItemId>) {
    let record = ArtifactRecord {
        id: ArtifactId::from_uuid(uuid::Uuid::now_v7()),
        item,
        path: relative.to_string(),
        size: 100,
        modified_millis: 0,
        history: vec![HashEntry {
            hash,
            run: LibraryRunId::new("earlier-run"),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            tool_version: "0.6.0-test".to_string(),
        }],
    };
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::write(
        records.join(format!("{}.toml", record.id)),
        record.to_toml(),
    )
    .unwrap();
}

/// `rename --apply` over a tracked file whose item's record renders a
/// different name: the file moves to the item's name, no source is
/// asked, and the artifact record keeps its identity and item link
/// (design D7).
#[test]
fn applying_rename_moves_a_tracked_file_to_its_items_name_asking_no_source() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"tracked rename bytes");
    let item_id = seed_tracked_file(
        &root,
        "original.pdf",
        hash_bytes(b"tracked rename bytes"),
        record_by("Smith", 2024, "10.1000/tracked-rename"),
    );
    let records_before = library::ArtifactStore::read(&root)
        .iter()
        .next()
        .cloned()
        .unwrap();
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"tracked rename bytes"),
        pdf_with_no_identifier(),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, .. } if *from == path
        )),
        "got {events:?}"
    );
    let records_after = library::ArtifactStore::read(&root)
        .iter()
        .next()
        .cloned()
        .unwrap();
    assert_eq!(records_after.id, records_before.id);
    assert_eq!(records_after.item, Some(item_id));
}

/// The same, under `--no-record`: the same target, the same `resolved`
/// event, but no store write (the setting still withholds the account
/// and every write, exactly as it does today; only the consultation
/// itself is unconditional).
#[test]
fn no_record_rename_still_consults_the_library_and_writes_nothing() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "original.pdf", b"no-record tracked bytes");
    seed_tracked_file(
        &root,
        "original.pdf",
        hash_bytes(b"no-record tracked bytes"),
        record_by("Doe", 2023, "10.1000/no-record-tracked"),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"no-record tracked bytes"),
        pdf_with_no_identifier(),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.record = Some(false);
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let target = root.join("Doe2023.pdf");
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Renamed { path: from, target: to, .. } if *from == path && *to == target
        )),
        "the library must still be consulted and the file still moved: got {events:?}"
    );
}

/// A missing-item ("dangling-item") record survives batch rename
/// unresolved with no identifier: the `skipped` event carries the
/// problem (design D11).
#[test]
fn batch_rename_over_a_dangling_item_file_with_no_identifier_carries_the_problem() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "mystery.pdf", b"dangling no identifier");
    seed_artifact_record(
        &root,
        "mystery.pdf",
        hash_bytes(b"dangling no identifier"),
        Some(ItemId::from_uuid(uuid::Uuid::now_v7())),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"dangling no identifier"),
        pdf_with_no_identifier(),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    match events
        .iter()
        .find(|event| matches!(event, Event::Skipped { .. }))
    {
        Some(Event::Skipped {
            reason: SkipReason::NoIdentifier,
            library,
            ..
        }) => assert!(matches!(library, Some(LibraryAnswer::DanglingItem { .. }))),
        other => panic!("expected a Skipped(NoIdentifier) event, got {other:?}"),
    }
}

/// design D8: an applying batch run over two files whose records link
/// the *same* missing item. Consultation reads the immutable snapshot,
/// so the second file's `dangling-item` problem must not be promoted to
/// `tracked` once the first has been admitted — under both `--record`
/// and `--no-record`.
#[test]
fn the_second_of_two_files_sharing_a_dangling_item_still_gets_dangling_item() {
    for no_record in [false, true] {
        let library = real_library();
        let root = library.path().to_path_buf();
        let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
        let first_path = write_real_file(&root, "first.pdf", b"dangling shared first");
        let second_path = write_real_file(&root, "second.pdf", b"dangling shared second");
        seed_artifact_record(
            &root,
            "first.pdf",
            hash_bytes(b"dangling shared first"),
            Some(missing_item.clone()),
        );
        seed_artifact_record(
            &root,
            "second.pdf",
            hash_bytes(b"dangling shared second"),
            Some(missing_item.clone()),
        );
        let documents = FakeDocuments::new()
            .with_file(
                &first_path,
                hash_bytes(b"dangling shared first"),
                pdf_with_embedded_doi("10.1000/dangling-shared-first"),
            )
            .with_file(
                &second_path,
                hash_bytes(b"dangling shared second"),
                pdf_with_embedded_doi("10.1000/dangling-shared-second"),
            );
        let crossref = fake_source(
            SourceName::Crossref,
            Ok(record_by("Smith", 2024, "10.1000/dangling-shared-first")),
        );
        let sources: Vec<&dyn Source> = vec![&crossref];
        let index = ContentIndex::new(MemoryCache::new());
        let filesystem = FakeFilesystem::new();
        let bib_files = FakeBibFiles::new();
        let effective = effective_with(|layer| {
            layer.record = Some(!no_record);
            layer.templates = Some(BTreeMap::from([(
                "default".to_string(),
                "[auth][year]".to_string(),
            )]));
        });
        let adapters = Adapters {
            documents: &documents,
            sources: &sources,
            index: &index,
            filesystem: &filesystem,
            bib_files: &bib_files,
            cache_root: None,
            now: fixed_now,
            collection_root: Some(root.clone()),
            state_root: None,
        };

        let events = events_for(
            &Command::rename(vec![first_path, second_path.clone()], true),
            &Configs::uniform(effective),
            &adapters,
            &mut Session::batch(),
        )
        .unwrap();

        let second_event = events
            .iter()
            .find(|event| {
                matches!(
                    event,
                    Event::Resolved { path, .. } | Event::Skipped { path, .. }
                        if path == &second_path
                )
            })
            .unwrap_or_else(|| {
                panic!("no event for second.pdf (no_record={no_record}): {events:?}")
            });
        let library_answer = match second_event {
            Event::Resolved { library, .. } | Event::Skipped { library, .. } => library,
            _ => unreachable!(),
        };
        assert!(
            matches!(library_answer, Some(LibraryAnswer::DanglingItem { .. })),
            "no_record={no_record}: the second file must not be promoted to tracked \
             by the first's admission: got {second_event:?}"
        );
    }
}

// ---------------------------------------------------------------------
// consult-library-first, task 6.1: `bib` reads the run's library too
// (design D7)
// ---------------------------------------------------------------------

/// `bib` over a tracked file whose item's title was corrected: the
/// `resolved` event is the library answer, and the entry written to
/// the master `.bib` carries the corrected title.
#[test]
fn bib_reports_the_librarys_corrected_title() {
    let library = real_library();
    let root = library.path().to_path_buf();
    let path = write_real_file(&root, "corrected.pdf", b"bib library bytes");
    let mut corrected = record_by("Smith", 2024, "10.1000/bib-library");
    corrected.title = Some("REVIEW CORRECTION: Smith et al.".to_string());
    seed_tracked_file(
        &root,
        "corrected.pdf",
        hash_bytes(b"bib library bytes"),
        corrected.clone(),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_bytes(b"bib library bytes"),
        pdf_with_no_identifier(),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.citation_keys = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::bib(vec![path.clone()]),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    match events.first() {
        Some(Event::Resolved {
            record, library, ..
        }) => {
            assert_eq!(record.title, corrected.title);
            assert!(matches!(library, Some(LibraryAnswer::Tracked { .. })));
        }
        other => panic!("expected a Resolved event first, got {other:?}"),
    }
    let (bib_path, content) = bib_files
        .writes()
        .into_iter()
        .find(|(path, _)| path == Path::new("refs.bib"))
        .unwrap_or_else(|| panic!("no write to refs.bib"));
    assert_eq!(bib_path, PathBuf::from("refs.bib"));
    assert!(
        content.contains("REVIEW CORRECTION"),
        "the master bib file must carry the corrected title: {content:?}"
    );
    let (_, sidecar_content) = bib_files
        .writes()
        .into_iter()
        .find(|(path, _)| path.to_string_lossy().ends_with(".bib") && path != Path::new("refs.bib"))
        .unwrap_or_else(|| panic!("no sidecar written: {:?}", bib_files.writes()));
    assert!(
        sidecar_content.contains("REVIEW CORRECTION"),
        "the sidecar must carry the corrected title too: {sidecar_content:?}"
    );
}

// ---------------------------------------------------------------------
// consult-library-first, second pass: interactive rename (task 5.2),
// the consultation surviving every interactive path (task 5.3), the
// remaining task 5.1 bullets, and the unlistable-store warning (4.1)
// ---------------------------------------------------------------------

/// A [`Source`] that fails its first `fails` calls with a retryable
/// error, then answers `then` — the fixture "an outage followed by
/// success" needs, and "conflict found on retry" builds on by making
/// `then` a record the file's own title disagrees with.
struct FlakySource {
    name: SourceName,
    fails: std::sync::atomic::AtomicUsize,
    then: Record,
}

impl FlakySource {
    fn new(name: SourceName, fails: usize, then: Record) -> FlakySource {
        FlakySource {
            name,
            fails: std::sync::atomic::AtomicUsize::new(fails),
            then,
        }
    }
}

impl Source for FlakySource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Fetched, SourceError> {
        let left = self.fails.load(std::sync::atomic::Ordering::Relaxed);
        if left > 0 {
            self.fails
                .store(left - 1, std::sync::atomic::Ordering::Relaxed);
            return Err(SourceError::Unavailable {
                message: "503".to_string(),
            });
        }
        Ok(Fetched::network(self.then.clone()))
    }
}

/// A [`Source`] that answers from a fixed sequence of outcomes, one per
/// call, and panics if asked more times than the sequence has answers —
/// for a lookup whose services disagree across a retry.
struct SequencedSource {
    name: SourceName,
    answers: std::sync::Mutex<std::collections::VecDeque<Result<Record, SourceError>>>,
}

impl SequencedSource {
    fn new(name: SourceName, answers: Vec<Result<Record, SourceError>>) -> SequencedSource {
        SequencedSource {
            name,
            answers: std::sync::Mutex::new(answers.into()),
        }
    }
}

impl Source for SequencedSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Fetched, SourceError> {
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| panic!("{} has no more answers queued", self.name))
            .map(Fetched::network)
    }
}

/// design D7: a tracked file is asked the move question with today's
/// choices (rename · supply · skip · quit, defaulting to rename), and
/// answering rename moves it and writes nothing to the content index —
/// `Offer::kept` holds and `remember` stays false for a library answer.
#[test]
fn interactive_tracked_file_offers_the_move_question_and_writes_nothing_to_the_index() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("original.pdf");
    let hash = hash_bytes(b"interactive tracked move");
    seed_tracked_file(
        &root,
        "original.pdf",
        hash.clone(),
        record_by("Smith", 2024, "10.1000/interactive-tracked-move"),
    );
    let documents = FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);

    events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 1, "got {questions:?}");
    assert_choices(
        &questions[0],
        Answer::Rename,
        &[Answer::Supply, Answer::Skip, Answer::Quit],
    );
    assert_eq!(
        filesystem.renames(),
        vec![(path, root.join("Smith2024.pdf"))],
        "the tracked file must still move"
    );
    assert_eq!(
        index.get(&hash),
        None,
        "a library answer must write nothing to the content index"
    );
}

/// design D7: with `rename.skip-named` on (the default), a tracked
/// file already carrying its item's name is shown nothing — no
/// question is asked — and its `AlreadyNamed` event is still in the
/// stream (the run log's own source).
#[test]
fn interactive_skip_named_hides_an_already_named_tracked_file() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("Smith2024.pdf");
    let hash = hash_bytes(b"interactive tracked already named");
    seed_tracked_file(
        &root,
        "Smith2024.pdf",
        hash.clone(),
        record_by("Smith", 2024, "10.1000/interactive-tracked-already-named"),
    );
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(Vec::new());

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    assert_eq!(
        asker.questions_asked().len(),
        0,
        "an already-named tracked file must never be asked about"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::AlreadyNamed { path: p } if *p == path)),
        "got {events:?}"
    );
}

/// design D7/D11: supplying a different identifier for a tracked file
/// reports `tier: \"supplied\"` and `library.kind: \"tracked\"` — the
/// library answered, and the operator re-identified the file — and the
/// `library-admission` `relinked` event follows, moving the record's
/// link from the old item to the new one.
#[test]
fn supplying_a_different_identifier_for_a_tracked_file_reports_supplied_and_relinks() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("original.pdf");
    let hash = hash_bytes(b"interactive tracked supply");
    let old_item = seed_tracked_file(
        &root,
        "original.pdf",
        hash.clone(),
        record_by("Smith", 2024, "10.1000/interactive-tracked-supply-old"),
    );
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/interactive-tracked-supply-new",
        record_by("Doe", 2023, "10.1000/interactive-tracked-supply-new"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    fs::write(&path, b"interactive tracked supply").unwrap();
    let filesystem = RealFilesystem;
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker =
        ScriptedAsker::new(vec![Answer::Supply, Answer::Rename]).with_texts(vec![Some(
            "10.1000/interactive-tracked-supply-new".to_string(),
        )]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
    {
        Some(Event::Resolved { tier, library, .. }) => {
            assert_eq!(tier.as_deref(), Some("supplied"));
            assert!(matches!(library, Some(LibraryAnswer::Tracked { .. })));
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::LibraryAdmission {
                admission: Admission::Relinked { from, .. },
                ..
            } if from == &old_item.to_string()
        )),
        "got {events:?}"
    );
}

/// design D11: interactive Skip of a file whose library could not
/// answer keeps the problem on the `skipped` event, and the
/// description shown before the question carries the `library` line
/// (after the reason's own lines).
#[test]
fn interactive_skip_of_a_dangling_item_file_keeps_the_problem() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("mystery.pdf");
    let hash = hash_bytes(b"interactive dangling no identifier");
    let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
    seed_artifact_record(
        &root,
        "mystery.pdf",
        hash.clone(),
        Some(missing_item.clone()),
    );
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at_path(&root, "mystery.pdf"),
        item: missing_item.to_string(),
    };
    match events
        .iter()
        .find(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
    {
        Some(Event::Skipped { library, .. }) => assert_eq!(library, &Some(expected_problem)),
        other => panic!("expected Event::Skipped, got {other:?}"),
    }
    assert_eq!(
        asker.questions_asked().len(),
        1,
        "got {:?}",
        asker.questions_asked()
    );
    assert!(
        asker.questions_asked()[0]
            .description
            .iter()
            .any(|line| line.contains("library")),
        "the description shown before the question must carry a library line: {:?}",
        asker.questions_asked()[0].description
    );
}

/// design D11: an interactive Retry after a service outage keeps the
/// library answer once the services then answer.
#[test]
fn interactive_retry_after_an_outage_keeps_the_dangling_item_problem() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("outage.pdf");
    let hash = hash_bytes(b"interactive dangling retry");
    let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
    seed_artifact_record(
        &root,
        "outage.pdf",
        hash.clone(),
        Some(missing_item.clone()),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/interactive-dangling-retry"),
    );
    let crossref = FlakySource::new(
        SourceName::Crossref,
        1,
        record_by("Smith", 2024, "10.1000/interactive-dangling-retry"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Retry, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at_path(&root, "outage.pdf"),
        item: missing_item.to_string(),
    };
    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
    {
        Some(Event::Resolved { library, .. }) => {
            assert_eq!(library, &Some(expected_problem));
        }
        other => panic!("expected Event::Resolved after the retry, got {other:?}"),
    }
}

/// design D11: a retry that turns up a conflict keeps the library
/// answer on the conflict skip too.
#[test]
fn interactive_retry_that_finds_a_conflict_keeps_the_dangling_item_problem() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("outage-conflict.pdf");
    let hash = hash_bytes(b"interactive dangling retry conflict");
    let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
    seed_artifact_record(
        &root,
        "outage-conflict.pdf",
        hash.clone(),
        Some(missing_item.clone()),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/interactive-dangling-retry-conflict")
            .with_title("Old Title Extracted from the PDF"),
    );
    let crossref = FlakySource::new(
        SourceName::Crossref,
        1,
        record_by_with_title(
            "Smith",
            2024,
            "10.1000/interactive-dangling-retry-conflict",
            "A Completely Different Title About Something Else",
        ),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Retry, Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at_path(&root, "outage-conflict.pdf"),
        item: missing_item.to_string(),
    };
    match events
        .iter()
        .find(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
    {
        Some(Event::Skipped {
            reason, library, ..
        }) => {
            assert!(
                matches!(reason, SkipReason::Conflict { .. }),
                "got {reason:?}"
            );
            assert_eq!(library, &Some(expected_problem));
        }
        other => panic!("expected a conflict Skipped after the retry, got {other:?}"),
    }
}

/// design D11: a retry's record keeps the extraction pass as its
/// origin. The driver no longer overwrites `tier` back to `supplied`.
#[test]
fn a_retry_after_an_outage_keeps_the_extraction_pass_as_tier_not_supplied() {
    let path = PathBuf::from("/lib/retry-tier.pdf");
    let hash = hash_for("d11-retry-tier");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/d11-retry-tier"),
    );
    let crossref = FlakySource::new(
        SourceName::Crossref,
        1,
        record_by("Smith", 2024, "10.1000/d11-retry-tier"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Retry, Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
    {
        Some(Event::Resolved { tier, .. }) => {
            assert_eq!(tier.as_deref(), Some("embedded-metadata"));
        }
        other => panic!("expected Event::Resolved after the retry, got {other:?}"),
    }
}

/// design D11: a retry replaces the file's own lookup. An outage
/// followed by every service answering not found is conclusive, so the
/// next question offers no retry, and the skip carries the retry's own
/// attempts.
#[test]
fn an_outage_then_not_found_on_retry_stops_offering_a_retry() {
    let path = PathBuf::from("/lib/retry-not-found.pdf");
    let hash = hash_for("d11-retry-not-found");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/d11-retry-not-found"),
    );
    let crossref = SequencedSource::new(
        SourceName::Crossref,
        vec![
            Err(SourceError::Unavailable {
                message: "503".to_string(),
            }),
            Err(SourceError::NotFound),
        ],
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Retry, Answer::Skip]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 2, "got {questions:?}");
    assert!(
        !questions[1].choices.contains(&Answer::Retry),
        "a conclusive retry must not offer another: got {:?}",
        questions[1].choices
    );
    match events
        .iter()
        .find(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
    {
        Some(Event::Skipped {
            reason: SkipReason::Unresolvable { attempts, .. },
            ..
        }) => {
            assert_eq!(attempts.len(), 1, "got {attempts:?}");
        }
        other => panic!("expected an unresolvable Skipped after the retry, got {other:?}"),
    }
}

/// design D11: a failed supply is a candidate that led nowhere. It does
/// not replace the file's own lookup, so a retry is still offered
/// afterwards.
#[test]
fn an_outage_then_a_failed_supply_still_offers_a_retry() {
    let path = PathBuf::from("/lib/supply-fails-retry-stays.pdf");
    let hash = hash_for("d11-failed-supply-retry-stays");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/d11-failed-supply-retry-stays"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Err(SourceError::Unavailable {
            message: "503".to_string(),
        }),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Supply, Answer::Skip])
        .with_texts(vec![Some("10.1000/d11-nobody-holds-this".to_string())]);

    events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let questions = asker.questions_asked();
    assert_eq!(questions.len(), 2, "got {questions:?}");
    assert_eq!(
        questions[1].choices.first().copied(),
        Some(Answer::Retry),
        "a failed supply must not replace the file's own inconclusive \
         lookup: got {:?}",
        questions[1].choices
    );
}

/// design D11: overriding a conflict still reports it before the move,
/// with no event about this file between the two.
#[test]
fn overriding_a_conflict_still_reports_resolved_before_renamed() {
    let path = PathBuf::from("/lib/override-order.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("d11-override-order"),
        pdf_with_embedded_doi("10.1000/d11-override-order").with_title("Title the File Claims"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by_with_title(
            "Smith",
            2024,
            "10.1000/d11-override-order",
            "A Completely Different Resolved Title",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: None,
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Rename]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let resolved_index = events
        .iter()
        .position(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
        .unwrap_or_else(|| panic!("no Event::Resolved for the file: {events:?}"));
    let renamed_index = events
        .iter()
        .position(|event| matches!(event, Event::Renamed { path: p, .. } if *p == path))
        .unwrap_or_else(|| panic!("no Event::Renamed for the file: {events:?}"));

    assert!(
        resolved_index < renamed_index,
        "resolved must come before renamed: got {events:?}"
    );
    assert!(
        !events[resolved_index + 1..renamed_index].iter().any(|event| matches!(
            event,
            Event::Resolved { path: p, .. } | Event::Skipped { path: p, .. } | Event::Renamed { path: p, .. }
            if *p == path
        )),
        "nothing about this file may appear between resolved and renamed: got {events:?}"
    );
}

/// design D11: a supplied candidate for a file the library could not
/// answer for carries the problem too.
#[test]
fn interactive_supplied_candidate_for_a_dangling_item_file_keeps_the_problem() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("mystery-supply.pdf");
    let hash = hash_bytes(b"interactive dangling supply");
    let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
    seed_artifact_record(
        &root,
        "mystery-supply.pdf",
        hash.clone(),
        Some(missing_item.clone()),
    );
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/interactive-dangling-supply",
        record_by("Smith", 2024, "10.1000/interactive-dangling-supply"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker =
        ScriptedAsker::new(vec![Answer::Supply, Answer::Rename]).with_texts(vec![Some(
            "10.1000/interactive-dangling-supply".to_string(),
        )]);

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
    )
    .unwrap();

    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at_path(&root, "mystery-supply.pdf"),
        item: missing_item.to_string(),
    };
    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
    {
        Some(Event::Resolved { tier, library, .. }) => {
            assert_eq!(tier.as_deref(), Some("supplied"));
            assert_eq!(library, &Some(expected_problem));
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D3: a batch conflict on a dangling-item file carries the
/// problem, confirming the batch (non-interactive) path pins the same
/// rule the interactive one does.
#[test]
fn batch_conflict_on_a_dangling_item_file_keeps_the_problem() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("batch-conflict.pdf");
    let hash = hash_bytes(b"batch dangling conflict");
    let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
    seed_artifact_record(
        &root,
        "batch-conflict.pdf",
        hash.clone(),
        Some(missing_item.clone()),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/batch-dangling-conflict")
            .with_title("Old Title Extracted from the PDF"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/batch-dangling-conflict",
        record_by_with_title(
            "Smith",
            2024,
            "10.1000/batch-dangling-conflict",
            "A Completely Different Title About Something Else",
        ),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at_path(&root, "batch-conflict.pdf"),
        item: missing_item.to_string(),
    };
    match events
        .iter()
        .find(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
    {
        Some(Event::Skipped {
            reason, library, ..
        }) => {
            assert!(
                matches!(reason, SkipReason::Conflict { .. }),
                "got {reason:?}"
            );
            assert_eq!(library, &Some(expected_problem));
        }
        other => panic!("expected a conflict Skipped, got {other:?}"),
    }
}

/// design D3: `target-taken` is a non-verdict skip and stays
/// `library: null`, even though the file's own `resolved` event —
/// reached by fallback from a dangling-item record — carries the
/// problem.
#[test]
fn batch_target_taken_stays_library_null_while_resolved_carries_the_problem() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("collides.pdf");
    let hash = hash_bytes(b"batch dangling target taken");
    let missing_item = ItemId::from_uuid(uuid::Uuid::now_v7());
    seed_artifact_record(
        &root,
        "collides.pdf",
        hash.clone(),
        Some(missing_item.clone()),
    );
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/batch-dangling-target-taken"),
    );
    let crossref = KeyedSource::new(SourceName::Crossref).answering(
        "doi:10.1000/batch-dangling-target-taken",
        record_by("Smith", 2024, "10.1000/batch-dangling-target-taken"),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new()
        .with_existing(root.clone(), [("Smith2024.pdf", Some("some-other-hash"))]);
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective_with(|layer| {
            layer.templates = Some(BTreeMap::from([(
                "default".to_string(),
                "[auth][year]".to_string(),
            )]));
            layer.rename = Some(borax::config::RenameLayer {
                collision: Some("skip".to_string()),
                ..Default::default()
            });
        })),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at_path(&root, "collides.pdf"),
        item: missing_item.to_string(),
    };
    match events
        .iter()
        .find(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
    {
        Some(Event::Resolved { library, .. }) => assert_eq!(library, &Some(expected_problem)),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
    match events
        .iter()
        .find(|event| matches!(event, Event::Skipped { path: p, .. } if *p == path))
    {
        Some(Event::Skipped {
            reason: SkipReason::TargetTaken { .. },
            library,
            ..
        }) => assert_eq!(library, &None, "target-taken is a non-verdict skip"),
        other => panic!("expected a TargetTaken Skipped, got {other:?}"),
    }
}

/// The artifact identity of the one record written at `relative` under
/// `root`, for a dangling-item fixture: read back through `consult`.
fn record_id_at_path(root: &Path, relative: &str) -> String {
    library::ArtifactStore::read(root)
        .by_path(relative)
        .unwrap_or_else(|| panic!("no artifact record at {relative}"))
        .id
        .to_string()
}

// ---------------------------------------------------------------------
// task 5.1: preview parity, and an already-named tracked file
// ---------------------------------------------------------------------

/// A preview reports the same `resolved` event and the same plan as
/// `--apply` would (design D7: the library is consulted whatever the
/// run will do with the answer).
#[test]
fn preview_reports_the_same_resolved_event_and_plan_as_apply() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("original.pdf");
    let hash = hash_bytes(b"preview tracked parity");
    seed_tracked_file(
        &root,
        "original.pdf",
        hash.clone(),
        record_by("Smith", 2024, "10.1000/preview-tracked-parity"),
    );
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let preview = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    let resolved = preview
        .iter()
        .find(|event| matches!(event, Event::Resolved { path: p, .. } if *p == path))
        .unwrap_or_else(|| panic!("no Resolved event: {preview:?}"));
    assert!(matches!(
        resolved,
        Event::Resolved {
            library: Some(LibraryAnswer::Tracked { .. }),
            ..
        }
    ));
    assert!(
        preview.iter().any(|event| matches!(
            event,
            Event::Planned { path: p, target } if *p == path && target == &root.join("Smith2024.pdf")
        )),
        "got {preview:?}"
    );
    assert!(
        !preview
            .iter()
            .any(|event| matches!(event, Event::Renamed { .. })),
        "a preview must move nothing: got {preview:?}"
    );
}

/// A tracked file already carrying its item's name is `already-named`,
/// with nothing written to the content index (design D7 — a library
/// answer is checked, but nothing is opened or looked up either way).
#[test]
fn tracked_file_already_named_reports_already_named_and_writes_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("Smith2024.pdf");
    let hash = hash_bytes(b"batch tracked already named");
    seed_tracked_file(
        &root,
        "Smith2024.pdf",
        hash.clone(),
        record_by("Smith", 2024, "10.1000/batch-tracked-already-named"),
    );
    let documents = FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective_with_default_template("[auth][year]")),
        &adapters,
        &mut Session::batch(),
    )
    .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::AlreadyNamed { path: p } if *p == path)),
        "got {events:?}"
    );
    assert_eq!(index.get(&hash), None);
    assert!(filesystem.renames().is_empty());
}

// ---------------------------------------------------------------------
// task 4.1: an unlistable `.borax/artifacts/` warns once (design D5)
// ---------------------------------------------------------------------

/// One unlistable `.borax/artifacts/`: stderr carries D5's unlistable
/// warning exactly once, and every file's event reports
/// `unreadable-records` with `listed: false`.
#[cfg(unix)]
#[test]
fn resolve_with_an_unlistable_artifact_store_warns_once_and_reports_unreadable_records() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let path = root.join("paper.pdf");
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping resolve_with_an_unlistable_artifact_store_warns_once_and_reports_unreadable_records: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let hash = hash_bytes(b"unlistable store bytes");
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::resolve(vec![path]), true),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();

    let stderr = String::from_utf8(err).unwrap();
    assert_eq!(
        stderr
            .matches("the library's artifact records could not be listed")
            .count(),
        1,
        "got {stderr:?}"
    );
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let resolved = lines
        .iter()
        .find(|line| line["event"] == "resolved" || line["event"] == "skipped")
        .unwrap_or_else(|| panic!("no per-file event: {lines:?}"));
    assert_eq!(resolved["library"]["kind"], "unreadable-records");
    assert_eq!(resolved["library"]["listed"], false);
}

/// design D2a: `borax validate` over a library whose
/// `.borax/artifacts/` cannot be listed reports an `unreadable`
/// finding at that directory and exits partial, instead of certifying
/// an empty, clean library.
#[cfg(unix)]
#[test]
fn validate_over_an_unlistable_artifact_store_exits_partial() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping validate_over_an_unlistable_artifact_store_exits_partial: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::validate(Some(root.clone())), true),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("\"kind\":\"unreadable\""), "got {text:?}");
}

/// design D2a: `borax status` over the same unlistable store reports
/// the counts it reports today — an empty store, unaffected by the
/// `unreadable-records` distinction that `resolve`/`rename`/`bib` now
/// draw.
#[cfg(unix)]
#[test]
fn status_over_an_unlistable_artifact_store_is_unaffected() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping status_over_an_unlistable_artifact_store_is_unaffected: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::status(Some(root.clone()), false), true),
        &Configs::uniform(resolve(Vec::new()).unwrap()),
        &adapters,
        &mut Session::batch(),
        &mut streams,
    );

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("\"records\":0"), "got {text:?}");
}

// ---------------------------------------------------------------------
// consult-library-first, third pass: the artifact-store warning only
// for a run that actually consulted the library (reviewer finding —
// `consultation_warning` is asked over every grouped input path, quit
// or not, rather than over the ones the run reached)
// ---------------------------------------------------------------------

/// An interactive rename under `--no-record` whose library has one
/// unparsable artifact record: the first file lies outside the
/// library, the second inside it. The operator quits at the first
/// file, so the second — and the library's own fault — is never
/// reached, and the run must not warn about it.
#[test]
fn quitting_before_the_librarys_own_file_writes_no_artifact_store_warning() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("library");
    fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let first = outside.join("first.pdf");
    let second = root.join("second.pdf");

    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::write(records.join("broken.toml"), "not valid toml {{{").unwrap();

    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_for("quit-before-library-first"),
            pdf_with_embedded_doi("10.1000/quit-before-library-first"),
        )
        .with_file(
            &second,
            hash_for("quit-before-library-second"),
            pdf_with_embedded_doi("10.1000/quit-before-library-second"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/quit-before-library-first",
            record_by("Smith", 2024, "10.1000/quit-before-library-first"),
        )
        .answering(
            "doi:10.1000/quit-before-library-second",
            record_by("Doe", 2023, "10.1000/quit-before-library-second"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.record = Some(false);
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Quit]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![first, second], false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let stderr = String::from_utf8(err).unwrap();
    assert!(
        !stderr.contains("artifact record"),
        "a run that quit before reaching the library's own file must not warn about it: \
         got {stderr:?}"
    );
}

/// The positive control: the same library and the same fault, but the
/// operator answers the first file and the run reaches the second,
/// inside the library — the warning is emitted.
#[test]
fn reaching_the_librarys_own_file_still_writes_the_artifact_store_warning() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("library");
    fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let first = outside.join("first.pdf");
    let second = root.join("second.pdf");

    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::write(records.join("broken.toml"), "not valid toml {{{").unwrap();

    let documents = FakeDocuments::new()
        .with_file(
            &first,
            hash_for("reach-library-first"),
            pdf_with_embedded_doi("10.1000/reach-library-first"),
        )
        .with_file(
            &second,
            hash_for("reach-library-second"),
            pdf_with_embedded_doi("10.1000/reach-library-second"),
        );
    let crossref = KeyedSource::new(SourceName::Crossref)
        .answering(
            "doi:10.1000/reach-library-first",
            record_by("Smith", 2024, "10.1000/reach-library-first"),
        )
        .answering(
            "doi:10.1000/reach-library-second",
            record_by("Doe", 2023, "10.1000/reach-library-second"),
        );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with(|layer| {
        layer.record = Some(false);
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
    });
    let adapters = Adapters {
        documents: &documents,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        collection_root: Some(root.clone()),
        state_root: None,
    };
    let mut asker = ScriptedAsker::new(vec![Answer::Skip, Answer::Skip]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(Command::rename(vec![first, second], false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut Session::interactive(&mut asker),
        &mut streams,
    );

    let stderr = String::from_utf8(err).unwrap();
    assert!(
        stderr.contains("artifact record"),
        "a run that reached the library's own file must warn about its fault: got {stderr:?}"
    );
}
