#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use borax::bib::{BibFiles, citation_key};
use borax::cache::{cleared_event, inspect, status_event};
use borax::cli::{Cli, Command, Settings};
use borax::config::{
    BibLayer, Effective, KeyColumns, Layer, Origin, TableDeclaration, ValueKindName, resolve,
};
use borax::event::{Event, Level, SkipReason};
use borax::pipeline::Library;
use borax::renaming::{Filesystem, RenameError, counts_for};
use borax::run::{Adapters, Configs, Streams, dispatch, entry_type, events_for, templates};
use borax::session::Outcome;
use borax_core::bib_output::{DuplicatePolicy, MergeOutcome, merge};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{DateParts, EntryType, Name, Record};
use borax_core::tables::{LookupTables, Lookups, NoTables, Table, TableSpec, ValueKind};
use borax_core::template::RenderInput;
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_sources::cache::MemoryCache;
use borax_sources::source::{Source, SourceError, SourceName};
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

/// What [`FakeLibrary`] answers for one path.
struct LibraryEntry {
    hash: Result<ContentHash, ExtractionError>,
    pdf: Result<FakePdf, ExtractionError>,
}

/// A [`Library`] fake backed by a map from path to a fixed `(hash, PDF
/// content or error)` pair, following the shape of the one in
/// `pipeline.rs`.
struct FakeLibrary {
    entries: BTreeMap<PathBuf, LibraryEntry>,
}

impl FakeLibrary {
    fn new() -> FakeLibrary {
        FakeLibrary {
            entries: BTreeMap::new(),
        }
    }

    /// A readable file: hashing and opening both succeed.
    fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        pdf: FakePdf,
    ) -> FakeLibrary {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Ok(hash),
                pdf: Ok(pdf),
            },
        );
        self
    }
}

impl Library for FakeLibrary {
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

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.response.clone()
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

    fn fetch(&self, identifier: &Identifier) -> Result<Record, SourceError> {
        match self.answers.get(&identifier.to_string()) {
            Some(record) => Ok(record.clone()),
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
    writes: RefCell<Vec<(PathBuf, String)>>,
}

impl FakeBibFiles {
    fn new() -> FakeBibFiles {
        FakeBibFiles {
            initial: Vec::new(),
            writes: RefCell::new(Vec::new()),
        }
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
        tier: tier.map(str::to_string),
        cached,
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
    Cli {
        command,
        settings: Settings::default(),
        json,
    }
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::Config,
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(root.clone()),
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::Cache { clear: false },
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: Some(root.clone()),
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::Cache { clear: true },
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::Cache { clear: false },
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new()
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::resolve(vec![good.clone(), bad.clone()]),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = library_with_resolvable(&path, hash.clone(), "10.1000/rename-apply");
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };
    let target = PathBuf::from("/lib/Smith2024.pdf");

    let events = events_for(
        &Command::rename(vec![path.clone()], true),
        &Configs::uniform(effective.clone()),
        &adapters,
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

/// A [`FakeLibrary`] with one file whose embedded DOI is `doi_value`,
/// factored out because the apply test needs the hash again to build its
/// expected `Renamed` event.
fn library_with_resolvable(path: &Path, hash: ContentHash, doi_value: &str) -> FakeLibrary {
    FakeLibrary::new().with_file(path, hash, pdf_with_embedded_doi(doi_value))
}

// ---------------------------------------------------------------------
// events_for: Command::Bib
// ---------------------------------------------------------------------

#[test]
fn bib_emits_resolved_then_the_bib_events_and_the_fake_bib_files_received_the_writes() {
    let path = PathBuf::from("/lib/paper.pdf");
    let record = record_by("Smith", 2024, "10.1000/bib");
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::bib(vec![path.clone()]),
        &Configs::uniform(effective.clone()),
        &adapters,
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

// ---------------------------------------------------------------------
// events_for: an uncompilable template propagates as a Diagnostic
// ---------------------------------------------------------------------

#[test]
fn rename_with_an_uncompilable_template_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective.clone()),
        &adapters,
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
}

#[test]
fn bib_with_an_uncompilable_template_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/paper.pdf");
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::bib(vec![path]),
        &Configs::uniform(effective.clone()),
        &adapters,
    )
    .unwrap_err();

    assert_eq!(error.level, Level::Error);
}

// ---------------------------------------------------------------------
// events_for: an uncompilable citation-key template propagates as a
// Diagnostic, before any file is processed
// ---------------------------------------------------------------------

#[test]
fn rename_with_an_uncompilable_citation_key_template_propagates_the_diagnostic() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::bib(vec![path]),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::bib(vec![path]),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::Config, true),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new()
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        Some(&"1 resolved, 0 renamed, 0 skipped"),
        "got {lines:?}"
    );
}

#[test]
fn run_finished_counts_match_counts_for_over_the_body_events() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let library = FakeLibrary::new()
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

#[test]
fn a_run_with_a_skip_returns_partial() {
    let good = PathBuf::from("/lib/good.pdf");
    let bad = PathBuf::from("/lib/bad.pdf");
    let library = FakeLibrary::new()
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let effective = effective_with_default_template("[nonexistentfield]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![path.clone()], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
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
    let library = FakeLibrary::new()
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![first, second], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
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
    let library = FakeLibrary::new()
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let events = events_for(
        &Command::rename(vec![first, second], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::Config, true),
        &Configs::uniform(effective.clone()),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
        &path,
        hash_for("table-missing"),
        pdf_with_embedded_doi("10.1000/table-missing"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
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
    let library = FakeLibrary::new().with_file(
        &path,
        hash_for("table-column"),
        pdf_with_embedded_doi("10.1000/table-column"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles::new();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };

    let error = events_for(
        &Command::rename(vec![path], false),
        &Configs::uniform(effective_looking_up(&table)),
        &adapters,
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

    let mut library = FakeLibrary::new();
    let mut crossref = KeyedSource::new(SourceName::Crossref);
    for (path, doi_value, journal, volume, _) in &batch {
        library = library.with_file(
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
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
