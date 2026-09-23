//! What the user typed.
//!
//! The command line is the highest configuration layer, and this module
//! is where it becomes one: [`Cli::settings`] assembles a [`Settings`]
//! from the invocation, and [`flag_layers`] turns that into layers
//! [`crate::config::resolve`] merges under everything else.
//!
//! A setting flag follows the subcommand that reads it, as in `borax
//! rename --mailto …`. A subcommand's arguments are the settings that
//! subcommand consumes, so its `--help` describes the command in front
//! of the reader rather than the tool as a whole, and naming a setting a
//! command cannot act on is a usage error rather than a silent no-op.
//!
//! `--json` is the exception, declared once on [`Cli`] and accepted
//! anywhere: it chooses the rendering of the event stream, which every
//! subcommand honours and none of them interprets differently.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};

use crate::config::{BibLayer, ExtractionLayer, Layer, NetworkLayer, Origin, RenameLayer};
use crate::event::Format;

/// The `borax` command line.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "borax",
    version,
    about = "Fetch bibliographic metadata for PDFs, then rename and cite them",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Write one JSON object per line instead of prose.
    #[arg(long, global = true)]
    pub json: bool,
}

/// What a subcommand needs to turn a file into a record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct ResolutionOptions {
    /// Which services may be asked, comma-separated.
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub sources: Option<Vec<String>>,

    /// The contact address sent to the polite pools.
    #[arg(long, value_name = "EMAIL")]
    pub mailto: Option<String>,

    /// How many pages of a PDF the text pass reads.
    #[arg(long, value_name = "N")]
    pub page_limit: Option<usize>,

    /// The floor on the gap between two requests to one service.
    #[arg(long, value_name = "MS")]
    pub min_interval_ms: Option<u64>,

    /// Read and write the on-disk response cache.
    #[arg(long)]
    pub cache: bool,

    /// Ask every service again and open every file again.
    #[arg(long, conflicts_with = "cache")]
    pub no_cache: bool,
}

/// What a subcommand needs to give a file its new name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct RenameOptions {
    /// What to do when two files want one name: suffix or skip.
    #[arg(long, value_name = "POLICY")]
    pub collision: Option<String>,

    /// Report the whole plan instead of asking about each file.
    #[arg(long)]
    pub batch: bool,

    /// Ask about each file, where there is a terminal to ask at.
    #[arg(long, conflicts_with = "batch")]
    pub no_batch: bool,

    /// Pass over a file that already carries its name, asking nothing
    /// and saying nothing about it.
    #[arg(long)]
    pub skip_named: bool,

    /// Report a file that already carries its name, as a batch run
    /// does.
    #[arg(long, conflicts_with = "skip_named")]
    pub no_skip_named: bool,
}

/// What a subcommand needs to write bibliography output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct BibliographyOptions {
    /// The master .bib file entries are merged into.
    #[arg(long, value_name = "PATH")]
    pub bib: Option<PathBuf>,

    /// What to do with an entry already in the master .bib: skip or
    /// update.
    #[arg(long, value_name = "POLICY")]
    pub duplicates: Option<String>,

    /// Write a sidecar beside each resolved file.
    #[arg(long)]
    pub sidecars: bool,

    /// Write no sidecars.
    #[arg(long, conflicts_with = "sidecars")]
    pub no_sidecars: bool,
}

/// What a subcommand needs to consult and add to the library's record
/// of what it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct RecordOptions {
    /// Check the library's records for duplicates and record what an
    /// applying run admits.
    #[arg(long)]
    pub record: bool,

    /// Neither read nor write the library's records.
    #[arg(long, conflicts_with = "record")]
    pub no_record: bool,
}

/// What a subcommand needs to read a document for what it claims about
/// itself.
///
/// The extraction settings alone, for a subcommand that opens files and
/// asks no service anything. [`ResolutionOptions`] carries the same
/// settings for the subcommands that go on to resolve what they
/// extracted, so a command declares one group or the other and never
/// both.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct ExtractionOptions {
    /// How many pages of a PDF the text pass reads.
    #[arg(long, value_name = "N")]
    pub page_limit: Option<usize>,
}

/// Whether a run keeps a log of its own event stream. Every subcommand
/// takes these, since every subcommand is a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct RunLogOptions {
    /// Write the run's event stream to a log in the collection.
    #[arg(long)]
    pub run_log: bool,

    /// Write no log for this run. An applying rename writes one
    /// anyway: that log is the only record of what it moved.
    #[arg(long, conflicts_with = "run_log")]
    pub no_run_log: bool,
}

/// Which subcommand to run, what it needs beyond the settings, and the
/// settings it reads.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Resolve each file to a record and report what was found.
    Resolve {
        /// The files and directories to work on.
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,

        #[command(flatten)]
        resolution: ResolutionOptions,

        /// How many files may be resolved at once.
        #[arg(long, value_name = "N")]
        concurrency: Option<usize>,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Rename each file after the record resolved for it.
    Rename {
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,

        /// Move the files the plan names, asking nothing. Selects a
        /// batch run; without it a batch run reports what it would
        /// move.
        #[arg(long, conflicts_with = "no_batch")]
        apply: bool,

        #[command(flatten)]
        resolution: ResolutionOptions,

        #[command(flatten)]
        rename: RenameOptions,

        #[command(flatten)]
        bibliography: BibliographyOptions,

        #[command(flatten)]
        record: RecordOptions,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Write bibliography output for each file, renaming nothing.
    Bib {
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,

        #[command(flatten)]
        resolution: ResolutionOptions,

        #[command(flatten)]
        bibliography: BibliographyOptions,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Print every setting, its value, and the layer it came from.
    Config {
        /// How many files may be resolved at once.
        #[arg(long, value_name = "N")]
        concurrency: Option<usize>,

        #[command(flatten)]
        resolution: ResolutionOptions,

        #[command(flatten)]
        rename: RenameOptions,

        #[command(flatten)]
        bibliography: BibliographyOptions,

        #[command(flatten)]
        record: RecordOptions,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Report what the response cache holds.
    Cache {
        /// Empty the cache instead of reporting it.
        #[arg(long)]
        clear: bool,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Report what a library holds.
    Status {
        /// The library to report on: the directory whose nearest
        /// `.borax.toml` establishes the root, or the directory itself
        /// when nothing above it is marked. The working directory by
        /// default.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Additionally report how many artifacts an identifier can be
        /// extracted from, which is a pass over the files.
        #[arg(long)]
        identify: bool,

        #[command(flatten)]
        extraction: ExtractionOptions,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Report a library's findings, repairing nothing.
    Validate {
        /// The library to validate, as `status` takes it.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Bring artifact records back into agreement with the library's
    /// tree.
    Reconcile {
        /// The library to reconcile, as `status` takes it.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Hash every artifact rather than trusting a recorded size and
        /// modification time that match the file.
        #[arg(long)]
        rehash: bool,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
    /// Record the artifacts the content index already knows, querying
    /// nothing and moving nothing.
    Adopt {
        /// The library to adopt into, as `status` takes it.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        #[command(flatten)]
        run_log: RunLogOptions,
    },
}

/// The settings an invocation overrides, one field per setting.
///
/// [`Cli::settings`] assembles one from the option groups the chosen
/// subcommand carries, leaving every setting that subcommand does not
/// offer absent. Absent means here what it means in a [`Layer`]: the
/// command line was silent about that setting and a lower layer shows
/// through. Every configurable boolean is offered as a two-flag pair
/// (`--sidecars` / `--no-sidecars`, `--cache` / `--no-cache`,
/// `--record` / `--no-record`, `--run-log` / `--no-run-log`), which is
/// how a boolean says all three things — on, off, and silent — with
/// flags that take no value, and what lets the command line override a
/// configured value in both directions. Naming both halves of a pair
/// is a parse error rather than a last-one-wins guess.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// Which services may be asked.
    pub sources: Option<Vec<String>>,

    /// The contact address sent to the polite pools.
    pub mailto: Option<String>,

    /// What to do when two files want one name: suffix or skip.
    pub collision: Option<String>,

    /// Report the whole plan instead of asking about each file.
    pub batch: bool,

    /// Ask about each file, where there is a terminal to ask at.
    pub no_batch: bool,

    /// Pass over a file that already carries its name.
    pub skip_named: bool,

    /// Report a file that already carries its name.
    pub no_skip_named: bool,

    /// The master .bib file entries are merged into.
    pub bib: Option<PathBuf>,

    /// What to do with an entry already in the master .bib: skip or
    /// update.
    pub duplicates: Option<String>,

    /// Write a sidecar beside each resolved file.
    pub sidecars: bool,

    /// Write no sidecars.
    pub no_sidecars: bool,

    /// How many pages of a PDF the text pass reads.
    pub page_limit: Option<usize>,

    /// How many files may be resolved at once.
    pub concurrency: Option<usize>,

    /// The floor on the gap between two requests to one service.
    pub min_interval_ms: Option<u64>,

    /// Read and write the on-disk response cache.
    pub cache: bool,

    /// Ask every service again and open every file again.
    pub no_cache: bool,

    /// Check the library's records for duplicates and record what an
    /// applying run admits.
    pub record: bool,

    /// Neither read nor write the library's records.
    pub no_record: bool,

    /// Write the run's event stream to a log in the collection.
    pub run_log: bool,

    /// Write no log for this run. An applying rename writes one
    /// anyway: that log is the only record of what it moved.
    pub no_run_log: bool,
}

impl ResolutionOptions {
    /// Copies this group's flags into the matching [`Settings`] fields.
    fn fill(&self, settings: &mut Settings) {
        settings.sources = self.sources.clone();
        settings.mailto = self.mailto.clone();
        settings.page_limit = self.page_limit;
        settings.min_interval_ms = self.min_interval_ms;
        settings.cache = self.cache;
        settings.no_cache = self.no_cache;
    }
}

impl RenameOptions {
    /// Copies this group's flags into the matching [`Settings`] fields.
    fn fill(&self, settings: &mut Settings) {
        settings.collision = self.collision.clone();
        settings.batch = self.batch;
        settings.no_batch = self.no_batch;
        settings.skip_named = self.skip_named;
        settings.no_skip_named = self.no_skip_named;
    }
}

impl BibliographyOptions {
    /// Copies this group's flags into the matching [`Settings`] fields.
    fn fill(&self, settings: &mut Settings) {
        settings.bib = self.bib.clone();
        settings.duplicates = self.duplicates.clone();
        settings.sidecars = self.sidecars;
        settings.no_sidecars = self.no_sidecars;
    }
}

impl RecordOptions {
    /// Copies this group's flags into the matching [`Settings`] fields.
    fn fill(&self, settings: &mut Settings) {
        settings.record = self.record;
        settings.no_record = self.no_record;
    }
}

impl ExtractionOptions {
    /// Copies this group's flags into the matching [`Settings`] fields.
    fn fill(&self, settings: &mut Settings) {
        settings.page_limit = self.page_limit;
    }
}

impl RunLogOptions {
    /// Copies this group's flags into the matching [`Settings`] fields.
    fn fill(&self, settings: &mut Settings) {
        settings.run_log = self.run_log;
        settings.no_run_log = self.no_run_log;
    }
}

impl Cli {
    /// Which rendering of the event stream this invocation asked for.
    pub fn format(&self) -> Format {
        match self.json {
            true => Format::Json,
            false => Format::Human,
        }
    }

    /// The settings this invocation overrode.
    ///
    /// Only the option groups the chosen subcommand carries contribute;
    /// every setting that subcommand does not offer comes back at its
    /// default, which is the absence a flag left unsaid produces.
    pub fn settings(&self) -> Settings {
        let mut settings = Settings::default();
        match &self.command {
            Command::Resolve {
                resolution,
                concurrency,
                run_log,
                ..
            } => {
                resolution.fill(&mut settings);
                settings.concurrency = *concurrency;
                run_log.fill(&mut settings);
            }
            Command::Rename {
                resolution,
                rename,
                bibliography,
                record,
                run_log,
                ..
            } => {
                resolution.fill(&mut settings);
                rename.fill(&mut settings);
                bibliography.fill(&mut settings);
                record.fill(&mut settings);
                run_log.fill(&mut settings);
            }
            Command::Bib {
                resolution,
                bibliography,
                run_log,
                ..
            } => {
                resolution.fill(&mut settings);
                bibliography.fill(&mut settings);
                run_log.fill(&mut settings);
            }
            Command::Config {
                concurrency,
                resolution,
                rename,
                bibliography,
                record,
                run_log,
            } => {
                settings.concurrency = *concurrency;
                resolution.fill(&mut settings);
                rename.fill(&mut settings);
                bibliography.fill(&mut settings);
                record.fill(&mut settings);
                run_log.fill(&mut settings);
            }
            Command::Status {
                extraction,
                run_log,
                ..
            } => {
                extraction.fill(&mut settings);
                run_log.fill(&mut settings);
            }
            Command::Validate { run_log, .. }
            | Command::Reconcile { run_log, .. }
            | Command::Adopt { run_log, .. } => run_log.fill(&mut settings),
            Command::Cache { run_log, .. } => run_log.fill(&mut settings),
        }
        settings
    }
}

impl Command {
    /// The `resolve` command over `paths`, with no setting overridden.
    pub fn resolve(paths: Vec<PathBuf>) -> Command {
        Command::Resolve {
            paths,
            resolution: ResolutionOptions::default(),
            concurrency: None,
            run_log: RunLogOptions::default(),
        }
    }

    /// The `rename` command over `paths`, moving the files when `apply`
    /// and reporting what it would move otherwise, with no setting
    /// overridden.
    pub fn rename(paths: Vec<PathBuf>, apply: bool) -> Command {
        Command::Rename {
            paths,
            apply,
            resolution: ResolutionOptions::default(),
            rename: RenameOptions::default(),
            bibliography: BibliographyOptions::default(),
            record: RecordOptions::default(),
            run_log: RunLogOptions::default(),
        }
    }

    /// The `bib` command over `paths`, with no setting overridden.
    pub fn bib(paths: Vec<PathBuf>) -> Command {
        Command::Bib {
            paths,
            resolution: ResolutionOptions::default(),
            bibliography: BibliographyOptions::default(),
            run_log: RunLogOptions::default(),
        }
    }

    /// The `config` command, with no setting overridden.
    pub fn config() -> Command {
        Command::Config {
            concurrency: None,
            resolution: ResolutionOptions::default(),
            rename: RenameOptions::default(),
            bibliography: BibliographyOptions::default(),
            record: RecordOptions::default(),
            run_log: RunLogOptions::default(),
        }
    }

    /// The `cache` command, emptying the cache when `clear` and
    /// reporting what it holds otherwise, with no setting overridden.
    pub fn cache(clear: bool) -> Command {
        Command::Cache {
            clear,
            run_log: RunLogOptions::default(),
        }
    }

    /// The `status` command over the library at `path`, reporting what
    /// can be identified when `identify`, with no setting overridden.
    pub fn status(path: Option<PathBuf>, identify: bool) -> Command {
        Command::Status {
            path,
            identify,
            extraction: ExtractionOptions::default(),
            run_log: RunLogOptions::default(),
        }
    }

    /// The `validate` command over the library at `path`, with no
    /// setting overridden.
    pub fn validate(path: Option<PathBuf>) -> Command {
        Command::Validate {
            path,
            run_log: RunLogOptions::default(),
        }
    }

    /// The `reconcile` command over the library at `path`, hashing
    /// every artifact when `rehash`, with no setting overridden.
    pub fn reconcile(path: Option<PathBuf>, rehash: bool) -> Command {
        Command::Reconcile {
            path,
            rehash,
            run_log: RunLogOptions::default(),
        }
    }

    /// The `adopt` command over the library at `path`, with no setting
    /// overridden.
    pub fn adopt(path: Option<PathBuf>) -> Command {
        Command::Adopt {
            path,
            run_log: RunLogOptions::default(),
        }
    }

    /// The subcommand's name, as [`crate::event::Event::RunStarted`]
    /// reports it and as the user typed it.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Resolve { .. } => "resolve",
            Command::Rename { .. } => "rename",
            Command::Bib { .. } => "bib",
            Command::Config { .. } => "config",
            Command::Cache { .. } => "cache",
            Command::Status { .. } => "status",
            Command::Validate { .. } => "validate",
            Command::Reconcile { .. } => "reconcile",
            Command::Adopt { .. } => "adopt",
        }
    }

    /// The files and directories the subcommand works on, empty for the
    /// subcommands that take none.
    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Command::Resolve { paths, .. }
            | Command::Rename { paths, .. }
            | Command::Bib { paths, .. } => paths,
            Command::Config { .. }
            | Command::Cache { .. }
            | Command::Status { .. }
            | Command::Validate { .. }
            | Command::Reconcile { .. }
            | Command::Adopt { .. } => &[],
        }
    }

    /// The directory the subcommand works on as a library, or `None`
    /// for a subcommand that works on files or on nothing.
    ///
    /// Not a path in [`Command::paths`]'s sense: these subcommands take
    /// a library rather than input files, so nothing expands it to the
    /// documents beneath it and no configuration is resolved per file
    /// under it. What it decides is where the run's own configuration
    /// is discovered from, and so which library the run is in.
    pub fn directory(&self) -> Option<&Path> {
        match self {
            Command::Status { path, .. }
            | Command::Validate { path, .. }
            | Command::Reconcile { path, .. }
            | Command::Adopt { path, .. } => path.as_deref(),
            _ => None,
        }
    }
}

/// `settings` as configuration layers, one per flag actually given.
///
/// [`crate::config::resolve`] records one [`Origin`] per layer and a
/// flag's origin names the flag, so each flag becomes a layer of its
/// own carrying nothing but the setting it sets. A flag left unsaid
/// produces no layer, which is what leaves the layers below it showing
/// through.
///
/// The layers are ordered as the fields are declared, and the order
/// does not matter: no two of them set the same key, so none can
/// override another. Callers append the whole run to the end of the
/// layer list, since flags outrank every other layer.
pub fn flag_layers(settings: &Settings) -> Vec<(Origin, Layer)> {
    let mut layers = Vec::new();
    let mut push = |name: &str, layer: Layer| layers.push((Origin::Flag(name.to_string()), layer));

    if let Some(sources) = &settings.sources {
        push(
            "sources",
            Layer {
                sources: Some(sources.clone()),
                ..Layer::default()
            },
        );
    }
    if let Some(mailto) = &settings.mailto {
        push(
            "mailto",
            Layer {
                mailto: Some(mailto.clone()),
                ..Layer::default()
            },
        );
    }
    if let Some(collision) = &settings.collision {
        push(
            "collision",
            Layer {
                rename: Some(RenameLayer {
                    collision: Some(collision.clone()),
                    ..RenameLayer::default()
                }),
                ..Layer::default()
            },
        );
    }
    if let Some(path) = &settings.bib {
        push(
            "bib",
            bib_layer(BibLayer {
                path: Some(path.clone()),
                ..BibLayer::default()
            }),
        );
    }
    if let Some(duplicates) = &settings.duplicates {
        push(
            "duplicates",
            bib_layer(BibLayer {
                duplicates: Some(duplicates.clone()),
                ..BibLayer::default()
            }),
        );
    }
    if settings.sidecars {
        push(
            "sidecars",
            bib_layer(BibLayer {
                sidecars: Some(true),
                ..BibLayer::default()
            }),
        );
    }
    if settings.no_sidecars {
        push(
            "no-sidecars",
            bib_layer(BibLayer {
                sidecars: Some(false),
                ..BibLayer::default()
            }),
        );
    }
    if let Some(page_limit) = settings.page_limit {
        push(
            "page-limit",
            Layer {
                extraction: Some(ExtractionLayer {
                    page_limit: Some(page_limit),
                }),
                ..Layer::default()
            },
        );
    }
    if let Some(concurrency) = settings.concurrency {
        push(
            "concurrency",
            network_layer(NetworkLayer {
                concurrency: Some(concurrency),
                ..NetworkLayer::default()
            }),
        );
    }
    if let Some(min_interval_ms) = settings.min_interval_ms {
        push(
            "min-interval-ms",
            network_layer(NetworkLayer {
                min_interval_ms: Some(min_interval_ms),
                ..NetworkLayer::default()
            }),
        );
    }
    if settings.cache {
        push(
            "cache",
            network_layer(NetworkLayer {
                cache: Some(true),
                ..NetworkLayer::default()
            }),
        );
    }
    if settings.no_cache {
        push(
            "no-cache",
            network_layer(NetworkLayer {
                cache: Some(false),
                ..NetworkLayer::default()
            }),
        );
    }
    if settings.batch {
        push(
            "batch",
            Layer {
                rename: Some(RenameLayer {
                    batch: Some(true),
                    ..RenameLayer::default()
                }),
                ..Layer::default()
            },
        );
    }
    if settings.no_batch {
        push(
            "no-batch",
            Layer {
                rename: Some(RenameLayer {
                    batch: Some(false),
                    ..RenameLayer::default()
                }),
                ..Layer::default()
            },
        );
    }
    if settings.skip_named {
        push(
            "skip-named",
            Layer {
                rename: Some(RenameLayer {
                    skip_named: Some(true),
                    ..RenameLayer::default()
                }),
                ..Layer::default()
            },
        );
    }
    if settings.no_skip_named {
        push(
            "no-skip-named",
            Layer {
                rename: Some(RenameLayer {
                    skip_named: Some(false),
                    ..RenameLayer::default()
                }),
                ..Layer::default()
            },
        );
    }
    if settings.record {
        push(
            "record",
            Layer {
                record: Some(true),
                ..Layer::default()
            },
        );
    }
    if settings.no_record {
        push(
            "no-record",
            Layer {
                record: Some(false),
                ..Layer::default()
            },
        );
    }
    if settings.run_log {
        push(
            "run-log",
            Layer {
                run_log: Some(true),
                ..Layer::default()
            },
        );
    }
    if settings.no_run_log {
        push(
            "no-run-log",
            Layer {
                run_log: Some(false),
                ..Layer::default()
            },
        );
    }

    layers
}

/// A layer carrying `bib` and nothing else.
fn bib_layer(bib: BibLayer) -> Layer {
    Layer {
        bib: Some(bib),
        ..Layer::default()
    }
}

/// A layer carrying `network` and nothing else.
fn network_layer(network: NetworkLayer) -> Layer {
    Layer {
        network: Some(network),
        ..Layer::default()
    }
}
