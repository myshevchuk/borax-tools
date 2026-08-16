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
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borax_core::record::{EntryType, Record};
use borax_core::template::{Template, TemplateTable};
use borax_pdf::tiered::ExtractionConfig;
use borax_sources::arxiv::ArxivClient;
use borax_sources::cache::{Cache, Cached, MemoryCache};
use borax_sources::crossref::CrossrefClient;
use borax_sources::http::Politeness;
use borax_sources::openalex::OpenAlexClient;
use borax_sources::pace::Paced;
use borax_sources::source::{Source, SourceName};
use borax_sources::store::{ContentIndex, FileCache, default_cache_root};
use borax_sources::transport::UreqTransport;

use crate::bib::{BibConfig, BibFiles, RealBibFiles, write_bib};
use crate::cli::{Cli, Command, flag_layers};
use crate::config::{
    Config, ConfigError, ENV_PREFIX, Effective, Layer, Origin, global_config_path, layer_from_env,
    layer_from_toml, nearest_override, resolve,
};
use crate::event::{Diagnostic, Event, Level, render};
use crate::journal::{Entry, FileJournal, Journal, RunId, undo_last};
use crate::pipeline::{
    FileOutcome, FileRecord, Library, RealLibrary, ResolveConfig, resolve_batch, resolve_file,
};
use crate::renaming::{
    Filesystem, LogFailure, MoveLog, RealFilesystem, apply_renames, counts_for, plan_renames,
};
use crate::session::{Outcome, outcome_for};

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
///
/// A path that cannot be reached at all contributes itself, so the
/// pipeline opens it and reports why it could not. Dropping it here
/// would let a mistyped filename produce an empty batch that skips
/// nothing and exits successfully — a run indistinguishable from one
/// where every file was fine.
pub fn inputs(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut collected: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path in paths {
        let mut reached = Vec::new();
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => {
                documents(path, &mut reached);
                reached.sort();
            }
            _ => reached.push(path.clone()),
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

    if let Some(path) = nearest_override(start, |candidate| present(candidate, read)) {
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
/// Only an absent file contributes nothing, which is what lets the
/// layers below it show through. Text that will not parse, and a file
/// that is there but cannot be read, are both a [`ConfigError`] naming
/// `path`: a file the user wrote and borax silently ignored is worse
/// than a run that stops and says so, and "not there" and "not readable"
/// are different answers.
fn file_layer(
    path: &Path,
    read: &dyn Fn(&Path) -> io::Result<String>,
) -> Result<Option<Layer>, ConfigError> {
    match read(path) {
        Ok(text) => layer_from_toml(&text, path).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ConfigError::Unreadable {
            path: path.to_path_buf(),
            message: error.to_string(),
        }),
    }
}

/// Whether `read` finds something at `path` — anything, readable or not.
///
/// Only [`io::ErrorKind::NotFound`] counts as absence. A file that is
/// there and cannot be read has to be found here so that [`file_layer`]
/// can refuse the run over it; answering "absent" would skip straight
/// past it.
fn present(path: &Path, read: &dyn Fn(&Path) -> io::Result<String>) -> bool {
    !matches!(read(path), Err(error) if error.kind() == io::ErrorKind::NotFound)
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

/// Where an invocation writes.
///
/// The event stream goes to `out` and diagnostics to `err`. Keeping the
/// two apart is what lets a `--json` consumer parse stdout line by line
/// however much the run has to complain about.
pub struct Streams<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Everything a run reaches the outside world through.
///
/// Every field is a seam already tested against a fake, so a whole
/// invocation runs in a test with neither a network, a PDF engine, nor
/// a disk.
pub struct Adapters<'a, C: Cache> {
    pub library: &'a dyn Library,
    pub sources: &'a [&'a dyn Source],
    pub index: &'a ContentIndex<C>,
    pub filesystem: &'a dyn Filesystem,
    /// Where applied renames are recorded, or `None` when there is
    /// nowhere to record them — no state directory, on a system that
    /// names none.
    pub journal: Option<&'a dyn Journal>,
    pub bib_files: &'a dyn BibFiles,
    /// The response cache's directory, or `None` when the system names
    /// no cache directory.
    pub cache_root: Option<PathBuf>,
    /// What an applied rename records as the time it happened.
    ///
    /// Called once per run, not once per file: its value timestamps
    /// every entry the run journals and, as a
    /// [`RunId`](crate::journal::RunId), is what makes
    /// them one run. `undo` reverts a run rather than a file, so entries
    /// that moved together have to be identified together.
    pub now: fn() -> String,
}

/// `name` as an entry type, or `None` when no entry type goes by it.
///
/// The names are the variants' own — `article`, `preprint`, `book`,
/// `chapter`, `thesis`, `report`, `patent`, `standard` — and not the
/// CSL-JSON strings they serialize to, where `article` names a preprint
/// and a journal article is `article-journal`. A configuration file is
/// written by a person, and `[templates.article]` has to mean what a
/// person means by it.
pub fn entry_type(name: &str) -> Option<EntryType> {
    match name {
        "article" => Some(EntryType::Article),
        "preprint" => Some(EntryType::Preprint),
        "book" => Some(EntryType::Book),
        "chapter" => Some(EntryType::Chapter),
        "thesis" => Some(EntryType::Thesis),
        "report" => Some(EntryType::Report),
        "patent" => Some(EntryType::Patent),
        "standard" => Some(EntryType::Standard),
        _ => None,
    }
}

/// The templates `config` describes, compiled.
///
/// `templates.default` is the table's fallback and is always present:
/// the built-in defaults supply it and merging removes no key. Every
/// other key names an entry type and overrides the default for it.
///
/// A key naming no entry type, and a template that will not compile,
/// are both [`Diagnostic`]s rather than skips: a template is
/// configuration, so a broken one is wrong for every file in the batch
/// and there is nothing to be gained by finding that out once per file.
pub fn templates(config: &Config) -> Result<TemplateTable, Diagnostic> {
    let compile = |key: &str, source: &str| {
        Template::compile(source).map_err(|failure| error(format!("templates.{key}: {failure}")))
    };

    let Some(default) = config.templates.get(DEFAULT_TEMPLATE) else {
        return Err(error(format!("templates.{DEFAULT_TEMPLATE} is unset")));
    };
    let mut table = TemplateTable::new(compile(DEFAULT_TEMPLATE, default)?);

    for (name, source) in &config.templates {
        if name == DEFAULT_TEMPLATE {
            continue;
        }
        let Some(entry_type) = entry_type(name) else {
            return Err(error(format!("templates.{name} names no entry type")));
        };
        table.insert(entry_type, compile(name, source)?);
    }

    Ok(table)
}

/// The key of the template every entry type without one of its own
/// falls back to.
const DEFAULT_TEMPLATE: &str = "default";

/// An error [`Diagnostic`] carrying `message`.
fn error(message: String) -> Diagnostic {
    Diagnostic {
        level: Level::Error,
        message,
    }
}

/// The events `command` produces, between the run's first and last.
///
/// One function per subcommand would repeat the same three lines of
/// setup six times; what differs between them is only which of
/// `adapters` they reach for.
///
/// Returns a [`Diagnostic`] for the failures that are about the run
/// rather than about a file, all three of which are the same shape —
/// something the whole invocation needs is missing, so there is no
/// per-file verdict to report:
///
/// - a template that will not compile, which is wrong for every file
///   in the batch;
/// - an applying rename with no journal to record it in, since an
///   unjournaled rename cannot be undone and being undoable is the
///   promise that makes renaming safe to offer;
/// - `cache` with no cache directory, because reporting an empty cache
///   would answer a question that was never asked.
pub fn events_for<C: Cache>(
    command: &Command,
    effective: &Effective,
    adapters: &Adapters<C>,
) -> Result<Vec<Event>, Diagnostic> {
    match command {
        Command::Config => Ok(effective.events()),
        Command::Cache { clear } => cache_events(*clear, adapters.cache_root.as_deref()),
        Command::Resolve { paths } => Ok(resolve_events(paths, effective, adapters)),
        Command::Rename { paths, apply } => rename_events(paths, *apply, effective, adapters),
        Command::Bib { paths } => bib_events(paths, effective, adapters),
        Command::Undo => Ok(undo_events(adapters)),
    }
}

/// The events `borax cache` produces over the cache at `root`.
///
/// One event either way: what the cache holds, or what emptying it
/// removed. A cache that cannot be read is a [`Diagnostic`] rather than
/// a count of zero, since an unreadable cache is not an empty one.
fn cache_events(clear: bool, root: Option<&Path>) -> Result<Vec<Event>, Diagnostic> {
    let Some(root) = root else {
        return Err(error("this system names no cache directory".to_string()));
    };

    match clear {
        true => crate::cache::clear(root).map(|stats| vec![crate::cache::cleared_event(&stats)]),
        false => crate::cache::inspect(root).map(|stats| vec![crate::cache::status_event(&stats)]),
    }
    .map_err(|failure| error(format!("\"{}\": {failure}", root.display())))
}

/// The events `borax resolve` produces for `paths`.
fn resolve_events<C: Cache>(
    paths: &[PathBuf],
    effective: &Effective,
    adapters: &Adapters<C>,
) -> Vec<Event> {
    let mut run = resolve_batch(
        paths,
        adapters.library,
        adapters.sources,
        adapters.index,
        &resolving(effective.config()),
    );
    // A batch closes its own stream; the one framing a whole invocation
    // is `dispatch`'s to open and close.
    run.events.pop();
    run.events
}

/// What a run may do while resolving, from `config`.
fn resolving(config: &Config) -> ResolveConfig {
    ResolveConfig {
        extraction: ExtractionConfig {
            page_limit: config.page_limit,
        },
        cache: config.cache,
    }
}

/// The events `borax rename` produces for `paths`, moving the files
/// when `apply` is set.
///
/// The journal is required before anything is resolved rather than
/// after the moves are planned, so a run that could not have been undone
/// costs no network and touches no file.
fn rename_events<C: Cache>(
    paths: &[PathBuf],
    apply: bool,
    effective: &Effective,
    adapters: &Adapters<C>,
) -> Result<Vec<Event>, Diagnostic> {
    let journal = match (apply, adapters.journal) {
        (true, None) => {
            return Err(error(
                "--apply needs a journal to record the moves in, and this system names no state \
                 directory"
                    .to_string(),
            ));
        }
        (true, journal) => journal,
        (false, _) => None,
    };
    let templates = templates(effective.config())?;

    let (mut events, resolved) = resolved_records(paths, effective, adapters);
    let log = journal.map(|journal| JournalLog {
        journal,
        resolved: &resolved,
        at: (adapters.now)(),
    });
    let applied = apply_renames(
        &plan_renames(
            &resolved,
            &templates,
            effective.config().collision,
            adapters.filesystem,
        ),
        adapters.filesystem,
        apply,
        log.as_ref().map(|log| log as &dyn MoveLog),
    );

    // A halt needs no separate report: every move it abandoned is
    // already an `Unjournalable` skip carrying the reason, so the run
    // says what happened through the same stream as everything else.
    let resolved = at_current_paths(resolved, &applied.events);
    events.extend(applied.events);
    events.extend(bib_output(&resolved, &templates, effective, adapters));
    Ok(events)
}

/// The [`MoveLog`] backed by the run's journal.
///
/// Every entry a run appends carries the same `at`, which is what makes
/// them one run for [`crate::journal::undo_last`]. The hash comes from
/// the record resolved for the file, since `undo` verifies by content
/// and an entry without a hash names a move nothing could reverse.
struct JournalLog<'a> {
    journal: &'a dyn Journal,
    resolved: &'a [(PathBuf, FileRecord)],
    at: String,
}

impl MoveLog for JournalLog<'_> {
    /// Append one entry for the move of `from` to `to`.
    ///
    /// A file whose content hash is unknown is a
    /// [`LogFailure::Move`]: `undo` verifies by hash, so an entry
    /// without one could never be acted on, and moving the file anyway
    /// would put it beyond reach of the command meant to bring it back.
    /// An append that fails is a [`LogFailure::Journal`], since a
    /// journal that would not take this entry will not take the next.
    fn record(&self, from: &Path, to: &Path) -> Result<(), LogFailure> {
        let hash = self
            .resolved
            .iter()
            .find(|(path, _)| path == from)
            .and_then(|(_, file)| file.hash.clone())
            .ok_or_else(|| LogFailure::Move("the file's content hash is unknown".to_string()))?;

        self.journal
            .append(&[Entry {
                run: RunId::new(self.at.clone()),
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                hash,
                at: self.at.clone(),
            }])
            .map_err(|error| LogFailure::Journal(error.to_string()))
    }
}

/// Resolve `paths`, keeping the records alongside the events.
///
/// Renaming and bibliography output both work from the records rather
/// than from the events, so the pair is what they need;
/// [`crate::pipeline::resolve_batch`] reports the same events and keeps
/// nothing. Only resolved files appear in the records, each paired with
/// the path it was resolved from.
fn resolved_records<C: Cache>(
    paths: &[PathBuf],
    effective: &Effective,
    adapters: &Adapters<C>,
) -> (Vec<Event>, Vec<(PathBuf, FileRecord)>) {
    let mut events = Vec::with_capacity(paths.len());
    let mut resolved = Vec::new();

    for path in paths {
        let outcome = resolve_file(
            path,
            adapters.library,
            adapters.sources,
            adapters.index,
            &resolving(effective.config()),
        );
        events.push(crate::pipeline::event_for(path, &outcome));
        if let FileOutcome::Resolved(file) = outcome {
            resolved.push((path.clone(), file));
        }
    }

    (events, resolved)
}

/// `resolved` with every path `events` reports a move for replaced by
/// where it moved, so a sidecar lands beside the name a file now carries
/// rather than beside the one it has just lost.
fn at_current_paths(
    resolved: Vec<(PathBuf, FileRecord)>,
    events: &[Event],
) -> Vec<(PathBuf, FileRecord)> {
    resolved
        .into_iter()
        .map(|(path, file)| {
            let moved = events.iter().find_map(|event| match event {
                Event::Renamed { path: from, target } if *from == path => Some(target.clone()),
                _ => None,
            });
            (moved.unwrap_or(path), file)
        })
        .collect()
}

/// The bibliography events a run that was not asked for bibliography
/// output still produces, which is none unless a destination is
/// configured.
///
/// A run with neither a master file nor sidecars has nowhere to write,
/// and asking [`crate::bib::write_bib`] anyway would report records too
/// sparse to cite as skipped in a run that was never going to cite them.
fn bib_output<C: Cache>(
    resolved: &[(PathBuf, FileRecord)],
    templates: &TemplateTable,
    effective: &Effective,
    adapters: &Adapters<C>,
) -> Vec<Event> {
    let config = bib_config(effective.config());
    match config.path.is_some() || config.sidecars {
        true => write_bib(resolved, templates, &config, adapters.bib_files),
        false => Vec::new(),
    }
}

/// The events `borax bib` produces for `paths`.
///
/// Unlike the bibliography output a rename run produces on the side,
/// this one is what was asked for, so it runs whether or not a
/// destination is configured — a run that writes nowhere still reports
/// what it resolved.
fn bib_events<C: Cache>(
    paths: &[PathBuf],
    effective: &Effective,
    adapters: &Adapters<C>,
) -> Result<Vec<Event>, Diagnostic> {
    let templates = templates(effective.config())?;
    let (mut events, resolved) = resolved_records(paths, effective, adapters);

    events.extend(write_bib(
        &resolved,
        &templates,
        &bib_config(effective.config()),
        adapters.bib_files,
    ));
    Ok(events)
}

/// Where bibliography output goes, from `config`.
fn bib_config(config: &Config) -> BibConfig {
    BibConfig {
        path: config.bib_path.clone(),
        duplicates: config.duplicates,
        sidecars: config.sidecars,
    }
}

/// The events `borax undo` produces.
///
/// A run with no journal reverts nothing and reports nothing, which is
/// what an absent journal means: there is no move on record to undo.
fn undo_events<C: Cache>(adapters: &Adapters<C>) -> Vec<Event> {
    let Some(journal) = adapters.journal else {
        return Vec::new();
    };

    undo_last(journal, adapters.library, adapters.filesystem)
        .iter()
        .map(crate::journal::event_for)
        .collect()
}

/// Carry out `cli` against `adapters`, writing to `streams`.
///
/// The stream always opens with [`Event::RunStarted`] and closes with
/// [`Event::RunFinished`], whatever happened in between, so a consumer
/// can tell a run that produced nothing from a run that was cut off.
/// Events a format has nothing to say about are simply not written.
///
/// A [`Diagnostic`] from [`events_for`] goes to `streams.err` and ends
/// the run as [`Outcome::Fatal`]: nothing was attempted, so there is no
/// event stream to close.
pub fn dispatch<C: Cache>(
    cli: &Cli,
    effective: &Effective,
    adapters: &Adapters<C>,
    streams: &mut Streams,
) -> Outcome {
    let body = match events_for(&cli.command, effective, adapters) {
        Ok(body) => body,
        Err(diagnostic) => {
            let _ = writeln!(streams.err, "{diagnostic}");
            return Outcome::Fatal;
        }
    };

    let counts = counts_for(&body);
    let mut events = Vec::with_capacity(body.len() + 2);
    events.push(Event::RunStarted {
        command: cli.command.name().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        applying: applying(&cli.command),
    });
    events.extend(body);
    events.push(Event::RunFinished { counts });

    for event in &events {
        if let Some(line) = render(cli.format(), event) {
            let _ = writeln!(streams.out, "{line}");
        }
    }

    outcome_for(&counts)
}

/// Whether `command` will change what is on disk rather than only
/// report on it.
fn applying(command: &Command) -> bool {
    match command {
        Command::Rename { apply, .. } => *apply,
        Command::Cache { clear } => *clear,
        Command::Bib { .. } | Command::Undo => true,
        Command::Resolve { .. } | Command::Config => false,
    }
}

/// Carry out `cli` against the real world.
///
/// Builds every adapter from `cli` and the environment, resolves the
/// configuration, and hands both to [`dispatch`]. A configuration that
/// will not resolve is written to `streams.err` and ends the run as
/// [`Outcome::Fatal`] before an adapter is built: a run on settings
/// borax could not read is a run on settings nobody chose.
pub fn execute(cli: &Cli, streams: &mut Streams) -> Outcome {
    let effective = match config_from_environment(
        &start_directory_for(&cli.command),
        flag_layers(&cli.settings),
    ) {
        Ok(effective) => effective,
        Err(failure) => {
            let _ = writeln!(streams.err, "{}", error(failure.to_string()));
            return Outcome::Fatal;
        }
    };

    let transport = UreqTransport::default();
    let politeness = Politeness {
        mailto: effective.config().mailto.clone(),
    };
    let interval = Duration::from_millis(effective.config().min_interval_ms);
    let caching = effective.config().cache;

    let mut owned: Vec<Box<dyn Source>> = Vec::new();
    let selected = |name| effective.config().sources.contains(&name);
    if selected(SourceName::Crossref) {
        let client = CrossrefClient::new(transport.clone(), politeness.clone());
        owned.push(polite(client, interval, caching));
    }
    if selected(SourceName::OpenAlex) {
        let client = OpenAlexClient::new(transport.clone(), politeness.clone());
        owned.push(polite(client, interval, caching));
    }
    if selected(SourceName::Arxiv) {
        let client = ArxivClient::new(transport, politeness);
        owned.push(polite(client, interval, caching));
    }
    let sources: Vec<&dyn Source> = owned.iter().map(Box::as_ref).collect();

    let index = ContentIndex::new(response_cache());
    let journal = FileJournal::open_default();

    dispatch(
        &Cli {
            command: expanded(&cli.command),
            settings: cli.settings.clone(),
            json: cli.json,
        },
        &effective,
        &Adapters {
            library: &RealLibrary,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            journal: journal.as_ref().map(|journal| journal as &dyn Journal),
            bib_files: &RealBibFiles,
            cache_root: default_cache_root(),
            now: timestamp,
        },
        streams,
    )
}

/// `source` wrapped in the decorators a real run adds to it: pacing
/// always, and the response cache unless the run was told to bypass it.
///
/// The cache goes outside the pacing so an answer borax already holds
/// costs neither a request nor the interval a request would have had to
/// wait out; a run over a directory it has seen before then finishes at
/// disk speed rather than at the network's.
///
/// Each source gets a cache of its own. They address the same store —
/// the key carries the service's name, so two services cannot collide
/// over one identifier — and a [`FileCache`] is a path, not an open
/// handle, so holding three costs nothing.
fn polite<S: Source + 'static>(source: S, interval: Duration, caching: bool) -> Box<dyn Source> {
    let paced = Paced::new(source, interval);
    match caching {
        true => Box::new(Cached::new(paced, response_cache())),
        false => Box::new(paced),
    }
}

/// A cache over the borax cache directory, or an in-memory one when the
/// system names no such directory.
///
/// The in-memory fallback still saves a run from asking twice about one
/// identifier; it simply forgets when the process ends.
fn response_cache() -> ResponseCache {
    match FileCache::open_default() {
        Some(cache) => ResponseCache::File(cache),
        None => ResponseCache::Memory(MemoryCache::new()),
    }
}

/// The directory the configuration search climbs from.
///
/// A path that names a directory is the starting point itself; anything
/// else starts at its parent. The distinction is the whole point:
/// `borax rename <dir>` and `borax rename <dir>/paper.pdf` have to find
/// the same `<dir>/.borax.toml`, and taking the parent of both climbs
/// one level too high for the first — past the override file, to
/// whatever sits above it.
///
/// `paths` is the paths as the user typed them, before a directory is
/// expanded into the files it holds, because that is where the question
/// "did they name a directory?" can still be asked. `is_directory`
/// answers it, and `working` is where a run given no usable path starts.
///
/// A path that is neither an existing file nor an existing directory
/// starts at its parent, since `is_directory` simply answers `false`
/// for it: a name that is not there is still a name in a directory.
pub fn start_directory(
    paths: &[PathBuf],
    is_directory: &dyn Fn(&Path) -> bool,
    working: &Path,
) -> PathBuf {
    let Some(first) = paths.first() else {
        return working.to_path_buf();
    };
    if is_directory(first) {
        return first.clone();
    }
    match first.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => working.to_path_buf(),
    }
}

/// [`start_directory`] over the real filesystem and working directory.
fn start_directory_for(command: &Command) -> PathBuf {
    let working = std::env::current_dir().unwrap_or_default();
    start_directory(command.paths(), &|path| path.is_dir(), &working)
}

/// `command` with each path it was given replaced by the files that path
/// names ([`inputs`]).
///
/// Expansion happens here rather than in [`events_for`], because which
/// files a directory holds is a question for the filesystem: a
/// subcommand's paths reach [`events_for`] as the files to work on,
/// whoever decided which those are.
fn expanded(command: &Command) -> Command {
    match command {
        Command::Resolve { paths } => Command::Resolve {
            paths: inputs(paths),
        },
        Command::Rename { paths, apply } => Command::Rename {
            paths: inputs(paths),
            apply: *apply,
        },
        Command::Bib { paths } => Command::Bib {
            paths: inputs(paths),
        },
        Command::Undo | Command::Config | Command::Cache { .. } => command.clone(),
    }
}

/// The time a real run records for the moves it journals: milliseconds
/// since the Unix epoch, in decimal.
///
/// Two runs of the same binary a millisecond apart read differently,
/// which is what [`Adapters::now`] needs of the value to make one run's
/// entries tell themselves from another's. A clock reading before the
/// epoch reports zero rather than ending the run.
fn timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_millis().to_string(),
        Err(_) => "0".to_string(),
    }
}

/// The response cache a real run reads and writes.
///
/// A system naming a cache directory gets a [`FileCache`] there, and one
/// naming none gets an in-memory cache: the run still avoids asking
/// twice about a file it has already seen, and forgets it when the
/// process ends.
enum ResponseCache {
    File(FileCache),
    Memory(MemoryCache),
}

impl Cache for ResponseCache {
    fn get(&self, key: &str) -> Option<Record> {
        match self {
            ResponseCache::File(cache) => cache.get(key),
            ResponseCache::Memory(cache) => cache.get(key),
        }
    }

    fn put(&self, key: &str, record: &Record) {
        match self {
            ResponseCache::File(cache) => cache.put(key, record),
            ResponseCache::Memory(cache) => cache.put(key, record),
        }
    }
}
