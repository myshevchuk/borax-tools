//! Comparing paths without asking the filesystem.
//!
//! Two paths naming one file can be spelled differently — one relative
//! and one absolute, one carrying `..`, one differing only in case on a
//! filesystem that does not — and borax has to know when that has
//! happened: to tell a ledger entry recording a file from one recording
//! a copy of it, and to tell whether a directory lies under a
//! collection root.
//!
//! Nothing here touches the disk beyond reading the working directory.
//! Symlinks are deliberately not resolved: the ledger records the path
//! borax moved a file to and a run names the path it was given, so a
//! link and its target are two names, and treating them as one would
//! let a link into a collection keep the file it points at out of it.

use std::path::{Component, Path, PathBuf};

/// `path` made absolute against the working directory and normalised
/// lexically: `.` dropped, `..` resolved against the component before
/// it, and separators unified by rebuilding the path component by
/// component.
///
/// Nothing is asked of the filesystem, so a symlink stays the name it
/// is and a path that is not there is normalised like any other. `None`
/// only when `path` is relative and there is no working directory to
/// resolve it against.
///
/// A `..` with nothing before it to cancel — which only a relative path
/// can have, and only one that climbs out of the working directory — is
/// kept, so two such paths still compare as themselves.
pub fn lexical(path: &Path) -> Option<PathBuf> {
    let absolute = match path.is_absolute() {
        true => path.to_path_buf(),
        false => std::env::current_dir().ok()?.join(path),
    };

    let mut normalised = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalised.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalised.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => normalised.push(component),
            },
            _ => normalised.push(component),
        }
    }
    Some(normalised)
}

/// Whether two normalised paths name the same file the way the platform
/// matches file names: case-sensitively on Unix, case-insensitively on
/// Windows.
pub fn same_name(left: &Path, right: &Path) -> bool {
    match cfg!(windows) {
        true => left
            .to_string_lossy()
            .to_lowercase()
            .eq(&right.to_string_lossy().to_lowercase()),
        false => left == right,
    }
}
