#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use borax::bib::BibFiles;
use borax::cli::{Cli, Command, Settings};
use borax::config::resolve;
use borax::event::{Counts, Event, SCHEMA, SkipReason, json_line};
use borax::ledger::ACCOUNTING_DIR;
use borax::pipeline::Library;
use borax::renaming::{Filesystem, RenameError};
use borax::run::{Adapters, Configs, Streams, dispatch, events_for};
use borax::runlog::RUNS_DIR;
use borax::session::Outcome;
use borax::undo::{Move, Refusal, UndoOutcome, Unrevertible, event_for, moves_in, undo_moves};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::record::{EntryType, Record};
use borax_pdf::source::{ExtractionError, PdfSource};
use borax_sources::cache::MemoryCache;
use borax_sources::source::Source;
use borax_sources::store::ContentIndex;
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes, following the shape of the ones formerly in `tests/journal.rs`
// ---------------------------------------------------------------------

/// A [`Library`] fake exposing only [`Library::hash`]; undo verifies
/// content and never opens a file, so [`Library::open`] is unreachable
/// here.
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
        unreachable!("undo verifies by hash and never opens a file")
    }
}

/// A [`Filesystem`] fake backed by a map from directory to the names
/// present there, following the shape of the one formerly in
/// `journal.rs`. Every [`Filesystem::rename`] call is recorded in
/// order.
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

/// A [`BibFiles`] fake that reads nothing and records nothing — undo
/// never touches the bibliography.
struct FakeBibFiles;

impl BibFiles for FakeBibFiles {
    fn read(&self, _path: &Path) -> std::io::Result<String> {
        Ok(String::new())
    }

    fn write(&self, _path: &Path, _content: &str) -> std::io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A [`Move`] from `from` to `to`, hashing `seed`'s bytes.
fn mv(from: &str, to: &str, seed: &str) -> Move {
    Move {
        from: PathBuf::from(from),
        to: PathBuf::from(to),
        hash: hash_of(seed),
    }
}

fn cli(command: Command) -> Cli {
    Cli {
        command,
        settings: Settings::default(),
        json: false,
    }
}

/// A `renamed` line as a real run log would carry it.
fn renamed_line(from: &str, to: &str, seed: &str) -> String {
    json_line(&Event::Renamed {
        path: PathBuf::from(from),
        target: PathBuf::from(to),
        hash: hash_of(seed),
    })
}

// ---------------------------------------------------------------------
// moves_in(): reading a run log's `renamed` lines
// ---------------------------------------------------------------------

#[test]
fn moves_in_of_an_empty_log_is_empty() {
    assert_eq!(moves_in("").unwrap(), Vec::new());
}

#[test]
fn moves_in_extracts_only_renamed_lines_in_file_order() {
    let text = [
        json_line(&Event::RunStarted {
            command: "rename".to_string(),
            version: "0.1.0".to_string(),
            applying: true,
        }),
        json_line(&Event::Resolved {
            path: PathBuf::from("/lib/one.pdf"),
            identifier: "doi:10.1000/one".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            source: "crossref".to_string(),
            tier: None,
            cached: false,
        }),
        renamed_line("/lib/one.pdf", "/lib/One2024.pdf", "one"),
        json_line(&Event::Skipped {
            path: PathBuf::from("/lib/two.pdf"),
            reason: SkipReason::NoIdentifier,
        }),
        renamed_line("/lib/three.pdf", "/lib/Three2024.pdf", "three"),
        json_line(&Event::RunFinished {
            counts: Counts::default(),
        }),
    ]
    .join("\n");

    let moves = moves_in(&text).unwrap();

    assert_eq!(
        moves,
        vec![
            mv("/lib/one.pdf", "/lib/One2024.pdf", "one"),
            mv("/lib/three.pdf", "/lib/Three2024.pdf", "three"),
        ]
    );
}

#[test]
fn moves_in_refuses_a_log_whose_renamed_line_carries_an_unknown_schema() {
    let bad = serde_json::json!({
        "schema": SCHEMA + 1,
        "event": "renamed",
        "path": "/lib/original.pdf",
        "target": "/lib/Smith2024.pdf",
        "hash": "sha256-deadbeef",
    })
    .to_string();

    let error = moves_in(&bad).unwrap_err();

    assert!(matches!(error, Refusal::Schema { found } if found == SCHEMA + 1));
}

#[test]
fn moves_in_refuses_the_whole_log_even_when_only_a_non_renamed_line_has_the_wrong_schema() {
    let bad_run_started = serde_json::json!({
        "schema": SCHEMA + 1,
        "event": "run-started",
        "command": "rename",
        "version": "0.1.0",
        "applying": true,
    })
    .to_string();
    let text = format!(
        "{bad_run_started}\n{}",
        renamed_line("/lib/one.pdf", "/lib/One2024.pdf", "one")
    );

    let error = moves_in(&text).unwrap_err();

    assert!(matches!(error, Refusal::Schema { found } if found == SCHEMA + 1));
}

/// design: "Parse defensively — ... only deserialize the ones tagged
/// `renamed`, so a log containing an event variant this build does not
/// know is still replayable rather than fatal." Behaviour pin 7.
#[test]
fn moves_in_is_defensive_about_an_event_variant_it_does_not_recognise() {
    let unknown = serde_json::json!({
        "schema": SCHEMA,
        "event": "some-future-event",
        "whatever": "this build has never heard of",
    })
    .to_string();
    let text = format!(
        "{unknown}\n{}",
        renamed_line("/lib/one.pdf", "/lib/One2024.pdf", "one")
    );

    let moves = moves_in(&text).unwrap();

    assert_eq!(moves, vec![mv("/lib/one.pdf", "/lib/One2024.pdf", "one")]);
}

#[test]
fn moves_in_refuses_a_line_that_is_not_json_at_all() {
    let text = format!(
        "{}\nthis is not json\n",
        renamed_line("/lib/one.pdf", "/lib/One2024.pdf", "one")
    );

    let error = moves_in(&text).unwrap_err();

    assert!(matches!(error, Refusal::Unreadable { .. }), "got {error:?}");
}

// ---------------------------------------------------------------------
// undo_moves(): the clean case
// ---------------------------------------------------------------------

/// Behaviour pin 1: a run that renamed `a`→`b` then `b`→`c` unwinds in
/// reverse without either step landing on a name the other holds.
#[test]
fn undo_moves_reverts_a_run_in_reverse_order() {
    let one = mv("/lib/one.pdf", "/lib/One2024.pdf", "one");
    let two = mv("/lib/two.pdf", "/lib/Two2024.pdf", "two");
    let three = mv("/lib/three.pdf", "/lib/Three2024.pdf", "three");
    let library = FakeLibrary::new()
        .with_hash("/lib/One2024.pdf", hash_of("one"))
        .with_hash("/lib/Two2024.pdf", hash_of("two"))
        .with_hash("/lib/Three2024.pdf", hash_of("three"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_moves(&[one, two, three], &library, &filesystem);

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
// undo_moves(): unrevertible entries
// ---------------------------------------------------------------------

/// Behaviour pin 2.
#[test]
fn undo_moves_reports_missing_when_nothing_is_at_the_recorded_path() {
    let one = mv("/lib/one.pdf", "/lib/One2024.pdf", "one");
    let library = FakeLibrary::new(); // no entry for /lib/One2024.pdf
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_moves(&[one], &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::Missing,
        }]
    );
    assert!(filesystem.renames().is_empty());
}

/// Behaviour pin 3 — the one this whole change exists to preserve.
#[test]
fn undo_moves_reports_content_changed_and_does_not_move_the_file() {
    let one = mv("/lib/one.pdf", "/lib/One2024.pdf", "one");
    let library = FakeLibrary::new().with_hash("/lib/One2024.pdf", hash_of("a different file"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_moves(&[one], &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::ContentChanged,
        }]
    );
    assert!(
        filesystem.renames().is_empty(),
        "a file whose content changed must never be moved"
    );
}

/// Behaviour pin 4.
#[test]
fn undo_moves_reports_original_taken_when_the_original_path_is_occupied() {
    let one = mv("/lib/one.pdf", "/lib/One2024.pdf", "one");
    let library = FakeLibrary::new().with_hash("/lib/One2024.pdf", hash_of("one"));
    let filesystem = FakeFilesystem::new().with_existing("/lib", ["one.pdf"]);

    let outcomes = undo_moves(&[one], &library, &filesystem);

    assert_eq!(
        outcomes,
        vec![UndoOutcome::Left {
            path: PathBuf::from("/lib/One2024.pdf"),
            reason: Unrevertible::OriginalTaken,
        }]
    );
    assert!(filesystem.renames().is_empty());
}

/// Behaviour pin 5.
#[test]
fn undo_moves_reports_failed_with_the_error_message_when_the_move_itself_fails() {
    let one = mv("/lib/one.pdf", "/lib/One2024.pdf", "one");
    let library = FakeLibrary::new().with_hash("/lib/One2024.pdf", hash_of("one"));
    let filesystem = FakeFilesystem::new().with_failure("/lib/One2024.pdf", "disk full");

    let outcomes = undo_moves(&[one], &library, &filesystem);

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
// undo_moves(): one bad entry says nothing about the rest
// ---------------------------------------------------------------------

/// Behaviour pin 3 combined with the batch-continues rule: one
/// unrevertible move says nothing about the others.
#[test]
fn one_unrevertible_move_does_not_stop_the_rest_of_the_run_from_reverting() {
    let one = mv("/lib/one.pdf", "/lib/One2024.pdf", "one");
    let two = mv("/lib/two.pdf", "/lib/Two2024.pdf", "two");
    let three = mv("/lib/three.pdf", "/lib/Three2024.pdf", "three");
    // "two" was moved away before undo runs: nothing is at its recorded
    // path any more.
    let library = FakeLibrary::new()
        .with_hash("/lib/One2024.pdf", hash_of("one"))
        .with_hash("/lib/Three2024.pdf", hash_of("three"));
    let filesystem = FakeFilesystem::new();

    let outcomes = undo_moves(&[one, two, three], &library, &filesystem);

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

#[test]
fn undo_moves_of_an_empty_slice_produces_no_outcomes() {
    let library = FakeLibrary::new();
    let filesystem = FakeFilesystem::new();

    assert_eq!(undo_moves(&[], &library, &filesystem), Vec::new());
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

// ---------------------------------------------------------------------
// Wired into `borax undo`: preflight finds the log via
// `runlog::latest_apply_log`, reads it, and refuses before anything
// moves when it cannot be replayed.
// ---------------------------------------------------------------------

/// Behaviour pin 9: `latest_apply_log`'s own precedence is tested in
/// `tests/runlog.rs`; this only confirms `borax undo` is wired through
/// it rather than, say, always preferring the state root.
#[test]
fn undo_prefers_the_collection_log_over_an_xdg_state_one() {
    let collection = tempdir().unwrap();
    let state = tempdir().unwrap();
    let collection_runs = collection.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let state_runs = state.path().join(RUNS_DIR);
    fs::create_dir_all(&collection_runs).unwrap();
    fs::create_dir_all(&state_runs).unwrap();

    let collection_to = PathBuf::from("/collection/Collection2024.pdf");
    let collection_from = PathBuf::from("/collection/original.pdf");
    fs::write(
        collection_runs.join("20240101T000000Z-rename-apply.jsonl"),
        renamed_line(
            collection_from.to_str().unwrap(),
            collection_to.to_str().unwrap(),
            "collection-move",
        ),
    )
    .unwrap();

    let state_to = PathBuf::from("/state/State2024.pdf");
    let state_from = PathBuf::from("/state/original.pdf");
    fs::write(
        state_runs.join("20240102T000000Z-rename-apply.jsonl"),
        renamed_line(
            state_from.to_str().unwrap(),
            state_to.to_str().unwrap(),
            "state-move",
        ),
    )
    .unwrap();

    let library = FakeLibrary::new()
        .with_hash(&collection_to, hash_of("collection-move"))
        .with_hash(&state_to, hash_of("state-move"));
    let filesystem = FakeFilesystem::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: || "undo-run".to_string(),
        ledger: None,
        collection_root: Some(collection.path().to_path_buf()),
        state_root: Some(state.path().to_path_buf()),
    };

    let events = events_for(&Command::Undo, &Configs::uniform(effective), &adapters).unwrap();

    assert_eq!(
        events,
        vec![Event::Reverted {
            path: collection_to.clone(),
            target: collection_from.clone(),
        }],
        "got {events:?}"
    );
    assert_eq!(
        filesystem.renames(),
        vec![(collection_to, collection_from)],
        "the state root's move must not be touched"
    );
}

/// Behaviour pin 6.
#[test]
fn a_log_with_an_unknown_schema_is_refused_before_anything_moves() {
    let dir = tempdir().unwrap();
    let runs = dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    fs::create_dir_all(&runs).unwrap();
    let bad = serde_json::json!({
        "schema": SCHEMA + 1,
        "event": "renamed",
        "path": "/lib/original.pdf",
        "target": "/lib/Smith2024.pdf",
        "hash": "sha256-deadbeef",
    })
    .to_string();
    fs::write(
        runs.join("20240101T000000Z-rename-apply.jsonl"),
        format!("{bad}\n"),
    )
    .unwrap();

    let library = FakeLibrary::new();
    let filesystem = FakeFilesystem::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: || "undo-run".to_string(),
        ledger: None,
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::Undo),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(!err.is_empty(), "expected a clear error");
    assert!(
        filesystem.renames().is_empty(),
        "nothing may move when the log cannot be replayed"
    );
    assert!(out.is_empty(), "a refused undo must write no event stream");
}

/// Behaviour pin 8.
#[test]
fn undo_with_no_apply_log_anywhere_reports_nothing_and_succeeds() {
    let library = FakeLibrary::new();
    let filesystem = FakeFilesystem::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: || "undo-run".to_string(),
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
        &cli(Command::Undo),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    assert!(
        err.is_empty(),
        "no apply log anywhere is not a failure: {:?}",
        String::from_utf8_lossy(&err)
    );
    assert!(filesystem.renames().is_empty());
}
