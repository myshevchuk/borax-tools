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
use borax_core::tables::ValueKind;
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
    /// Citation-key templates by entry type, in the same shape as
    /// `templates` and read by the same engine, but separate from it:
    /// how a work is cited and how its file is named move
    /// independently. `default` is always present, for the reason it is
    /// under `templates`.
    pub citation_keys: BTreeMap<String, String>,
    /// External lookup tables by the name a template addresses them
    /// by, in the same open-ended shape as `templates`. Empty when no
    /// layer declared one, which is the configuration a run with no
    /// `lookup` in any template uses. Values are declarations, not
    /// loaded tables: reading the files is
    /// [`borax_core::tables::Table::load`]'s job and reports its own
    /// errors.
    pub tables: BTreeMap<String, TableDeclaration>,
    /// Which services may be asked. The order within it carries no
    /// meaning: which candidate an identifier goes to first is
    /// [`borax_sources::dispatch::priority`]'s decision, and this set
    /// only says which of its candidates are available.
    pub sources: Vec<SourceName>,
    /// The contact address sent to the Crossref and OpenAlex polite
    /// pools.
    pub mailto: Option<String>,
    /// The directory anchoring the collection's `.borax/` accounting,
    /// or `None` when nothing configured one and
    /// [`collection_root`]'s discovery decides it.
    pub collection_root: Option<PathBuf>,
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
    /// Whether the collection's ledger is consulted for duplicates and
    /// added to by an applied run. Off means the run keeps no
    /// accounting and reports none missing.
    pub ledger: bool,
    /// Whether a run writes its event stream to a run log. An applying
    /// rename writes one regardless, since that log is the only record
    /// of what it moved; this setting is what a preview run obeys.
    pub run_log: bool,
}

impl Default for Config {
    /// The built-in defaults: the lowest layer, and the configuration a
    /// run with no configuration file, environment, or flags uses.
    ///
    /// `templates` holds `default` alone, as
    /// `[auth:lower][year]_[shorttitle3:camel]`, and `citation_keys`
    /// holds `default` alone, as `[auth:lower][year]`; every service borax
    /// speaks to is enabled ([`SourceName::SUPPORTED`]); there is no
    /// contact address, no master `.bib`, no configured collection
    /// root, and no sidecars. Concurrency and pacing take
    /// [`borax_sources::pace::DEFAULT_CONCURRENCY`] and
    /// [`borax_sources::pace::DEFAULT_MIN_INTERVAL`], extraction takes
    /// [`borax_pdf::tiered::DEFAULT_PAGE_LIMIT`], collisions are
    /// suffixed, duplicate entries skipped, and the cache, the ledger
    /// and the run log are on.
    fn default() -> Config {
        Config {
            templates: BTreeMap::from([(
                "default".to_string(),
                "[auth:lower][year]_[shorttitle3:camel]".to_string(),
            )]),
            citation_keys: BTreeMap::from([(
                "default".to_string(),
                "[auth:lower][year]".to_string(),
            )]),
            tables: BTreeMap::new(),
            sources: SourceName::SUPPORTED.to_vec(),
            mailto: None,
            collection_root: None,
            concurrency: DEFAULT_CONCURRENCY,
            min_interval_ms: DEFAULT_MIN_INTERVAL.as_millis() as u64,
            page_limit: DEFAULT_PAGE_LIMIT,
            collision: CollisionPolicy::Suffix,
            bib_path: None,
            duplicates: DuplicatePolicy::Skip,
            sidecars: false,
            cache: true,
            ledger: true,
            run_log: true,
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
    #[serde(default, rename = "citation-keys")]
    pub citation_keys: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub tables: Option<BTreeMap<String, TableDeclaration>>,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    #[serde(default)]
    pub mailto: Option<String>,
    #[serde(default, rename = "collection-root")]
    pub collection_root: Option<PathBuf>,
    #[serde(default)]
    pub ledger: Option<bool>,
    #[serde(default, rename = "run-log")]
    pub run_log: Option<bool>,
    #[serde(default)]
    pub rename: Option<RenameLayer>,
    #[serde(default)]
    pub bib: Option<BibLayer>,
    #[serde(default)]
    pub extraction: Option<ExtractionLayer>,
    #[serde(default)]
    pub network: Option<NetworkLayer>,
}

/// One `[tables.<name>]` declaration: the file to read and which of
/// its columns mean what.
///
/// Named columns rather than positional ones, because the file is
/// maintained for other readers too and borax must not require a
/// column order of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableDeclaration {
    /// The file to read. A relative path is resolved against the
    /// directory of the configuration file that declared it, not the
    /// working directory, so a file may name a table beside itself.
    pub path: PathBuf,
    /// The column, or columns, supplying keys.
    pub key: KeyColumns,
    /// The column supplying values.
    pub value: String,
    /// Whether values are literal text or template fragments. Text
    /// unless the declaration says otherwise, so a value containing
    /// brackets is data until a file asks for it to be a template.
    #[serde(default)]
    pub values: ValueKindName,
}

/// The key columns of a declaration, written as one name or as a list
/// of them.
///
/// A single name is by far the common case and `key = "title"` is what
/// a person writes; the list exists so that one row can be reachable
/// by both its full title and its abbreviation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum KeyColumns {
    One(String),
    Many(Vec<String>),
}

impl KeyColumns {
    /// The columns named, in the order given.
    pub fn columns(&self) -> Vec<String> {
        match self {
            KeyColumns::One(column) => vec![column.clone()],
            KeyColumns::Many(columns) => columns.clone(),
        }
    }
}

/// How a declaration spells [`borax_core::tables::ValueKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueKindName {
    #[default]
    Text,
    Template,
}

impl ValueKindName {
    /// The kind this names.
    pub fn kind(self) -> ValueKind {
        match self {
            ValueKindName::Text => ValueKind::Text,
            ValueKindName::Template => ValueKind::Template,
        }
    }

    /// The word a configuration file writes for this kind, for
    /// `borax config`.
    pub fn as_str(self) -> &'static str {
        match self {
            ValueKindName::Text => "text",
            ValueKindName::Template => "template",
        }
    }
}

/// `declared` as the run should read it, given the configuration file
/// that declared it.
///
/// An absolute path is its own answer. A relative one is taken from
/// the directory holding `config_file`, so a configuration file and
/// the data file beside it travel together and a run started anywhere
/// reads the same table.
pub fn table_path(declared: &Path, config_file: &Path) -> PathBuf {
    todo!("table_path")
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
        for (entry_type, template) in &self.config.citation_keys {
            rendered.insert(format!("citation-keys.{entry_type}"), quote(template));
        }
        for (name, declaration) in &self.config.tables {
            rendered.insert(
                format!("tables.{name}.path"),
                quote(&declaration.path.to_string_lossy()),
            );
            rendered.insert(
                format!("tables.{name}.key"),
                match &declaration.key {
                    KeyColumns::One(column) => quote(column),
                    KeyColumns::Many(columns) => format!(
                        "[{}]",
                        columns
                            .iter()
                            .map(|column| quote(column))
                            .collect::<Vec<String>>()
                            .join(", ")
                    ),
                },
            );
            rendered.insert(format!("tables.{name}.value"), quote(&declaration.value));
            rendered.insert(
                format!("tables.{name}.values"),
                quote(declaration.values.as_str()),
            );
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
    /// A file sets a key that exists only as a command-line flag.
    ///
    /// Distinct from [`ConfigError::Unreadable`]'s unknown key: this is
    /// not a setting borax failed to recognise but one it recognises
    /// and refuses, so the message says where the flag belongs instead
    /// of listing the keys that were expected in its place.
    CommandLineOnly { path: PathBuf, key: String },
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
            ConfigError::CommandLineOnly { path, key } => write!(
                f,
                "{}: `{key}` must be passed on the command line and cannot be set in a \
                 configuration file",
                path.display()
            ),
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
/// `templates` and `citation-keys` are absent because their keys are
/// open-ended: they are merged and rendered per entry type instead, and
/// cannot be addressed by one environment variable.
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
        key: "collection-root",
        slot: |layer| Slot::Path(&mut layer.collection_root),
        render: |config| match &config.collection_root {
            Some(path) => quote(&path.to_string_lossy()),
            None => String::new(),
        },
    },
    Setting {
        key: "extraction.page-limit",
        slot: |layer| Slot::Count(&mut layer.extraction.get_or_insert_default().page_limit),
        render: |config| config.page_limit.to_string(),
    },
    Setting {
        key: "ledger",
        slot: |layer| Slot::Flag(&mut layer.ledger),
        render: |config| config.ledger.to_string(),
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
        key: "run-log",
        slot: |layer| Slot::Flag(&mut layer.run_log),
        render: |config| config.run_log.to_string(),
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

/// The keys one `[tables.<name>]` declaration is addressed by, for
/// origins and for `borax config`.
const TABLE_FIELDS: [&str; 4] = ["path", "key", "value", "values"];

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

/// The sources a configuration may name, as a quoted, comma-separated
/// list for an error message.
fn supported() -> String {
    SourceName::SUPPORTED
        .iter()
        .map(|source| format!("{:?}", source.as_str()))
        .collect::<Vec<String>>()
        .join(", ")
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
    toml::from_str(text).map_err(|error| match command_line_only_key(text) {
        Some(key) => ConfigError::CommandLineOnly {
            path: path.to_path_buf(),
            key: key.to_string(),
        },
        None => ConfigError::Unreadable {
            path: path.to_path_buf(),
            message: error.to_string(),
        },
    })
}

/// The flags a configuration file may not set, whatever value it gives
/// them.
///
/// `apply` is the whole of it. What it selects is destructive and
/// meant to be chosen once, for one invocation: a file that could turn
/// it on would make every run beneath that file a live one, and the
/// preview a user gets by not having asked to apply would be gone
/// without their having said anything.
const COMMAND_LINE_ONLY: &[&str] = &["apply"];

/// The [`COMMAND_LINE_ONLY`] flag `text` sets, if it sets one.
///
/// Consulted only after `text` has already failed to deserialize, so
/// the second parse costs nothing to a file that was going to load.
/// The top level and every table under it are searched, because
/// `[rename]` is where a reader who found `collision` there would look
/// for `apply` next, and being refused for the right reason should not
/// depend on having guessed the right table.
fn command_line_only_key(text: &str) -> Option<&'static str> {
    let document: toml::Table = text.parse().ok()?;
    std::iter::once(&document)
        .chain(document.values().filter_map(toml::Value::as_table))
        .find_map(|table| {
            COMMAND_LINE_ONLY
                .iter()
                .copied()
                .find(|key| table.contains_key(*key))
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
/// comma-separated list, booleans are `true`/`false`, and `templates`
/// and `citation-keys` cannot be set this way (their keys are
/// open-ended).
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
/// The `templates` and `citation-keys` tables merge per entry type for
/// the same reason — an override file defining `thesis` keeps the
/// global `default` — and merge separately from each other, so a layer
/// that renames files differently cites works the same way.
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
        .chain(
            config
                .citation_keys
                .keys()
                .map(|entry_type| (format!("citation-keys.{entry_type}"), Origin::Default)),
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

        for (entry_type, template) in layer.citation_keys.take().unwrap_or_default() {
            origins.insert(format!("citation-keys.{entry_type}"), origin.clone());
            config.citation_keys.insert(entry_type, template);
        }

        for (name, declaration) in layer.tables.take().unwrap_or_default() {
            for field in TABLE_FIELDS {
                origins.insert(format!("tables.{name}.{field}"), origin.clone());
            }
            config.tables.insert(name, declaration);
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
    if let Some(collection_root) = winning.collection_root {
        config.collection_root = Some(collection_root);
    }
    if let Some(ledger) = winning.ledger {
        config.ledger = ledger;
    }
    if let Some(run_log) = winning.run_log {
        config.run_log = run_log;
    }
    if let Some(names) = winning.sources {
        let mut sources = Vec::with_capacity(names.len());
        for name in names {
            // A source borax names but cannot query is refused rather
            // than accepted and then dropped when the clients are built:
            // a run configured to ask only that one would ask nobody and
            // report every file unresolvable, which looks like the
            // service having no record.
            match SourceName::parse(&name).filter(SourceName::is_supported) {
                Some(source) => sources.push(source),
                None => {
                    return Err(invalid(
                        "sources",
                        format!("unknown source {name:?}, expected one of {}", supported()),
                    ));
                }
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

/// The directory anchoring the collection a run in `start` belongs to.
///
/// `configured` is the `collection-root` setting, and when it is set it
/// is the answer outright: the search is replaced, not merely
/// outranked, so an unusual layout can put the accounting somewhere no
/// override file sits. Otherwise the root is the directory holding the
/// nearest [`OVERRIDE_FILE`] at or above `start` — one mechanism
/// serving both roles, so the file that configures a tree also says
/// where the tree's `.borax/` lives. `exists` answers as it does for
/// [`nearest_override`].
///
/// `None` when nothing is configured and no override file is found up
/// to the filesystem root, which is a run outside any collection: no
/// ledger is read or written for it.
pub fn collection_root(
    start: &Path,
    configured: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    match configured {
        Some(configured) => Some(configured.to_path_buf()),
        None => Some(nearest_override(start, exists)?.parent()?.to_path_buf()),
    }
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
