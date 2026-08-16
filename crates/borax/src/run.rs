//! Assembling an invocation: which files it works on, and what it runs
//! on.
//!
//! Both answers are made of pieces that already exist — the config
//! module knows how to find and merge layers, the CLI module knows what
//! the flags said — and what lives here is the order they go in.
//!
//! Each function comes in two forms: one taking the environment and the
//! filesystem as arguments, and one supplying the real ones. The first
//! is what tests use; the second is what the binary calls.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{
    ConfigError, ENV_PREFIX, Effective, Layer, Origin, global_config_path, layer_from_env,
    layer_from_toml, nearest_override, resolve,
};

/// The extension a file needs to be picked up from a directory.
pub const PDF_EXTENSION: &str = "pdf";

/// The files `paths` names.
///
/// A path that is a directory contributes every file below it, at any
/// depth, whose extension is [`PDF_EXTENSION`] ignoring case. A path
/// that is not a directory contributes itself whatever its extension:
/// naming a file is how a user says they meant that one.
///
/// Order is the order given, and within a directory it is sorted by
/// path, so the same arguments produce the same batch and the same
/// event stream twice running. A path reached twice — named directly
/// and again through a directory that holds it — appears once, at the
/// position it was first reached.
///
/// A directory that cannot be read contributes nothing rather than
/// ending the run: a batch is not the place to discover a permissions
/// problem, and the files that were readable still deserve their run.
pub fn inputs(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut collected: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path in paths {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };

        let mut reached = Vec::new();
        if metadata.is_dir() {
            documents(path, &mut reached);
            reached.sort();
        } else {
            reached.push(path.clone());
        }

        collected.extend(
            reached
                .into_iter()
                .filter(|reached| seen.insert(reached.clone())),
        );
    }

    collected
}

/// Add every file below `directory` whose extension is
/// [`PDF_EXTENSION`], ignoring case, to `found`.
///
/// A directory that cannot be read adds nothing, so one unreadable
/// subtree costs its own files and no others.
///
/// Metadata is read without following symlinks, so a link is neither a
/// file nor a directory here: it cannot send the walk round a loop, and
/// naming it directly is still how a user says they meant it.
fn documents(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(listing) = fs::read_dir(directory) else {
        return;
    };

    for entry in listing.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let path = entry.path();
        if metadata.is_dir() {
            documents(&path, found);
        } else if metadata.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(PDF_EXTENSION))
        {
            found.push(path);
        }
    }
}

/// The configuration a run started in `start` uses.
///
/// The layers, lowest precedence first: the built-in defaults that
/// [`crate::config::resolve`] supplies, the global configuration file,
/// the nearest `.borax.toml` at or above `start`, the environment, and
/// `flags`.
///
/// `start` is the directory the override search climbs from — the
/// directory of the first input path, or the working directory for a
/// subcommand that takes no paths.
///
/// `environment` is the process environment as name/value pairs, and
/// `read` answers the way [`std::fs::read_to_string`] does, so the
/// whole stack resolves in a test with neither a home directory nor a
/// disk.
///
/// A configuration file that is not there contributes no layer. One
/// that is there and will not parse is a [`ConfigError`] and ends the
/// run, because a file the user wrote and borax silently ignored is
/// worse than a run that stops and says so.
pub fn config_for(
    start: &Path,
    flags: Vec<(Origin, Layer)>,
    environment: &[(String, String)],
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Result<Effective, ConfigError> {
    let mut layers: Vec<(Origin, Layer)> = Vec::new();

    let global = global_config_path(|name| {
        environment
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| OsString::from(value))
    });
    if let Some(path) = global {
        if let Some(layer) = file_layer(&path, read)? {
            layers.push((Origin::GlobalFile(path), layer));
        }
    }

    if let Some(path) = nearest_override(start, |candidate| read(candidate).is_ok()) {
        if let Some(layer) = file_layer(&path, read)? {
            layers.push((Origin::DirectoryFile(path), layer));
        }
    }

    // One layer per variable rather than one for the whole environment,
    // so `borax config` names the variable a value came from.
    for (name, value) in environment {
        let Some(suffix) = name.strip_prefix(ENV_PREFIX) else {
            continue;
        };
        layers.push((
            Origin::Env(suffix.to_string()),
            layer_from_env([(name, value)])?,
        ));
    }

    layers.extend(flags);
    resolve(layers)
}

/// The layer the configuration file at `path` holds, or `None` when
/// there is no file there to read.
///
/// A read that fails contributes nothing, which is what lets the layers
/// below an absent file show through. Text that will not parse is a
/// [`ConfigError`] naming `path`.
fn file_layer(
    path: &Path,
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Result<Option<Layer>, ConfigError> {
    match read(path) {
        Ok(text) => layer_from_toml(&text, path).map(Some),
        Err(_) => Ok(None),
    }
}

/// [`config_for`] over the real environment and filesystem.
pub fn config_from_environment(
    start: &Path,
    flags: Vec<(Origin, Layer)>,
) -> Result<Effective, ConfigError> {
    let environment: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(name, value)| Some((into_string(name)?, into_string(value)?)))
        .collect();
    config_for(start, flags, &environment, &|path| {
        std::fs::read_to_string(path)
    })
}

/// `value` as a `String`, or `None` when it is not UTF-8.
///
/// A variable borax cannot read is a variable borax was not meant to
/// read: every name it looks for is ASCII.
fn into_string(value: OsString) -> Option<String> {
    value.into_string().ok()
}
