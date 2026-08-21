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

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borax_core::content::ContentHash;
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

use crate::bib::{BibConfig, BibFiles, Keyed, RealBibFiles, merge_master, write_sidecar};
use crate::cli::{Cli, Command, flag_layers};
use crate::config::{
    Config, ConfigError, ENV_PREFIX, Effective, Layer, Origin, global_config_path, layer_from_env,
    layer_from_toml, nearest_override, resolve,
};
use crate::event::{Counts, Diagnostic, Event, Format, Level, render};
use crate::journal::{Entry, FileJournal, Journal, RunId, undo_last};
use crate::pipeline::{
    FileOutcome, FileRecord, Library, RealLibrary, ResolveConfig, resolve_batch, resolve_file,
};
use crate::renaming::{Applying, Filesystem, LogFailure, MoveLog, Planning, RealFilesystem};
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

/// The configuration each input runs under.
///
/// `.borax.toml` is discovered upward from each input file's own
/// directory, so an invocation spanning two trees applies each tree's
/// overrides to its own files. Resolving once for the whole run instead
/// would make the answer depend on which path the user typed first.
///
/// Every layer except the override file is the same throughout: the
/// defaults, the global file, the environment and the flags do not
/// change with where a file sits.
///
/// # What a per-directory override cannot reach
///
/// The settings that decide which services a run talks to and how it
/// identifies itself — `sources`, `mailto`, and the `network` table —
/// are taken from [`Configs::run`] alone. The clients are built once,
/// before any file is looked at, so there is no per-file moment at
/// which a different set of them could be chosen. Everything a file's
/// own directory can change — its template, its collision policy, its
/// bibliography destination, how far extraction reads — is what
/// [`Configs::for_path`] carries.
#[derive(Debug)]
pub struct Configs {
    by_directory: BTreeMap<PathBuf, Effective>,
    run: Effective,
}

impl Configs {
    /// Resolve the configuration for every directory `paths` touches,
    /// and the run's own by climbing from `working`.
    ///
    /// `paths` are the input files after directory expansion, so the
    /// directory a file belongs to is its parent. Each distinct
    /// directory is resolved once however many files share it.
    ///
    /// The remaining arguments are [`config_for`]'s, and the layers
    /// stack exactly as they do there. An override file that will not
    /// parse — in any of the directories — is a [`ConfigError`] and
    /// ends the run, since a file the user wrote and borax ignored is
    /// worse than a run that stops and says so.
    pub fn resolve(
        paths: &[PathBuf],
        working: &Path,
        flags: Vec<(Origin, Layer)>,
        environment: &[(String, String)],
        read: &dyn Fn(&Path) -> io::Result<String>,
    ) -> Result<Configs, ConfigError> {
        let mut by_directory: BTreeMap<PathBuf, Effective> = BTreeMap::new();
        for directory in paths.iter().filter_map(|path| path.parent()) {
            if by_directory.contains_key(directory) {
                continue;
            }
            let effective = config_for(directory, flags.clone(), environment, read)?;
            by_directory.insert(directory.to_path_buf(), effective);
        }

        Ok(Configs {
            by_directory,
            run: config_for(working, flags, environment, read)?,
        })
    }

    /// One configuration for every path.
    ///
    /// What a caller with nothing to discover uses — a run whose inputs
    /// are already known to share a configuration, and the tests that
    /// are not about discovery.
    pub fn uniform(effective: Effective) -> Configs {
        Configs {
            by_directory: BTreeMap::new(),
            run: effective,
        }
    }

    /// The configuration the file at `path` runs under.
    ///
    /// A path whose directory was never resolved falls back to
    /// [`Configs::run`]: the run's own configuration is the right
    /// answer for a file nothing more specific was worked out for.
    pub fn for_path(&self, path: &Path) -> &Effective {
        match path.parent() {
            Some(directory) => self.for_directory(directory),
            None => &self.run,
        }
    }

    /// The configuration files in `directory` run under.
    ///
    /// A directory that was never resolved falls back to
    /// [`Configs::run`], as [`Configs::for_path`] does.
    pub fn for_directory(&self, directory: &Path) -> &Effective {
        self.by_directory.get(directory).unwrap_or(&self.run)
    }

    /// The configuration for the run as a whole, resolved from the
    /// working directory.
    ///
    /// What `borax config` prints and what a subcommand taking no paths
    /// uses.
    pub fn run(&self) -> &Effective {
        &self.run
    }
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

/// [`Configs::resolve`] over the real environment and filesystem.
fn configs_from_environment(
    paths: &[PathBuf],
    working: &Path,
    flags: Vec<(Origin, Layer)>,
) -> Result<Configs, ConfigError> {
    let environment: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(name, value)| Some((into_string(name)?, into_string(value)?)))
        .collect();
    Configs::resolve(paths, working, flags, &environment, &|path| {
        std::fs::read_to_string(path)
    })
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

/// The templates `map` describes, compiled.
///
/// `map` is one of the configuration's template tables — the filename
/// templates or the citation-key ones — and `prefix` is the name that
/// table goes by in a configuration file, which every diagnostic here
/// names the offending key under: `templates.thesis`,
/// `citation-keys.default`.
///
/// The `default` key is the table's fallback and is always present: the
/// built-in defaults supply it and merging removes no key. Every other
/// key names an entry type and overrides the default for it.
///
/// A key naming no entry type, and a template that will not compile,
/// are both [`Diagnostic`]s rather than skips: a template is
/// configuration, so a broken one is wrong for every file in the batch
/// and there is nothing to be gained by finding that out once per file.
pub fn templates(
    map: &BTreeMap<String, String>,
    prefix: &str,
) -> Result<TemplateTable, Diagnostic> {
    let compile = |key: &str, source: &str| {
        Template::compile(source).map_err(|failure| error(format!("{prefix}.{key}: {failure}")))
    };

    let Some(default) = map.get(DEFAULT_TEMPLATE) else {
        return Err(error(format!("{prefix}.{DEFAULT_TEMPLATE} is unset")));
    };
    let mut table = TemplateTable::new(compile(DEFAULT_TEMPLATE, default)?);

    for (name, source) in map {
        if name == DEFAULT_TEMPLATE {
            continue;
        }
        let Some(entry_type) = entry_type(name) else {
            return Err(error(format!("{prefix}.{name} names no entry type")));
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

/// Where a run's events go as it produces them.
///
/// A run writes each event when it happens rather than returning them
/// all at the end, which is what lets a reader watch a network-bound
/// run make progress. What becomes of an event is the sink's own
/// business: [`dispatch`] renders one and writes the line, and a
/// `Vec<Event>` collects it for a caller that wants the whole run as a
/// value.
pub trait Sink {
    /// Take `event`.
    fn emit(&mut self, event: Event);
}

/// Collecting a run rather than showing it, for a caller that wants to
/// look at the whole stream.
impl Sink for Vec<Event> {
    fn emit(&mut self, event: Event) {
        self.push(event);
    }
}

/// One directory's share of a run: the files below it, and the
/// compiled templates its configuration calls for.
///
/// A run spanning two trees is a run under two configurations, so both
/// tables are the ones that directory's own configuration resolved to.
pub struct Group {
    /// The directory the group's configuration was resolved for.
    pub directory: PathBuf,
    /// The files in it, in the order the run reached them.
    pub paths: Vec<PathBuf>,
    /// The filename templates, which name a renamed file.
    pub filenames: TemplateTable,
    /// The citation-key templates, which name a cited record.
    pub citation_keys: TemplateTable,
}

/// What a run's fallible checks produced.
///
/// Every way a run can end before it starts — a template that will not
/// compile, `--apply` with nowhere to journal, `cache` on a system that
/// names no cache directory — is settled by [`preflight`], which runs
/// before the first event is written. What it hands back is what those
/// checks yielded, which is why [`emit_events`] has no failure left to
/// report and a [`Diagnostic`] can still mean that nothing was emitted.
pub enum Prepared<'a> {
    /// A command with nothing that could fail: `config`, `resolve`,
    /// `undo`.
    Unchecked,
    /// `cache`, with the report inspecting or clearing produced.
    ///
    /// This command's whole work is the one event, and that work is
    /// what can fail — an unreadable cache is not an empty one — so it
    /// happens in [`preflight`], where there is still a way to refuse
    /// the run.
    Cache { report: Event },
    /// `rename` or `bib`: the run's paths grouped by the directory
    /// holding them, in the order those directories are first reached,
    /// each paired with the tables compiled for it.
    ///
    /// Holding the compiled tables is what makes "every template
    /// compiles before any file is touched" a property of the type
    /// rather than an ordering inside a function: emitting cannot reach
    /// a file without already holding the tables for its directory.
    Grouped {
        groups: Vec<Group>,
        /// Where an applying rename records its moves. `None` for a
        /// preview and for `bib`, neither of which moves anything.
        journal: Option<&'a dyn Journal>,
    },
}

/// Settle everything about `command` that could end the run, before any
/// of it is reported.
///
/// Returns a [`Diagnostic`] for the failures that are about the run
/// rather than about a file, all three of which are the same shape —
/// something the whole invocation needs is missing, so there is no
/// per-file verdict to report:
///
/// - a template that will not compile, which is wrong for every file in
///   the batch;
/// - an applying rename with no journal to record it in, since an
///   unjournaled rename cannot be undone and being undoable is the
///   promise that makes renaming safe to offer;
/// - `cache` with no cache directory, or with one that cannot be read,
///   because reporting an empty cache would answer a question that was
///   never asked.
///
/// For a command that works on files, nothing here reads one, queries a
/// source, or moves anything, so a run refused at this point costs no
/// network and leaves no trace. `borax cache` is the exception, and the
/// third failure above is why: its whole work is a single event, and
/// the work is the part that can fail.
pub fn preflight<'a, C: Cache>(
    command: &Command,
    configs: &Configs,
    adapters: &'a Adapters<'a, C>,
) -> Result<Prepared<'a>, Diagnostic> {
    match command {
        Command::Config | Command::Resolve { .. } | Command::Undo => Ok(Prepared::Unchecked),
        Command::Cache { clear } => match adapters.cache_root.as_deref() {
            Some(root) => Ok(Prepared::Cache {
                report: cache_report(*clear, root)?,
            }),
            None => Err(error("this system names no cache directory".to_string())),
        },
        Command::Rename { paths, apply } => {
            // An invocation missing both a journal and a compilable
            // template is told about the journal. Nothing is touched
            // either way, so the order decides only which of the two
            // refusals the user reads first.
            let journal = match (*apply, adapters.journal) {
                (true, None) => {
                    return Err(error(
                        "--apply needs a journal to record the moves in, and this system names no \
                         state directory"
                            .to_string(),
                    ));
                }
                (true, journal) => journal,
                (false, _) => None,
            };
            Ok(Prepared::Grouped {
                groups: compiled_groups(paths, configs)?,
                journal,
            })
        }
        Command::Bib { paths } => Ok(Prepared::Grouped {
            groups: compiled_groups(paths, configs)?,
            // A bibliography run moves nothing, so it has nothing to
            // record and needs no journal to record it in.
            journal: None,
        }),
    }
}

/// `paths` grouped by directory ([`by_directory`]), each group paired
/// with the template tables its directory's configuration compiles to.
///
/// Every group is compiled before any of them is worked, and both of a
/// group's tables before either is used, so a template that will not
/// compile — filename or citation key, in any of the directories the
/// run spans — ends the run before a single file is read.
fn compiled_groups(paths: &[PathBuf], configs: &Configs) -> Result<Vec<Group>, Diagnostic> {
    by_directory(paths)
        .into_iter()
        .map(|(directory, paths)| {
            let config = configs.for_directory(&directory).config();
            Ok(Group {
                filenames: templates(&config.templates, "templates")?,
                citation_keys: templates(&config.citation_keys, "citation-keys")?,
                directory,
                paths,
            })
        })
        .collect()
}

/// Write the events `command` produces into `sink`, between the run's
/// first and last.
///
/// Infallible by construction: `prepared` is what [`preflight`] made of
/// everything that could have gone wrong, so what is left is work that
/// reports its own outcome per file.
///
/// Events reach `sink` as the run decides them rather than at the end,
/// so a reader watches a slow run make progress. Which command writes
/// as it decides and which has to gather a batch first is the
/// command's own contract.
pub fn emit_events<C: Cache>(
    prepared: &Prepared<'_>,
    command: &Command,
    configs: &Configs,
    adapters: &Adapters<'_, C>,
    sink: &mut dyn Sink,
) {
    match (command, prepared) {
        (Command::Config, _) => {
            for event in configs.run().events() {
                sink.emit(event);
            }
        }
        (Command::Cache { .. }, Prepared::Cache { report }) => sink.emit(report.clone()),
        (Command::Resolve { paths }, _) => resolve_events(paths, configs, adapters, sink),
        (Command::Rename { apply, .. }, Prepared::Grouped { groups, journal }) => {
            rename_events(groups, *apply, *journal, configs, adapters, sink);
        }
        (Command::Bib { .. }, Prepared::Grouped { groups, .. }) => {
            bib_events(groups, configs, adapters, sink);
        }
        (Command::Undo, _) => undo_events(adapters, sink),
        // A `Prepared` that does not go with the command could only
        // come of pairing one command's preflight with another's
        // emission, which no caller does: `events_for` and `dispatch`
        // both preflight the command they go on to emit. There is
        // nothing such a pair could report, so it reports nothing.
        _ => {}
    }
}

/// The events `command` produces, between the run's first and last,
/// collected rather than streamed.
///
/// [`preflight`] and [`emit_events`], with a `Vec<Event>` for the sink:
/// a caller that wants a whole run as a value — to assert on it, or to
/// look at its order — runs exactly the code a streaming caller runs,
/// so neither can drift from the other.
///
/// Returns whatever [`preflight`] refuses the run for, in which case
/// nothing was emitted and nothing was touched.
pub fn events_for<C: Cache>(
    command: &Command,
    configs: &Configs,
    adapters: &Adapters<C>,
) -> Result<Vec<Event>, Diagnostic> {
    let prepared = preflight(command, configs, adapters)?;
    let mut events: Vec<Event> = Vec::new();
    emit_events(&prepared, command, configs, adapters, &mut events);
    Ok(events)
}

/// What `borax cache` makes of the cache at `root`.
///
/// One event either way: what the cache holds, or what emptying it
/// removed. A cache that cannot be read is a [`Diagnostic`] rather than
/// a count of zero, since an unreadable cache is not an empty one.
fn cache_report(clear: bool, root: &Path) -> Result<Event, Diagnostic> {
    match clear {
        true => crate::cache::clear(root).map(|stats| crate::cache::cleared_event(&stats)),
        false => crate::cache::inspect(root).map(|stats| crate::cache::status_event(&stats)),
    }
    .map_err(|failure| error(format!("\"{}\": {failure}", root.display())))
}

/// Write the events `borax resolve` produces for `paths` into `sink`.
///
/// One event per file, in the order given, each resolved under the
/// configuration its own directory implies.
///
/// The batch is resolved before the first line is written, which is the
/// price of `concurrency`: files finish in whatever order the network
/// answers, and reporting them as they finish would make the stream's
/// order depend on that. Order is the property worth keeping — a run
/// stays diffable against itself — so `resolve` waits where `rename`,
/// which is serial, does not.
fn resolve_events<C: Cache>(
    paths: &[PathBuf],
    configs: &Configs,
    adapters: &Adapters<C>,
    sink: &mut dyn Sink,
) {
    let mut run = resolve_batch(
        paths,
        adapters.library,
        adapters.sources,
        adapters.index,
        &|path| resolving(configs.for_path(path).config()),
        // How many files at once is a network setting, so it comes from
        // the run rather than from any one file's directory.
        configs.run().config().concurrency,
    );
    // A batch closes its own stream; the one framing a whole invocation
    // is `dispatch`'s to open and close.
    run.events.pop();
    for event in run.events {
        sink.emit(event);
    }
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

/// Write the events `borax rename` produces for `groups` into `sink`,
/// moving the files when `apply` is set.
///
/// `groups` is what [`preflight`] made of the run's paths: each
/// directory the run spans, the files in it, and the template tables its
/// configuration compiled to. `journal` is where an applying run records
/// its moves, and is `None` for a preview, which makes none.
///
/// A group is worked under its own directory's configuration — its
/// templates, its collision policy, its bibliography destination — since
/// a run spanning two trees is a run under two configurations.
///
/// Within a group a file is finished before the next is opened: its
/// verdict, the move planned or made for it, and any sidecar written
/// beside it are one contiguous run of events, in the order the group
/// gives its files. The merge into the master `.bib` is the exception
/// and trails the group, being work about the whole of it rather than
/// about any one file.
fn rename_events<C: Cache>(
    groups: &[Group],
    apply: bool,
    journal: Option<&dyn Journal>,
    configs: &Configs,
    adapters: &Adapters<C>,
    sink: &mut dyn Sink,
) {
    let at = (adapters.now)();
    for group in groups {
        let effective = configs.for_directory(&group.directory);
        let config = bib_config(effective.config());
        // A run with neither a master file nor sidecars has nowhere to
        // write, and citing anyway would report records too sparse to
        // cite as skipped in a run that was never going to cite them.
        let cites = config.path.is_some() || config.sidecars;

        let mut planning = Planning::new(
            &group.directory,
            &group.filenames,
            effective.config().collision,
            adapters.filesystem,
        );
        // A halt needs no separate report: every move it abandoned is
        // already an `Unjournalable` skip carrying the reason, so the run
        // says what happened through the same stream as everything else.
        let mut applying = Applying::new(adapters.filesystem, apply);
        let mut cited = Citations::default();

        for path in &group.paths {
            let Some(file) = resolved_record(path, effective, adapters, sink) else {
                continue;
            };

            let log = journal.map(|journal| JournalLog {
                journal,
                hash: file.hash.clone(),
                at: at.clone(),
            });
            let event = applying.carry_out(
                &planning.plan(path, &file),
                log.as_ref().map(|log| log as &dyn MoveLog),
            );
            // A sidecar goes beside the name the file now carries rather
            // than beside the one it has just lost, so where it lands is
            // where the move that just happened put it.
            let current = match &event {
                Event::Renamed { target, .. } => target.clone(),
                _ => path.clone(),
            };
            sink.emit(event);

            if cites {
                cited.add(
                    current,
                    file,
                    &group.citation_keys,
                    &config,
                    adapters.bib_files,
                    sink,
                );
            }
        }

        if cites {
            cited.merge(&config, adapters.bib_files, sink);
        }
    }
}

/// A directory group's citable records, held from the file each was
/// resolved from until the group's merge into the master `.bib`.
///
/// A sidecar is one file's own output and is written as that file is
/// reached. The master file is one read-modify-write over the whole
/// group, and the merge assigns keys unique across everything it is
/// given, so it waits until the group has nothing left to add.
#[derive(Default)]
struct Citations {
    entries: Vec<(PathBuf, FileRecord, String)>,
}

impl Citations {
    /// Write the sidecar for the file now at `path`, reporting it into
    /// `sink`, and keep `file` for the merge when it has a citation key.
    fn add(
        &mut self,
        path: PathBuf,
        file: FileRecord,
        citation_keys: &TemplateTable,
        config: &BibConfig,
        files: &dyn BibFiles,
        sink: &mut dyn Sink,
    ) {
        let (key, event) = write_sidecar(&path, &file, citation_keys, config, files);
        if let Some(event) = event {
            sink.emit(event);
        }
        if let Some(key) = key {
            self.entries.push((path, file, key));
        }
    }

    /// Merge everything kept into the master file, reporting each entry's
    /// outcome into `sink`.
    fn merge(self, config: &BibConfig, files: &dyn BibFiles, sink: &mut dyn Sink) {
        let keyed: Vec<Keyed> = self
            .entries
            .iter()
            .map(|(path, file, key)| Keyed {
                path,
                record: &file.record,
                key: key.clone(),
            })
            .collect();
        for event in merge_master(&keyed, config, files) {
            sink.emit(event);
        }
    }
}

/// `paths` grouped by the directory holding them, in the order those
/// directories are first reached.
///
/// A run spanning two trees is a run under two configurations, and
/// nearly everything renaming does is per directory already: collisions
/// are a property of a directory, and so are the template and the
/// bibliography destination a file's own `.borax.toml` chooses. Working
/// a group at a time is what lets each group use its own.
///
/// A run over one directory — which is nearly every run — yields a
/// single group, and its event stream is exactly what it was before
/// there was any grouping at all.
fn by_directory(paths: &[PathBuf]) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let mut groups: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    for path in paths {
        let directory = path.parent().unwrap_or(Path::new("")).to_path_buf();
        match groups.iter_mut().find(|(known, _)| *known == directory) {
            Some((_, group)) => group.push(path.clone()),
            None => groups.push((directory, vec![path.clone()])),
        }
    }
    groups
}

/// The [`MoveLog`] recording one file's move in the run's journal.
///
/// Every entry a run appends carries the same `at`, which is what makes
/// them one run for [`crate::journal::undo_last`]. `hash` is what was
/// resolved for the file this log is about, since `undo` verifies by
/// content and an entry without a hash names a move nothing could
/// reverse.
struct JournalLog<'a> {
    journal: &'a dyn Journal,
    hash: Option<ContentHash>,
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
            .hash
            .clone()
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

/// Resolve the file at `path` under `effective`, writing its verdict
/// into `sink`, and hand back the record when there is one.
///
/// Renaming and bibliography output both work from the record rather
/// than from the event, so the record is what this hands back; the event
/// is already written by the time it does.
///
/// Each file is resolved and reported before the next is opened, so a
/// reader watching a network-bound run sees it make progress.
fn resolved_record<C: Cache>(
    path: &Path,
    effective: &Effective,
    adapters: &Adapters<C>,
    sink: &mut dyn Sink,
) -> Option<FileRecord> {
    let outcome = resolve_file(
        path,
        adapters.library,
        adapters.sources,
        adapters.index,
        &resolving(effective.config()),
    );
    sink.emit(crate::pipeline::event_for(path, &outcome));
    match outcome {
        FileOutcome::Resolved(file) => Some(file),
        _ => None,
    }
}

/// Write the events `borax bib` produces for `groups` into `sink`.
///
/// `groups` is what [`preflight`] made of the run's paths, as it is for
/// [`rename_events`]: each directory, its files, and the template tables
/// compiled for it. A file's verdict and its sidecar are adjacent, in the
/// order the group gives its files, and the merge into the master `.bib`
/// trails the group.
///
/// Unlike the bibliography output a rename run produces on the side,
/// this one is what was asked for, so it runs whether or not a
/// destination is configured — a run that writes nowhere still reports
/// what it resolved.
fn bib_events<C: Cache>(
    groups: &[Group],
    configs: &Configs,
    adapters: &Adapters<C>,
    sink: &mut dyn Sink,
) {
    for group in groups {
        let effective = configs.for_directory(&group.directory);
        let config = bib_config(effective.config());
        let mut cited = Citations::default();

        for path in &group.paths {
            if let Some(file) = resolved_record(path, effective, adapters, sink) {
                cited.add(
                    path.clone(),
                    file,
                    &group.citation_keys,
                    &config,
                    adapters.bib_files,
                    sink,
                );
            }
        }

        cited.merge(&config, adapters.bib_files, sink);
    }
}

/// Where bibliography output goes, from `config`.
fn bib_config(config: &Config) -> BibConfig {
    BibConfig {
        path: config.bib_path.clone(),
        duplicates: config.duplicates,
        sidecars: config.sidecars,
    }
}

/// Write the events `borax undo` produces into `sink`.
///
/// A run with no journal reverts nothing and reports nothing, which is
/// what an absent journal means: there is no move on record to undo.
fn undo_events<C: Cache>(adapters: &Adapters<C>, sink: &mut dyn Sink) {
    let Some(journal) = adapters.journal else {
        return;
    };

    for reversal in undo_last(journal, adapters.library, adapters.filesystem) {
        sink.emit(crate::journal::event_for(&reversal));
    }
}

/// The sink a real run writes through: each event rendered in the run's
/// format, written as its own line, and folded into the run's totals.
///
/// An event a format has nothing to say about — [`Event::RunStarted`]
/// in [`Format::Human`] — writes no line and is counted all the same,
/// so what the two formats report at the end agrees however much they
/// differ in between.
///
/// A write that fails is dropped rather than reported: the stream is
/// where a run says things, and a run whose stream has gone has nowhere
/// left to say that it went.
struct Rendering<'a> {
    format: Format,
    out: &'a mut dyn Write,
    counts: Counts,
}

impl Sink for Rendering<'_> {
    fn emit(&mut self, event: Event) {
        if let Some(line) = render(self.format, &event) {
            let _ = writeln!(self.out, "{line}");
        }
        self.counts.observe(&event);
    }
}

/// Carry out `cli` against `adapters`, writing to `streams`.
///
/// The stream always opens with [`Event::RunStarted`] and closes with
/// [`Event::RunFinished`], whatever happened in between, so a consumer
/// can tell a run that produced nothing from a run that was cut off.
/// Events a format has nothing to say about are simply not written.
///
/// Each event is written when it happens rather than when the run ends,
/// so a network-bound run is watchable while it is bound.
///
/// A [`Diagnostic`] from [`preflight`] goes to `streams.err` and ends
/// the run as [`Outcome::Fatal`] with `streams.out` untouched: nothing
/// was attempted, so there is no event stream to close — which is why
/// everything that can fail is settled before the first event rather
/// than as the run goes.
pub fn dispatch<C: Cache>(
    cli: &Cli,
    configs: &Configs,
    adapters: &Adapters<C>,
    streams: &mut Streams,
) -> Outcome {
    let prepared = match preflight(&cli.command, configs, adapters) {
        Ok(prepared) => prepared,
        Err(diagnostic) => {
            let _ = writeln!(streams.err, "{diagnostic}");
            return Outcome::Fatal;
        }
    };

    let mut sink = Rendering {
        format: cli.format(),
        out: streams.out,
        counts: Counts::default(),
    };
    sink.emit(Event::RunStarted {
        command: cli.command.name().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        applying: applying(&cli.command),
    });
    emit_events(&prepared, &cli.command, configs, adapters, &mut sink);

    // Read before the last event is emitted, so what `RunFinished`
    // reports is the body's totals and nothing else: the framing events
    // are about the run rather than about a file, and `Counts::observe`
    // leaves them out either way.
    let counts = sink.counts;
    sink.emit(Event::RunFinished { counts });

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
    // Expansion first: which directory a file belongs to is the question
    // configuration is resolved by, and that cannot be asked of an
    // argument that is still a directory standing for the files under it.
    let command = expanded(&cli.command);
    // The run's own configuration climbs from the arguments as the user
    // typed them, not from the files they expanded to: `borax rename
    // <dir>` is a run in `<dir>`, whatever depth its files sit at.
    let working = start_directory_for(&cli.command);
    let configs =
        match configs_from_environment(command.paths(), &working, flag_layers(&cli.settings)) {
            Ok(configs) => configs,
            Err(failure) => {
                let _ = writeln!(streams.err, "{}", error(failure.to_string()));
                return Outcome::Fatal;
            }
        };
    let effective = configs.run();

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
            command,
            settings: cli.settings.clone(),
            json: cli.json,
        },
        &configs,
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
