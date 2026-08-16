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
use borax_pdf::tiered::DEFAULT_PAGE_LIMIT;
use borax_sources::pace::{DEFAULT_CONCURRENCY, DEFAULT_MIN_INTERVAL};
use borax_sources::source::SourceName;
use serde::Deserialize;

use crate::event::Event;

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
        Config {
            templates: BTreeMap::from([(
                "default".to_string(),
                "[auth:lower][year]_[shorttitle3:camel]".to_string(),
            )]),
            sources: SourceName::ALL.to_vec(),
            mailto: None,
            concurrency: DEFAULT_CONCURRENCY,
            min_interval_ms: DEFAULT_MIN_INTERVAL.as_millis() as u64,
            page_limit: DEFAULT_PAGE_LIMIT,
            collision: CollisionPolicy::Suffix,
            bib_path: None,
            duplicates: DuplicatePolicy::Skip,
            sidecars: false,
            cache: true,
        }
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
        match self {
            Origin::Default => f.write_str("defaults"),
            Origin::GlobalFile(path) => write!(f, "global {}", path.display()),
            Origin::DirectoryFile(path) => write!(f, "override {}", path.display()),
            Origin::Env(name) => write!(f, "env {ENV_PREFIX}{name}"),
            Origin::Flag(name) => write!(f, "flag --{name}"),
        }
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

    /// Every setting as an [`Event::ConfigSetting`], in key order.
    ///
    /// The event stream `borax config` writes: one line per setting,
    /// carrying the same key, rendered value, and origin wording that
    /// [`Effective::entries`] produces.
    pub fn events(&self) -> Vec<Event> {
        self.entries()
            .into_iter()
            .map(|(key, value, origin)| Event::ConfigSetting {
                key,
                value,
                origin: origin.to_string(),
            })
            .collect()
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
        let mut rendered: BTreeMap<Key, String> = SETTINGS
            .iter()
            .map(|setting| (setting.key.to_string(), (setting.render)(&self.config)))
            .collect();
        for (entry_type, template) in &self.config.templates {
            rendered.insert(format!("templates.{entry_type}"), quote(template));
        }

        rendered
            .into_iter()
            .map(|(key, value)| {
                let origin = self.origins.get(&key).cloned().unwrap_or(Origin::Default);
                (key, value, origin)
            })
            .collect()
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
        match self {
            ConfigError::Unreadable { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            ConfigError::Invalid {
                key,
                origin,
                message,
            } => write!(f, "{key} ({origin}): {message}"),
            ConfigError::UnknownEnv { name } => {
                write!(f, "{ENV_PREFIX}{name} names no setting")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// One setting that holds a single value: the name it is addressed by,
/// where a [`Layer`] carries it, and how its effective value is written
/// out.
struct Setting {
    key: &'static str,
    slot: fn(&mut Layer) -> Slot<'_>,
    render: fn(&Config) -> String,
}

/// Every single-valued setting, in key order.
///
/// `templates` is absent because its keys are open-ended: it is merged
/// and rendered per entry type instead, and cannot be addressed by one
/// environment variable.
const SETTINGS: &[Setting] = &[
    Setting {
        key: "bib.duplicates",
        slot: |layer| Slot::Text(&mut layer.bib.get_or_insert_default().duplicates),
        render: |config| quote(duplicates_name(config.duplicates)),
    },
    Setting {
        key: "bib.path",
        slot: |layer| Slot::Path(&mut layer.bib.get_or_insert_default().path),
        render: |config| match &config.bib_path {
            Some(path) => quote(&path.to_string_lossy()),
            None => String::new(),
        },
    },
    Setting {
        key: "bib.sidecars",
        slot: |layer| Slot::Flag(&mut layer.bib.get_or_insert_default().sidecars),
        render: |config| config.sidecars.to_string(),
    },
    Setting {
        key: "extraction.page-limit",
        slot: |layer| Slot::Count(&mut layer.extraction.get_or_insert_default().page_limit),
        render: |config| config.page_limit.to_string(),
    },
    Setting {
        key: "mailto",
        slot: |layer| Slot::Text(&mut layer.mailto),
        render: |config| match &config.mailto {
            Some(mailto) => quote(mailto),
            None => String::new(),
        },
    },
    Setting {
        key: "network.cache",
        slot: |layer| Slot::Flag(&mut layer.network.get_or_insert_default().cache),
        render: |config| config.cache.to_string(),
    },
    Setting {
        key: "network.concurrency",
        slot: |layer| Slot::Count(&mut layer.network.get_or_insert_default().concurrency),
        render: |config| config.concurrency.to_string(),
    },
    Setting {
        key: "network.min-interval-ms",
        slot: |layer| Slot::Millis(&mut layer.network.get_or_insert_default().min_interval_ms),
        render: |config| config.min_interval_ms.to_string(),
    },
    Setting {
        key: "rename.collision",
        slot: |layer| Slot::Text(&mut layer.rename.get_or_insert_default().collision),
        render: |config| quote(collision_name(config.collision)),
    },
    Setting {
        key: "sources",
        slot: |layer| Slot::List(&mut layer.sources),
        render: |config| {
            let names: Vec<String> = config
                .sources
                .iter()
                .map(|source| quote(source.as_str()))
                .collect();
            format!("[{}]", names.join(", "))
        },
    },
];

/// A mutable handle on one setting's place in a [`Layer`], carrying the
/// type that setting's values take.
enum Slot<'a> {
    Text(&'a mut Option<String>),
    Path(&'a mut Option<PathBuf>),
    Count(&'a mut Option<usize>),
    Millis(&'a mut Option<u64>),
    Flag(&'a mut Option<bool>),
    List(&'a mut Option<Vec<String>>),
}

/// Move whatever `from` holds into `to`, and report whether it held
/// anything.
fn transfer(from: Slot<'_>, to: Slot<'_>) -> bool {
    match (from, to) {
        (Slot::Text(from), Slot::Text(to)) => move_value(from, to),
        (Slot::Path(from), Slot::Path(to)) => move_value(from, to),
        (Slot::Count(from), Slot::Count(to)) => move_value(from, to),
        (Slot::Millis(from), Slot::Millis(to)) => move_value(from, to),
        (Slot::Flag(from), Slot::Flag(to)) => move_value(from, to),
        (Slot::List(from), Slot::List(to)) => move_value(from, to),
        // Both slots are opened by the same accessor, so their variants
        // always agree.
        _ => false,
    }
}

fn move_value<T>(from: &mut Option<T>, to: &mut Option<T>) -> bool {
    match from.take() {
        Some(value) => {
            *to = Some(value);
            true
        }
        None => false,
    }
}

/// Read `text` as the type `slot` takes and store it there.
///
/// The error is the message an [`ConfigError::Invalid`] carries.
fn store(slot: Slot<'_>, text: &str) -> Result<(), String> {
    match slot {
        Slot::Text(value) => *value = Some(text.to_string()),
        Slot::Path(value) => *value = Some(PathBuf::from(text)),
        Slot::Count(value) => *value = Some(number(text)?),
        Slot::Millis(value) => *value = Some(number(text)?),
        Slot::Flag(value) => {
            *value = Some(match text {
                "true" => true,
                "false" => false,
                _ => return Err(format!("expected true or false, got {text:?}")),
            });
        }
        Slot::List(value) => {
            *value = Some(
                text.split(',')
                    .map(|item| item.trim().to_string())
                    .collect(),
            );
        }
    }
    Ok(())
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("expected a whole number, got {text:?}"))
}

/// The environment variable, without its [`ENV_PREFIX`], that addresses
/// the setting named `key`.
fn env_name(key: &str) -> String {
    key.to_uppercase().replace(['.', '-'], "_")
}

/// `value` as a TOML basic string.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn collision_name(policy: CollisionPolicy) -> &'static str {
    match policy {
        CollisionPolicy::Suffix => "suffix",
        CollisionPolicy::Skip => "skip",
    }
}

fn parse_collision(value: &str) -> Option<CollisionPolicy> {
    match value {
        "suffix" => Some(CollisionPolicy::Suffix),
        "skip" => Some(CollisionPolicy::Skip),
        _ => None,
    }
}

fn duplicates_name(policy: DuplicatePolicy) -> &'static str {
    match policy {
        DuplicatePolicy::Skip => "skip",
        DuplicatePolicy::Update => "update",
    }
}

fn parse_duplicates(value: &str) -> Option<DuplicatePolicy> {
    match value {
        "skip" => Some(DuplicatePolicy::Skip),
        "update" => Some(DuplicatePolicy::Update),
        _ => None,
    }
}

/// Read a layer from the TOML in `text`.
///
/// `path` names the file for error reporting only; it is not read.
/// Fails when the text is not TOML or carries a key borax does not
/// know — a misspelled setting is a configuration error, never a
/// silently ignored one.
pub fn layer_from_toml(text: &str, path: &Path) -> Result<Layer, ConfigError> {
    toml::from_str(text).map_err(|error| ConfigError::Unreadable {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
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
    let mut layer = Layer::default();

    for (name, value) in vars {
        let Some(suffix) = name.as_ref().strip_prefix(ENV_PREFIX) else {
            continue;
        };
        let Some(setting) = SETTINGS
            .iter()
            .find(|setting| env_name(setting.key) == suffix)
        else {
            return Err(ConfigError::UnknownEnv {
                name: suffix.to_string(),
            });
        };

        store((setting.slot)(&mut layer), value.as_ref()).map_err(|message| {
            ConfigError::Invalid {
                key: setting.key.to_string(),
                origin: Origin::Env(suffix.to_string()),
                message,
            }
        })?;
    }

    Ok(layer)
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
    let mut config = Config::default();
    let mut origins: BTreeMap<Key, Origin> = SETTINGS
        .iter()
        .map(|setting| (setting.key.to_string(), Origin::Default))
        .chain(
            config
                .templates
                .keys()
                .map(|entry_type| (format!("templates.{entry_type}"), Origin::Default)),
        )
        .collect();

    let mut winning = Layer::default();
    for (origin, mut layer) in layers {
        for setting in SETTINGS {
            if transfer((setting.slot)(&mut layer), (setting.slot)(&mut winning)) {
                origins.insert(setting.key.to_string(), origin.clone());
            }
        }

        for (entry_type, template) in layer.templates.take().unwrap_or_default() {
            origins.insert(format!("templates.{entry_type}"), origin.clone());
            config.templates.insert(entry_type, template);
        }
    }

    let origin_of = |key: &str| origins.get(key).cloned().unwrap_or(Origin::Default);
    let invalid = |key: &str, message: String| ConfigError::Invalid {
        key: key.to_string(),
        origin: origin_of(key),
        message,
    };

    if let Some(mailto) = winning.mailto {
        config.mailto = Some(mailto);
    }
    if let Some(names) = winning.sources {
        let mut sources = Vec::with_capacity(names.len());
        for name in names {
            match SourceName::parse(&name) {
                Some(source) => sources.push(source),
                None => return Err(invalid("sources", format!("unknown source {name:?}"))),
            }
        }
        config.sources = sources;
    }

    if let Some(collision) = winning.rename.unwrap_or_default().collision {
        config.collision = parse_collision(&collision).ok_or_else(|| {
            invalid(
                "rename.collision",
                format!("expected suffix or skip, got {collision:?}"),
            )
        })?;
    }

    let bib = winning.bib.unwrap_or_default();
    if let Some(path) = bib.path {
        config.bib_path = Some(path);
    }
    if let Some(duplicates) = bib.duplicates {
        config.duplicates = parse_duplicates(&duplicates).ok_or_else(|| {
            invalid(
                "bib.duplicates",
                format!("expected skip or update, got {duplicates:?}"),
            )
        })?;
    }
    if let Some(sidecars) = bib.sidecars {
        config.sidecars = sidecars;
    }

    if let Some(page_limit) = winning.extraction.unwrap_or_default().page_limit {
        config.page_limit = page_limit;
    }

    let network = winning.network.unwrap_or_default();
    if let Some(concurrency) = network.concurrency {
        config.concurrency = concurrency;
    }
    if let Some(min_interval_ms) = network.min_interval_ms {
        config.min_interval_ms = min_interval_ms;
    }
    if let Some(cache) = network.cache {
        config.cache = cache;
    }

    Ok(Effective { config, origins })
}

/// The nearest [`OVERRIDE_FILE`] at or above `start`.
///
/// `start` is the directory an input file lives in. Directories are
/// tried from `start` upward to the filesystem root and the first hit
/// wins — nearest overrides furthest, and no more than one override
/// file applies to any file. `exists` answers whether a path is a
/// readable file, so the walk is testable without a filesystem.
pub fn nearest_override(start: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join(OVERRIDE_FILE))
        .find(|candidate| exists(candidate))
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
    let mut path = CANDIDATES.iter().find_map(|(name, suffix)| {
        let base = PathBuf::from(lookup(name)?);
        if !base.is_absolute() {
            return None;
        }

        Some(match suffix {
            Some(suffix) => base.join(suffix),
            None => base,
        })
    })?;

    path.push("borax");
    path.push("config.toml");
    Some(path)
}

/// The variables that may name a configuration directory, in the order
/// they are tried, each with what to append to its value.
#[cfg(not(windows))]
const CANDIDATES: &[(&str, Option<&str>)] = &[("XDG_CONFIG_HOME", None), ("HOME", Some(".config"))];

/// The variables that may name a configuration directory, in the order
/// they are tried, each with what to append to its value.
#[cfg(windows)]
const CANDIDATES: &[(&str, Option<&str>)] = &[("APPDATA", None), ("XDG_CONFIG_HOME", None)];
