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
    /// Move back everything the last applied run moved.
    Undo,
    /// Print every setting, its value, and the layer it came from.
    Config,
    /// Report what the response cache holds.
    Cache {
        /// Empty the cache instead of reporting it.
        #[arg(long)]
        clear: bool,
    },
}

/// The settings a command line may override, one field per flag.
///
/// Every field is optional in the sense a [`Layer`]'s fields are: absent
/// means the command line was silent about that setting and a lower
/// layer shows through. The two-flag pairs (`--sidecars` /
/// `--no-sidecars`, `--cache` / `--no-cache`) are how a boolean says
/// all three things — on, off, and silent — with flags that take no
/// value.
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
}

impl Cli {
    /// Which rendering of the event stream this invocation asked for.
    pub fn format(&self) -> Format {
        todo!("json or human")
    }
}

impl Command {
    /// The subcommand's name, as [`crate::event::Event::RunStarted`]
    /// reports it and as the user typed it.
    pub fn name(&self) -> &'static str {
        todo!("name each variant")
    }

    /// The files and directories the subcommand works on, empty for the
    /// subcommands that take none.
    pub fn paths(&self) -> &[PathBuf] {
        todo!("the paths of the variants that have them")
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
    todo!("one single-setting layer per flag given")
}
