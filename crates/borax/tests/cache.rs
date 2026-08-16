#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use borax::cache::{CacheStats, clear, cleared_event, inspect, status_event};
use borax::event::Event;
use tempfile::tempdir;

// ---------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------

#[test]
fn inspect_on_a_missing_root_reports_zero_entries_and_bytes_with_the_root_echoed_back() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("does-not-exist");

    let stats = inspect(&root).unwrap();

    assert_eq!(
        stats,
        CacheStats {
            root: root.clone(),
            entries: 0,
            bytes: 0,
        },
        "got {stats:?}"
    );
}

#[test]
fn inspect_counts_regular_files_at_any_depth_and_totals_their_sizes() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    fs::create_dir(root.join("empty-sub")).unwrap();
    fs::write(root.join("a.bin"), vec![0u8; 5]).unwrap();
    fs::write(root.join("sub/b.bin"), vec![0u8; 3]).unwrap();
    fs::write(root.join("sub/deeper/c.bin"), vec![0u8; 7]).unwrap();

    let stats = inspect(&root).unwrap();

    assert_eq!(
        stats,
        CacheStats {
            root: root.clone(),
            entries: 3,
            bytes: 15,
        },
        "got {stats:?}"
    );
}

#[cfg(unix)]
#[test]
fn inspect_on_an_unreadable_root_returns_an_error_rather_than_reporting_empty() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path().join("locked");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("entry.bin"), vec![0u8; 1]).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();

    // A process whose effective privileges bypass directory modes (root,
    // typically) makes this case unobservable rather than wrong, so it
    // is skipped instead of asserted falsely.
    if fs::read_dir(&root).is_ok() {
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping inspect_on_an_unreadable_root_returns_an_error_rather_than_reporting_empty: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let result = inspect(&root);

    // Restored before the temp directory's own cleanup, which needs to
    // read and remove what is inside it.
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        result.is_err(),
        "expected inspect to fail on an unreadable root, got {result:?}"
    );
}

// ---------------------------------------------------------------------
// clear
// ---------------------------------------------------------------------

#[test]
fn clear_removes_the_tree_and_returns_the_stats_counted_before_removal() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("a.bin"), vec![0u8; 5]).unwrap();
    fs::write(root.join("sub/b.bin"), vec![0u8; 3]).unwrap();

    let stats = clear(&root).unwrap();

    assert_eq!(
        stats,
        CacheStats {
            root: root.clone(),
            entries: 2,
            bytes: 8,
        },
        "got {stats:?}"
    );
    assert!(!root.exists(), "the cache tree must be removed");
}

#[test]
fn clear_on_a_missing_root_succeeds_and_reports_zero() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("does-not-exist");

    let stats = clear(&root).unwrap();

    assert_eq!(
        stats,
        CacheStats {
            root: root.clone(),
            entries: 0,
            bytes: 0,
        },
        "got {stats:?}"
    );
}

// ---------------------------------------------------------------------
// status_event / cleared_event
// ---------------------------------------------------------------------

#[test]
fn status_event_renders_as_cache_status_carrying_the_stats() {
    let stats = CacheStats {
        root: PathBuf::from("/cache"),
        entries: 4,
        bytes: 100,
    };

    assert_eq!(
        status_event(&stats),
        Event::CacheStatus {
            root: PathBuf::from("/cache"),
            entries: 4,
            bytes: 100,
        }
    );
}

#[test]
fn cleared_event_renders_as_cache_cleared_carrying_the_stats() {
    let stats = CacheStats {
        root: PathBuf::from("/cache"),
        entries: 4,
        bytes: 100,
    };

    assert_eq!(
        cleared_event(&stats),
        Event::CacheCleared {
            root: PathBuf::from("/cache"),
            entries: 4,
            bytes: 100,
        }
    );
}
