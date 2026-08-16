#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use borax::bib::{BibConfig, BibFiles, citation_key, sidecar_path, write_bib};
use borax::event::{Event, SkipReason};
use borax::pipeline::FileRecord;
use borax_core::bib_output::{DuplicatePolicy, MergeOutcome, merge, sidecar};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::Doi;
use borax_core::record::{DateParts, EntryType, Name, Record};
use borax_core::template::{RenderInput, Template, TemplateTable};

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`BibFiles`] fake backed by an initial map of path to content, with
/// per-path failures for `read` and `write`, following the shape of the
/// fakes in `pipeline.rs` and `renaming.rs`.
///
/// Every call to `write` is logged in `write_attempts`, whether or not it
/// fails, so a test can prove exactly how many times a path was written
/// to even when that write is made to fail. Only the writes that
/// succeeded are kept, with their content, in `writes`.
struct FakeBibFiles {
    initial: Vec<(PathBuf, String)>,
    read_failures: BTreeSet<PathBuf>,
    write_failures: BTreeSet<PathBuf>,
    read_calls: RefCell<Vec<PathBuf>>,
    write_attempts: RefCell<Vec<PathBuf>>,
    writes: RefCell<Vec<(PathBuf, String)>>,
}

impl FakeBibFiles {
    fn new() -> FakeBibFiles {
        FakeBibFiles {
            initial: Vec::new(),
            read_failures: BTreeSet::new(),
            write_failures: BTreeSet::new(),
            read_calls: RefCell::new(Vec::new()),
            write_attempts: RefCell::new(Vec::new()),
            writes: RefCell::new(Vec::new()),
        }
    }

    /// Seed `path` with `content`, as if a previous run had already
    /// written it.
    fn with_initial(
        mut self,
        path: impl Into<PathBuf>,
        content: impl Into<String>,
    ) -> FakeBibFiles {
        self.initial.push((path.into(), content.into()));
        self
    }

    /// Make every `read` of `path` fail.
    fn with_read_failure(mut self, path: impl Into<PathBuf>) -> FakeBibFiles {
        self.read_failures.insert(path.into());
        self
    }

    /// Make every `write` to `path` fail.
    fn with_write_failure(mut self, path: impl Into<PathBuf>) -> FakeBibFiles {
        self.write_failures.insert(path.into());
        self
    }

    /// Every path `write` was called with, in call order, regardless of
    /// whether that call succeeded.
    fn write_attempts(&self) -> Vec<PathBuf> {
        self.write_attempts.borrow().clone()
    }

    /// The `(path, content)` pairs of every `write` call that succeeded,
    /// in call order.
    fn writes(&self) -> Vec<(PathBuf, String)> {
        self.writes.borrow().clone()
    }

    /// Every path `read` was called with, in call order.
    fn read_calls(&self) -> Vec<PathBuf> {
        self.read_calls.borrow().clone()
    }
}

impl BibFiles for FakeBibFiles {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.read_calls.borrow_mut().push(path.to_path_buf());
        if self.read_failures.contains(path) {
            return Err(io::Error::other("fake read failure"));
        }
        Ok(self
            .initial
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, content)| content.clone())
            .unwrap_or_default())
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        self.write_attempts.borrow_mut().push(path.to_path_buf());
        if self.write_failures.contains(path) {
            return Err(io::Error::other("fake write failure"));
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

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
}

fn compile(source: &str) -> Template {
    Template::compile(source).unwrap()
}

/// A [`TemplateTable`] whose default is `source`.
fn table(source: &str) -> TemplateTable {
    TemplateTable::new(compile(source))
}

/// An `Article` by one author in the given year, with an optional DOI.
/// The title is fixed and irrelevant to every test that does not select
/// it through a template.
fn record(author: &str, year: i32, doi_value: Option<&str>) -> Record {
    Record {
        title: Some("A Study of Borax".to_string()),
        authors: vec![Name {
            family: author.to_string(),
            given: None,
        }],
        issued: Some(DateParts {
            year,
            month: None,
            day: None,
        }),
        doi: doi_value.map(doi),
        ..Record::new(EntryType::Article)
    }
}

fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A resolved file at `path` carrying `record`, with the provenance
/// fields bibliography output does not look at.
fn resolved(path: &str, record: Record) -> (PathBuf, FileRecord) {
    (
        PathBuf::from(path),
        FileRecord {
            record,
            source: None,
            tier: None,
            cached: false,
            hash: None,
        },
    )
}

fn bib_config(path: Option<&str>, duplicates: DuplicatePolicy, sidecars: bool) -> BibConfig {
    BibConfig {
        path: path.map(PathBuf::from),
        duplicates,
        sidecars,
    }
}

/// The outcome string [`write_bib`]'s `Event::BibEntry` reports for a
/// [`MergeOutcome`], per the doc comment: `added`, `already-present`, or
/// `updated`.
fn outcome_name(outcome: &MergeOutcome) -> &'static str {
    match outcome {
        MergeOutcome::Added { .. } => "added",
        MergeOutcome::AlreadyPresent { .. } => "already-present",
        MergeOutcome::Updated { .. } => "updated",
    }
}

/// The key an `Event::BibEntry` names for a [`MergeOutcome`]: the
/// requested (possibly suffixed) key for an addition or an update, the
/// pre-existing entry's key for a duplicate that was left alone.
fn outcome_key(outcome: &MergeOutcome) -> &str {
    match outcome {
        MergeOutcome::Added { key } | MergeOutcome::Updated { key } => key,
        MergeOutcome::AlreadyPresent { existing_key } => existing_key,
    }
}

/// The `Event::BibEntry` sequence [`write_bib`] should produce for a
/// batch, computed directly from [`borax_core::bib_output::merge`] so the
/// expectation is measured against the real merge rules rather than
/// hand-guessed.
fn bib_entry_events(paths: &[PathBuf], outcomes: &[MergeOutcome]) -> Vec<Event> {
    paths
        .iter()
        .zip(outcomes)
        .map(|(path, outcome)| Event::BibEntry {
            path: path.clone(),
            key: outcome_key(outcome).to_string(),
            outcome: outcome_name(outcome).to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------
// citation_key
// ---------------------------------------------------------------------

#[test]
fn citation_key_renders_from_the_template_table() {
    let record = record("Smith", 2024, None);
    let templates = table("[auth][year]");

    let key = citation_key(&record, None, &templates);

    assert_eq!(key, Some("Smith2024".to_string()));
}

#[test]
fn citation_key_is_none_for_a_record_too_sparse_to_render_anything() {
    let record = Record::new(EntryType::Article);
    let templates = table("[title]");

    let key = citation_key(&record, None, &templates);

    assert_eq!(key, None);
}

/// The template's literal text alone carries every character the doc
/// comment names as stripped (`,` `{` `}` `%` and whitespace) interleaved
/// with characters a BibTeX key can carry, so the expected result is
/// exact rather than a guess about which characters survive.
#[test]
fn citation_key_strips_commas_braces_whitespace_and_percent_but_keeps_other_characters() {
    let record = record("Smith", 2024, None);
    let templates = table("a,b{c}d%e f-g_h:i.j");

    let key = citation_key(&record, None, &templates);

    assert_eq!(key, Some("abcdef-g_h:i.j".to_string()));
}

#[test]
fn citation_key_is_none_when_stripping_removes_everything_the_template_rendered() {
    let record = record("Smith", 2024, None);
    let templates = table(" ,{}%\t");

    let key = citation_key(&record, None, &templates);

    assert_eq!(key, None);
}

#[test]
fn citation_key_renders_the_sha1_field_from_the_supplied_hash() {
    let record = Record::new(EntryType::Article);
    let templates = table("[sha1]");
    let hash = hash_of("file contents");

    let key = citation_key(&record, Some(&hash), &templates);

    assert_eq!(key, Some(hash.as_str().to_string()));
}

/// Cross-check: the template render alone (no stripping involved, since
/// this template renders only letters and digits) already matches
/// [`TemplateTable::render`] called directly, pinning that `citation_key`
/// uses the table the way [`crate::renaming::target_name`] does.
#[test]
fn citation_key_matches_a_direct_template_render_when_nothing_needs_stripping() {
    let record = record("Doe", 2020, None);
    let templates = table("[auth][year]");
    let direct = templates.render(&RenderInput {
        record: &record,
        sha1: None,
    });

    let key = citation_key(&record, None, &templates);

    assert_eq!(key, Some(direct));
}

// ---------------------------------------------------------------------
// sidecar_path
// ---------------------------------------------------------------------

// The extension is appended rather than replaced, so a sidecar can
// never land on a name a user's own `.bib` might already hold.
#[test]
fn sidecar_path_appends_the_extension() {
    let path = Path::new("dir/paper.pdf");
    assert_eq!(sidecar_path(path), PathBuf::from("dir/paper.pdf.bib"));
}

#[test]
fn sidecar_path_appends_the_extension_when_the_path_has_none() {
    let path = Path::new("dir/paper");
    assert_eq!(sidecar_path(path), PathBuf::from("dir/paper.bib"));
}

#[test]
fn sidecar_path_keeps_every_dot_in_the_name() {
    let path = Path::new("dir/my.paper.v2.pdf");
    assert_eq!(sidecar_path(path), PathBuf::from("dir/my.paper.v2.pdf.bib"));
}

// ---------------------------------------------------------------------
// write_bib: sidecars off, no master — a no-op
// ---------------------------------------------------------------------

#[test]
fn sidecars_off_and_no_master_path_performs_no_file_operations_and_produces_no_events() {
    let resolved = [resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")))];
    let config = bib_config(None, DuplicatePolicy::Skip, false);
    let files = FakeBibFiles::new();

    let events = write_bib(&resolved, &table("[auth][year]"), &config, &files);

    assert_eq!(events, Vec::new());
    assert!(
        files.read_calls().is_empty(),
        "no read should happen with no master path"
    );
    assert!(
        files.write_attempts().is_empty(),
        "no write should happen with sidecars off and no master path"
    );
}

// ---------------------------------------------------------------------
// write_bib: sidecars
// ---------------------------------------------------------------------

#[test]
fn sidecars_on_emits_one_sidecar_event_per_file_with_the_documented_content() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let p2 = resolved("b.pdf", record("Doe", 2023, Some("10.1000/b")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(None, DuplicatePolicy::Skip, true);
    let files = FakeBibFiles::new();

    let events = write_bib(&resolved, &templates, &config, &files);

    let target1 = sidecar_path(&p1.0);
    let target2 = sidecar_path(&p2.0);
    assert_eq!(
        events,
        vec![
            Event::Sidecar {
                path: p1.0.clone(),
                target: target1.clone(),
            },
            Event::Sidecar {
                path: p2.0.clone(),
                target: target2.clone(),
            },
        ]
    );
    assert_eq!(
        files.writes(),
        vec![
            (target1, sidecar(&p1.1.record, "Smith2024")),
            (target2, sidecar(&p2.1.record, "Doe2023")),
        ]
    );
}

// A file borax did not write is never overwritten, whatever it holds.
#[test]
fn a_sidecar_target_holding_foreign_content_is_left_alone_and_reported() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let resolved = [p1.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(None, DuplicatePolicy::Skip, true);
    let target = sidecar_path(&p1.0);
    let files = FakeBibFiles::new().with_initial(
        target.clone(),
        "@article{mine2020, title = {Notes I keep by hand}}\n",
    );

    let events = write_bib(&resolved, &templates, &config, &files);

    assert_eq!(
        events,
        vec![Event::Skipped {
            path: p1.0.clone(),
            reason: SkipReason::SidecarTaken {
                target: target.clone(),
            },
        }]
    );
    assert!(
        files.writes().is_empty(),
        "a foreign sidecar target must not be written to"
    );
}

// A sidecar borax wrote is recognisable by its record marker, and
// rewriting one is how a re-run keeps it current.
#[test]
fn a_sidecar_borax_wrote_before_is_overwritten() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let resolved = [p1.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(None, DuplicatePolicy::Skip, true);
    let target = sidecar_path(&p1.0);
    let stale = record("Smith", 1999, Some("10.1000/a"));
    let files = FakeBibFiles::new().with_initial(target.clone(), sidecar(&stale, "Smith1999"));

    let events = write_bib(&resolved, &templates, &config, &files);

    assert_eq!(
        events,
        vec![Event::Sidecar {
            path: p1.0.clone(),
            target: target.clone(),
        }]
    );
    assert_eq!(
        files.writes(),
        vec![(target, sidecar(&p1.1.record, "Smith2024"))]
    );
}

// ---------------------------------------------------------------------
// write_bib: master file, one pass
// ---------------------------------------------------------------------

#[test]
fn master_path_set_merges_every_keyed_record_in_one_pass_and_writes_the_file_once() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let p2 = resolved("b.pdf", record("Doe", 2023, Some("10.1000/b")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, false);
    let files = FakeBibFiles::new();

    let events = write_bib(&resolved, &templates, &config, &files);

    let expected = merge(
        "",
        &[("Smith2024", &p1.1.record), ("Doe2023", &p2.1.record)],
        DuplicatePolicy::Skip,
    );
    assert_eq!(
        events,
        bib_entry_events(&[p1.0.clone(), p2.0.clone()], &expected.outcomes)
    );
    assert_eq!(
        files.write_attempts(),
        vec![PathBuf::from("refs.bib")],
        "the master file must be written exactly once"
    );
    assert_eq!(
        files.writes(),
        vec![(PathBuf::from("refs.bib"), expected.content)]
    );
    assert_eq!(files.read_calls(), vec![PathBuf::from("refs.bib")]);
}

#[test]
fn master_duplicate_policy_skip_reports_already_present_against_an_existing_entry() {
    let existing = record("Smith", 2020, Some("10.1000/dup"));
    let seed = merge("", &[("smith2020", &existing)], DuplicatePolicy::Skip);

    let p1 = resolved("a.pdf", existing.clone());
    let p2 = resolved("b.pdf", record("Doe", 2023, Some("10.1000/new")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, false);
    let files = FakeBibFiles::new().with_initial("refs.bib", seed.content.clone());

    let events = write_bib(&resolved, &templates, &config, &files);

    let expected = merge(
        &seed.content,
        &[("Smith2020", &p1.1.record), ("Doe2023", &p2.1.record)],
        DuplicatePolicy::Skip,
    );
    assert_eq!(
        events,
        bib_entry_events(&[p1.0.clone(), p2.0.clone()], &expected.outcomes)
    );
    assert!(matches!(
        &events[0],
        Event::BibEntry { outcome, .. } if outcome == "already-present"
    ));
}

#[test]
fn master_duplicate_policy_update_reports_updated_against_an_existing_entry() {
    let existing = record("Smith", 2020, Some("10.1000/dup"));
    let seed = merge("", &[("smith2020", &existing)], DuplicatePolicy::Skip);

    let updated = record("Smith", 2021, Some("10.1000/dup"));
    let p1 = resolved("a.pdf", updated.clone());
    let resolved = [p1.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Update, false);
    let files = FakeBibFiles::new().with_initial("refs.bib", seed.content.clone());

    let events = write_bib(&resolved, &templates, &config, &files);

    let expected = merge(
        &seed.content,
        &[("Smith2021", &p1.1.record)],
        DuplicatePolicy::Update,
    );
    assert_eq!(
        events,
        bib_entry_events(std::slice::from_ref(&p1.0), &expected.outcomes)
    );
    assert!(matches!(
        &events[0],
        Event::BibEntry { outcome, .. } if outcome == "updated"
    ));
}

/// Merging as one pass is what buys the merge the ability to suffix
/// colliding keys across the whole run; two records with distinct
/// identities that render the same requested key must therefore leave
/// under distinct keys, matching what a single call to `merge` with both
/// additions produces.
#[test]
fn two_records_rendering_the_same_citation_key_leave_under_distinct_suffixed_keys() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/first")));
    let p2 = resolved("b.pdf", record("Smith", 2024, Some("10.1000/second")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, false);
    let files = FakeBibFiles::new();

    let events = write_bib(&resolved, &templates, &config, &files);

    let expected = merge(
        "",
        &[("Smith2024", &p1.1.record), ("Smith2024", &p2.1.record)],
        DuplicatePolicy::Skip,
    );
    assert_eq!(
        events,
        bib_entry_events(&[p1.0.clone(), p2.0.clone()], &expected.outcomes)
    );
    let keys: Vec<&str> = events
        .iter()
        .map(|event| match event {
            Event::BibEntry { key, .. } => key.as_str(),
            other => panic!("expected BibEntry, got {other:?}"),
        })
        .collect();
    assert_ne!(
        keys[0], keys[1],
        "colliding keys must be told apart: got {keys:?}"
    );
}

// ---------------------------------------------------------------------
// write_bib: unciteable records
// ---------------------------------------------------------------------

#[test]
fn a_record_with_no_citation_key_is_skipped_as_unciteable_and_touches_neither_destination() {
    let mystery = resolved("mystery.pdf", Record::new(EntryType::Article));
    let mut named = record("Smith", 2024, Some("10.1000/named"));
    named.title = Some("Borax".to_string());
    let keyed = resolved("keyed.pdf", named);
    let resolved = [mystery.clone(), keyed.clone()];
    let templates = table("[title]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, true);
    let files = FakeBibFiles::new();

    let events = write_bib(&resolved, &templates, &config, &files);

    let expected_merge = merge("", &[("Borax", &keyed.1.record)], DuplicatePolicy::Skip);
    assert_eq!(
        events,
        vec![
            Event::Skipped {
                path: mystery.0.clone(),
                reason: SkipReason::Unciteable,
            },
            Event::Sidecar {
                path: keyed.0.clone(),
                target: sidecar_path(&keyed.0),
            },
        ]
        .into_iter()
        .chain(bib_entry_events(
            std::slice::from_ref(&keyed.0),
            &expected_merge.outcomes,
        ))
        .collect::<Vec<_>>()
    );
    assert_eq!(
        files.write_attempts().len(),
        2,
        "the unciteable file must not reach either destination: {:?}",
        files.write_attempts()
    );
}

// ---------------------------------------------------------------------
// write_bib: ordering — every sidecar event, then every master event
// ---------------------------------------------------------------------

#[test]
fn events_come_back_as_every_sidecar_and_unciteable_event_in_input_order_then_every_master_event_in_input_order()
 {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let p2 = resolved("mystery.pdf", Record::new(EntryType::Article));
    let p3 = resolved("c.pdf", record("Doe", 2023, Some("10.1000/c")));
    let p4 = resolved("another-mystery.pdf", Record::new(EntryType::Article));
    let resolved = [p1.clone(), p2.clone(), p3.clone(), p4.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, true);
    let files = FakeBibFiles::new();

    let events = write_bib(&resolved, &templates, &config, &files);

    let expected_merge = merge(
        "",
        &[("Smith2024", &p1.1.record), ("Doe2023", &p3.1.record)],
        DuplicatePolicy::Skip,
    );
    let mut expected = vec![
        Event::Sidecar {
            path: p1.0.clone(),
            target: sidecar_path(&p1.0),
        },
        Event::Skipped {
            path: p2.0.clone(),
            reason: SkipReason::Unciteable,
        },
        Event::Sidecar {
            path: p3.0.clone(),
            target: sidecar_path(&p3.0),
        },
        Event::Skipped {
            path: p4.0.clone(),
            reason: SkipReason::Unciteable,
        },
    ];
    expected.extend(bib_entry_events(
        &[p1.0.clone(), p3.0.clone()],
        &expected_merge.outcomes,
    ));

    assert_eq!(events, expected);
}

// ---------------------------------------------------------------------
// write_bib: failures continue the run
// ---------------------------------------------------------------------

#[test]
fn a_sidecar_write_failure_is_skipped_for_that_file_alone_and_the_batch_continues() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let p2 = resolved("b.pdf", record("Doe", 2023, Some("10.1000/b")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(None, DuplicatePolicy::Skip, true);
    let files = FakeBibFiles::new().with_write_failure(sidecar_path(&p1.0));

    let events = write_bib(&resolved, &templates, &config, &files);

    assert_eq!(events.len(), 2);
    assert!(
        matches!(
            &events[0],
            Event::Skipped {
                path,
                reason: SkipReason::BibWriteFailed { .. },
            } if *path == p1.0
        ),
        "expected the failing file to be BibWriteFailed, got {:?}",
        events[0]
    );
    assert_eq!(
        events[1],
        Event::Sidecar {
            path: p2.0.clone(),
            target: sidecar_path(&p2.0),
        },
        "the other file's sidecar must still be written"
    );
}

#[test]
fn a_master_read_failure_skips_every_keyed_file_as_bib_write_failed_and_writes_nothing() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let p2 = resolved("b.pdf", record("Doe", 2023, Some("10.1000/b")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, false);
    let files = FakeBibFiles::new().with_read_failure("refs.bib");

    let events = write_bib(&resolved, &templates, &config, &files);

    assert_eq!(
        events.len(),
        2,
        "both keyed files must be reported: {events:?}"
    );
    for (event, path) in events.iter().zip([&p1.0, &p2.0]) {
        assert!(
            matches!(
                event,
                Event::Skipped {
                    path: got,
                    reason: SkipReason::BibWriteFailed { .. },
                } if got == path
            ),
            "expected BibWriteFailed for {}, got {event:?}",
            path.display()
        );
    }
    assert!(
        files.write_attempts().is_empty(),
        "a failed read must not be followed by a write attempt"
    );
}

#[test]
fn a_master_write_failure_skips_every_keyed_file_as_bib_write_failed_after_one_attempt() {
    let p1 = resolved("a.pdf", record("Smith", 2024, Some("10.1000/a")));
    let p2 = resolved("b.pdf", record("Doe", 2023, Some("10.1000/b")));
    let resolved = [p1.clone(), p2.clone()];
    let templates = table("[auth][year]");
    let config = bib_config(Some("refs.bib"), DuplicatePolicy::Skip, false);
    let files = FakeBibFiles::new().with_write_failure("refs.bib");

    let events = write_bib(&resolved, &templates, &config, &files);

    assert_eq!(
        events.len(),
        2,
        "both keyed files must be reported: {events:?}"
    );
    for (event, path) in events.iter().zip([&p1.0, &p2.0]) {
        assert!(
            matches!(
                event,
                Event::Skipped {
                    path: got,
                    reason: SkipReason::BibWriteFailed { .. },
                } if got == path
            ),
            "expected BibWriteFailed for {}, got {event:?}",
            path.display()
        );
    }
    assert_eq!(
        files.write_attempts(),
        vec![PathBuf::from("refs.bib")],
        "the master file must be attempted exactly once even though it fails"
    );
}
