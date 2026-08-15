#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use borax::event::{Event, SkipReason};
use borax::journal::{
    Entry, FORMAT_VERSION, FileJournal, Journal, RunId, UndoOutcome, Unrevertible, event_for,
    last_run, state_root, undo_last,
};
use borax::pipeline::Library;
use borax::renaming::{Filesystem, RenameError};
use borax_core::content::{ContentHash, hash_bytes};
use borax_pdf::source::{ExtractionError, PdfSource};
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`Journal`] fake that reads back a fixed set of entries, following
/// the shape of the fakes in `pipeline.rs` and `renaming.rs`.
struct FakeJournal {
    entries: Vec<Entry>,
}

impl FakeJournal {
    fn new(entries: Vec<Entry>) -> FakeJournal {
        FakeJournal { entries }
    }
}

impl Journal for FakeJournal {
    fn append(&self, _entries: &[Entry]) -> io::Result<()> {
        Ok(())
    }

    fn read(&self) -> Vec<Entry> {
        self.entries.clone()
    }
}

/// A [`Library`] fake exposing only [`Library::hash`]; `undo_last`
/// verifies content and never opens a file, so [`Library::open`] is
/// unreachable here.
struct FakeLibrary {
    hashes: BTreeMap<PathBuf, ContentHash>,
}

impl FakeLibrary {
    fn new() -> FakeLibrary {
        FakeLibrary {
            hashes: BTreeMap::new(),
        }
    }

    fn with_hash(mut self, path: impl Into<PathBuf>, hash: ContentHash) -> FakeLibrary {
        self.hashes.insert(path.into(), hash);
        self
    }
}

impl Library for FakeLibrary {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.hashes
            .get(path)
            .cloned()
            .ok_or_else(|| ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            })
    }

    fn open(&self, _path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        unreachable!("undo_last verifies by hash and never opens a file")
    }
}

/// A [`Filesystem`] fake backed by a map from directory to the names
/// present there, following the shape of `FakeFilesystem` in
/// `renaming.rs`. Every [`Filesystem::rename`] call is recorded in
/// order, so a test can assert exactly which moves happened.
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

    /// Populate `directory` with `names`, so `existing` reports them as
    /// occupied.
    fn with_existing(
        mut self,
        directory: impl Into<PathBuf>,
        names: impl IntoIterator<Item = &'static str>,
    ) -> FakeFilesystem {
        self.existing.insert(
            directory.into(),
            names
                .into_iter()
                .map(|name| (name.to_string(), None))
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
    /// means nothing was moved.
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

fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// An [`Entry`] moving `from` to `to`, journaled under `run` and hashing
/// `seed`'s bytes.
fn entry(run: &str, from: &str, to: &str, seed: &str, at: &str) -> Entry {
    Entry {
        run: RunId::new(run),
        from: PathBuf::from(from),
        to: PathBuf::from(to),
        hash: hash_of(seed),
        at: at.to_string(),
    }
}

/// A `lookup` that answers from `entries` and knows nothing else, so a
/// test states the whole environment it depends on. Follows the shape of
/// `env` in `borax-sources/tests/store.rs`.
#[cfg(any(unix, windows))]
fn env(entries: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

// ---------------------------------------------------------------------
// state_root()
// ---------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn state_root_honors_xdg_state_home_on_unix() {
    let root = state_root(env(&[
        ("XDG_STATE_HOME", "/base/home/.local/state"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home/.local/state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_falls_back_to_home_dot_local_state_on_unix() {
    let root = state_root(env(&[("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local")
                .join("state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_prefers_xdg_state_home_over_home_on_unix() {
    let root = state_root(env(&[
        ("XDG_STATE_HOME", "/xdg/state"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/xdg/state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_skips_an_empty_xdg_state_home_on_unix() {
    let root = state_root(env(&[("XDG_STATE_HOME", ""), ("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local")
                .join("state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_skips_a_relative_xdg_state_home_on_unix() {
    let root = state_root(env(&[
        ("XDG_STATE_HOME", "relative/state"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local")
                .join("state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_is_none_when_nothing_qualifies_on_unix() {
    assert_eq!(state_root(env(&[])), None);
}

#[cfg(unix)]
#[test]
fn state_root_is_none_for_an_empty_home_with_no_xdg_state_home_on_unix() {
    assert_eq!(state_root(env(&[("HOME", "")])), None);
}

#[cfg(unix)]
#[test]
fn state_root_is_none_for_a_relative_home_with_no_xdg_state_home_on_unix() {
    assert_eq!(state_root(env(&[("HOME", "relative")])), None);
}

#[cfg(windows)]
#[test]
fn state_root_honors_localappdata_on_windows() {
    let root = state_root(env(&[("LOCALAPPDATA", r"C:\Users\test\AppData\Local")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Local")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_falls_back_to_xdg_state_home_on_windows() {
    let root = state_root(env(&[("XDG_STATE_HOME", r"C:\Users\test\.state")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_prefers_localappdata_over_xdg_state_home_on_windows() {
    let root = state_root(env(&[
        ("LOCALAPPDATA", r"C:\Users\test\AppData\Local"),
        ("XDG_STATE_HOME", r"C:\Users\test\.state"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Local")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_skips_an_empty_localappdata_on_windows() {
    let root = state_root(env(&[
        ("LOCALAPPDATA", ""),
        ("XDG_STATE_HOME", r"C:\Users\test\.state"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_skips_a_relative_localappdata_on_windows() {
    let root = state_root(env(&[
        ("LOCALAPPDATA", r"AppData\Local"),
        ("XDG_STATE_HOME", r"C:\Users\test\.state"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.state")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_is_none_when_nothing_qualifies_on_windows() {
    assert_eq!(state_root(env(&[])), None);
}

#[cfg(windows)]
#[test]
fn state_root_is_none_for_an_empty_xdg_state_home_with_no_localappdata_on_windows() {
    assert_eq!(state_root(env(&[("XDG_STATE_HOME", "")])), None);
}

#[cfg(windows)]
#[test]
fn state_root_is_none_for_a_relative_xdg_state_home_with_no_localappdata_on_windows() {
    assert_eq!(state_root(env(&[("XDG_STATE_HOME", "relative")])), None);
}

// ---------------------------------------------------------------------
// Entry: round-trip
// ---------------------------------------------------------------------

#[test]
fn an_entry_round_trips_through_json() {
    let original = entry(
        "run-1",
        "/lib/original.pdf",
        "/lib/Smith2024.pdf",
        "contents",
        "2024-05-17T10:00:00Z",
    );

    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed, original);
}

#[test]
fn an_entrys_hash_renders_as_a_bare_string_in_json() {
    let original = entry(
        "run-1",
        "/lib/original.pdf",
        "/lib/Smith2024.pdf",
        "contents",
        "2024-05-17T10:00:00Z",
    );

    let json = serde_json::to_string(&original).unwrap();

    assert!(
        json.contains(&format!(r#""hash":"{}""#, hash_of("contents").as_str())),
        "expected the hash to render as a bare string in {json}"
    );
}

// ---------------------------------------------------------------------
// FileJournal
// ---------------------------------------------------------------------

#[test]
fn file_journal_new_does_not_create_the_file_or_its_parent_directory() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("renames.jsonl");

    FileJournal::new(&path);

    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn file_journal_append_creates_the_file_and_its_parent_directory() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("renames.jsonl");
    let journal = FileJournal::new(&path);

    journal
        .append(&[entry(
            "r1",
            "/lib/one.pdf",
            "/lib/One2024.pdf",
            "one",
            "2024-01-01T00:00:00Z",
        )])
        .unwrap();

    assert!(path.exists());
}

#[test]
fn file_journal_read_on_an_absent_journal_is_empty() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("renames.jsonl");
    let journal = FileJournal::new(&path);

    assert_eq!(journal.read(), Vec::new());
}

#[test]
fn file_journal_round_trips_entries_across_two_append_calls() {
    let dir = tempdir().unwrap();
    let journal = FileJournal::new(dir.path().join("renames.jsonl"));
    let one = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let two = entry(
        "r1",
        "/lib/two.pdf",
        "/lib/Two2024.pdf",
        "two",
        "2024-01-01T00:00:01Z",
    );
    let three = entry(
        "r2",
        "/lib/three.pdf",
        "/lib/Three2024.pdf",
        "three",
        "2024-01-02T00:00:00Z",
    );

    journal.append(&[one.clone(), two.clone()]).unwrap();
    journal.append(std::slice::from_ref(&three)).unwrap();

    assert_eq!(journal.read(), vec![one, two, three]);
}

#[test]
fn file_journal_append_of_an_empty_slice_is_a_no_op_that_still_succeeds() {
    let dir = tempdir().unwrap();
    let journal = FileJournal::new(dir.path().join("renames.jsonl"));

    assert!(journal.append(&[]).is_ok());
    assert_eq!(journal.read(), Vec::new());
}

#[test]
fn file_journal_path_reports_what_it_was_constructed_with() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("renames.jsonl");
    let journal = FileJournal::new(&path);

    assert_eq!(journal.path(), path);
}

#[test]
fn file_journal_writes_one_json_object_per_line_with_no_embedded_newlines() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("renames.jsonl");
    let journal = FileJournal::new(&path);
    let one = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let two = entry(
        "r1",
        "/lib/two.pdf",
        "/lib/Two2024.pdf",
        "two",
        "2024-01-01T00:00:01Z",
    );

    journal.append(&[one, two]).unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected one line per entry in {contents:?}"
    );
    for line in lines {
        assert!(!line.contains('\n'), "a line must not embed a newline");
        let value: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("{line:?} did not parse as JSON: {error}"));
        assert!(value.is_object(), "{line:?} did not parse as a JSON object");
    }
}

#[test]
fn file_journal_skips_a_corrupt_line_and_still_reads_the_entries_around_it() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("renames.jsonl");
    let one = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let two = entry(
        "r1",
        "/lib/two.pdf",
        "/lib/Two2024.pdf",
        "two",
        "2024-01-01T00:00:01Z",
    );
    let contents = format!(
        "{}\nnot json at all\n{}\n",
        serde_json::to_string(&one).unwrap(),
        serde_json::to_string(&two).unwrap(),
    );
    std::fs::write(&path, contents).unwrap();
    let journal = FileJournal::new(&path);

    assert_eq!(journal.read(), vec![one, two]);
}

// ---------------------------------------------------------------------
// last_run()
// ---------------------------------------------------------------------

#[test]
fn last_run_of_an_empty_journal_is_empty() {
    assert_eq!(last_run(&[]), Vec::new());
}

#[test]
fn last_run_of_a_single_run_returns_all_of_it_in_applied_order() {
    let entries = vec![
        entry(
            "r1",
            "/lib/a.pdf",
            "/lib/A.pdf",
            "a",
            "2024-01-01T00:00:00Z",
        ),
        entry(
            "r1",
            "/lib/b.pdf",
            "/lib/B.pdf",
            "b",
            "2024-01-01T00:00:01Z",
        ),
    ];

    assert_eq!(last_run(&entries), entries);
}

#[test]
fn last_run_of_three_runs_returns_only_the_last_ones_entries_in_applied_order() {
    let run1 = entry(
        "r1",
        "/lib/a.pdf",
        "/lib/A.pdf",
        "a",
        "2024-01-01T00:00:00Z",
    );
    let run2 = entry(
        "r2",
        "/lib/b.pdf",
        "/lib/B.pdf",
        "b",
        "2024-01-02T00:00:00Z",
    );
    let run3a = entry(
        "r3",
        "/lib/c.pdf",
        "/lib/C.pdf",
        "c",
        "2024-01-03T00:00:00Z",
    );
    let run3b = entry(
        "r3",
        "/lib/d.pdf",
        "/lib/D.pdf",
        "d",
        "2024-01-03T00:00:01Z",
    );
    let entries = vec![run1, run2, run3a.clone(), run3b.clone()];

    assert_eq!(last_run(&entries), vec![run3a, run3b]);
}

#[test]
fn last_run_is_positional_not_chronological() {
    // The last-positioned run carries an earlier timestamp than the run
    // before it: a clock that went backwards must not change what undo
    // reverts.
    let earlier_run = entry(
        "r1",
        "/lib/a.pdf",
        "/lib/A.pdf",
        "a",
        "2024-06-01T00:00:00Z",
    );
    let later_positioned_but_earlier_timestamp = entry(
        "r2",
        "/lib/b.pdf",
        "/lib/B.pdf",
        "b",
        "2020-01-01T00:00:00Z",
    );
    let entries = vec![earlier_run, later_positioned_but_earlier_timestamp.clone()];

    assert_eq!(
        last_run(&entries),
        vec![later_positioned_but_earlier_timestamp]
    );
}

#[test]
fn last_run_with_an_interleaved_run_id_collects_every_entry_of_the_last_entrys_run() {
    // r1, then r2, then r1 again: the run of the last entry is r1, and
    // "the run of the last entry" is read as every entry sharing that
    // run id, not just the contiguous tail.
    let r1_first = entry(
        "r1",
        "/lib/a.pdf",
        "/lib/A.pdf",
        "a",
        "2024-01-01T00:00:00Z",
    );
    let r2_only = entry(
        "r2",
        "/lib/b.pdf",
        "/lib/B.pdf",
        "b",
        "2024-01-02T00:00:00Z",
    );
    let r1_last = entry(
        "r1",
        "/lib/c.pdf",
        "/lib/C.pdf",
        "c",
        "2024-01-03T00:00:00Z",
    );
    let entries = vec![r1_first.clone(), r2_only, r1_last.clone()];

    assert_eq!(last_run(&entries), vec![r1_first, r1_last]);
}

// ---------------------------------------------------------------------
// undo_last(): the clean case
// ---------------------------------------------------------------------

#[test]
fn undo_last_reverts_a_clean_run_in_reverse_order() {
    let one = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let two = entry(
        "r1",
        "/lib/two.pdf",
        "/lib/Two2024.pdf",
        "two",
        "2024-01-01T00:00:01Z",
    );
    let three = entry(
        "r1",
        "/lib/three.pdf",
        "/lib/Three2024.pdf",
        "three",
        "2024-01-01T00:00:02Z",
    );
    let journal = FakeJournal::new(vec![one, two, three]);
    let library = FakeLibrary::new()
        .with_hash("/lib/One2024.pdf", hash_of("one"))
        .with_hash("/lib/Two2024.pdf", hash_of("two"))
        .with_hash("/lib/Three2024.pdf", hash_of("three"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![
            UndoOutcome::Reverted {
                from: PathBuf::from("/lib/Three2024.pdf"),
                to: PathBuf::from("/lib/three.pdf"),
            },
            UndoOutcome::Reverted {
                from: PathBuf::from("/lib/Two2024.pdf"),
                to: PathBuf::from("/lib/two.pdf"),
            },
            UndoOutcome::Reverted {
                from: PathBuf::from("/lib/One2024.pdf"),
                to: PathBuf::from("/lib/one.pdf"),
            },
        ]
    );
    assert_eq!(
        filesystem.renames(),
        vec![
            (
                PathBuf::from("/lib/Three2024.pdf"),
                PathBuf::from("/lib/three.pdf")
            ),
            (
                PathBuf::from("/lib/Two2024.pdf"),
                PathBuf::from("/lib/two.pdf")
            ),
            (
                PathBuf::from("/lib/One2024.pdf"),
                PathBuf::from("/lib/one.pdf")
            ),
        ]
    );
}

// ---------------------------------------------------------------------
// undo_last(): unrevertible entries
// ---------------------------------------------------------------------

#[test]
fn undo_last_reports_missing_when_nothing_is_at_the_journaled_path() {
    let e = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let journal = FakeJournal::new(vec![e]);
    let library = FakeLibrary::new(); // no entry for /lib/One2024.pdf
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::Missing,
        }]
    );
    assert!(filesystem.renames().is_empty());
}

#[test]
fn undo_last_reports_content_changed_when_the_hash_no_longer_matches() {
    let e = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let journal = FakeJournal::new(vec![e]);
    let library = FakeLibrary::new().with_hash("/lib/One2024.pdf", hash_of("a different file"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::ContentChanged,
        }]
    );
    assert!(filesystem.renames().is_empty());
}

#[test]
fn undo_last_reports_original_taken_when_the_original_path_is_occupied() {
    let e = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let journal = FakeJournal::new(vec![e]);
    let library = FakeLibrary::new().with_hash("/lib/One2024.pdf", hash_of("one"));
    let filesystem = FakeFilesystem::new().with_existing("/lib", ["one.pdf"]);

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::OriginalTaken,
        }]
    );
    assert!(filesystem.renames().is_empty());
}

#[test]
fn undo_last_reports_failed_with_the_error_message_when_the_move_itself_fails() {
    let e = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let journal = FakeJournal::new(vec![e]);
    let library = FakeLibrary::new().with_hash("/lib/One2024.pdf", hash_of("one"));
    let filesystem = FakeFilesystem::new().with_failure("/lib/One2024.pdf", "disk full");

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::Failed {
                message: "disk full".to_string(),
            },
        }]
    );
    assert!(filesystem.renames().is_empty());
}

// ---------------------------------------------------------------------
// undo_last(): the spec scenario — one unrevertible entry, rest reverted
// ---------------------------------------------------------------------

#[test]
fn one_unrevertible_entry_does_not_stop_the_rest_of_the_run_from_reverting() {
    let one = entry(
        "r1",
        "/lib/one.pdf",
        "/lib/One2024.pdf",
        "one",
        "2024-01-01T00:00:00Z",
    );
    let two = entry(
        "r1",
        "/lib/two.pdf",
        "/lib/Two2024.pdf",
        "two",
        "2024-01-01T00:00:01Z",
    );
    let three = entry(
        "r1",
        "/lib/three.pdf",
        "/lib/Three2024.pdf",
        "three",
        "2024-01-01T00:00:02Z",
    );
    let journal = FakeJournal::new(vec![one, two, three]);
    // "two" was moved away before undo runs: nothing is at its journaled
    // path any more.
    let library = FakeLibrary::new()
        .with_hash("/lib/One2024.pdf", hash_of("one"))
        .with_hash("/lib/Three2024.pdf", hash_of("three"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![
            UndoOutcome::Reverted {
                from: PathBuf::from("/lib/Three2024.pdf"),
                to: PathBuf::from("/lib/three.pdf"),
            },
            UndoOutcome::Left {
                path: PathBuf::from("/lib/Two2024.pdf"),
                reason: Unrevertible::Missing,
            },
            UndoOutcome::Reverted {
                from: PathBuf::from("/lib/One2024.pdf"),
                to: PathBuf::from("/lib/one.pdf"),
            },
        ]
    );
    assert_eq!(
        filesystem.renames(),
        vec![
            (
                PathBuf::from("/lib/Three2024.pdf"),
                PathBuf::from("/lib/three.pdf")
            ),
            (
                PathBuf::from("/lib/One2024.pdf"),
                PathBuf::from("/lib/one.pdf")
            ),
        ]
    );
}

// ---------------------------------------------------------------------
// undo_last(): scope
// ---------------------------------------------------------------------

#[test]
fn undo_last_on_an_empty_journal_produces_no_outcomes() {
    let journal = FakeJournal::new(vec![]);
    let library = FakeLibrary::new();
    let filesystem = FakeFilesystem::new();

    assert_eq!(undo_last(&journal, &library, &filesystem), Vec::new());
}

#[test]
fn undo_last_leaves_an_earlier_runs_files_alone() {
    let earlier = entry(
        "r1",
        "/lib/old.pdf",
        "/lib/Old2020.pdf",
        "old",
        "2024-01-01T00:00:00Z",
    );
    let latest = entry(
        "r2",
        "/lib/new.pdf",
        "/lib/New2024.pdf",
        "new",
        "2024-01-02T00:00:00Z",
    );
    let journal = FakeJournal::new(vec![earlier, latest]);
    let library = FakeLibrary::new()
        .with_hash("/lib/Old2020.pdf", hash_of("old"))
        .with_hash("/lib/New2024.pdf", hash_of("new"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_last(&journal, &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Reverted {
            from: PathBuf::from("/lib/New2024.pdf"),
            to: PathBuf::from("/lib/new.pdf"),
        }]
    );
    assert_eq!(
        filesystem.renames(),
        vec![(
            PathBuf::from("/lib/New2024.pdf"),
            PathBuf::from("/lib/new.pdf")
        )]
    );
}

// ---------------------------------------------------------------------
// event_for()
// ---------------------------------------------------------------------

#[test]
fn event_for_a_reverted_outcome_is_a_reverted_event() {
    let outcome = UndoOutcome::Reverted {
        from: PathBuf::from("/lib/Smith2024.pdf"),
        to: PathBuf::from("/lib/original.pdf"),
    };

    assert_eq!(
        event_for(&outcome),
        Event::Reverted {
            path: PathBuf::from("/lib/Smith2024.pdf"),
            target: PathBuf::from("/lib/original.pdf"),
        }
    );
}

#[test]
fn event_for_missing_is_a_skipped_event_with_the_missing_reason() {
    let outcome = UndoOutcome::Left {
        path: PathBuf::from("/lib/Smith2024.pdf"),
        reason: Unrevertible::Missing,
    };

    assert_eq!(
        event_for(&outcome),
        Event::Skipped {
            path: PathBuf::from("/lib/Smith2024.pdf"),
            reason: SkipReason::Missing,
        }
    );
}

#[test]
fn event_for_content_changed_is_a_skipped_event_with_the_content_changed_reason() {
    let outcome = UndoOutcome::Left {
        path: PathBuf::from("/lib/Smith2024.pdf"),
        reason: Unrevertible::ContentChanged,
    };

    assert_eq!(
        event_for(&outcome),
        Event::Skipped {
            path: PathBuf::from("/lib/Smith2024.pdf"),
            reason: SkipReason::ContentChanged,
        }
    );
}

#[test]
fn event_for_original_taken_is_a_skipped_event_with_the_original_taken_reason() {
    let outcome = UndoOutcome::Left {
        path: PathBuf::from("/lib/Smith2024.pdf"),
        reason: Unrevertible::OriginalTaken,
    };

    assert_eq!(
        event_for(&outcome),
        Event::Skipped {
            path: PathBuf::from("/lib/Smith2024.pdf"),
            reason: SkipReason::OriginalTaken,
        }
    );
}

#[test]
fn event_for_failed_is_a_skipped_event_with_the_rename_failed_reason() {
    let outcome = UndoOutcome::Left {
        path: PathBuf::from("/lib/Smith2024.pdf"),
        reason: Unrevertible::Failed {
            message: "disk full".to_string(),
        },
    };

    assert_eq!(
        event_for(&outcome),
        Event::Skipped {
            path: PathBuf::from("/lib/Smith2024.pdf"),
            reason: SkipReason::RenameFailed {
                message: "disk full".to_string(),
            },
        }
    );
}
