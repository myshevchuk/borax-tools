#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use borax::event::{Counts, Event, SkipReason};
use borax::pipeline::FileRecord;
use borax::renaming::{
    Applying, Filesystem, PlannedRename, RealFilesystem, RenameError, apply_renames, counts_for,
    plan_renames, target_name,
};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::record::{DateParts, EntryType, Name, Record};
use borax_core::rename::CollisionPolicy;
use borax_core::template::{Template, TemplateTable};
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`Filesystem`] fake backed by a map from directory to the names
/// present there (with optional content hashes), following the shape of
/// `FakeLibrary` in `pipeline.rs`.
///
/// Every [`Filesystem::rename`] call is recorded in order, so a test can
/// assert exactly which moves happened — including that none did. A
/// specific `from` path can be made to fail.
struct FakeFilesystem {
    existing: BTreeMap<PathBuf, BTreeMap<String, Option<String>>>,
    failures: BTreeMap<PathBuf, String>,
    renames: RefCell<Vec<(PathBuf, PathBuf)>>,
}

impl FakeFilesystem {
    fn new() -> FakeFilesystem {
        FakeFilesystem {
            existing: BTreeMap::new(),
            failures: BTreeMap::new(),
            renames: RefCell::new(Vec::new()),
        }
    }

    /// Populate `directory` with `names`, each paired with the content
    /// hash known for it (`None` when unknown).
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

    /// Make the next call to `rename` with this `from` path fail with
    /// `message`.
    fn with_failure(
        mut self,
        from: impl Into<PathBuf>,
        message: impl Into<String>,
    ) -> FakeFilesystem {
        self.failures.insert(from.into(), message.into());
        self
    }

    /// The `(from, to)` pairs passed to `rename`, in call order. Empty
    /// means nothing was moved — the assertion a preview test rests on.
    fn renames(&self) -> Vec<(PathBuf, PathBuf)> {
        self.renames.borrow().clone()
    }
}

impl Filesystem for FakeFilesystem {
    fn existing(&self, directory: &Path) -> BTreeMap<String, Option<String>> {
        self.existing.get(directory).cloned().unwrap_or_default()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError> {
        if let Some(message) = self.failures.get(from) {
            return Err(RenameError {
                message: message.clone(),
            });
        }
        self.renames
            .borrow_mut()
            .push((from.to_path_buf(), to.to_path_buf()));
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

fn author(family: &str) -> Name {
    Name {
        family: family.to_string(),
        given: None,
    }
}

/// A minimal `Article` record by one author in the given year.
fn record_by(family: &str, year: i32) -> Record {
    Record {
        title: None,
        authors: vec![author(family)],
        issued: Some(DateParts {
            year,
            month: None,
            day: None,
        }),
        ..Record::new(EntryType::Article)
    }
}

/// A record with no fields a template could render into a name.
fn empty_record() -> Record {
    Record::new(EntryType::Article)
}

fn compile(source: &str) -> Template {
    Template::compile(source).unwrap()
}

/// A [`TemplateTable`] whose default is `source`.
fn table(source: &str) -> TemplateTable {
    TemplateTable::new(compile(source))
}

/// A [`TemplateTable`] whose default is `default` and whose template for
/// `entry_type` is `specific`.
fn table_with(default: &str, entry_type: EntryType, specific: &str) -> TemplateTable {
    let mut table = table(default);
    table.insert(entry_type, compile(specific));
    table
}

fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A resolved file at `path`, carrying `record` and `hash`, with the
/// provenance fields renaming does not look at.
fn resolved(path: &str, record: Record, hash: Option<ContentHash>) -> (PathBuf, FileRecord) {
    (
        PathBuf::from(path),
        FileRecord {
            record,
            source: None,
            tier: None,
            cached: false,
            hash,
        },
    )
}

fn rename(path: &str, target: &str) -> PlannedRename {
    PlannedRename::Rename {
        path: PathBuf::from(path),
        target: PathBuf::from(target),
    }
}

fn already_named(path: &str) -> PlannedRename {
    PlannedRename::AlreadyNamed {
        path: PathBuf::from(path),
    }
}

fn target_taken(path: &str, target: &str) -> PlannedRename {
    PlannedRename::TargetTaken {
        path: PathBuf::from(path),
        target: PathBuf::from(target),
    }
}

fn unnameable(path: &str) -> PlannedRename {
    PlannedRename::Unnameable {
        path: PathBuf::from(path),
    }
}

// ---------------------------------------------------------------------
// target_name
// ---------------------------------------------------------------------

#[test]
fn renders_through_the_template_sanitizes_and_reattaches_the_extension() {
    let path = Path::new("/library/original.pdf");
    let record = record_by("Smith", 2024);
    let templates = table("[auth][year]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("Smith2024.pdf".to_string()));
}

#[test]
fn a_record_whose_template_renders_empty_gives_none() {
    let path = Path::new("/library/original.pdf");
    let record = empty_record();
    let templates = table("[title]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, None);
}

#[test]
fn an_uppercase_extension_is_preserved_exactly() {
    let path = Path::new("/library/original.PDF");
    let record = record_by("Smith", 2024);
    let templates = table("[auth][year]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("Smith2024.PDF".to_string()));
}

#[test]
fn a_file_with_no_extension_gets_none_added() {
    let path = Path::new("/library/original");
    let record = record_by("Smith", 2024);
    let templates = table("[auth][year]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("Smith2024".to_string()));
}

#[test]
fn a_per_entry_type_template_is_chosen_over_the_default() {
    let path = Path::new("/library/original.pdf");
    let mut record = record_by("Jones", 2020);
    record.entry_type = EntryType::Book;
    record.title = Some("Borax Handbook".to_string());
    let templates = table_with("[auth][year]", EntryType::Book, "[title]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("Borax Handbook.pdf".to_string()));
}

#[test]
fn the_default_template_still_applies_to_a_type_with_no_specific_one() {
    let path = Path::new("/library/original.pdf");
    let record = record_by("Jones", 2020);
    let templates = table_with("[auth][year]", EntryType::Book, "[title]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("Jones2020.pdf".to_string()));
}

#[test]
fn sha1_renders_from_the_supplied_hash() {
    let path = Path::new("/library/original.pdf");
    let record = empty_record();
    let templates = table("sha[sha1]");
    let hash = hash_of("file contents");

    let name = target_name(path, &record, Some(&hash), &templates);

    assert_eq!(name, Some(format!("sha{}.pdf", hash.as_str())));
}

#[test]
fn sha1_renders_empty_when_the_hash_is_none() {
    let path = Path::new("/library/original.pdf");
    let record = empty_record();
    let templates = table("sha[sha1]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("sha.pdf".to_string()));
}

#[test]
fn characters_the_sanitizer_strips_yield_the_sanitized_name() {
    // `sanitize` replaces `:` (forbidden on Windows) with `_`.
    let path = Path::new("/library/original.pdf");
    let record = record_by("Smith", 2024);
    let templates = table("[auth]:[year]");

    let name = target_name(path, &record, None, &templates);

    assert_eq!(name, Some("Smith_2024.pdf".to_string()));
}

// ---------------------------------------------------------------------
// plan_renames: happy path and already-named
// ---------------------------------------------------------------------

#[test]
fn distinct_names_in_one_directory_all_become_rename() {
    let resolved = [
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", record_by("Doe", 2023), None),
    ];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
            rename("/lib/b.pdf", "/lib/Doe2023.pdf"),
        ]
    );
}

#[test]
fn a_file_already_carrying_its_target_name_is_already_named() {
    let resolved = [resolved(
        "/lib/Smith2024.pdf",
        record_by("Smith", 2024),
        None,
    )];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(plan, vec![already_named("/lib/Smith2024.pdf")]);
}

// ---------------------------------------------------------------------
// plan_renames: collisions within a directory
// ---------------------------------------------------------------------

#[test]
fn two_files_wanting_the_same_name_are_suffixed_under_the_suffix_policy() {
    let resolved = [
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", record_by("Smith", 2024), None),
    ];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
            rename("/lib/b.pdf", "/lib/Smith2024a.pdf"),
        ]
    );
}

#[test]
fn two_files_wanting_the_same_name_are_target_taken_under_the_skip_policy() {
    let resolved = [
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", record_by("Smith", 2024), None),
    ];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Skip, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
            target_taken("/lib/b.pdf", "/lib/Smith2024.pdf"),
        ]
    );
}

#[test]
fn files_in_different_directories_wanting_the_same_name_do_not_collide() {
    let resolved = [
        resolved("/lib/a/original.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b/original.pdf", record_by("Smith", 2024), None),
    ];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/a/original.pdf", "/lib/a/Smith2024.pdf"),
            rename("/lib/b/original.pdf", "/lib/b/Smith2024.pdf"),
        ]
    );
}

#[test]
fn an_existing_file_occupying_the_target_is_suffixed_under_the_suffix_policy() {
    let resolved = [resolved(
        "/lib/original.pdf",
        record_by("Smith", 2024),
        None,
    )];
    let templates = table("[auth][year]");
    let filesystem =
        FakeFilesystem::new().with_existing("/lib", [("Smith2024.pdf", Some("other-hash"))]);

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![rename("/lib/original.pdf", "/lib/Smith2024a.pdf")]
    );
}

#[test]
fn an_existing_file_occupying_the_target_is_target_taken_under_the_skip_policy() {
    let resolved = [resolved(
        "/lib/original.pdf",
        record_by("Smith", 2024),
        None,
    )];
    let templates = table("[auth][year]");
    let filesystem =
        FakeFilesystem::new().with_existing("/lib", [("Smith2024.pdf", Some("other-hash"))]);

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Skip, &filesystem);

    assert_eq!(
        plan,
        vec![target_taken("/lib/original.pdf", "/lib/Smith2024.pdf")]
    );
}

// ---------------------------------------------------------------------
// plan_renames: nested subdirectory targets
// ---------------------------------------------------------------------

#[test]
fn a_template_with_a_slash_plans_a_rename_into_a_subdirectory() {
    let resolved = [resolved("/lib/a.pdf", record_by("Smith", 2024), None)];
    let templates = table("sub/[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(plan, vec![rename("/lib/a.pdf", "/lib/sub/Smith2024.pdf")]);
}

#[test]
fn a_file_occupying_the_target_subdirectory_is_suffixed_under_the_suffix_policy() {
    let resolved = [resolved("/lib/a.pdf", record_by("Smith", 2024), None)];
    let templates = table("sub/[auth][year]");
    let filesystem =
        FakeFilesystem::new().with_existing("/lib/sub", [("Smith2024.pdf", Some("other-hash"))]);

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(plan, vec![rename("/lib/a.pdf", "/lib/sub/Smith2024a.pdf")]);
}

#[test]
fn a_file_occupying_the_target_subdirectory_is_target_taken_under_the_skip_policy() {
    let resolved = [resolved("/lib/a.pdf", record_by("Smith", 2024), None)];
    let templates = table("sub/[auth][year]");
    let filesystem =
        FakeFilesystem::new().with_existing("/lib/sub", [("Smith2024.pdf", Some("other-hash"))]);

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Skip, &filesystem);

    assert_eq!(
        plan,
        vec![target_taken("/lib/a.pdf", "/lib/sub/Smith2024.pdf")]
    );
}

#[test]
fn two_files_in_one_directory_targeting_the_same_subdirectory_collide() {
    let resolved = [
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", record_by("Smith", 2024), None),
    ];
    let templates = table("sub/[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/a.pdf", "/lib/sub/Smith2024.pdf"),
            rename("/lib/b.pdf", "/lib/sub/Smith2024a.pdf"),
        ]
    );
}

#[test]
fn files_targeting_the_same_name_in_different_subdirectories_do_not_collide() {
    let mut second = record_by("Smith", 2024);
    second.entry_type = EntryType::Book;
    let resolved = [
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", second, None),
    ];
    // The default template files under "x", the Book template under "y",
    // so the two files want the same stem in different subdirectories.
    let templates = table_with("x/[auth][year]", EntryType::Book, "y/[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/a.pdf", "/lib/x/Smith2024.pdf"),
            rename("/lib/b.pdf", "/lib/y/Smith2024.pdf"),
        ]
    );
}

#[test]
fn an_absent_target_subdirectory_plans_normally() {
    let resolved = [resolved("/lib/a.pdf", record_by("Smith", 2024), None)];
    let templates = table("sub/[auth][year]");
    // No `with_existing` call at all for "/lib/sub": the directory is
    // absent, not merely empty.
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(plan, vec![rename("/lib/a.pdf", "/lib/sub/Smith2024.pdf")]);
}

#[test]
fn a_target_subdirectory_explicitly_seeded_as_empty_plans_normally() {
    let resolved = [resolved("/lib/a.pdf", record_by("Smith", 2024), None)];
    let templates = table("sub/[auth][year]");
    let filesystem = FakeFilesystem::new().with_existing("/lib/sub", []);

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(plan, vec![rename("/lib/a.pdf", "/lib/sub/Smith2024.pdf")]);
}

#[test]
fn a_relative_escape_in_the_template_sanitizes_to_a_literal_underscore_directory() {
    // `sanitize` trims a component of nothing but dots down to "" and
    // then substitutes "_", so "../" becomes a literal subdirectory
    // named "_" rather than a step out of the file's own directory.
    let resolved = [resolved(
        "/lib/original.pdf",
        record_by("Smith", 2024),
        None,
    )];
    let templates = table("../[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![rename("/lib/original.pdf", "/lib/_/Smith2024.pdf")]
    );
}

// ---------------------------------------------------------------------
// plan_renames: unnameable records
// ---------------------------------------------------------------------

#[test]
fn a_record_that_renders_no_name_is_unnameable() {
    let resolved = [resolved("/lib/mystery.pdf", empty_record(), None)];
    let templates = table("[title]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(plan, vec![unnameable("/lib/mystery.pdf")]);
}

#[test]
fn an_unnameable_record_does_not_consume_a_collision_slot() {
    // The unnameable file sits between two files that genuinely collide
    // on "Smith2024.pdf". If the planner mistakenly gave the unnameable
    // record's (empty) name a slot in the batch, the second colliding
    // file would be shifted to a suffix other than "a".
    let resolved = [
        resolved("/lib/mystery.pdf", empty_record(), None),
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", record_by("Smith", 2024), None),
    ];
    let templates = table_with("[title]", EntryType::Article, "[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            unnameable("/lib/mystery.pdf"),
            rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
            rename("/lib/b.pdf", "/lib/Smith2024a.pdf"),
        ]
    );
}

// ---------------------------------------------------------------------
// plan_renames: ordering and determinism
// ---------------------------------------------------------------------

#[test]
fn input_order_determines_which_file_gets_the_suffix() {
    let resolved = [
        resolved("/lib/second.pdf", record_by("Smith", 2024), None),
        resolved("/lib/first.pdf", record_by("Smith", 2024), None),
    ];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let plan = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(
        plan,
        vec![
            rename("/lib/second.pdf", "/lib/Smith2024.pdf"),
            rename("/lib/first.pdf", "/lib/Smith2024a.pdf"),
        ]
    );
}

#[test]
fn the_plan_is_deterministic_across_repeated_calls() {
    let resolved = [
        resolved("/lib/a.pdf", record_by("Smith", 2024), None),
        resolved("/lib/b.pdf", record_by("Smith", 2024), None),
        resolved("/lib/c.pdf", record_by("Doe", 2020), None),
    ];
    let templates = table("[auth][year]");
    let filesystem = FakeFilesystem::new();

    let first = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);
    let second = plan_renames(&resolved, &templates, CollisionPolicy::Suffix, &filesystem);

    assert_eq!(first, second);
}

// ---------------------------------------------------------------------
// apply_renames: preview (apply = false)
// ---------------------------------------------------------------------

#[test]
fn preview_reports_every_rename_as_planned_and_moves_nothing() {
    let plan = vec![
        rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
        rename("/lib/b.pdf", "/lib/Doe2023.pdf"),
    ];
    let filesystem = FakeFilesystem::new();

    let events = apply_renames(&plan, &filesystem, false, &[None, None]);

    assert_eq!(
        events,
        vec![
            Event::Planned {
                path: PathBuf::from("/lib/a.pdf"),
                target: PathBuf::from("/lib/Smith2024.pdf"),
            },
            Event::Planned {
                path: PathBuf::from("/lib/b.pdf"),
                target: PathBuf::from("/lib/Doe2023.pdf"),
            },
        ]
    );
    assert!(
        filesystem.renames().is_empty(),
        "a preview run must not move any file"
    );
}

// ---------------------------------------------------------------------
// apply_renames: applying (apply = true)
// ---------------------------------------------------------------------

#[test]
fn applying_reports_every_rename_as_renamed_and_moves_each_one_in_plan_order() {
    let plan = vec![
        rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
        rename("/lib/b.pdf", "/lib/Doe2023.pdf"),
    ];
    let filesystem = FakeFilesystem::new();

    let hashes = [Some(hash_of("a.pdf")), Some(hash_of("b.pdf"))];
    let events = apply_renames(&plan, &filesystem, true, &hashes);

    assert_eq!(
        events,
        vec![
            Event::Renamed {
                path: PathBuf::from("/lib/a.pdf"),
                target: PathBuf::from("/lib/Smith2024.pdf"),
                hash: hash_of("a.pdf"),
            },
            Event::Renamed {
                path: PathBuf::from("/lib/b.pdf"),
                target: PathBuf::from("/lib/Doe2023.pdf"),
                hash: hash_of("b.pdf"),
            },
        ]
    );
    assert_eq!(
        filesystem.renames(),
        vec![
            (
                PathBuf::from("/lib/a.pdf"),
                PathBuf::from("/lib/Smith2024.pdf")
            ),
            (
                PathBuf::from("/lib/b.pdf"),
                PathBuf::from("/lib/Doe2023.pdf")
            ),
        ]
    );
}

#[test]
fn a_failing_rename_is_skipped_with_the_error_message_and_the_batch_continues() {
    let plan = vec![
        rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
        rename("/lib/b.pdf", "/lib/Doe2023.pdf"),
    ];
    let filesystem = FakeFilesystem::new().with_failure("/lib/a.pdf", "permission denied");

    let hashes = [Some(hash_of("a.pdf")), Some(hash_of("b.pdf"))];
    let events = apply_renames(&plan, &filesystem, true, &hashes);

    assert_eq!(
        events,
        vec![
            Event::Skipped {
                path: PathBuf::from("/lib/a.pdf"),
                reason: SkipReason::RenameFailed {
                    message: "permission denied".to_string(),
                },
            },
            Event::Renamed {
                path: PathBuf::from("/lib/b.pdf"),
                target: PathBuf::from("/lib/Doe2023.pdf"),
                hash: hash_of("b.pdf"),
            },
        ]
    );
    assert_eq!(
        filesystem.renames(),
        vec![(
            PathBuf::from("/lib/b.pdf"),
            PathBuf::from("/lib/Doe2023.pdf")
        )]
    );
}

// ---------------------------------------------------------------------
// apply_renames: non-moving variants report identically in both modes
// ---------------------------------------------------------------------

#[test]
fn already_named_reports_the_same_way_in_preview_and_applying() {
    let plan = vec![already_named("/lib/Smith2024.pdf")];
    let filesystem = FakeFilesystem::new();

    let preview = apply_renames(&plan, &filesystem, false, &[None]);
    let applying = apply_renames(&plan, &filesystem, true, &[None]);

    let expected = vec![Event::Skipped {
        path: PathBuf::from("/lib/Smith2024.pdf"),
        reason: SkipReason::AlreadyNamed,
    }];
    assert_eq!(preview, expected);
    assert_eq!(applying, expected);
    assert!(filesystem.renames().is_empty());
}

#[test]
fn target_taken_reports_the_same_way_in_preview_and_applying() {
    let plan = vec![target_taken("/lib/b.pdf", "/lib/Smith2024.pdf")];
    let filesystem = FakeFilesystem::new();

    let preview = apply_renames(&plan, &filesystem, false, &[None]);
    let applying = apply_renames(&plan, &filesystem, true, &[None]);

    let expected = vec![Event::Skipped {
        path: PathBuf::from("/lib/b.pdf"),
        reason: SkipReason::TargetTaken {
            target: PathBuf::from("/lib/Smith2024.pdf"),
        },
    }];
    assert_eq!(preview, expected);
    assert_eq!(applying, expected);
    assert!(filesystem.renames().is_empty());
}

#[test]
fn unnameable_reports_the_same_way_in_preview_and_applying() {
    let plan = vec![unnameable("/lib/mystery.pdf")];
    let filesystem = FakeFilesystem::new();

    let preview = apply_renames(&plan, &filesystem, false, &[None]);
    let applying = apply_renames(&plan, &filesystem, true, &[None]);

    let expected = vec![Event::Skipped {
        path: PathBuf::from("/lib/mystery.pdf"),
        reason: SkipReason::Unnameable,
    }];
    assert_eq!(preview, expected);
    assert_eq!(applying, expected);
    assert!(filesystem.renames().is_empty());
}

// ---------------------------------------------------------------------
// apply_renames: empty plan
// ---------------------------------------------------------------------

#[test]
fn an_empty_plan_produces_no_events_in_either_mode() {
    let filesystem = FakeFilesystem::new();

    assert_eq!(apply_renames(&[], &filesystem, false, &[]), Vec::new());
    assert_eq!(apply_renames(&[], &filesystem, true, &[]), Vec::new());
}

// ---------------------------------------------------------------------
// counts_for
// ---------------------------------------------------------------------

#[test]
fn counts_for_totals_resolved_renamed_and_skipped_events() {
    let events = vec![
        Event::Resolved {
            path: PathBuf::from("a.pdf"),
            identifier: "doi:10.1000/a".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            source: "crossref".to_string(),
            tier: None,
            cached: false,
        },
        Event::Resolved {
            path: PathBuf::from("b.pdf"),
            identifier: "doi:10.1000/b".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            source: "crossref".to_string(),
            tier: None,
            cached: false,
        },
        Event::Renamed {
            path: PathBuf::from("a.pdf"),
            target: PathBuf::from("Smith2024.pdf"),
            hash: hash_of("a.pdf"),
        },
        Event::Skipped {
            path: PathBuf::from("c.pdf"),
            reason: SkipReason::NoIdentifier,
        },
        Event::Skipped {
            path: PathBuf::from("d.pdf"),
            reason: SkipReason::AlreadyNamed,
        },
        Event::Skipped {
            path: PathBuf::from("e.pdf"),
            reason: SkipReason::Unnameable,
        },
    ];

    let counts = counts_for(&events);

    assert_eq!(
        counts,
        Counts {
            resolved: 2,
            renamed: 1,
            skipped: 3,
        }
    );
}

#[test]
fn a_preview_runs_counts_report_zero_renamed_however_many_moves_were_planned() {
    let events = vec![
        Event::Resolved {
            path: PathBuf::from("a.pdf"),
            identifier: "doi:10.1000/a".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            source: "crossref".to_string(),
            tier: None,
            cached: false,
        },
        Event::Planned {
            path: PathBuf::from("a.pdf"),
            target: PathBuf::from("Smith2024.pdf"),
        },
        Event::Planned {
            path: PathBuf::from("b.pdf"),
            target: PathBuf::from("Doe2023.pdf"),
        },
    ];

    let counts = counts_for(&events);

    assert_eq!(
        counts,
        Counts {
            resolved: 1,
            renamed: 0,
            skipped: 0,
        }
    );
}

// ---------------------------------------------------------------------
// RealFilesystem
// ---------------------------------------------------------------------

#[test]
fn existing_maps_each_bare_file_name_to_the_hash_of_its_bytes() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.pdf"), b"contents of a").unwrap();
    fs::write(dir.path().join("b.pdf"), b"contents of b").unwrap();
    let filesystem = RealFilesystem;

    let found = filesystem.existing(dir.path());

    assert_eq!(
        found,
        BTreeMap::from([
            (
                "a.pdf".to_string(),
                Some(hash_of("contents of a").as_str().to_string())
            ),
            (
                "b.pdf".to_string(),
                Some(hash_of("contents of b").as_str().to_string())
            ),
        ]),
        "got {found:?}"
    );
}

#[test]
fn existing_on_a_directory_that_does_not_exist_is_empty() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    let filesystem = RealFilesystem;

    let found = filesystem.existing(&missing);

    assert!(found.is_empty(), "got {found:?}");
}

#[test]
fn existing_does_not_include_subdirectory_names() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("file.pdf"), b"contents").unwrap();
    let filesystem = RealFilesystem;

    let found = filesystem.existing(dir.path());

    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found.contains_key("file.pdf"), "got {found:?}");
    assert!(!found.contains_key("sub"), "got {found:?}");
}

#[test]
fn rename_moves_the_file_leaving_the_old_path_gone_and_the_new_path_holding_the_bytes() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("original.pdf");
    let to = dir.path().join("Smith2024.pdf");
    fs::write(&from, b"the original bytes").unwrap();
    let filesystem = RealFilesystem;

    filesystem.rename(&from, &to).unwrap();

    assert!(!from.exists(), "the old path must be gone");
    assert_eq!(fs::read(&to).unwrap(), b"the original bytes");
}

#[test]
fn rename_refuses_to_overwrite_an_existing_destination() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("original.pdf");
    let to = dir.path().join("Smith2024.pdf");
    fs::write(&from, b"new bytes").unwrap();
    fs::write(&to, b"bytes already there").unwrap();
    let filesystem = RealFilesystem;

    let result = filesystem.rename(&from, &to);

    assert!(matches!(result, Err(RenameError { .. })), "got {result:?}");
    assert_eq!(
        fs::read(&to).unwrap(),
        b"bytes already there",
        "the destination must keep its own bytes"
    );
    assert!(from.exists(), "the source must still be there");
    assert_eq!(fs::read(&from).unwrap(), b"new bytes");
}

#[test]
fn rename_into_a_missing_subdirectory_creates_it_and_the_bytes_survive_the_move() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("original.pdf");
    let to = dir.path().join("sub").join("Smith2024.pdf");
    fs::write(&from, b"the original bytes").unwrap();
    let filesystem = RealFilesystem;

    filesystem.rename(&from, &to).unwrap();

    assert!(!from.exists(), "the old path must be gone");
    assert_eq!(fs::read(&to).unwrap(), b"the original bytes");
}

#[test]
fn rename_into_a_new_subdirectory_still_refuses_to_overwrite_an_existing_destination() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("original.pdf");
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let to = sub.join("Smith2024.pdf");
    fs::write(&from, b"new bytes").unwrap();
    fs::write(&to, b"bytes already there").unwrap();
    let filesystem = RealFilesystem;

    let result = filesystem.rename(&from, &to);

    assert!(matches!(result, Err(RenameError { .. })), "got {result:?}");
    assert_eq!(
        fs::read(&to).unwrap(),
        b"bytes already there",
        "the destination must keep its own bytes"
    );
    assert!(from.exists(), "the source must still be there");
    assert_eq!(fs::read(&from).unwrap(), b"new bytes");
}

// ---------------------------------------------------------------------
// Applying::carry_out: the hash gate an applying rename cannot skip
//
// `MoveLog`/`LogFailure`/`JournalLog` and the halt they drove are gone:
// the record of a move now travels on `Event::Renamed` itself, and
// `carry_out` takes the hash directly rather than a place to log it.
// ---------------------------------------------------------------------

/// design: "an applying rename already refuses to move a file whose
/// hash is unknown" — the invariant `Event::Renamed`'s required `hash`
/// field encodes, enforced here.
#[test]
fn carry_out_of_an_applying_rename_with_a_known_hash_moves_the_file_and_reports_it() {
    let filesystem = FakeFilesystem::new();
    let mut applying = Applying::new(&filesystem, true);
    let decision = rename("/lib/a.pdf", "/lib/Smith2024.pdf");

    let event = applying.carry_out(&decision, Some(hash_of("a.pdf")));

    assert_eq!(
        event,
        Event::Renamed {
            path: PathBuf::from("/lib/a.pdf"),
            target: PathBuf::from("/lib/Smith2024.pdf"),
            hash: hash_of("a.pdf"),
        }
    );
    assert_eq!(
        filesystem.renames(),
        vec![(
            PathBuf::from("/lib/a.pdf"),
            PathBuf::from("/lib/Smith2024.pdf")
        )]
    );
}

/// The one thing in this change that protects a file, carried over
/// unchanged from the journal's write-ahead gate: a hash borax cannot
/// identifiable record is a move borax does not make.
#[test]
fn carry_out_of_an_applying_rename_with_no_hash_skips_it_unrecordable_and_moves_nothing() {
    let filesystem = FakeFilesystem::new();
    let mut applying = Applying::new(&filesystem, true);
    let decision = rename("/lib/a.pdf", "/lib/Smith2024.pdf");

    let event = applying.carry_out(&decision, None);

    assert_eq!(
        event,
        Event::Skipped {
            path: PathBuf::from("/lib/a.pdf"),
            reason: SkipReason::Unrecordable {
                message: "the file's content hash is unknown".to_string(),
            },
        }
    );
    assert!(
        filesystem.renames().is_empty(),
        "a file whose hash is unknown must not be moved"
    );
}

/// A preview never looks at the hash at all: there is nothing to record
/// for a move that is not made.
#[test]
fn carry_out_of_a_preview_rename_ignores_a_missing_hash_and_still_plans_it() {
    let filesystem = FakeFilesystem::new();
    let mut applying = Applying::new(&filesystem, false);
    let decision = rename("/lib/a.pdf", "/lib/Smith2024.pdf");

    let event = applying.carry_out(&decision, None);

    assert_eq!(
        event,
        Event::Planned {
            path: PathBuf::from("/lib/a.pdf"),
            target: PathBuf::from("/lib/Smith2024.pdf"),
        }
    );
    assert!(filesystem.renames().is_empty());
}

/// design: "The rest of the batch continues after such a skip — one
/// unrecordable file says nothing about the next." Replaces the old
/// journal halt, which has no successor now that there is no per-move
/// log left to become unusable.
#[test]
fn a_batch_continues_past_an_unrecordable_file_to_rename_the_rest() {
    let plan = vec![
        rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
        rename("/lib/b.pdf", "/lib/Doe2023.pdf"),
        rename("/lib/c.pdf", "/lib/Roe2022.pdf"),
    ];
    let filesystem = FakeFilesystem::new();
    let hashes = [None, Some(hash_of("b.pdf")), Some(hash_of("c.pdf"))];

    let events = apply_renames(&plan, &filesystem, true, &hashes);

    assert_eq!(
        events,
        vec![
            Event::Skipped {
                path: PathBuf::from("/lib/a.pdf"),
                reason: SkipReason::Unrecordable {
                    message: "the file's content hash is unknown".to_string(),
                },
            },
            Event::Renamed {
                path: PathBuf::from("/lib/b.pdf"),
                target: PathBuf::from("/lib/Doe2023.pdf"),
                hash: hash_of("b.pdf"),
            },
            Event::Renamed {
                path: PathBuf::from("/lib/c.pdf"),
                target: PathBuf::from("/lib/Roe2022.pdf"),
                hash: hash_of("c.pdf"),
            },
        ]
    );
    assert_eq!(
        filesystem.renames(),
        vec![
            (
                PathBuf::from("/lib/b.pdf"),
                PathBuf::from("/lib/Doe2023.pdf")
            ),
            (
                PathBuf::from("/lib/c.pdf"),
                PathBuf::from("/lib/Roe2022.pdf")
            ),
        ],
        "only the files with a known hash may move, and the batch does not stop"
    );
}

/// Several unrecordable files in one batch each say nothing about the
/// others: there is no halt left to trigger.
#[test]
fn several_unrecordable_files_in_one_batch_are_each_skipped_independently() {
    let plan = vec![
        rename("/lib/a.pdf", "/lib/Smith2024.pdf"),
        rename("/lib/b.pdf", "/lib/Doe2023.pdf"),
    ];
    let filesystem = FakeFilesystem::new();
    let hashes = [None, None];

    let events = apply_renames(&plan, &filesystem, true, &hashes);

    let unrecordable = SkipReason::Unrecordable {
        message: "the file's content hash is unknown".to_string(),
    };
    assert_eq!(
        events,
        vec![
            Event::Skipped {
                path: PathBuf::from("/lib/a.pdf"),
                reason: unrecordable.clone(),
            },
            Event::Skipped {
                path: PathBuf::from("/lib/b.pdf"),
                reason: unrecordable,
            },
        ]
    );
    assert!(filesystem.renames().is_empty());
}

/// Recording is now just reporting: a file that fails to move after its
/// hash was known reports the filesystem's own failure, not anything
/// about recording.
#[test]
fn a_known_hash_that_then_fails_to_move_reports_the_filesystem_failure() {
    let filesystem = FakeFilesystem::new().with_failure("/lib/a.pdf", "permission denied");
    let mut applying = Applying::new(&filesystem, true);
    let decision = rename("/lib/a.pdf", "/lib/Smith2024.pdf");

    let event = applying.carry_out(&decision, Some(hash_of("a.pdf")));

    assert_eq!(
        event,
        Event::Skipped {
            path: PathBuf::from("/lib/a.pdf"),
            reason: SkipReason::RenameFailed {
                message: "permission denied".to_string(),
            },
        }
    );
}
