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

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Identifier, supplied};
use borax_core::ledger::Index;
use borax_core::record::{EntryType, Record};
use borax_core::tables::{LookupTables, Lookups, Table, TableSpec};
use borax_core::template::{Miss, Template, TemplateTable};
use borax_core::time::utc_basic;
use borax_pdf::tiered::ExtractionConfig;
use borax_sources::arxiv::ArxivClient;
use borax_sources::cache::{Cache, Cached, MemoryCache};
use borax_sources::crossref::CrossrefClient;
use borax_sources::dispatch::Unresolved;
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
    layer_from_toml, nearest_override, resolve, table_path,
};
use crate::describe::{self, Candidate, Position, Proposal, describe};
use crate::event::{
    Counts, Diagnostic, Event, Format, Level, Overridden, SkipReason, TableUsed, human_summary,
    render,
};
use crate::ledger::{Collection, FileLedger, Ledger, admission_entry, collection_relative};
use crate::pipeline::{
    Documents, FileOutcome, FileRecord, Provenance, RealDocuments, ResolveConfig, Standing,
    resolve_batch, resolve_file, resolve_file_checking_ledger, resolved_event,
};
use crate::renaming::{
    Applying, Filesystem, Namespace, PlannedRename, Planning, Proposed, RealFilesystem,
};
use crate::session::{
    Answer, Asker, Mode, Outcome, Question, Session, TerminalAsker, TextPrompt, outcome_for,
    stdin_is_terminal, terminal_width,
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
                documents(path, &|_| true, &mut reached);
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
/// `descend` decides which subdirectories the walk enters; one it
/// refuses contributes nothing, itself or below it. A walk that enters
/// everything passes `&|_| true`.
///
/// A directory that cannot be read adds nothing, so one unreadable
/// subtree costs its own files and no others.
///
/// Metadata is read without following symlinks, so a link is neither a
/// file nor a directory here: it cannot send the walk round a loop, and
/// naming it directly is still how a user says they meant it.
pub(crate) fn documents(
    directory: &Path,
    descend: &dyn Fn(&Path) -> bool,
    found: &mut Vec<PathBuf>,
) {
    let Ok(listing) = fs::read_dir(directory) else {
        return;
    };

    for entry in listing.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let path = entry.path();
        if metadata.is_dir() {
            if descend(&path) {
                documents(&path, descend, found);
            }
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
    pub documents: &'a dyn Documents,
    pub sources: &'a [&'a dyn Source],
    pub index: &'a ContentIndex<C>,
    pub filesystem: &'a dyn Filesystem,
    pub bib_files: &'a dyn BibFiles,
    /// The response cache's directory, or `None` when the system names
    /// no cache directory.
    pub cache_root: Option<PathBuf>,
    /// The collection's record of what it has admitted — what the
    /// duplicate checks are asked of, and where an applied admission is
    /// appended — or `None` when the run is outside any collection and
    /// there is none to keep.
    ///
    /// Whether it is touched at all is still the `ledger` setting's to
    /// say; this only carries the one the run would touch.
    pub ledger: Option<&'a dyn Ledger>,
    /// The directory the run's `.borax/` accounting is anchored at, or
    /// `None` when the run is outside any collection.
    ///
    /// A ledger entry's path is relative to it, so it is what turns an
    /// entry into a path on this machine and a renamed file into an
    /// entry.
    pub collection_root: Option<PathBuf>,
    /// Where an applying run's log goes when there is no collection
    /// root to keep it in, or `None` when the system names no state
    /// directory either.
    pub state_root: Option<PathBuf>,
    /// What a run records as the time it happened.
    ///
    /// Asked once for the whole batch, never once per file: its value
    /// timestamps every entry the run admits to the ledger and, as a
    /// [`RunId`](borax_core::ledger::RunId), is what makes them one
    /// run. The files a batch admitted are one act of accounting rather
    /// than a series of them, so entries that entered together have to
    /// be identifiable together afterwards.
    ///
    /// Naming the run's log is a second reading, taken before the batch
    /// begins. Nothing joins a log to a run by its name — the log's
    /// contents carry the run — so the two readings need not agree to
    /// the second.
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
/// `declared` is the run's loaded tables, and a template looking up a
/// name it does not hold is `{prefix}.{key}: unknown table "<name>"`.
/// The check belongs here rather than in `Template::compile` because
/// which tables exist is the configuration's business and not the
/// template grammar's, and it happens here rather than at render time
/// because a `lookup` that could never resolve would otherwise render
/// empty on every file without ever saying why.
///
/// A key naming no entry type, a template that will not compile, and a
/// template looking up an undeclared table are all [`Diagnostic`]s
/// rather than skips: a template is configuration, so a broken one is
/// wrong for every file in the batch and there is nothing to be gained
/// by finding that out once per file.
pub fn templates(
    map: &BTreeMap<String, String>,
    prefix: &str,
    declared: &LookupTables,
) -> Result<TemplateTable, Diagnostic> {
    let compile = |key: &str, source: &str| {
        let template = Template::compile(source)
            .map_err(|failure| error(format!("{prefix}.{key}: {failure}")))?;
        match template
            .tables()
            .into_iter()
            .find(|name| !declared.contains(name))
        {
            Some(name) => Err(error(format!("{prefix}.{key}: unknown table {name:?}"))),
            None => Ok(template),
        }
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

/// A warning [`Diagnostic`] carrying `message`.
fn warning(message: String) -> Diagnostic {
    Diagnostic {
        level: Level::Warning,
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

    /// Take `event` as the record of a move that is about to be made,
    /// and fail rather than lose it.
    ///
    /// [`Sink::emit`] is best-effort: a run whose stream has gone has
    /// nowhere left to say so, and losing the line about a skip costs
    /// nothing that cannot be worked out again. The record of a move is
    /// the exception — the name a file used to carry is nowhere else —
    /// so a sink that keeps one says here whether it kept this one, and
    /// the caller does not move the file when it did not.
    ///
    /// A sink that keeps no record has nothing to fail at and nothing
    /// to write: the default takes `event` and does nothing with it,
    /// since [`Sink::emit`] reports the move once it has been made.
    fn record(&mut self, event: Event) -> Result<(), Diagnostic> {
        let _ = event;
        Ok(())
    }

    /// Hold back what a person is shown about the file now being
    /// decided, until [`Sink::release`] or [`Sink::withhold`] says what
    /// became of it.
    ///
    /// Only what is shown: the event stream is complete whatever is
    /// held, so a sink that keeps the events rather than rendering them
    /// has nothing to hold and the default does nothing. Holding twice
    /// without ending the first hold is not meaningful and is not
    /// supported.
    fn hold(&mut self) {}

    /// Show what the hold kept back, in the order it was emitted.
    fn release(&mut self) {}

    /// Drop what the hold kept back: the file it was about is passed
    /// over, and the run's summary is what says how many were.
    fn withhold(&mut self) {}
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
/// A run spanning two trees is a run under two configurations, so every
/// table here is the one that directory's own configuration resolved
/// to.
///
/// Which template tables a group carries follows from what the command
/// renders, which [`Renders`] names: a group built for a command that
/// renders no filename holds `None` in [`Group::filenames`].
pub struct Group {
    /// The directory the group's configuration was resolved for.
    pub directory: PathBuf,
    /// The files in it, in the order the run reached them.
    pub paths: Vec<PathBuf>,
    /// The filename templates, which name a renamed file, and
    /// `None` for a command that renders no filename and so never
    /// compiled them.
    pub filenames: Option<TemplateTable>,
    /// The citation-key templates, which name a cited record.
    pub citation_keys: TemplateTable,
    /// The external tables this directory's templates may look up.
    pub tables: LookupTables,
    /// Those same tables as the run log names them.
    pub used: Vec<TableUsed>,
}

/// What a run's fallible checks produced.
///
/// Every way a run can end before it starts — a template that will not
/// compile, `cache` on a system that names no cache directory — is
/// settled by [`preflight`], which runs
/// before the first event is written. What it hands back is what those
/// checks yielded, which is why [`emit_events`] has no failure left to
/// report and a [`Diagnostic`] can still mean that nothing was emitted.
pub enum Prepared {
    /// A command with nothing that could fail: `config` and
    /// `resolve`.
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
    /// Holding the compiled tables is what makes "every template the
    /// command renders from compiles before any file is touched" a
    /// property of the type rather than an ordering inside a function:
    /// emitting cannot reach a file without already holding the tables
    /// for its directory.
    Grouped {
        groups: Vec<Group>,
        /// What the collection has admitted already, keyed for the
        /// duplicate checks. Empty whenever detection is off — the
        /// setting says so, there is no collection, or the ledger could
        /// not be read — so the checks run against it either way and
        /// simply miss.
        ledger: Index,
        /// What the run has to say, before it starts, about files that
        /// are not its inputs: that there was no ledger to read or that
        /// it could not be trusted, and every row a lookup table
        /// dropped. Empty when there is nothing to report.
        ///
        /// A list rather than one slot because a table may drop several
        /// rows and a run may read several tables, and a row silently
        /// absent is the mistake external tables exist to prevent.
        warnings: Vec<Diagnostic>,
    },
    /// `ledger rebuild`, with the report the regenerated ledger
    /// produced.
    ///
    /// The ledger is already written by the time this exists, for
    /// [`Prepared::Cache`]'s reason: the subcommand's whole work is the
    /// one event, and that work — scanning the collection and writing
    /// what it found — is the part that can fail.
    Rebuilt { report: Event },
}

/// Settle everything about `command` that could end the run, before any
/// of it is reported.
///
/// Returns a [`Diagnostic`] for the failures that are about the run
/// rather than about a file, all of which are the same shape —
/// something the whole invocation needs is missing, so there is no
/// per-file verdict to report:
///
/// - a template the command renders from that will not compile, or
///   one looking up a table no configuration declares, which is wrong
///   for every file in the batch — `rename` renders both a filename
///   and a citation key, so it compiles both tables, while `bib`
///   renders only a citation key and compiles only that one, which is
///   why a filename template it would never read cannot refuse it;
/// - a declared table that cannot be read or will not load, whichever
///   template tables the command compiles, since loading a table is
///   what validates the `lookup` tokens in the ones it does compile and
///   a table that will not load is a fault in the configuration;
/// - `cache` with no cache directory, or with one that cannot be read,
///   because reporting an empty cache would answer a question that was
///   never asked;
/// - `ledger rebuild` outside any collection, or over a ledger that
///   cannot be written, since a rebuild reported but not written would
///   leave the user believing the accounting was put right.
///
/// An applying rename with nowhere to record itself is not among them:
/// what an apply run has to be able to write is its run log, and
/// [`dispatch`] settles that — before the first event, and equally
/// before anything moves.
///
/// For a command that works on files, nothing here reads one, queries a
/// source, or moves anything, so a run refused at this point costs no
/// network and leaves no trace. `borax cache` and `borax ledger
/// rebuild` are the exceptions, and the last two failures above are
/// why: each one's whole work is a single event, and the work is the
/// part that can fail, so it belongs where there is still a way to
/// refuse the run.
///
/// A `rename` also reads the ledger here — once, before the first file,
/// since every file in the batch is checked against the same entries.
/// Nothing about it can refuse a run: whatever reading it had to say
/// comes back as a [`Diagnostic`] alongside the groups, for the caller
/// to write out, and the run proceeds with duplicate detection off.
pub fn preflight<C: Cache>(
    command: &Command,
    configs: &Configs,
    adapters: &Adapters<C>,
) -> Result<Prepared, Diagnostic> {
    match command {
        Command::Config { .. } | Command::Resolve { .. } => Ok(Prepared::Unchecked),
        // The root and the ledger are discovered together, so a run
        // holding one holds the other; either being absent is the one
        // situation of being outside a collection.
        Command::Ledger { .. } => match (adapters.collection_root.as_deref(), adapters.ledger) {
            (Some(root), Some(ledger)) => Ok(Prepared::Rebuilt {
                report: rebuilt_ledger(root, ledger, (adapters.now)())?,
            }),
            _ => Err(error(
                "this directory is in no collection, so there is no ledger to rebuild".to_string(),
            )),
        },
        Command::Cache { clear, .. } => match adapters.cache_root.as_deref() {
            Some(root) => Ok(Prepared::Cache {
                report: cache_report(*clear, root)?,
            }),
            None => Err(error("this system names no cache directory".to_string())),
        },
        Command::Rename { paths, .. } => {
            let (groups, mut warnings) =
                compiled_groups(paths, configs, Renders::FilenamesAndCitationKeys)?;
            let prepared = crate::ledger::prepare(configs.run().config().ledger, adapters.ledger);
            // After the tables', because a run that cannot trust its
            // ledger should hear that before it hears about a row.
            warnings.extend(prepared.diagnostic);
            Ok(Prepared::Grouped {
                groups,
                ledger: prepared.index,
                warnings,
            })
        }
        Command::Bib { paths, .. } => {
            let (groups, warnings) = compiled_groups(paths, configs, Renders::CitationKeysOnly)?;
            Ok(Prepared::Grouped {
                groups,
                // A bibliography run admits nothing, so it neither
                // consults the ledger nor has cause to complain about
                // not finding one.
                ledger: Index::build(&[]),
                warnings,
            })
        }
    }
}

/// Which template tables a command renders from, and so which ones
/// [`compiled_groups`] compiles for it.
///
/// A command compiles what it renders and no more, so a template it
/// would never have rendered cannot refuse its run.
#[derive(Clone, Copy)]
enum Renders {
    /// `rename`, which names each file it moves and cites the records
    /// it admits: both tables.
    FilenamesAndCitationKeys,
    /// `bib`, which writes bibliography entries and no filename: the
    /// citation-key table alone.
    CitationKeysOnly,
}

/// `paths` grouped by directory ([`by_directory`]), each group paired
/// with the template tables `renders` calls for, compiled from its
/// directory's configuration, and the lookup tables that configuration
/// declares.
///
/// Every group's lookup tables are loaded and every template table
/// `renders` names compiled before any group is worked, so a table that
/// will not load and a template that will not compile — in any of the
/// directories the run spans — each end the run before a single file is
/// read. A template table `renders` leaves out is not compiled and so
/// cannot end anything: [`Renders::CitationKeysOnly`] carries no
/// filename templates, and a `templates.default` that will not compile
/// is a fault only for the command that would have rendered it.
///
/// Lookup tables are loaded whichever tables are compiled. Loading one
/// is what validates the `lookup` tokens in the templates that are
/// compiled, and a table that will not load is a fault in the
/// configuration rather than in a template.
fn compiled_groups(
    paths: &[PathBuf],
    configs: &Configs,
    renders: Renders,
) -> Result<(Vec<Group>, Vec<Diagnostic>), Diagnostic> {
    let mut warnings = Vec::new();
    let groups = by_directory(paths)
        .into_iter()
        .map(|(directory, paths)| {
            let effective = configs.for_directory(&directory);
            // Before the templates, because compiling one is what
            // checks its `lookup` tokens against the declared names.
            let (tables, used, dropped) = loaded_tables(effective)?;
            warnings.extend(dropped);
            let config = effective.config();
            Ok(Group {
                filenames: match renders {
                    Renders::FilenamesAndCitationKeys => {
                        Some(templates(&config.templates, "templates", &tables)?)
                    }
                    Renders::CitationKeysOnly => None,
                },
                citation_keys: templates(&config.citation_keys, "citation-keys", &tables)?,
                tables,
                used,
                directory,
                paths,
            })
        })
        .collect::<Result<Vec<Group>, Diagnostic>>()?;

    Ok((groups, warnings))
}

/// The tables `effective` declares, read and loaded, with the record of
/// them [`Event::RunStarted`] carries.
///
/// Every failure is a [`Diagnostic`] naming the table under
/// `tables.<name>`: a file that cannot be read, one that is not UTF-8
/// text, and everything [`borax_core::tables::TableError`] covers — a
/// header without a declared column, a key two rows claim with
/// different values. All of them are properties of the declaration or
/// of the file, so they are the same for every input file and are
/// reported before any of them is touched.
///
/// A relative `path` is resolved with [`table_path`] against the
/// configuration file that declared it, which [`Effective::origin`]
/// names; a declaration from anywhere else — which no configuration
/// file syntax produces, only a test building a layer by hand — is read
/// as given.
///
/// Every row [`Table::load`] drops — for want of a key cell or a value
/// cell, or because its key folded to nothing — comes back as a warning
/// naming the table, the file and the line. Dropping a row is not
/// fatal, but a row silently absent from an abbreviation table is a
/// wrong name nobody can account for, so it is said out loud.
fn loaded_tables(
    effective: &Effective,
) -> Result<(LookupTables, Vec<TableUsed>, Vec<Diagnostic>), Diagnostic> {
    let mut tables = LookupTables::new();
    let mut used = Vec::new();
    let mut warnings = Vec::new();

    for (name, declaration) in &effective.config().tables {
        let path = match effective.origin(&format!("tables.{name}.path")) {
            Some(Origin::GlobalFile(file) | Origin::DirectoryFile(file)) => {
                table_path(&declaration.path, file)
            }
            _ => declaration.path.clone(),
        };
        let bytes = fs::read(&path).map_err(|failure| {
            error(format!(
                "tables.{name}: \"{}\" could not be read: {failure}",
                path.display()
            ))
        })?;
        let text = std::str::from_utf8(&bytes).map_err(|failure| {
            error(format!(
                "tables.{name}: \"{}\" is not UTF-8 text: {failure}",
                path.display()
            ))
        })?;
        let spec = TableSpec {
            key_columns: declaration.key.columns(),
            value_column: declaration.value.clone(),
            values: declaration.values.kind(),
        };
        // The path is named as well as the table: a declaration is one
        // line and the file it points at may be anywhere, so "which
        // file has the bad line?" should not need a second lookup.
        let (table, dropped) = Table::load(text, &spec).map_err(|failure| {
            error(format!("tables.{name}: \"{}\": {failure}", path.display()))
        })?;
        warnings.extend(dropped.into_iter().map(|row| {
            warning(format!(
                "tables.{name}: \"{}\" line {}: {}",
                path.display(),
                row.line,
                row.message
            ))
        }));

        used.push(TableUsed {
            name: name.clone(),
            path,
            digest: hash_bytes(&bytes).as_str().to_string(),
        });
        tables.insert(name.clone(), table);
    }

    Ok((tables, used, warnings))
}

/// The tables every group of `prepared` read, by name and without
/// repeats, in name order.
///
/// A run spanning two trees is a run under two configurations and so
/// possibly two sets of tables; what it reports is their union, since
/// the question the run log has to answer is which files were consulted
/// at all. A name two groups declare differently is reported as the
/// first group read it, there being one slot for it and no way to say
/// that a run used two files under one name.
///
/// Empty for every command that groups nothing, which is every command
/// that reads no table.
fn tables_used(prepared: &Prepared) -> Vec<TableUsed> {
    let Prepared::Grouped { groups, .. } = prepared else {
        return Vec::new();
    };

    let mut by_name: BTreeMap<&str, &TableUsed> = BTreeMap::new();
    for used in groups.iter().flat_map(|group| &group.used) {
        by_name.entry(&used.name).or_insert(used);
    }
    by_name.into_values().cloned().collect()
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
///
/// Returns the [`Aftermath`]: what the run has to say for itself once
/// its events are written, which only a `rename` has anything to fill
/// in — the files it never reached, and what it found that is about the
/// run rather than about any one file.
pub fn emit_events<C: Cache>(
    prepared: &Prepared,
    command: &Command,
    configs: &Configs,
    adapters: &Adapters<'_, C>,
    session: &mut Session<'_>,
    sink: &mut dyn Sink,
) -> Aftermath {
    match (command, prepared) {
        (Command::Config { .. }, _) => {
            for event in configs.run().events() {
                sink.emit(event);
            }
            Aftermath::default()
        }
        (Command::Cache { .. }, Prepared::Cache { report })
        | (Command::Ledger { .. }, Prepared::Rebuilt { report }) => {
            sink.emit(report.clone());
            Aftermath::default()
        }
        (Command::Resolve { paths, .. }, _) => {
            resolve_events(paths, configs, adapters, sink);
            Aftermath::default()
        }
        (Command::Rename { apply, .. }, Prepared::Grouped { groups, ledger, .. }) => {
            rename_events(groups, *apply, session, ledger, configs, adapters, sink)
        }
        (Command::Bib { .. }, Prepared::Grouped { groups, .. }) => {
            bib_events(groups, configs, adapters, sink);
            Aftermath::default()
        }
        // A `Prepared` that does not go with the command could only
        // come of pairing one command's preflight with another's
        // emission, which no caller does: `events_for` and `dispatch`
        // both preflight the command they go on to emit. There is
        // nothing such a pair could report, so it reports nothing.
        _ => Aftermath::default(),
    }
}

/// What a run has to say for itself once its events are written.
///
/// Both fields are about the run rather than about any one file, and
/// neither is known until the last file has been reached — which is why
/// they come back from the emission rather than travelling as events.
#[derive(Debug, Default)]
pub struct Aftermath {
    /// Input files the run left without a fate: the file an interactive
    /// run was quit at, or the one a move that could not be recorded
    /// stopped it at, and every file after it. Zero for a run that
    /// reached the end of its inputs.
    pub unreached: usize,
    /// What the run discovered that is about the run rather than about
    /// a file: a ledger holding entries for files that have gone, or the
    /// record failure that ended the run.
    pub diagnostic: Option<Diagnostic>,
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
    session: &mut Session<'_>,
) -> Result<Vec<Event>, Diagnostic> {
    let prepared = preflight(command, configs, adapters)?;
    let mut events: Vec<Event> = Vec::new();
    // The stream is the whole of what this hands back, so what the run
    // has to say about itself has nowhere to go here; [`dispatch`] is
    // what writes it out.
    let _ = emit_events(&prepared, command, configs, adapters, session, &mut events);
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

/// Regenerate the ledger of the collection at `root` and report what it
/// now holds.
///
/// The collection is scanned ([`crate::ledger::scan_collection`]) and
/// what the scan found becomes the whole ledger: a file the scan does
/// not count keeps no entry, which is what compacts away the records of
/// files that have been deleted or moved. Every entry is stamped with
/// `at`, as both its timestamp and its run identifier, since one
/// rebuild is one admission of everything it found.
///
/// The run's `ledger` setting has no say here. It governs whether the
/// pipeline consults and adds to the ledger; `ledger rebuild` is an
/// instruction to do ledger work, and refusing it because the pipeline
/// was told to leave the ledger alone would refuse the very command
/// that puts a disabled ledger back in order.
///
/// A ledger that will not take the entries is a [`Diagnostic`] rather
/// than a clean report: a rebuild announced but not written would leave
/// the user believing the accounting had been put right.
fn rebuilt_ledger(root: &Path, ledger: &dyn Ledger, at: String) -> Result<Event, Diagnostic> {
    let entries = crate::ledger::rebuild(
        &crate::ledger::scan_collection(root),
        borax_core::ledger::RunId::new(&at),
        &at,
        env!("CARGO_PKG_VERSION"),
    );

    ledger
        .replace(&entries)
        .map_err(|failure| error(format!("\"{}\": {failure}", root.display())))?;

    Ok(Event::LedgerRebuilt {
        root: root.to_path_buf(),
        entries: entries.len(),
    })
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
        adapters.documents,
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
/// configuration compiled to. `admitted` is what the collection has
/// admitted already, which every file in the batch is checked against
/// and which an applied move adds to.
///
/// A group is worked under its own directory's configuration — its
/// templates, its collision policy, its bibliography destination — since
/// a run spanning two trees is a run under two configurations. The
/// ledger is the exception: it belongs to the collection the whole run
/// sits in, so whether it is consulted at all is the run's own setting
/// and not any one directory's.
///
/// Within a group a file is finished before the next is opened: its
/// verdict, the move planned or made for it, and any sidecar written
/// beside it are one contiguous run of events, in the order the group
/// gives its files. The merge into the master `.bib` is the exception
/// and trails the group, being work about the whole of it rather than
/// about any one file.
///
/// The lookups that found no row trail the whole run, one event per
/// distinct table and input: a miss is about the table rather than
/// about the file that happened to reveal it, and however many files
/// met it there is one line to add.
///
/// An interactive run puts each file it could not settle on its own to
/// its operator ([`asked`]) and holds the file's verdict until their
/// answers settle it, so a file re-identified in the session is
/// reported once, for the record it was settled on. Nothing else about
/// it differs: the file it renames is moved, cited and admitted
/// exactly as an applying batch run moves, cites and admits it, and a
/// file it asks nothing about is reported exactly as a batch run
/// reports it. A run ended by a quit leaves the file it was ended at
/// and every file after it untouched, says nothing about it, counts it
/// and every file after it in [`Aftermath::unreached`], and still
/// merges what the files it visited produced.
///
/// Returns a warning when any entry the run matched turned out to
/// name a file that is no longer there. It comes back at the end
/// rather than as an event because it is one fact about the ledger and
/// not about the file that happened to reveal it — however many files
/// find stale entries, the run says so once. A run stopped by a move it
/// could not record reports that instead, being the more urgent of the
/// two and the reason the rest of the run did not happen.
fn rename_events<C: Cache>(
    groups: &[Group],
    apply: bool,
    session: &mut Session<'_>,
    admitted: &Index,
    configs: &Configs,
    adapters: &Adapters<C>,
    sink: &mut dyn Sink,
) -> Aftermath {
    let at = (adapters.now)();
    // Whether the ledger is in play at all: the setting says so, and
    // the run is in a collection that has one.
    let ledger = match configs.run().config().ledger {
        true => adapters.ledger,
        false => None,
    };
    let root = adapters.collection_root.clone().unwrap_or_default();
    // `exists` is asked about a ledger entry that matched and about
    // nothing else, so an answer of "not there" is exactly an entry
    // that outlived its file. A file matching no entry never reaches
    // the question, which is what keeps a plain miss from reading as
    // staleness.
    let stale = Cell::new(false);
    let exists = |path: &Path| {
        let present = is_present(adapters.filesystem, path);
        stale.set(stale.get() || !present);
        present
    };
    let collection = Collection {
        ledger: admitted,
        root: &root,
        exists: &exists,
    };
    // A run with no ledger is resolved with no collection to check
    // against rather than against an empty one, which is what keeps it
    // from hashing every file a second time for a check that could only
    // miss.
    let checked = ledger.is_some().then_some(&collection);

    // Across groups, because a table is named once for the run however
    // many directories consult one under that name.
    let mut missed = Missed::default();
    // Likewise across groups: the names a base holds are the run's,
    // not any one directory's.
    let mut namespaces: BTreeMap<PathBuf, Namespace> = BTreeMap::new();
    // What ended the run early, if anything did: a quit, or a move that
    // could not be recorded. Either way the files after it are left
    // untouched and counted.
    let mut stopped: Option<Stopped> = None;

    for (index, group) in groups.iter().enumerate() {
        let effective = configs.for_directory(&group.directory);
        let config = bib_config(effective.config());
        // A run with neither a master file nor sidecars has nowhere to
        // write, and citing anyway would report records too sparse to
        // cite as skipped in a run that was never going to cite them.
        let cites = config.path.is_some() || config.sidecars;
        // A group's own tables, since a run spanning two trees is a run
        // under two configurations.
        let mut lookups = Lookups::new(&group.tables);

        // Only `Rename` reaches here, and that is the command
        // `preflight` compiles the filename templates for.
        let filenames = group
            .filenames
            .as_ref()
            .expect("a group built for a rename carries filename templates");
        // What a rendered subdirectory files from, so a collection run
        // over a filed directory proposes nothing. A run outside any
        // collection has no root and files from the file's own
        // directory. The namespace outlives the group: two directories
        // filing into one collection claim names in the same place.
        let base = Planning::base_for(&group.directory, adapters.collection_root.as_deref());
        let namespace = namespaces
            .entry(Planning::key_for(&base))
            .or_insert_with(|| Namespace::new(&base));
        let mut planning = Planning::new(
            &group.directory,
            namespace,
            filenames,
            effective.config().collision,
            adapters.filesystem,
        );
        // A yes is the authorisation for one move, so an interactive
        // run carries its accepted decisions out as an applying run
        // does; `--apply` is what authorises a batch one.
        let mut applying = Applying::new(
            adapters.filesystem,
            apply || session.mode == Mode::Interactive,
        );
        let mut cited = Citations::default();
        // Whether a file this directory's configuration reports as
        // already named is passed over without a line of its own. Only
        // an interactive run does: a batch run's output is the complete
        // per-file account a person reads after the fact.
        let passing_over = session.mode == Mode::Interactive && effective.config().skip_named;

        // Left as a block so that what a group owes whatever happens —
        // its merge into the master `.bib` and the misses its templates
        // recorded — is written once, on the way out of a group the run
        // finished and a group it was stopped in alike.
        let ended = 'files: {
            for (position, path) in group.paths.iter().enumerate() {
                // The hold covers this file alone and ends the moment
                // its fate is settled, which is when the first thing is
                // said about it: an interactive run holds every file's
                // verdict until then (design D5), so a file passed over
                // is one nothing was ever written about.
                if passing_over {
                    sink.hold();
                }
                // Resolved without reporting. What is reported is what
                // the file's fate made of the verdict, which for a
                // batch run is settled the moment the verdict is and
                // for an interactive one is settled by its operator.
                let standing = crate::pipeline::standing(
                    path,
                    adapters.documents,
                    adapters.sources,
                    adapters.index,
                    &resolving(effective.config()),
                    checked,
                );
                let about = About {
                    path,
                    position: among(groups, (index, position)),
                    passing_over,
                };
                let settled = match session.mode {
                    Mode::Batch => alone(standing, &about, &mut planning, &mut lookups),
                    Mode::Interactive => asked(
                        session,
                        standing,
                        &about,
                        &mut planning,
                        &mut lookups,
                        adapters,
                        checked,
                    ),
                };

                let (file, event) = match settled {
                    // This file and every file after it are left as they
                    // are, and nothing is said about any of them: a file
                    // the run was ended at was given no fate, so it has
                    // no verdict to report either.
                    Settled::Stop => {
                        // The hold is still open, and nothing drains it
                        // after this: without ending it here the run's
                        // own summary — and any bibliography line after
                        // it — would be buffered and never shown. The
                        // hold is empty, since a file's verdict is only
                        // emitted once it has a fate.
                        sink.release();
                        break 'files Some(Stopped {
                            at: (index, position),
                            diagnostic: None,
                        });
                    }
                    Settled::Skip { file, reason } => {
                        sink.release();
                        if let Some(file) = &file {
                            sink.emit(crate::pipeline::resolved_event(path, file));
                        }
                        let event = Event::Skipped {
                            path: path.clone(),
                            reason,
                        };
                        sink.emit(event.clone());
                        (file, event)
                    }
                    Settled::CarryOut {
                        file,
                        decision,
                        remember,
                    } => {
                        // A file already named is passed over whole, its
                        // own outcome line included, so the hold
                        // outlasts the event that reports it.
                        let named = matches!(decision, PlannedRename::AlreadyNamed { .. });
                        if !named {
                            sink.release();
                        }
                        sink.emit(crate::pipeline::resolved_event(path, &file));
                        planning.accept(&decision);
                        // The hash goes to the move rather than to a log
                        // beside it: it travels on the `Renamed` event,
                        // so the run's log records what each file was
                        // when it moved.
                        let event = match moved(&mut applying, &decision, file.hash.clone(), sink) {
                            Ok(event) => event,
                            // The move was not made and no later one
                            // will be: a run that cannot record what it
                            // moves stops rather than moving unrecorded.
                            Err(diagnostic) => {
                                break 'files Some(Stopped {
                                    at: (index, position),
                                    diagnostic: Some(diagnostic),
                                });
                            }
                        };
                        if named {
                            sink.withhold();
                        }
                        // On the move and never before: an operator who
                        // supplies an identifier and then walks away
                        // from the record has not accepted it, and a
                        // move that did not happen is not an
                        // acceptance either (design D7). Best-effort,
                        // like every write to the response cache: an
                        // answer that could not be kept means the file
                        // is asked about again.
                        if remember && matches!(event, Event::Renamed { .. }) {
                            crate::pipeline::remember(
                                adapters.index,
                                file.hash.as_ref(),
                                &file.record,
                            );
                        }
                        (Some(file), event)
                    }
                };

                // Nothing further is owed for a file no record stands
                // for: there is nothing to admit and nothing to cite.
                let Some(file) = file else {
                    continue;
                };

                // A sidecar goes beside the name the file now carries
                // rather than beside the one it has just lost, so where
                // it lands is where the move that just happened put it.
                let current = match &event {
                    Event::Renamed { target, .. } => {
                        // Only a move that happened is an admission: a
                        // preview reports the same target and admits
                        // nothing, which is why this reads the event
                        // rather than `apply`.
                        admit(ledger, &root, &file, target, &at);
                        target.clone()
                    }
                    _ => path.clone(),
                };

                if cites {
                    cited.add(
                        current,
                        file,
                        &Citing {
                            citation_keys: &group.citation_keys,
                            config: &config,
                            files: adapters.bib_files,
                        },
                        sink,
                        &mut lookups,
                    );
                }
            }
            None
        };

        // The files visited before a stop produced entries as correct as
        // a finished run's, so they are merged rather than dropped.
        if cites {
            cited.merge(&config, adapters.bib_files, sink);
        }

        missed.absorb(lookups.take());

        if let Some(end) = ended {
            stopped = Some(end);
            break;
        }
    }

    missed.report(sink);

    Aftermath {
        unreached: stopped
            .as_ref()
            .map_or(0, |stopped| unreached(groups, stopped.at)),
        // A run that could not record a move says so; otherwise the only
        // thing left to report is a ledger naming files that have gone.
        diagnostic: stopped
            .and_then(|stopped| stopped.diagnostic)
            .or_else(|| stale.get().then(crate::ledger::stale_entries_warning)),
    }
}

/// Where a run stopped before its inputs ran out, and why.
struct Stopped {
    /// The group and the position within it of the file the run stopped
    /// at. That file is the first of the unreached ones: it was given no
    /// fate.
    at: (usize, usize),
    /// What to tell the operator, for a stop that was not their own
    /// choice. A quit has nothing to report.
    diagnostic: Option<Diagnostic>,
}

/// How many of `groups`' files are left without a fate by a run that
/// stopped at `at` — that file and every file after it.
fn unreached(groups: &[Group], at: (usize, usize)) -> usize {
    let (group, position) = at;
    groups
        .iter()
        .skip(group)
        .map(|group| group.paths.len())
        .sum::<usize>()
        - position
}

/// What one file's resolution and its operator's answers came to.
///
/// The verdict and the fate travel together because an interactive run
/// reports neither until both are known: a file the operator
/// re-identified is reported once, for the record they settled on.
enum Settled {
    /// Claim the name `decision` names and carry it out — which for a
    /// decision that moves nothing is to report it — from the record
    /// `file`, which is reported as the file's resolution.
    CarryOut {
        file: FileRecord,
        decision: PlannedRename,
        /// Whether the record is written to the content index once the
        /// move is made.
        ///
        /// A record the run resolved on its own was written as it
        /// resolved; one the operator reached — by supplying an
        /// identifier, by accepting a conflict, by asking the services
        /// again — is written when they accept it, and their
        /// acceptance is the rename.
        remember: bool,
    },
    /// Leave the file as it is and report `reason`, with the record
    /// that still stands for it where one does: a file that resolved
    /// and was not moved has one to be cited from, and a file nothing
    /// identified has none.
    Skip {
        file: Option<FileRecord>,
        reason: SkipReason,
    },
    /// End the run at this file, saying nothing about it.
    Stop,
}

/// The file a decision is about: where it lies, where it sits among
/// the run's files, and whether one already carrying its record's name
/// is passed over rather than asked about.
struct About<'a> {
    path: &'a Path,
    position: Position,
    /// `rename.skip-named` for the directory this file lies in, in a
    /// run that asks. A passed-over file is shown nothing and asked
    /// nothing, and is reported exactly as a batch run reports it.
    passing_over: bool,
}

/// What a run with nobody to ask makes of `standing`: its own verdict,
/// and the planner's decision about the record it reached.
fn alone(
    standing: Standing,
    about: &About<'_>,
    planning: &mut Planning<'_>,
    lookups: &mut Lookups<'_>,
) -> Settled {
    match standing.verdict {
        FileOutcome::Skipped(reason) => Settled::Skip { file: None, reason },
        FileOutcome::Resolved(file) => Settled::CarryOut {
            decision: planning.proposed(about.path, &file, lookups).decision,
            file,
            // A batch run's resolution wrote itself to the content
            // index as it resolved; there is nothing here it has not
            // already kept.
            remember: false,
        },
    }
}

/// A record on offer for a file, and what stands against it.
#[derive(Clone)]
struct Offer {
    file: FileRecord,
    /// The disagreement between the record's title and the file's own,
    /// where there is one. Shown, and never by itself a refusal: an
    /// identifier a person stands behind is a stronger statement than
    /// the heuristic that would refuse it.
    conflict: Option<SkipReason>,
    /// Whether the content index already holds this record for the
    /// file, which decides whether accepting it writes one
    /// ([`Settled::CarryOut::remember`]).
    kept: bool,
}

/// What a file's situation calls for.
enum Situation {
    /// Put these choices, the first being the default an unadorned
    /// Enter takes.
    Ask(Vec<Answer>),
    /// Carry the planner's decision out, as a batch run would: no
    /// answer would change it.
    Settle,
    /// Report the file's own verdict, as a batch run would.
    Report,
}

/// What `session`'s operator makes of the file `standing` was reached
/// for.
///
/// The one place an interactive run differs from a batch one. Every
/// file whose situation an answer could change is put to the operator
/// a move to make, a conflict to accept or refuse, a file
/// nothing identified, one no service holds a record for, one that
/// could not be read, and — where `rename.skip-named` is off — one
/// already carrying its record's name. Everything else is settled as
/// [`alone`] settles it, since no answer would change what happens.
///
/// Answering `supply` opens a loop rather than ending the question:
/// the identifier is resolved, the record it reaches is described, and
/// the file's situation becomes whatever that record puts it in. A
/// candidate that leads nowhere leaves the file exactly as it was,
/// with the record and the choices it already had, and what became of
/// it is reported in the description of the question put again — the
/// only channel left, since a candidate reaches no event stream.
///
/// A run whose mode says to ask and that has nowhere to ask stops
/// rather than carrying on, since a move nobody was asked about is the
/// one thing it may not make.
fn asked<C: Cache>(
    session: &mut Session<'_>,
    standing: Standing,
    about: &About<'_>,
    planning: &mut Planning<'_>,
    lookups: &mut Lookups<'_>,
    adapters: &Adapters<'_, C>,
    collection: Option<&Collection<'_>>,
) -> Settled {
    let Standing {
        verdict,
        hash,
        found,
        mut unresolved,
        refused,
    } = standing;
    // The verdict the run is holding for the file, which is what its
    // question is described from and what a skip reports.
    let mut held = crate::pipeline::event_for(about.path, &verdict);
    // The record the file's own passes put on offer: the one they
    // resolved, or the one the conflict check refused and an operator
    // may yet accept.
    let mut own = match (verdict, refused) {
        (FileOutcome::Resolved(file), _) => Some(Offer {
            file,
            conflict: None,
            // Written to the content index as it resolved, or served
            // from it.
            kept: true,
        }),
        (FileOutcome::Skipped(conflict), Some(file)) => Some(Offer {
            file,
            conflict: Some(conflict),
            kept: false,
        }),
        (FileOutcome::Skipped(_), None) => None,
    };
    // What is on offer now: the file's own record, or one a supplied
    // identifier reached in its place.
    let mut offer = own.clone();
    let mut candidate = false;
    // What became of the last candidate, prepended to the description
    // of the question put again and reported nowhere else.
    let mut report: Vec<String> = Vec::new();

    let width = session.width;
    let name = shown(about.path, &session.working);
    let Some(asker) = session.asker.as_deref_mut() else {
        return Settled::Stop;
    };

    loop {
        // Proposing claims nothing (`add-interactive-rename` D3), so a
        // file goes round this loop as often as its operator likes and
        // leaves the plan exactly as it found it.
        let proposal = offer
            .as_ref()
            .map(|on| planning.proposed(about.path, &on.file, lookups));
        let decision = proposal.as_ref().map(|proposed| &proposed.decision);

        // A candidate there is no moving to is no offer at all: what
        // became of it is reported, and the file's own situation — the
        // one its own record left it in — is put again (design D1's
        // second table).
        if candidate && !matches!(decision, Some(PlannedRename::Rename { .. })) {
            report = elsewhere(offer.as_ref(), decision, width);
            offer = own.clone();
            candidate = false;
            continue;
        }

        let choices = match situation(
            offer.as_ref(),
            decision,
            &held,
            unresolved.as_ref(),
            hash.is_some(),
            about.passing_over,
        ) {
            Situation::Ask(choices) => choices,
            Situation::Settle => {
                return match (offer, proposal) {
                    (Some(on), Some(proposed)) => Settled::CarryOut {
                        file: on.file,
                        decision: proposed.decision,
                        // Nothing reaches here with a move on offer, so
                        // there is no acceptance to keep: an operator
                        // who was asked nothing has accepted nothing.
                        remember: false,
                    },
                    _ => skipped(own, &held),
                };
            }
            Situation::Report => return skipped(own, &held),
        };

        let answer = asker.choose(&Question {
            path: about.path.to_path_buf(),
            // Where the file would go, as the operator is shown it. A
            // situation with no move on offer names the file itself:
            // there is no other name in play.
            target: match decision {
                Some(PlannedRename::Rename { target, .. }) => target.clone(),
                _ => about.path.to_path_buf(),
            },
            choices,
            // What the answer rests on, rendered by the run: the
            // driver holds the verdict and the move being proposed,
            // and the asker only draws what it is given.
            description: report
                .iter()
                .cloned()
                .chain(describe(
                    &described(about.path, offer.as_ref(), candidate, &held),
                    &name,
                    proposed_move(proposal.as_ref()).as_ref(),
                    about.position,
                    width,
                ))
                .collect(),
        });

        match answer {
            Answer::Quit => return Settled::Stop,
            Answer::Skip => return skipped(own, &held),
            // The three answers that act on the record in hand, none
            // of which is offered without a decision to carry out.
            Answer::Rename | Answer::Override | Answer::Keep => {
                return match (offer, proposal) {
                    (Some(on), Some(proposed)) => {
                        // A record the run resolved on its own was put
                        // to the ledger as it resolved. One the
                        // operator reached — supplied, retried, or
                        // accepted over a conflict — never was, and
                        // admitting a second copy of a work the
                        // collection already holds is the one thing the
                        // ledger exists to prevent. Its verdict is
                        // about the collection, not about where the
                        // identifier came from.
                        let second_copy = (!on.kept)
                            .then(|| {
                                collection.and_then(|collection| {
                                    crate::pipeline::work_duplicate(
                                        about.path,
                                        &on.file.record,
                                        collection,
                                    )
                                })
                            })
                            .flatten();
                        match second_copy {
                            Some(reason) => Settled::Skip { file: None, reason },
                            None => Settled::CarryOut {
                                file: accepted(&on),
                                decision: proposed.decision,
                                remember: !on.kept,
                            },
                        }
                    }
                    _ => skipped(own, &held),
                };
            }
            Answer::Supply => {
                let Some(identifier) = supplied_identifier(asker) else {
                    // Abandoned: nothing happened, so nothing is
                    // reported and the question is put again exactly
                    // as it was.
                    continue;
                };
                match crate::pipeline::resolve_supplied(
                    about.path,
                    &identifier,
                    adapters.documents,
                    adapters.sources,
                ) {
                    Ok(supplied) => {
                        offer = Some(Offer {
                            file: supplied.file,
                            conflict: supplied.conflict,
                            kept: false,
                        });
                        candidate = true;
                        // The record itself is what the next question
                        // describes; there is nothing left to report.
                        report.clear();
                    }
                    Err(unheld) => {
                        report = describe::reported(
                            "supplied",
                            &identifier.to_string(),
                            &Candidate::Unheld {
                                attempts: &crate::pipeline::attempts_of(&unheld),
                            },
                            width,
                        );
                        offer = own.clone();
                        candidate = false;
                    }
                }
            }
            Answer::Retry => {
                // Offered only where the services failed to answer, so
                // there is an identifier of the file's own to ask them
                // about again.
                let Some((identifier, tier)) = found.clone() else {
                    continue;
                };
                match crate::pipeline::resolve_supplied(
                    about.path,
                    &identifier,
                    adapters.documents,
                    adapters.sources,
                ) {
                    Ok(supplied) => {
                        let file = FileRecord {
                            // The identifier was the file's own;
                            // asking a second time does not make it
                            // the operator's.
                            tier: Some(Provenance::Extracted(tier)),
                            ..supplied.file
                        };
                        held = match &supplied.conflict {
                            Some(conflict) => Event::Skipped {
                                path: about.path.to_path_buf(),
                                reason: conflict.clone(),
                            },
                            None => resolved_event(about.path, &file),
                        };
                        own = Some(Offer {
                            file,
                            conflict: supplied.conflict,
                            kept: false,
                        });
                        offer = own.clone();
                        candidate = false;
                        report.clear();
                    }
                    Err(unheld) => {
                        report = describe::reported(
                            "tried again",
                            &identifier.to_string(),
                            &Candidate::Unheld {
                                attempts: &crate::pipeline::attempts_of(&unheld),
                            },
                            width,
                        );
                        // A retry is the file's own resolution rather
                        // than a candidate, so what it came to is the
                        // verdict the run now holds for it.
                        held = Event::Skipped {
                            path: about.path.to_path_buf(),
                            reason: crate::pipeline::unresolvable(&unheld, &identifier, tier),
                        };
                        unresolved = Some(unheld);
                        own = None;
                        offer = None;
                        candidate = false;
                    }
                }
            }
        }
    }
}

/// The menu `offer` and `decision` call for, or the outcome to settle
/// for where no answer would change anything.
///
/// The first choice is the default an unadorned Enter takes, so it is
/// never the answer that moves a file against a doubt: rename leads
/// only where the record is the file's own and nothing stands against
/// it, and a conflict defaults to skipping however plainly the move is
/// on offer.
///
/// `supply` is whether an identifier may usefully be supplied for this
/// file at all. A file whose content hash is unknown cannot be renamed
/// in an applying run ([`SkipReason::Unrecordable`]), so supplying one
/// could only lead to a move that then fails: it is left off the menu,
/// and a file for which it would have been the only useful answer is
/// not asked at all.
fn situation(
    offer: Option<&Offer>,
    decision: Option<&PlannedRename>,
    held: &Event,
    unresolved: Option<&Unresolved>,
    supply: bool,
    passing_over: bool,
) -> Situation {
    let Some(offer) = offer else {
        // Nothing was identified. The questions that offer to put that
        // right, and no question for a verdict an identifier would not
        // change: a duplicate is the ledger's word about the
        // collection, not about what the file is.
        if !supply {
            return Situation::Report;
        }
        let Event::Skipped { reason, .. } = held else {
            return Situation::Report;
        };
        return match reason {
            SkipReason::NoIdentifier | SkipReason::Unreadable { .. } => {
                Situation::Ask(vec![Answer::Supply, Answer::Skip, Answer::Quit])
            }
            // An outage is no evidence about the file, so the two are
            // not presented alike: where the services merely failed to
            // answer, asking them again is what failed and it leads.
            SkipReason::Unresolvable { .. } => {
                match unresolved.is_none_or(Unresolved::is_conclusive) {
                    true => Situation::Ask(vec![Answer::Supply, Answer::Skip, Answer::Quit]),
                    false => Situation::Ask(vec![
                        Answer::Retry,
                        Answer::Supply,
                        Answer::Skip,
                        Answer::Quit,
                    ]),
                }
            }
            _ => Situation::Report,
        };
    };

    match (&offer.conflict, decision) {
        // A conflict on a file nothing can be supplied for has only
        // the answer a batch run gives.
        (Some(_), _) if !supply => Situation::Report,
        // Skip leads: Enter must never be the override.
        (Some(_), Some(PlannedRename::Rename { .. })) => Situation::Ask(vec![
            Answer::Skip,
            Answer::Override,
            Answer::Supply,
            Answer::Quit,
        ]),
        // Nothing to override where there is no move to make.
        (Some(_), _) => Situation::Ask(vec![Answer::Skip, Answer::Supply, Answer::Quit]),
        // A file whose content hash is unknown is not asked about at
        // all: an applying run refuses to move what it cannot record,
        // so every answer but Skip would end in `Unrecordable`, and a
        // question whose default answer cannot succeed is worse than
        // the report a batch run gives.
        (None, Some(PlannedRename::Rename { .. })) if !supply => Situation::Report,
        (None, Some(PlannedRename::Rename { .. })) => Situation::Ask(vec![
            Answer::Rename,
            Answer::Supply,
            Answer::Skip,
            Answer::Quit,
        ]),
        // A file already named is asked about only where the run is
        // not passing such files over: keeping the name is the default,
        // and a name that came from the wrong record can be put right.
        (None, Some(PlannedRename::AlreadyNamed { .. })) if !passing_over && supply => {
            Situation::Ask(vec![Answer::Keep, Answer::Supply, Answer::Quit])
        }
        // A file passed over, a target taken, a record too sparse to
        // name a file: no answer would change any of them.
        (None, _) => Situation::Settle,
    }
}

/// What a skip reports.
///
/// The reason borax had, where it had one: an operator's skip leaves
/// the file exactly as a batch run would have left it, and for exactly
/// that reason. A file that resolved had no reason of its own, and
/// what was declined is the move that was on offer — so it is
/// `declined`, and its record still stands and is still cited.
///
/// A candidate the operator supplied is nowhere in either answer. It
/// was never the file's resolution and is not reported as one.
fn skipped(own: Option<Offer>, held: &Event) -> Settled {
    match held {
        Event::Skipped { reason, .. } => Settled::Skip {
            file: None,
            reason: reason.clone(),
        },
        _ => Settled::Skip {
            file: own.map(|on| on.file),
            reason: SkipReason::Declined,
        },
    }
}

/// `offer`'s record as accepting it makes it: the conflict it was
/// accepted over recorded on the record, in the vocabulary the skip
/// would have used, and nothing recorded where the record
/// cleared the check on its own.
fn accepted(offer: &Offer) -> FileRecord {
    FileRecord {
        overrode: match &offer.conflict {
            Some(SkipReason::Conflict {
                field,
                extracted,
                resolved,
                similarity,
            }) => Some(Overridden {
                field: field.clone(),
                extracted: extracted.clone(),
                resolved: resolved.clone(),
                similarity: *similarity,
            }),
            _ => None,
        },
        ..offer.file.clone()
    }
}

/// The event a question's description renders.
///
/// The verdict the run is holding, which is the whole of what it knows
/// about the file — or, while a candidate is on offer, the `resolved`
/// event that candidate would produce were the operator to accept it,
/// conflict and all. A candidate has no verdict of its own, and what
/// the operator is shown before deciding is what the stream carries
/// after.
fn described(path: &Path, offer: Option<&Offer>, candidate: bool, held: &Event) -> Event {
    match offer {
        Some(offer) if candidate => resolved_event(path, &accepted(offer)),
        _ => held.clone(),
    }
}

/// The move `proposal` puts on offer, as a description shows it, and
/// `None` where there is no move to make: a question about a file
/// staying where it is shows no new name.
fn proposed_move(proposal: Option<&Proposed>) -> Option<Proposal> {
    match proposal {
        Some(Proposed {
            decision: PlannedRename::Rename { path, target },
            rendered,
        }) => Some(Proposal {
            target: beside(target, path),
            rendered: rendered.as_deref().map(|rendered| beside(rendered, path)),
        }),
        _ => None,
    }
}

/// What became of a candidate whose record leads somewhere other than
/// a move, as the question put again reports it.
fn elsewhere(offer: Option<&Offer>, decision: Option<&PlannedRename>, width: usize) -> Vec<String> {
    let identifier = offer
        .and_then(|offer| offer.file.found.as_ref())
        .map(Identifier::to_string)
        .unwrap_or_default();
    // Rendered before the outcome that names it, so that the name
    // outlives the value borrowing it. Named relative to the file's
    // own directory, as every other name a question shows is.
    let taken = match decision {
        Some(PlannedRename::TargetTaken { path, target }) => beside(target, path),
        _ => String::new(),
    };
    let outcome = match decision {
        Some(PlannedRename::TargetTaken { .. }) => Candidate::NameTaken { target: &taken },
        Some(PlannedRename::Unnameable { .. }) => Candidate::Unnameable,
        _ => Candidate::AlreadyNamed,
    };
    describe::reported("supplied", &identifier, &outcome, width)
}

/// The identifier the operator types when asked for one, parsed.
///
/// `None` is their declining to answer — an escape, or an empty line —
/// which leaves the file exactly as the question found it.
///
/// Input that names no identifier is refused where it was typed and
/// asked for again, without the choice menu being reopened: somebody
/// who asked to supply an identifier and mistyped it has not changed
/// their mind. The refusal quotes what was typed and names the forms
/// that are taken, and nothing reaches any service until something
/// parses.
fn supplied_identifier(asker: &mut dyn Asker) -> Option<Identifier> {
    let mut refused: Option<String> = None;
    loop {
        let input = asker.text(&TextPrompt {
            asking: ASKING.to_string(),
            refused: refused.take(),
        })?;
        if let Some(identifier) = supplied(&input) {
            return Some(identifier);
        }
        refused = Some(format!(
            "\"{}\" is not a DOI, an arXiv identifier, or a pmid: or isbn: number.",
            crate::describe::escaped(input.trim())
        ));
    }
}

/// The one line the prompt for an identifier asks with.
///
/// It names the forms that are taken rather than asking for "an
/// identifier": the prefixes are required of a PMID and an ISBN
/// because a bare run of digits is both, and somebody who is not told
/// that types the digits.
const ASKING: &str = "Identifier — a DOI, an arXiv id, or pmid:/isbn: and the number";

/// Where the file at `at` — the group, and the file within it — sits
/// among all of `groups`' files, counted from one.
///
/// Counted across the whole run rather than within the group: the
/// operator answering questions is working through one list of files,
/// and the directories it was assembled from are not what they are
/// counting down.
fn among(groups: &[Group], at: (usize, usize)) -> Position {
    let (group, position) = at;
    Position {
        of_this: groups
            .iter()
            .take(group)
            .map(|group| group.paths.len())
            .sum::<usize>()
            + position
            + 1,
        total: groups.iter().map(|group| group.paths.len()).sum(),
    }
}

/// `path` as a question names it, from a run started in `working`:
/// relative to that directory where it lies under it, and whole where
/// it does not.
///
/// A file name alone would be shorter and is what a run over one
/// directory shows either way, but it stops identifying a file as soon
/// as a run spans two: `tree-a/paper.pdf` and `tree-b/paper.pdf` are
/// different moves, and a question that named both `paper.pdf` would
/// take one answer for the other.
fn shown(path: &Path, working: &Path) -> String {
    // Normalised on both sides before the comparison: a run given `.`
    // hands its files paths like `./a.pdf`, which do not start with an
    // absolute working directory as text however plainly they lie
    // under it, and the question would name a file `./a.pdf`.
    match (crate::paths::lexical(path), crate::paths::lexical(working)) {
        (Some(absolute), Some(working)) => absolute
            .strip_prefix(&working)
            .unwrap_or(&absolute)
            .display()
            .to_string(),
        _ => path.display().to_string(),
    }
}

/// `target` as a description names it: relative to the directory the
/// file at `path` sits in, so a name the template files elsewhere keeps
/// the subdirectory that says where it goes.
fn beside(target: &Path, path: &Path) -> String {
    // A route rather than a prefix stripped: filing from the collection
    // root moves a file between sibling directories, and
    // `../Science/paper.pdf` says where it is going where the whole
    // path only says where it ends up.
    match path.parent() {
        Some(parent) => crate::paths::route(target, parent).display().to_string(),
        None => target.display().to_string(),
    }
}

/// Carry `decision` out through `applying`, recording a move in `sink`
/// before it is made, and report what happened.
///
/// The record comes first so that a run interrupted at any point has
/// already recorded every move it may have made: a log naming a move
/// that did not happen sends its reader to look, while a move nothing
/// names is a rename the collection cannot account for. A record that
/// cannot be written is a [`Diagnostic`] and the move is not made.
///
/// Only a move takes that path. Every other decision — a preview's
/// [`Event::Planned`], a skip, a file already named — goes through
/// [`Sink::emit`], because losing the record of something that did not
/// move costs nothing that cannot be worked out again.
///
/// A move the filesystem then refuses is reported after the record of
/// the intent, so the stream and the log carry both.
fn moved(
    applying: &mut Applying<'_>,
    decision: &PlannedRename,
    hash: Option<ContentHash>,
    sink: &mut dyn Sink,
) -> Result<Event, Diagnostic> {
    let Some(intended) = applying.intended(decision, hash.as_ref()) else {
        let event = applying.carry_out(decision, hash);
        sink.emit(event.clone());
        return Ok(event);
    };

    sink.record(intended)?;
    let event = applying.carry_out(decision, hash);
    sink.emit(event.clone());
    Ok(event)
}

/// What a group cites under: the same three things for every file in
/// it, so they travel together rather than as three parameters a caller
/// could pair with the wrong group.
struct Citing<'a> {
    /// The citation-key templates compiled for the group's directory.
    citation_keys: &'a TemplateTable,
    /// Where the group's bibliography output goes.
    config: &'a BibConfig,
    /// The filesystem the sidecars and the master file are written
    /// through.
    files: &'a dyn BibFiles,
}

/// The lookups a run found no row for, gathered across its groups.
///
/// A miss is reported once per distinct table and input however many
/// files produced it: what it tells the user is which line to add to
/// their table, and that is one line whether one document wanted it or
/// a hundred. Sorted rather than kept in the order the run met them, so
/// the same batch reports the same lines whatever order its files came
/// in.
#[derive(Default)]
struct Missed(BTreeSet<Miss>);

impl Missed {
    /// Take in everything one group's rendering recorded.
    fn absorb(&mut self, misses: Vec<Miss>) {
        self.0.extend(misses);
    }

    /// Report one [`Event::LookupMissed`] per distinct miss into `sink`.
    ///
    /// Emitted after the per-file events and before the run's last, the
    /// misses being about the run rather than about any one of the
    /// files that met them.
    fn report(self, sink: &mut dyn Sink) {
        for miss in self.0 {
            sink.emit(Event::LookupMissed {
                table: miss.table,
                input: miss.input,
            });
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
    ///
    /// `citing` is what the group cites under, which is the same for
    /// every file in it, and `lookups` supplies the tables the key's
    /// template consults and collects the misses consulting them
    /// produced.
    fn add(
        &mut self,
        path: PathBuf,
        file: FileRecord,
        citing: &Citing<'_>,
        sink: &mut dyn Sink,
        lookups: &mut Lookups<'_>,
    ) {
        let (key, event) = write_sidecar(
            &path,
            &file,
            citing.citation_keys,
            citing.config,
            citing.files,
            lookups,
        );
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

/// Whether `filesystem` reports a file at `path`.
///
/// [`Filesystem::existing`] answers about a directory, so a path is
/// there when the directory holding it lists its file name; a directory
/// that is unreadable or absent lists nothing, and everything in it
/// reads as gone. A path naming no file in any directory — a bare root
/// — is not there.
fn is_present(filesystem: &dyn Filesystem, path: &Path) -> bool {
    let Some((directory, name)) = path.parent().zip(path.file_name()) else {
        return false;
    };
    filesystem
        .existing(directory)
        .contains_key(&*name.to_string_lossy())
}

/// Record in `ledger` that `file` was admitted to the collection at
/// `root` and now sits at `path`, as of `at`.
///
/// The entry's path is `path` relative to `root` and `/`-separated, so
/// the collection can be moved or opened on another machine and still
/// find what it recorded. `at` is both the entry's timestamp and the
/// run identifier that ties every entry of one run together.
///
/// Nothing is recorded when the run keeps no ledger, when the file
/// landed outside the collection its ledger accounts for, or when the
/// file's content hash is unknown ([`admission_entry`]). An append that
/// fails costs nothing beyond itself and is not reported: the ledger is
/// derived accounting, rebuildable from the collection, and a file that
/// was renamed correctly was renamed correctly whether or not the note
/// about it landed.
fn admit(ledger: Option<&dyn Ledger>, root: &Path, file: &FileRecord, path: &Path, at: &str) {
    let Some(ledger) = ledger else {
        return;
    };
    let Some(relative) = collection_relative(root, path) else {
        return;
    };
    let Some(entry) = admission_entry(
        file,
        &relative,
        borax_core::ledger::RunId::new(at),
        at,
        env!("CARGO_PKG_VERSION"),
    ) else {
        return;
    };

    let _ = ledger.append(&[entry]);
}

/// Resolve the file at `path` under `effective`, writing its verdict
/// into `sink`, and hand back the record when there is one.
///
/// Renaming and bibliography output both work from the record rather
/// than from the event, so the record is what this hands back; the event
/// is already written by the time it does.
///
/// `collection` is what the file is checked against for duplicates —
/// once on its hash before it is opened, and again on the identifiers it
/// resolved to. `None` is a run that admits nothing to any collection,
/// which is resolved without either check and so hashes the file once.
///
/// Each file is resolved and reported before the next is opened, so a
/// reader watching a network-bound run sees it make progress.
fn resolved_record<C: Cache>(
    path: &Path,
    effective: &Effective,
    adapters: &Adapters<C>,
    collection: Option<&Collection<'_>>,
    sink: &mut dyn Sink,
) -> Option<FileRecord> {
    let config = resolving(effective.config());
    let outcome = match collection {
        Some(collection) => resolve_file_checking_ledger(
            path,
            adapters.documents,
            adapters.sources,
            adapters.index,
            &config,
            collection,
        ),
        None => resolve_file(
            path,
            adapters.documents,
            adapters.sources,
            adapters.index,
            &config,
        ),
    };
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
///
/// Nothing here admits a file to the collection, so the ledger has no
/// say in it: every file is resolved with no collection to check
/// against, exactly as a run with no ledger is.
///
/// The lookups that found no row trail the whole run, as they do for
/// [`rename_events`] and for the same reason.
fn bib_events<C: Cache>(
    groups: &[Group],
    configs: &Configs,
    adapters: &Adapters<C>,
    sink: &mut dyn Sink,
) {
    // Across groups, because a table is named once for the run however
    // many directories consult one under that name.
    let mut missed = Missed::default();

    for group in groups {
        let effective = configs.for_directory(&group.directory);
        let config = bib_config(effective.config());
        let mut cited = Citations::default();
        // A group's own tables, since a run spanning two trees is a run
        // under two configurations.
        let mut lookups = Lookups::new(&group.tables);

        for path in &group.paths {
            if let Some(file) = resolved_record(path, effective, adapters, None, sink) {
                cited.add(
                    path.clone(),
                    file,
                    &Citing {
                        citation_keys: &group.citation_keys,
                        config: &config,
                        files: adapters.bib_files,
                    },
                    sink,
                    &mut lookups,
                );
            }
        }

        cited.merge(&config, adapters.bib_files, sink);
        missed.absorb(lookups.take());
    }

    missed.report(sink);
}

/// Where bibliography output goes, from `config`.
fn bib_config(config: &Config) -> BibConfig {
    BibConfig {
        path: config.bib_path.clone(),
        duplicates: config.duplicates,
        sidecars: config.sidecars,
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
///
/// A hold ([`Sink::hold`]) keeps a file's lines here until the run says
/// what became of the file, and is what lets an interactive run pass
/// over a file that needs no decision without having already printed
/// its resolution. Only the prose is held: the totals take every event
/// as it arrives, so a withheld file is counted as it would have been
/// shown, and `hidden` is what the summary says was passed over.
///
/// A [`Format::Json`] rendering never holds. It is the complete account
/// of the run, and a consumer cannot reconstruct one from a stream with
/// holes in it.
struct Rendering<'a> {
    format: Format,
    out: &'a mut dyn Write,
    counts: Counts,
    /// The lines of the file being decided, while its fate is unknown.
    held: Option<Vec<String>>,
    /// How many files' lines were dropped rather than written.
    hidden: usize,
}

impl Rendering<'_> {
    /// Write `line` to the stream, or hold it when a hold is on.
    fn put(&mut self, line: String) {
        match &mut self.held {
            Some(held) => held.push(line),
            None => {
                let _ = writeln!(self.out, "{line}");
            }
        }
    }
}

impl Sink for Rendering<'_> {
    fn emit(&mut self, event: Event) {
        // The summary is the one line the hold changes rather than
        // delays: a run that passed files over says so there.
        let line = match (&event, self.format) {
            (Event::RunFinished { counts }, Format::Human) => {
                Some(human_summary(counts, self.hidden))
            }
            _ => render(self.format, &event),
        };
        if let Some(line) = line {
            self.put(line);
        }
        self.counts.observe(&event);
    }

    fn hold(&mut self) {
        if self.format == Format::Human {
            self.held = Some(Vec::new());
        }
    }

    fn release(&mut self) {
        for line in self.held.take().unwrap_or_default() {
            let _ = writeln!(self.out, "{line}");
        }
    }

    fn withhold(&mut self) {
        if self.held.take().is_some() {
            self.hidden += 1;
        }
    }
}

/// The terminal's sink with the run's log behind it: every event goes
/// to the log as JSON before it goes wherever the terminal wants it.
///
/// The log is written in the versioned JSON Lines schema whatever
/// format the terminal is in, so the record of a run is the same file
/// whether the person watching it asked for prose or for JSON — and a
/// `--json` run's log is its stdout, byte for byte.
///
/// Wrapping the terminal's sink rather than sitting beside it is what
/// puts [`Event::RunStarted`] and [`Event::RunFinished`] in the log
/// too: the framing is emitted through the outermost sink, and a log
/// missing it would not be the stream it claims to be.
///
/// The file is unbuffered, so an event is on disk by the time the next
/// one is decided. A write that fails is dropped, for
/// [`Rendering`]'s reason turned around: a run that cannot record
/// itself still has a person to report to.
///
/// A move is the exception at both ends. Its event is written by
/// [`Sink::record`] before the move is made, so the log names it
/// whatever becomes of the run; a write that fails there is reported
/// rather than dropped, because the move must not happen without it.
/// The terminal is told nothing at that point: a move that has not been
/// attempted is not a move that happened, and the run reports it once
/// the filesystem has answered. `recorded` is what keeps the log from
/// carrying the event twice when it does.
struct Logging<'a> {
    terminal: Rendering<'a>,
    log: Option<fs::File>,
    /// The move written to the log and not yet reported, if any.
    recorded: Option<Event>,
}

impl Sink for Logging<'_> {
    fn emit(&mut self, event: Event) {
        let already_logged = self.recorded.take().is_some_and(|last| last == event);
        if let Some(log) = &mut self.log {
            if !already_logged {
                let _ = writeln!(log, "{}", crate::event::json_line(&event));
            }
        }
        self.terminal.emit(event);
    }

    /// Write `event` to the log and flush it, and report a write that
    /// failed rather than dropping it.
    ///
    /// The event does not reach the terminal here. It is the move a run
    /// is about to attempt, and a reader is told what happened to a
    /// file once it has happened; [`Sink::emit`] is what reports it, and
    /// writes it to the log only if this did not.
    ///
    /// A run with no log has nothing that can fail here and nothing to
    /// refuse: the mode that may move files is the mode whose log is
    /// mandatory, so a rename run reaching this without one has already
    /// been through the refusal.
    fn record(&mut self, event: Event) -> Result<(), Diagnostic> {
        if let Some(log) = &mut self.log {
            write_event(log, &event).map_err(|failure| {
                error(format!(
                    "the run log could not be written, so the move it records was not made: \
                     {failure}"
                ))
            })?;
        }
        self.recorded = Some(event);
        Ok(())
    }

    /// The hold is the terminal's alone: the log is the run's complete
    /// account and carries every event whatever a person is shown.
    fn hold(&mut self) {
        self.terminal.hold();
    }

    fn release(&mut self) {
        self.terminal.release();
    }

    fn withhold(&mut self) {
        self.terminal.withhold();
    }
}

/// What came of opening a run's log.
enum Opened {
    /// The file the run writes itself to, or `None` when it keeps no
    /// log — the setting says so, or there is nowhere a log of this run
    /// would belong.
    Log(Option<fs::File>),
    /// There was a log to write and it could not be opened.
    /// `mandatory` is [`crate::runlog::Destination`]'s: when it is set,
    /// the run is abandoned rather than run unrecorded.
    Failed {
        diagnostic: Diagnostic,
        mandatory: bool,
    },
}

/// A run-log failure, at the level its consequence deserves: an error
/// when the run cannot go ahead without the log, a warning when losing
/// the record is the whole of the damage.
fn log_failure(mandatory: bool, message: String) -> Opened {
    Opened::Failed {
        diagnostic: match mandatory {
            true => error(message),
            false => warning(message),
        },
        mandatory,
    }
}

/// Open the log `cli` calls for against `adapters`.
///
/// Where it goes and whether the run depends on it are
/// [`crate::runlog::destination`]'s to say. What is decided here is
/// only what to do about a destination that will not open, and the one
/// case the placement rules express as a refusal: an applying rename
/// with neither a collection nor a state directory has nowhere to
/// record itself, and a rename nothing recorded is a rename nothing can
/// account for afterwards.
fn open_log<C: Cache>(
    cli: &Cli,
    configs: &Configs,
    adapters: &Adapters<C>,
    mode: Mode,
    started: &Event,
) -> Opened {
    let applying = applying(&cli.command, mode);
    let destination = crate::runlog::destination(
        &cli.command,
        applying,
        configs.run().config().run_log,
        &(adapters.now)(),
        adapters.collection_root.as_deref(),
        adapters.state_root.as_deref(),
    );

    let Some(destination) = destination else {
        return match crate::runlog::mandatory(&cli.command, applying) {
            true => log_failure(
                true,
                "a run that may move files needs somewhere to record what it moves — --apply \
                 and a run that asks about each file both may — and this is neither in a \
                 collection nor on a system that names a state directory"
                    .to_string(),
            ),
            false => Opened::Log(None),
        };
    };

    let opened = crate::runlog::create(&destination)
        .and_then(|mut file| write_event(&mut file, started).map(|()| file));

    match opened {
        Ok(file) => Opened::Log(Some(file)),
        Err(failure) => log_failure(
            destination.mandatory,
            format!(
                "the run log \"{}\" could not be written: {failure}",
                destination.path.display()
            ),
        ),
    }
}

/// Write `event` to `log` as a line of JSON and flush it.
///
/// The one write of a run log that is allowed to fail loudly. Creating
/// a file says almost nothing about being able to fill it — a full
/// filesystem, a quota, a read-only mount noticed late — so the run's
/// first event is written while nothing has moved yet, and an applying
/// run that cannot get that far is refused rather than left moving
/// files into a record that will not take them.
fn write_event(log: &mut fs::File, event: &Event) -> io::Result<()> {
    writeln!(log, "{}", crate::event::json_line(event))?;
    log.flush()
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
///
/// The run's log is created before the first event, so an applying run
/// that cannot record itself is refused with `streams.out` untouched
/// and nothing moved. A log the run does not depend on failing to open
/// is a warning on `streams.err`, and the run goes on unrecorded.
///
/// What [`preflight`] has to say about the ledger goes to `streams.err`
/// too, and the run goes ahead: a ledger that could not be read costs
/// the run its duplicate detection and nothing else. What the run
/// itself discovers about the ledger — that it names files which are no
/// longer there — follows on `streams.err` once the stream has closed,
/// which is the first moment the whole batch has been checked.
pub fn dispatch<C: Cache>(
    cli: &Cli,
    configs: &Configs,
    adapters: &Adapters<C>,
    session: &mut Session<'_>,
    streams: &mut Streams,
) -> Outcome {
    let prepared = match preflight(&cli.command, configs, adapters) {
        Ok(prepared) => prepared,
        Err(diagnostic) => {
            let _ = writeln!(streams.err, "{diagnostic}");
            return Outcome::Fatal;
        }
    };

    // Before the stream opens, so what a `--json` consumer reads on
    // stdout is the run and nothing else.
    if let Prepared::Grouped { warnings, .. } = &prepared {
        for warning in warnings {
            let _ = writeln!(streams.err, "{warning}");
        }
    }

    let interactive = session.mode == Mode::Interactive;
    let started = Event::RunStarted {
        command: cli.command.name().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        // Settled by `session::mode` before the first event; a run that
        // asks is a run that may move files, so it reports both.
        applying: applying(&cli.command, session.mode),
        interactive,
        tables: tables_used(&prepared),
    };

    // Before the first event reaches the terminal, and so before any
    // file is touched: opening the log writes `started` into it, so an
    // applying run that cannot record itself is refused while there is
    // still nothing to regret and `streams.out` is still untouched.
    let log = match open_log(cli, configs, adapters, session.mode, &started) {
        Opened::Log(log) => log,
        Opened::Failed {
            diagnostic,
            mandatory,
        } => {
            let _ = writeln!(streams.err, "{diagnostic}");
            if mandatory {
                return Outcome::Fatal;
            }
            None
        }
    };

    let mut sink = Logging {
        terminal: Rendering {
            format: cli.format(),
            out: streams.out,
            counts: Counts::default(),
            held: None,
            hidden: 0,
        },
        log,
        recorded: None,
    };
    // Through the terminal alone: the log has `started` already, from
    // the write that proved it writable.
    sink.terminal.emit(started);
    let aftermath = emit_events(
        &prepared,
        &cli.command,
        configs,
        adapters,
        session,
        &mut sink,
    );

    // Read before the last event is emitted, so what `RunFinished`
    // reports is the body's totals and nothing else: the framing events
    // are about the run rather than about a file, and `Counts::observe`
    // leaves them out either way. The files the run never reached are
    // the one total no event carries, no event being emitted for a file
    // nothing happened to.
    let counts = Counts {
        unreached: aftermath.unreached,
        ..sink.terminal.counts
    };
    sink.emit(Event::RunFinished { counts });

    if let Some(diagnostic) = aftermath.diagnostic {
        let _ = writeln!(streams.err, "{diagnostic}");
    }

    outcome_for(&counts)
}

/// Whether a run of `command` in `mode` will change what is on disk
/// rather than only report on it.
///
/// An interactive rename may move files — a yes to a question is the
/// authorisation for the move it names — so it is an applying run
/// whether or not it turns out to move anything.
fn applying(command: &Command, mode: Mode) -> bool {
    match command {
        Command::Rename { apply, .. } => *apply || mode == Mode::Interactive,
        Command::Cache { clear, .. } => *clear,
        Command::Bib { .. } | Command::Ledger { .. } => true,
        Command::Resolve { .. } | Command::Config { .. } => false,
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
        match configs_from_environment(command.paths(), &working, flag_layers(&cli.settings())) {
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
    // The library the run sits in decides where its state goes, so it
    // is discovered from the same directory the configuration was,
    // under the `library-root` that configuration may have named.
    let collection_root = crate::config::library_root(
        &working,
        effective.config().library_root.as_deref(),
        |candidate| candidate.is_file(),
    );
    let ledger = collection_root
        .as_deref()
        .map(FileLedger::at_collection_root);

    let mut asker = TerminalAsker;
    let mut session = session_for(
        &command,
        cli.format(),
        effective.config(),
        // Where the run was started, which is not where its
        // configuration was found: `working` above climbs from the
        // first path the run was given, and a question naming a file
        // relative to one of its own inputs would name the files of
        // one input differently from the files of another.
        &std::env::current_dir().unwrap_or_default(),
        &mut asker,
    );

    dispatch(
        &Cli {
            command,
            json: cli.json,
        },
        &configs,
        &Adapters {
            documents: &RealDocuments,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            bib_files: &RealBibFiles,
            cache_root: default_cache_root(),
            now: timestamp,
            ledger: ledger.as_ref().map(|ledger| ledger as &dyn Ledger),
            collection_root,
            state_root: crate::runlog::default_state_root(),
        },
        &mut session,
        streams,
    )
}

/// The session `command` runs in against the real terminal.
///
/// [`crate::session::mode`] settles the mode from this process's own
/// stdin, the format asked for, the `batch` setting and `--apply`; an
/// interactive one puts its questions through [`TerminalAsker`], which
/// is the only place borax reads a keystroke.
///
/// `working` is the run's own directory — the one its configuration
/// climbs from — and is what a question names its file relative to, so
/// a run over one directory asks about bare file names while one
/// spanning two tells two files of the same name apart.
///
/// `asker` is the caller's because a session borrows it: the run needs
/// it for as long as it may ask, and only the caller has a frame it can
/// live in.
fn session_for<'a>(
    command: &Command,
    format: Format,
    config: &Config,
    working: &Path,
    asker: &'a mut TerminalAsker,
) -> Session<'a> {
    let interactive = matches!(command, Command::Rename { .. })
        && crate::session::mode(
            stdin_is_terminal(),
            format,
            config.batch,
            matches!(command, Command::Rename { apply: true, .. }),
        ) == Mode::Interactive;

    match interactive {
        // The width is read here rather than deeper in, because this is
        // the one frame that is allowed to ask the terminal anything.
        true => Session::interactive(asker)
            .at_width(terminal_width())
            .started_in(working.to_path_buf()),
        false => Session::batch(),
    }
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
        Command::Resolve {
            paths,
            resolution,
            concurrency,
            run_log,
        } => Command::Resolve {
            paths: inputs(paths),
            resolution: resolution.clone(),
            concurrency: *concurrency,
            run_log: run_log.clone(),
        },
        Command::Rename {
            paths,
            apply,
            resolution,
            rename,
            bibliography,
            accounting,
            run_log,
        } => Command::Rename {
            paths: inputs(paths),
            apply: *apply,
            resolution: resolution.clone(),
            rename: rename.clone(),
            bibliography: bibliography.clone(),
            accounting: accounting.clone(),
            run_log: run_log.clone(),
        },
        Command::Bib {
            paths,
            resolution,
            bibliography,
            run_log,
        } => Command::Bib {
            paths: inputs(paths),
            resolution: resolution.clone(),
            bibliography: bibliography.clone(),
            run_log: run_log.clone(),
        },
        Command::Config { .. } | Command::Cache { .. } | Command::Ledger { .. } => command.clone(),
    }
}

/// The time a real run stamps its records with: the current UTC
/// instant in ISO 8601 basic form ([`borax_core::time::utc_basic`]).
///
/// Legible in a ledger entry, sortable as a string, and legal as part
/// of a filename on every platform — which it has to be, since a run
/// log is named after it.
///
/// Two runs of the same binary within one second read the same, so what
/// [`Adapters::now`] promises of the value — that one run's records
/// tell themselves from another's — holds only down to the second. A
/// clock reading before the epoch reports the epoch rather than ending
/// the run.
fn timestamp() -> String {
    utc_basic(match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_millis(),
        Err(_) => 0,
    })
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
