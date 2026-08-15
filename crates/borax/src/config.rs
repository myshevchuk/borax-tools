//! Settings, where they come from, and which one wins.
//!
//! Configuration arrives in layers — built-in defaults, a global file, a
//! per-directory override, the environment, the command line — and the
//! effective value of every setting is the one from the
//! highest-precedence layer that mentioned it. Which layer that was is
//! kept alongside the value, because `borax config` has to answer "why
//! is this the template?" and a merged struct alone cannot.
//!
//! Merging is pure: [`resolve`] takes layers and returns the effective
//! configuration. Finding the files to make layers out of is the
//! adapter, and lives at the bottom of this module.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use borax_core::bib_output::DuplicatePolicy;
use borax_core::rename::CollisionPolicy;
use borax_sources::source::SourceName;
use serde::Deserialize;

/// The file a directory-level override is read from.
pub const OVERRIDE_FILE: &str = ".borax.toml";

/// The prefix an environment variable needs to be read as a setting.
pub const ENV_PREFIX: &str = "BORAX_";

/// Every setting borax runs on, with no absent values left.
///
/// Produced by [`resolve`]; the layers it is merged from are [`Layer`]s,
/// where every field is optional.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Filename templates by entry type. `default` is always present:
    /// the built-in defaults supply it and merging removes no key, so
    /// an entry type with no template of its own always has one to fall
    /// back to. Values are the unparsed template strings; parsing them
    /// is [`borax_core::template::TemplateTable`]'s job and reports its
    /// own errors.
    pub templates: BTreeMap<String, String>,
    /// Which services may be asked. The order within it carries no
    /// meaning: which candidate an identifier goes to first is
    /// [`borax_sources::dispatch::priority`]'s decision, and this set
    /// only says which of its candidates are available.
    pub sources: Vec<SourceName>,
    /// The contact address sent to the Crossref and OpenAlex polite
    /// pools.
    pub mailto: Option<String>,
    /// How many files may be resolved at once.
    pub concurrency: usize,
    /// The floor on the gap between two requests to the same service,
    /// in milliseconds.
    pub min_interval_ms: u64,
    /// How many pages of a PDF the text-layer pass reads.
    pub page_limit: usize,
    /// What to do when two files want the same name.
    pub collision: CollisionPolicy,
    /// The master `.bib` file resolved entries are merged into, if any.
    pub bib_path: Option<PathBuf>,
    /// What to do when an entry is already in the master `.bib`.
    pub duplicates: DuplicatePolicy,
    /// Whether to write a sidecar next to each renamed file.
    pub sidecars: bool,
    /// Whether to read and write the on-disk response cache.
    pub cache: bool,
}

impl Default for Config {
    /// The built-in defaults: the lowest layer, and the configuration a
    /// run with no configuration file, environment, or flags uses.
    ///
    /// `templates` holds `default` alone, as
    /// `[auth:lower][year]_[shorttitle3:camel]`; every service borax
    /// speaks to is enabled ([`SourceName::ALL`]); there is no
    /// contact address, no master `.bib`, and no sidecars. Concurrency
    /// and pacing take [`borax_sources::pace::DEFAULT_CONCURRENCY`] and
    /// [`borax_sources::pace::DEFAULT_MIN_INTERVAL`], extraction takes
    /// [`borax_pdf::tiered::DEFAULT_PAGE_LIMIT`], collisions are
    /// suffixed, duplicate entries skipped, and the cache is on.
    fn default() -> Config {
        todo!("the built-in defaults")
    }
}

/// One layer of configuration: the settings a single source of
/// configuration had an opinion about.
///
/// Deserialized from TOML, built from the environment, or assembled
/// from command-line flags. A field left `None` means "this layer is
/// silent about that setting", which is what lets a lower layer show
/// through.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    #[serde(default)]
    pub templates: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    #[serde(default)]
    pub mailto: Option<String>,
    #[serde(default)]
    pub rename: Option<RenameLayer>,
    #[serde(default)]
    pub bib: Option<BibLayer>,
    #[serde(default)]
    pub extraction: Option<ExtractionLayer>,
    #[serde(default)]
    pub network: Option<NetworkLayer>,
}

/// The `[rename]` table of a layer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameLayer {
    /// `"suffix"` or `"skip"`.
    #[serde(default)]
    pub collision: Option<String>,
}

/// The `[bib]` table of a layer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BibLayer {
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// `"skip"` or `"update"`.
    #[serde(default)]
    pub duplicates: Option<String>,
    #[serde(default)]
    pub sidecars: Option<bool>,
}

/// The `[extraction]` table of a layer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ExtractionLayer {
    #[serde(default)]
    pub page_limit: Option<usize>,
}

/// The `[network]` table of a layer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct NetworkLayer {
    #[serde(default)]
    pub concurrency: Option<usize>,
    #[serde(default)]
    pub min_interval_ms: Option<u64>,
    #[serde(default)]
    pub cache: Option<bool>,
}

/// Where a value came from.
///
/// Ordering is by precedence, lowest first, so the winning layer for a
/// key is the greatest [`Origin`] that set it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// The built-in defaults.
    Default,
    /// The global configuration file.
    GlobalFile(PathBuf),
    /// The nearest `.borax.toml` above an input file.
    DirectoryFile(PathBuf),
    /// An environment variable, named without its prefix.
    Env(String),
    /// A command-line flag, named as the user would type it.
    Flag(String),
}

impl fmt::Display for Origin {
    /// `defaults`, `global <path>`, `override <path>`, `env
    /// BORAX_<NAME>`, or `flag --<name>` — the origin column of
    /// `borax config`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("render the origin for display")
    }
}

/// The dotted name a setting is addressed by in diagnostics and in the
/// environment: `mailto`, `network.concurrency`,
/// `templates.default`, ….
pub type Key = String;

/// A resolved configuration and the origin of every value in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Effective {
    config: Config,
    origins: BTreeMap<Key, Origin>,
}

impl Effective {
    /// The settings to run with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Which layer supplied `key`, or `None` when no such key exists.
    ///
    /// Every key of a resolved configuration has an origin, so `None`
    /// means the key was misspelled rather than unset.
    pub fn origin(&self, key: &str) -> Option<&Origin> {
        self.origins.get(key)
    }

    /// Every setting as `(key, rendered value, origin)`, ordered by
    /// key.
    ///
    /// Values render in TOML value syntax — strings quoted, numbers and
    /// booleans bare, `sources` as an array — so a line of
    /// `borax config` output can be pasted back into a configuration
    /// file. A setting that is optional and unset renders as the empty
    /// string, which is the one form that is not TOML and so cannot be
    /// mistaken for a value.
    ///
    /// Unset settings are listed rather than omitted: `borax config`
    /// answers "what is borax running on", and "nothing" is an answer
    /// about `mailto` that a user needs to see.
    pub fn entries(&self) -> Vec<(Key, String, Origin)> {
        todo!("render every setting with its origin")
    }
}

/// Why a configuration could not be assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A file was not TOML, or held a key borax does not know.
    Unreadable { path: PathBuf, message: String },
    /// A value is of the right type but not one of the accepted
    /// values — a collision policy that is neither `suffix` nor `skip`,
    /// a source name that is not a service borax speaks to.
    Invalid {
        key: Key,
        origin: Origin,
        message: String,
    },
    /// An environment variable carries the prefix but no known key
    /// follows it. `name` is the variable without its [`ENV_PREFIX`],
    /// as [`Origin::Env`] holds it.
    UnknownEnv { name: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("render the error")
    }
}

impl std::error::Error for ConfigError {}

/// Read a layer from the TOML in `text`.
///
/// `path` names the file for error reporting only; it is not read.
/// Fails when the text is not TOML or carries a key borax does not
/// know — a misspelled setting is a configuration error, never a
/// silently ignored one.
pub fn layer_from_toml(text: &str, path: &Path) -> Result<Layer, ConfigError> {
    let _ = (text, path);
    todo!("parse the TOML into a layer")
}

/// Read a layer from environment variables.
///
/// `vars` yields `(name, value)` pairs the way [`std::env::vars`] does.
/// A variable is a setting when its name is [`ENV_PREFIX`] followed by
/// a key's dotted name uppercased with `.` and `-` replaced by `_`, so
/// `network.min-interval-ms` is `BORAX_NETWORK_MIN_INTERVAL_MS`.
/// Variables without the prefix are ignored; a prefixed variable naming
/// no key is [`ConfigError::UnknownEnv`].
///
/// Values are parsed as their setting's type: `sources` is a
/// comma-separated list, booleans are `true`/`false`, and
/// `templates` cannot be set this way (its keys are open-ended).
pub fn layer_from_env<I, N, V>(vars: I) -> Result<Layer, ConfigError>
where
    I: IntoIterator<Item = (N, V)>,
    N: AsRef<str>,
    V: AsRef<str>,
{
    let _ = vars;
    todo!("read the prefixed variables into a layer")
}

/// Merge `layers` into the effective configuration.
///
/// Layers are given lowest precedence first, so a later layer's value
/// wins over an earlier one's and the origin recorded for a key is the
/// last layer that set it. Keys no layer set take their default value
/// and [`Origin::Default`]. Merging is per key, not per layer: a
/// `.borax.toml` that sets only the collision policy leaves every other
/// setting to the layers below it.
///
/// The `templates` table merges per entry type for the same reason —
/// an override file defining `thesis` keeps the global `default`.
pub fn resolve(layers: Vec<(Origin, Layer)>) -> Result<Effective, ConfigError> {
    let _ = layers;
    todo!("merge the layers, keeping origins")
}

/// The nearest [`OVERRIDE_FILE`] at or above `start`.
///
/// `start` is the directory an input file lives in. Directories are
/// tried from `start` upward to the filesystem root and the first hit
/// wins — nearest overrides furthest, and no more than one override
/// file applies to any file. `exists` answers whether a path is a
/// readable file, so the walk is testable without a filesystem.
pub fn nearest_override(start: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let _ = (start, exists);
    todo!("walk upward looking for the override file")
}

/// The global configuration file implied by `lookup`, which answers the
/// way [`std::env::var_os`] does.
///
/// The candidates are tried in order and the first whose value is a
/// non-empty absolute path is taken: on Unix `XDG_CONFIG_HOME`, then
/// `HOME` (as `HOME/.config`); on Windows `APPDATA`, then
/// `XDG_CONFIG_HOME`. The returned path ends in `borax/config.toml` and
/// is neither created nor checked for existence.
///
/// Returns `None` when no candidate qualifies.
pub fn global_config_path(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let _ = lookup;
    todo!("resolve the XDG (or Windows) config file path")
}
