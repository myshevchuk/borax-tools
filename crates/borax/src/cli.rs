//! What the user typed.
//!
//! The command line is the highest configuration layer, and this module
//! is where it becomes one: [`Settings`] is the flag surface, and
//! [`flag_layers`] turns it into layers [`crate::config::resolve`]
//! merges under everything else.
//!
//! Every setting flag is global rather than per-subcommand, so
//! `borax --mailto … rename` and `borax rename --mailto …` are the same
//! invocation. A person who has learned a flag once should not have to
//! learn where it goes.

use std::collections::BTreeMap;
use std::path::PathBuf;

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

    #[command(flatten)]
    pub settings: Settings,

    /// Write one JSON object per line instead of prose.
    #[arg(long, global = true)]
    pub json: bool,
}

/// Which subcommand to run, and what it needs beyond the settings.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Resolve each file to a record and report what was found.
    Resolve {
        /// The files and directories to work on.
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,
    },
    /// Rename each file after the record resolved for it.
    Rename {
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,
        /// Move the files. Without it the run only reports what it
        /// would move.
        #[arg(long)]
        apply: bool,
    },
    /// Write bibliography output for each file, renaming nothing.
    Bib {
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,
    },
    /// Print every setting, its value, and the layer it came from.
    Config,
    /// Report what the response cache holds.
    Cache {
        /// Empty the cache instead of reporting it.
        #[arg(long)]
        clear: bool,
    },
    /// Work on the collection's record of what it has admitted.
    Ledger {
        #[command(subcommand)]
        action: LedgerAction,
    },
}

/// What `borax ledger` is being asked to do.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum LedgerAction {
    /// Regenerate the ledger from the collection's files and sidecars.
    Rebuild,
}

/// The settings a command line may override, one field per flag.
///
/// Every field is optional in the sense a [`Layer`]'s fields are: absent
/// means the command line was silent about that setting and a lower
/// layer shows through. Every configurable boolean comes as a two-flag
/// pair (`--sidecars` / `--no-sidecars`, `--cache` / `--no-cache`,
/// `--ledger` / `--no-ledger`, `--run-log` / `--no-run-log`), which is
/// how a boolean says all three things — on, off, and silent — with
/// flags that take no value, and what lets the command line override a
/// configured value in both directions. Naming both halves of a pair
/// is a parse error rather than a last-one-wins guess.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct Settings {
    /// The filename template for entry types with none of their own.
    #[arg(long, global = true, value_name = "TEMPLATE")]
    pub template: Option<String>,

    /// Which services may be asked, comma-separated.
    #[arg(long, global = true, value_name = "NAME", value_delimiter = ',')]
    pub sources: Option<Vec<String>>,

    /// The contact address sent to the polite pools.
    #[arg(long, global = true, value_name = "EMAIL")]
    pub mailto: Option<String>,

    /// What to do when two files want one name: suffix or skip.
    #[arg(long, global = true, value_name = "POLICY")]
    pub collision: Option<String>,

    /// The master .bib file entries are merged into.
    #[arg(long, global = true, value_name = "PATH")]
    pub bib: Option<PathBuf>,

    /// What to do with an entry already in the master .bib: skip or
    /// update.
    #[arg(long, global = true, value_name = "POLICY")]
    pub duplicates: Option<String>,

    /// Write a sidecar beside each resolved file.
    #[arg(long, global = true)]
    pub sidecars: bool,

    /// Write no sidecars.
    #[arg(long, global = true, conflicts_with = "sidecars")]
    pub no_sidecars: bool,

    /// How many pages of a PDF the text pass reads.
    #[arg(long, global = true, value_name = "N")]
    pub page_limit: Option<usize>,

    /// How many files may be resolved at once.
    #[arg(long, global = true, value_name = "N")]
    pub concurrency: Option<usize>,

    /// The floor on the gap between two requests to one service.
    #[arg(long, global = true, value_name = "MS")]
    pub min_interval_ms: Option<u64>,

    /// Read and write the on-disk response cache.
    #[arg(long, global = true)]
    pub cache: bool,

    /// Ask every service again and open every file again.
    #[arg(long, global = true, conflicts_with = "cache")]
    pub no_cache: bool,

    /// Check the collection's ledger for duplicates and record what an
    /// applied run admits.
    #[arg(long, global = true)]
    pub ledger: bool,

    /// Neither read nor write the collection's ledger.
    #[arg(long, global = true, conflicts_with = "ledger")]
    pub no_ledger: bool,

    /// Write the run's event stream to a log in the collection.
    #[arg(long, global = true)]
    pub run_log: bool,

    /// Write no log for this run. An applying rename writes one
    /// anyway: that log is the only record of what it moved.
    #[arg(long, global = true, conflicts_with = "run_log")]
    pub no_run_log: bool,
}

impl Cli {
    /// Which rendering of the event stream this invocation asked for.
    pub fn format(&self) -> Format {
        match self.json {
            true => Format::Json,
            false => Format::Human,
        }
    }
}

impl Command {
    /// The subcommand's name, as [`crate::event::Event::RunStarted`]
    /// reports it and as the user typed it. A subcommand with an
    /// action of its own is named by both words, as `ledger rebuild`.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Resolve { .. } => "resolve",
            Command::Rename { .. } => "rename",
            Command::Bib { .. } => "bib",
            Command::Config => "config",
            Command::Cache { .. } => "cache",
            Command::Ledger {
                action: LedgerAction::Rebuild,
            } => "ledger rebuild",
        }
    }

    /// The files and directories the subcommand works on, empty for the
    /// subcommands that take none.
    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Command::Resolve { paths } | Command::Rename { paths, .. } | Command::Bib { paths } => {
                paths
            }
            Command::Config | Command::Cache { .. } | Command::Ledger { .. } => &[],
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

    if let Some(template) = &settings.template {
        push(
            "template",
            Layer {
                templates: Some(BTreeMap::from([("default".to_string(), template.clone())])),
                ..Layer::default()
            },
        );
    }
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
    if settings.ledger {
        push(
            "ledger",
            Layer {
                ledger: Some(true),
                ..Layer::default()
            },
        );
    }
    if settings.no_ledger {
        push(
            "no-ledger",
            Layer {
                ledger: Some(false),
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
