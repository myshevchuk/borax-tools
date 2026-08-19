#![allow(clippy::unwrap_used)]

//! Part 3 of `stream-per-file-events`: a rename run reports one file at
//! a time.
//!
//! `rename_events` groups a run's inputs by directory and, within a
//! group, works one file completely — its resolution, the rename
//! planned or applied for it, and any sidecar written beside it — before
//! moving to the next. The master `.bib` merge is exempt, since it is
//! work about the whole group rather than about one file.
//!
//! Tasks 3.1 and 3.3 pin ordering by mapping each event to a `(path,
//! kind)` pair and comparing the sequence, which proves adjacency and
//! group separation without pinning every field of every event. Task 3.2
//! pins the same batch's names, suffix, and journal entries by full
//! equality, so an ordering fix that also changes a decision fails here
//! even where it might pass 3.1.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use borax::bib::BibFiles;
use borax::cli::Command;
use borax::config::{BibLayer, Effective, Layer, Origin, resolve};
use borax::event::{Event, SkipReason};
use borax::journal::{Entry, Journal, RunId};
use borax::pipeline::Library;
use borax::renaming::{Filesystem, RenameError};
use borax::run::{Adapters, Configs, events_for};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{DateParts, EntryType, Name, Record};
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_sources::cache::MemoryCache;
use borax_sources::source::{Source, SourceError, SourceName};
use borax_sources::store::ContentIndex;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`PdfSource`] fake driven by canned pages and an XMP packet,
/// following the shape of the one in `dispatch.rs`.
#[derive(Clone)]
struct FakePdf {
    pages: Vec<Result<String, ExtractionError>>,
    info: InfoMetadata,
    xmp: Option<String>,
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
    FakePdf {
        pages: Vec::new(),
        info: InfoMetadata::default(),
        xmp: Some(format!("<prism:doi>{value}</prism:doi>")),
    }
}

/// A PDF with a page of ordinary prose holding no identifier.
fn pdf_with_no_identifier() -> FakePdf {
    FakePdf {
        pages: vec![Ok("just some prose, no identifiers here".to_string())],
        info: InfoMetadata::default(),
        xmp: None,
    }
}

/// What [`FakeLibrary`] answers for one path.
struct LibraryEntry {
    hash: ContentHash,
    pdf: FakePdf,
}

/// A [`Library`] fake backed by a map from path to a fixed `(hash, PDF
/// content)` pair, following the shape of the one in `dispatch.rs`.
struct FakeLibrary {
    entries: BTreeMap<PathBuf, LibraryEntry>,
}

impl FakeLibrary {
    fn new() -> FakeLibrary {
        FakeLibrary {
            entries: BTreeMap::new(),
        }
    }

    fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        pdf: FakePdf,
    ) -> FakeLibrary {
        self.entries.insert(path.into(), LibraryEntry { hash, pdf });
        self
    }
}

impl Library for FakeLibrary {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.entries
            .get(path)
            .map(|entry| entry.hash.clone())
            .ok_or_else(|| ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            })
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        self.entries
            .get(path)
            .map(|entry| Box::new(entry.pdf.clone()) as Box<dyn PdfSource>)
            .ok_or_else(|| ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            })
    }
}

/// A [`Source`] whose response depends on which DOI was asked for.
///
/// The single-canned-response fakes elsewhere answer every identifier
/// the same way, which is enough when a test does not care which record
/// a file gets. A batch built to collide, or not to, needs distinct
/// records per file, so this fake keys its answers by the DOI's `doi:`
/// display form.
struct KeyedSource {
    name: SourceName,
    responses: BTreeMap<String, Record>,
}

impl KeyedSource {
    /// `responses` pairs a bare DOI value (as passed to
    /// [`pdf_with_embedded_doi`]) with the record its lookup answers.
    fn new(name: SourceName, responses: Vec<(&str, Record)>) -> KeyedSource {
        KeyedSource {
            name,
            responses: responses
                .into_iter()
                .map(|(doi_value, record)| (format!("doi:{doi_value}"), record))
                .collect(),
        }
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
        self.responses
            .get(&identifier.to_string())
            .cloned()
            .ok_or(SourceError::NotFound)
    }
}

/// A [`Filesystem`] fake with nothing already on disk, recording every
/// [`Filesystem::rename`] call in order, following the shape of the one
/// in `dispatch.rs`.
struct FakeFilesystem {
    renames: RefCell<Vec<(PathBuf, PathBuf)>>,
}

impl FakeFilesystem {
    fn new() -> FakeFilesystem {
        FakeFilesystem {
            renames: RefCell::new(Vec::new()),
        }
    }

    fn renames(&self) -> Vec<(PathBuf, PathBuf)> {
        self.renames.borrow().clone()
    }
}

impl Filesystem for FakeFilesystem {
    fn existing(&self, _directory: &Path) -> BTreeMap<String, Option<String>> {
        BTreeMap::new()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError> {
        self.renames
            .borrow_mut()
            .push((from.to_path_buf(), to.to_path_buf()));
        Ok(())
    }
}

/// A [`Journal`] fake that records every append and reads back as
/// empty, following the shape of the one in `dispatch.rs` trimmed to
/// what this file needs: no test here seeds pre-existing entries.
struct FakeJournal {
    appended: RefCell<Vec<Entry>>,
}

impl FakeJournal {
    fn new() -> FakeJournal {
        FakeJournal {
            appended: RefCell::new(Vec::new()),
        }
    }

    fn appended(&self) -> Vec<Entry> {
        self.appended.borrow().clone()
    }
}

impl Journal for FakeJournal {
    fn append(&self, entries: &[Entry]) -> io::Result<()> {
        self.appended.borrow_mut().extend_from_slice(entries);
        Ok(())
    }

    fn read(&self) -> Vec<Entry> {
        Vec::new()
    }
}

/// A [`BibFiles`] fake that reads as empty and discards every write,
/// following the shape of the one in `streaming.rs` — no test here
/// inspects what was written, only that a sidecar was.
struct FakeBibFiles;

impl BibFiles for FakeBibFiles {
    fn read(&self, _path: &Path) -> io::Result<String> {
        Ok(String::new())
    }

    fn write(&self, _path: &Path, _content: &str) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
}

fn hash_for(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// An `Article` by one author in the given year, carrying `doi_value`.
/// Renders as `"{family}{year}"` under the `[auth][year]` template most
/// fixtures in this file use.
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

fn resolved_event(path: &Path, identifier: &str, record: &Record) -> Event {
    Event::Resolved {
        path: path.to_path_buf(),
        identifier: identifier.to_string(),
        record: Box::new(record.clone()),
        source: "crossref".to_string(),
        tier: Some("embedded-metadata".to_string()),
        cached: false,
    }
}

/// The `now` every fixture in this file uses: a fixed string, so a
/// journaled entry's timestamp and run identifier are pinned rather than
/// depending on the clock.
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

/// A `read` fake serving the TOML text paired with a path in `entries`,
/// and reporting [`io::ErrorKind::NotFound`] for every other path,
/// following the shape of the one in `run.rs`.
fn fake_read(
    entries: &'static [(&'static str, &'static str)],
) -> impl Fn(&Path) -> io::Result<String> {
    move |path| {
        entries
            .iter()
            .find(|(candidate, _)| Path::new(candidate) == path)
            .map(|(_, content)| content.to_string())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }
}

/// The file a `path`-carrying event is about.
///
/// Every variant these tests produce carries `path`; a variant that
/// does not is not one any fixture here reaches.
fn path_of(event: &Event) -> PathBuf {
    match event {
        Event::Resolved { path, .. }
        | Event::Planned { path, .. }
        | Event::Renamed { path, .. }
        | Event::Skipped { path, .. }
        | Event::BibEntry { path, .. }
        | Event::Sidecar { path, .. } => path.clone(),
        other => unreachable!("event carries no path: {other:?}"),
    }
}

/// A coarse label for `event`'s variant, fine enough to distinguish a
/// resolution from a plan from a sidecar without pinning every field —
/// what a test asserting on order alone needs.
fn kind(event: &Event) -> &'static str {
    match event {
        Event::Resolved { .. } => "resolved",
        Event::Planned { .. } => "planned",
        Event::Renamed { .. } => "renamed",
        Event::Sidecar { .. } => "sidecar",
        Event::BibEntry { .. } => "bib-entry",
        Event::Skipped {
            reason: SkipReason::NoIdentifier,
            ..
        } => "skipped-no-identifier",
        Event::Skipped {
            reason: SkipReason::AlreadyNamed,
            ..
        } => "skipped-already-named",
        other => unreachable!("event not produced by any fixture here: {other:?}"),
    }
}

/// `events` reduced to the `(path, kind)` sequence [`kind`] and
/// [`path_of`] answer, in event order.
fn sequence(events: &[Event]) -> Vec<(PathBuf, &'static str)> {
    events
        .iter()
        .map(|event| (path_of(event), kind(event)))
        .collect()
}

// ---------------------------------------------------------------------
// 3.1: a file's verdict and its fate are adjacent
// ---------------------------------------------------------------------

#[test]
fn each_files_resolution_plan_and_sidecar_are_adjacent_in_input_order() {
    // Four files exercising the four outcomes a group can hold: a file
    // that fails to resolve, two files that resolve and collide on the
    // same rendered name, and a file already named what its record
    // implies.
    let blank = PathBuf::from("/lib/blank.pdf");
    let paper1 = PathBuf::from("/lib/paper1.pdf");
    let paper2 = PathBuf::from("/lib/paper2.pdf");
    let already = PathBuf::from("/lib/Jones2020.pdf");

    let library = FakeLibrary::new()
        .with_file(&blank, hash_for("per-file-blank"), pdf_with_no_identifier())
        .with_file(
            &paper1,
            hash_for("per-file-paper1"),
            pdf_with_embedded_doi("10.1000/per-file-orig1"),
        )
        .with_file(
            &paper2,
            hash_for("per-file-paper2"),
            pdf_with_embedded_doi("10.1000/per-file-orig2"),
        )
        .with_file(
            &already,
            hash_for("per-file-already"),
            pdf_with_embedded_doi("10.1000/per-file-already"),
        );
    let crossref = KeyedSource::new(
        SourceName::Crossref,
        vec![
            (
                "10.1000/per-file-orig1",
                record_by("Smith", 2024, "10.1000/per-file-orig1"),
            ),
            (
                "10.1000/per-file-orig2",
                record_by("Smith", 2024, "10.1000/per-file-orig2"),
            ),
            (
                "10.1000/per-file-already",
                record_by("Jones", 2020, "10.1000/per-file-already"),
            ),
        ],
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: None,
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![
                blank.clone(),
                paper1.clone(),
                paper2.clone(),
                already.clone(),
            ],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        sequence(&events),
        vec![
            (blank.clone(), "skipped-no-identifier"),
            (paper1.clone(), "resolved"),
            (paper1.clone(), "planned"),
            (paper1.clone(), "sidecar"),
            (paper2.clone(), "resolved"),
            (paper2.clone(), "planned"),
            (paper2.clone(), "sidecar"),
            (already.clone(), "resolved"),
            (already.clone(), "skipped-already-named"),
            (already.clone(), "sidecar"),
        ],
        "got {events:?}"
    );
}

// ---------------------------------------------------------------------
// 3.2: ordering does not move a name, a suffix, or a journal entry
// ---------------------------------------------------------------------

/// The same four-file batch [`each_files_resolution_plan_and_sidecar_are_adjacent_in_input_order`]
/// orders, without sidecars, so the whole stream can be pinned by
/// equality: the interleaving 3.1 requires must not be reachable by
/// assigning `paper2` the unsuffixed name, or by leaving `already`
/// unskipped.
fn mixed_batch() -> (PathBuf, PathBuf, PathBuf, PathBuf, FakeLibrary, KeyedSource) {
    let blank = PathBuf::from("/lib/blank.pdf");
    let paper1 = PathBuf::from("/lib/paper1.pdf");
    let paper2 = PathBuf::from("/lib/paper2.pdf");
    let already = PathBuf::from("/lib/Jones2020.pdf");

    let library = FakeLibrary::new()
        .with_file(&blank, hash_for("per-file-blank"), pdf_with_no_identifier())
        .with_file(
            &paper1,
            hash_for("per-file-paper1"),
            pdf_with_embedded_doi("10.1000/per-file-orig1"),
        )
        .with_file(
            &paper2,
            hash_for("per-file-paper2"),
            pdf_with_embedded_doi("10.1000/per-file-orig2"),
        )
        .with_file(
            &already,
            hash_for("per-file-already"),
            pdf_with_embedded_doi("10.1000/per-file-already"),
        );
    let crossref = KeyedSource::new(
        SourceName::Crossref,
        vec![
            (
                "10.1000/per-file-orig1",
                record_by("Smith", 2024, "10.1000/per-file-orig1"),
            ),
            (
                "10.1000/per-file-orig2",
                record_by("Smith", 2024, "10.1000/per-file-orig2"),
            ),
            (
                "10.1000/per-file-already",
                record_by("Jones", 2020, "10.1000/per-file-already"),
            ),
        ],
    );

    (blank, paper1, paper2, already, library, crossref)
}

#[test]
fn the_same_mixed_batch_keeps_its_names_and_suffix_in_preview() {
    let (blank, paper1, paper2, already, library, crossref) = mixed_batch();
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![
                blank.clone(),
                paper1.clone(),
                paper2.clone(),
                already.clone(),
            ],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::Skipped {
                path: blank,
                reason: SkipReason::NoIdentifier,
            },
            resolved_event(
                &paper1,
                "doi:10.1000/per-file-orig1",
                &record_by("Smith", 2024, "10.1000/per-file-orig1"),
            ),
            Event::Planned {
                path: paper1,
                target: PathBuf::from("/lib/Smith2024.pdf"),
            },
            resolved_event(
                &paper2,
                "doi:10.1000/per-file-orig2",
                &record_by("Smith", 2024, "10.1000/per-file-orig2"),
            ),
            // The second file to claim "Smith2024.pdf" takes the first
            // free suffix, exactly as planning the batch as a whole
            // would assign it.
            Event::Planned {
                path: paper2,
                target: PathBuf::from("/lib/Smith2024a.pdf"),
            },
            resolved_event(
                &already,
                "doi:10.1000/per-file-already",
                &record_by("Jones", 2020, "10.1000/per-file-already"),
            ),
            Event::Skipped {
                path: already,
                reason: SkipReason::AlreadyNamed,
            },
        ],
        "got {events:?}"
    );
}

#[test]
fn the_same_mixed_batch_keeps_its_names_suffix_and_journal_entries_when_applied() {
    let (blank, paper1, paper2, already, library, crossref) = mixed_batch();
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let journal = FakeJournal::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: Some(&journal as &dyn Journal),
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![
                blank.clone(),
                paper1.clone(),
                paper2.clone(),
                already.clone(),
            ],
            apply: true,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    let target1 = PathBuf::from("/lib/Smith2024.pdf");
    let target2 = PathBuf::from("/lib/Smith2024a.pdf");

    assert_eq!(
        events,
        vec![
            Event::Skipped {
                path: blank,
                reason: SkipReason::NoIdentifier,
            },
            resolved_event(
                &paper1,
                "doi:10.1000/per-file-orig1",
                &record_by("Smith", 2024, "10.1000/per-file-orig1"),
            ),
            Event::Renamed {
                path: paper1.clone(),
                target: target1.clone(),
            },
            resolved_event(
                &paper2,
                "doi:10.1000/per-file-orig2",
                &record_by("Smith", 2024, "10.1000/per-file-orig2"),
            ),
            Event::Renamed {
                path: paper2.clone(),
                target: target2.clone(),
            },
            resolved_event(
                &already,
                "doi:10.1000/per-file-already",
                &record_by("Jones", 2020, "10.1000/per-file-already"),
            ),
            Event::Skipped {
                path: already,
                reason: SkipReason::AlreadyNamed,
            },
        ],
        "got {events:?}"
    );
    assert_eq!(
        filesystem.renames(),
        vec![
            (paper1.clone(), target1.clone()),
            (paper2.clone(), target2.clone()),
        ],
        "only the two renamed files must be moved, in plan order"
    );
    assert_eq!(
        journal.appended(),
        vec![
            Entry {
                run: RunId::new(fixed_now()),
                from: paper1,
                to: target1,
                hash: hash_for("per-file-paper1"),
                at: fixed_now(),
            },
            Entry {
                run: RunId::new(fixed_now()),
                from: paper2,
                to: target2,
                hash: hash_for("per-file-paper2"),
                at: fixed_now(),
            },
        ],
        "got {:?}",
        journal.appended()
    );
}

// ---------------------------------------------------------------------
// 3.3: two directories report as two uninterleaved groups
// ---------------------------------------------------------------------

#[test]
fn a_parent_and_its_subdirectory_report_as_two_uninterleaved_groups_each_under_its_own_template() {
    let a = PathBuf::from("/library/a.pdf");
    let b = PathBuf::from("/library/b.pdf");
    let c = PathBuf::from("/library/sub/c.pdf");
    let d = PathBuf::from("/library/sub/d.pdf");

    let library = FakeLibrary::new()
        .with_file(
            &a,
            hash_for("per-file-a"),
            pdf_with_embedded_doi("10.1000/per-file-a"),
        )
        .with_file(
            &b,
            hash_for("per-file-b"),
            pdf_with_embedded_doi("10.1000/per-file-b"),
        )
        .with_file(
            &c,
            hash_for("per-file-c"),
            pdf_with_embedded_doi("10.1000/per-file-c"),
        )
        .with_file(
            &d,
            hash_for("per-file-d"),
            pdf_with_embedded_doi("10.1000/per-file-d"),
        );
    let crossref = KeyedSource::new(
        SourceName::Crossref,
        vec![
            (
                "10.1000/per-file-a",
                record_by("Aaron", 2020, "10.1000/per-file-a"),
            ),
            (
                "10.1000/per-file-b",
                record_by("Baker", 2021, "10.1000/per-file-b"),
            ),
            (
                "10.1000/per-file-c",
                record_by("Carol", 2022, "10.1000/per-file-c"),
            ),
            (
                "10.1000/per-file-d",
                record_by("Dave", 2023, "10.1000/per-file-d"),
            ),
        ],
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;

    // The parent directory and its subdirectory each carry their own
    // override, so a file's target depends on which of the two it was
    // read from.
    let read = fake_read(&[
        (
            "/library/.borax.toml",
            "[templates]\ndefault = \"[auth][year]\"",
        ),
        (
            "/library/sub/.borax.toml",
            "[templates]\ndefault = \"[year]-[auth]\"",
        ),
    ]);
    // Paths interleave the two directories on purpose: grouping by
    // directory must reassemble them into two contiguous groups
    // whatever order they arrive in.
    let paths = vec![a.clone(), c.clone(), b.clone(), d.clone()];
    let configs =
        Configs::resolve(&paths, Path::new("/library"), vec![], &Vec::new(), &read).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
    };

    let events = events_for(
        &Command::Rename {
            paths: paths.clone(),
            apply: false,
        },
        &configs,
        &adapters,
    )
    .unwrap();

    // The parent group (first reached, through `a`) is reported whole
    // before the subdirectory group, and within each group `a` precedes
    // `b`, `c` precedes `d` — the order the paths were given in, once
    // grouped by directory.
    assert_eq!(
        sequence(&events),
        vec![
            (a.clone(), "resolved"),
            (a.clone(), "planned"),
            (b.clone(), "resolved"),
            (b.clone(), "planned"),
            (c.clone(), "resolved"),
            (c.clone(), "planned"),
            (d.clone(), "resolved"),
            (d.clone(), "planned"),
        ],
        "got {events:?}"
    );

    let target_of = |path: &Path| {
        events
            .iter()
            .find_map(|event| match event {
                Event::Planned { path: p, target } if p == path => Some(target.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no Planned event for {}", path.display()))
    };
    assert_eq!(target_of(&a), PathBuf::from("/library/Aaron2020.pdf"));
    assert_eq!(target_of(&b), PathBuf::from("/library/Baker2021.pdf"));
    assert_eq!(target_of(&c), PathBuf::from("/library/sub/2022-Carol.pdf"));
    assert_eq!(target_of(&d), PathBuf::from("/library/sub/2023-Dave.pdf"));
}

// ---------------------------------------------------------------------
// 3.6: the master bib merge stays a trailing block
// ---------------------------------------------------------------------

/// Pins the property `design.md` states for a group with bibliography
/// output configured: "one contiguous block of lines per file, then the
/// master-bibliography lines." Sidecars are per-file work and move into
/// each file's own block; the master merge is batch work — a
/// read-modify-write of the whole file — and stays once per group,
/// after every file's own lines.
#[test]
fn master_bib_entries_trail_every_files_resolve_plan_and_sidecar_block_in_input_order() {
    let paper1 = PathBuf::from("/lib/paper1.pdf");
    let paper2 = PathBuf::from("/lib/paper2.pdf");
    let paper3 = PathBuf::from("/lib/paper3.pdf");

    let library = FakeLibrary::new()
        .with_file(
            &paper1,
            hash_for("per-file-bib-1"),
            pdf_with_embedded_doi("10.1000/per-file-bib-1"),
        )
        .with_file(
            &paper2,
            hash_for("per-file-bib-2"),
            pdf_with_embedded_doi("10.1000/per-file-bib-2"),
        )
        .with_file(
            &paper3,
            hash_for("per-file-bib-3"),
            pdf_with_embedded_doi("10.1000/per-file-bib-3"),
        );
    let crossref = KeyedSource::new(
        SourceName::Crossref,
        vec![
            (
                "10.1000/per-file-bib-1",
                record_by("Aaron", 2020, "10.1000/per-file-bib-1"),
            ),
            (
                "10.1000/per-file-bib-2",
                record_by("Baker", 2021, "10.1000/per-file-bib-2"),
            ),
            (
                "10.1000/per-file-bib-3",
                record_by("Carol", 2022, "10.1000/per-file-bib-3"),
            ),
        ],
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
        layer.bib = Some(BibLayer {
            path: Some(PathBuf::from("/lib/refs.bib")),
            duplicates: None,
            sidecars: Some(true),
        });
    });
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![paper1.clone(), paper2.clone(), paper3.clone()],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        sequence(&events),
        vec![
            (paper1.clone(), "resolved"),
            (paper1.clone(), "planned"),
            (paper1.clone(), "sidecar"),
            (paper2.clone(), "resolved"),
            (paper2.clone(), "planned"),
            (paper2.clone(), "sidecar"),
            (paper3.clone(), "resolved"),
            (paper3.clone(), "planned"),
            (paper3.clone(), "sidecar"),
            (paper1.clone(), "bib-entry"),
            (paper2.clone(), "bib-entry"),
            (paper3.clone(), "bib-entry"),
        ],
        "got {events:?}"
    );
}
