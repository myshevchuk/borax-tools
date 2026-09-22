//! What a run says about itself.
//!
//! A run produces one stream of events, and the two output modes are
//! two renderings of it: `--json` writes each event as a JSON object on
//! its own line, and the default mode writes the same events as prose.
//! Neither mode learns anything the other does not, so a script and a
//! person are told the same story.
//!
//! Diagnostics are deliberately not events. Progress, warnings, and
//! failures that are about the run rather than about a file go to
//! stderr as [`Diagnostic`]s, which is what keeps stdout parseable line
//! by line.

use std::fmt;
use std::path::PathBuf;

use borax_core::content::ContentHash;
use borax_core::library::DuplicateReason;
use borax_core::record::Record;
use serde::{Deserialize, Serialize};

/// The version of the event schema, emitted on every JSON line.
///
/// Consumers pin it: within a major version of borax the shape of an
/// event with a given `event` tag does not change, and a new schema
/// version is how a breaking change announces itself.
pub const SCHEMA: u32 = 3;

/// Something that happened to a file, or to the run as a whole.
///
/// Serialized with an `event` tag naming the variant in kebab-case, so
/// a consumer dispatches on one field and can ignore variants it does
/// not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    /// The run is starting. `applying` distinguishes a preview from a
    /// run that will touch the filesystem.
    RunStarted {
        command: String,
        version: String,
        applying: bool,
        /// Whether the run puts its decisions to the operator. An
        /// interactive run also reports `applying`, since it may move
        /// files; the two together say a session decided them one at a
        /// time.
        interactive: bool,
        /// The external lookup tables the run loaded, in name order.
        /// Rendering is deterministic given a table, but a table is a
        /// file that changes, so a run log without this says what a run
        /// did and not why it did it.
        tables: Vec<TableUsed>,
    },
    /// A lookup found no row. Reported once per distinct table and
    /// input however many files hit it, because what it tells the user
    /// is which line to add to their file, and that is one line however
    /// many documents wanted it.
    LookupMissed { table: String, input: String },
    /// A file's identifier was found and a record fetched for it.
    Resolved {
        path: PathBuf,
        identifier: String,
        /// The whole canonical record, so a consumer of the JSON stream
        /// has what the run resolved rather than only what it looked up.
        /// `borax resolve` exists to emit records; an identifier alone
        /// would send a caller back to the network for what borax
        /// already held.
        ///
        /// Boxed because events accumulate in a `Vec` for the whole
        /// run, where every event pays the size of the largest variant.
        record: Box<Record>,
        source: String,
        /// The identifier the run looked up, in the form the stream
        /// writes one — `doi:…`, `arXiv:…`. Not always `identifier`,
        /// which is what the record is filed under: a file resolved
        /// from an arXiv identifier may come back with a DOI, and only
        /// what was looked up is evidence about the file.
        found: String,
        /// Every title the file claims for itself, in the order they
        /// were read. Empty when the file was not opened, which is
        /// what a content-index answer means.
        claims: Vec<Claim>,
        /// Which extraction pass supplied the identifier, `supplied`
        /// when the operator named it, or `None` when neither a file
        /// nor an operator did — a content-index answer.
        tier: Option<String>,
        /// The conflict the operator accepted to reach this record, or
        /// `None` when nothing was overridden. A record that cleared
        /// the conflict check on its own and one accepted despite it
        /// are the same record; only this says which happened.
        overrode: Option<Overridden>,
        /// Whether the content index answered, so the file was neither
        /// opened nor looked up. A response cache hit behind a source
        /// is not visible here and reports `false`.
        cached: bool,
    },
    /// A rename that would happen. Emitted by a preview run only; an
    /// applying run emits [`Event::Renamed`] instead, so no file is
    /// ever reported twice in one run.
    Planned { path: PathBuf, target: PathBuf },
    /// A rename that did happen, with the content hash of the file
    /// that moved.
    ///
    /// The hash is what identifies the file the move was about,
    /// independently of the name it now carries: a reader of the run
    /// log can tell whether what sits at `target` is still what this
    /// event describes. It is required rather than optional because an
    /// applying rename refuses to move a file whose hash it does not
    /// know ([`SkipReason::Unrecordable`]), so a move that happened
    /// always has one.
    Renamed {
        path: PathBuf,
        target: PathBuf,
        hash: ContentHash,
    },
    /// A file the run declined to act on, and why.
    Skipped { path: PathBuf, reason: SkipReason },
    /// A file that already carries the name its record implies, or
    /// whose target is held by a byte-identical file.
    ///
    /// Not a skip: nothing was declined and nothing needs attention.
    /// The file is in the state the run exists to put it in, which is
    /// why it has an outcome of its own and why a run of nothing else
    /// succeeds.
    AlreadyNamed { path: PathBuf },
    /// An entry written to the master bibliography.
    BibEntry {
        path: PathBuf,
        key: String,
        outcome: String,
    },
    /// A sidecar written beside a resolved file.
    Sidecar { path: PathBuf, target: PathBuf },
    /// One setting of the effective configuration, with the layer that
    /// supplied it. Emitted by `borax config`, one per setting.
    ///
    /// `value` is rendered in TOML value syntax and `origin` in the
    /// wording [`crate::config::Origin`] displays, so a consumer reads
    /// the same two strings a person does.
    ConfigSetting {
        key: String,
        value: String,
        origin: String,
    },
    /// What the response cache holds. `bytes` is the size on disk of
    /// the entries counted.
    CacheStatus {
        root: PathBuf,
        entries: usize,
        bytes: u64,
    },
    /// What clearing the response cache removed.
    CacheCleared {
        root: PathBuf,
        entries: usize,
        bytes: u64,
    },
    /// What a library holds, counted from its tree and from its two
    /// stores.
    ///
    /// `identifiable` is `None` when the run was not asked for it: a
    /// count nobody asked for and a count of zero are different
    /// answers, and only `--identify` opens a document.
    LibraryStatus {
        root: PathBuf,
        artifacts: usize,
        items: usize,
        records: usize,
        orphans: usize,
        /// The nested libraries the walk stopped at, library-relative
        /// and in path order. A marker below the root takes its whole
        /// subtree out of every count above it, so the counts are only
        /// legible beside the list of what they leave out.
        nested: Vec<String>,
        identifiable: Option<usize>,
    },
    /// Something wrong with a library's own records, about the file at
    /// `path` and about no other.
    LibraryFinding { path: PathBuf, finding: Finding },
    /// What an applying run's admission of one file came to, beyond
    /// the record it wrote.
    ///
    /// An ordinary admission emits nothing. The file's own outcome —
    /// `renamed`, or `already-named` — is what says borax settled it,
    /// and the record written for it is the library's state rather
    /// than news. What is reported here is what a reader could not
    /// work out from the rest of the stream: a link the run moved, a
    /// file it settled outside the library and recorded nothing for,
    /// and a record it could not write.
    LibraryAdmission {
        /// The file the admission was about, at the path it holds
        /// after the run's work on it.
        path: PathBuf,
        admission: Admission,
    },
    /// What validating a library amounted to: how many findings were
    /// reported, and the three counts that are not findings.
    LibraryValidated {
        root: PathBuf,
        findings: usize,
        orphans: usize,
        /// Records whose last-known path holds no file. History the
        /// library deliberately keeps, not a malformed record.
        missing: usize,
        /// Items no artifact record links to. An ordinary item for a
        /// work the library holds no file for.
        unlinked: usize,
    },
    /// What a reconcile made of one artifact record.
    ///
    /// Only a record the run had something to do about produces one: a
    /// record its own path still holds is confirmed silently, so a
    /// reconcile over a library nothing has touched emits nothing
    /// between its first event and its last.
    LibraryRepair {
        /// The record this is about, by the identity it carries. A
        /// record outlives every path its artifact has had, so the
        /// identity is the one handle that follows it across runs.
        id: String,
        /// Where the artifact stands after the run, library-relative:
        /// the path repaired to for a repair, and the record's own
        /// last-known path for every other outcome.
        path: String,
        repair: Repair,
    },
    /// What reconciling a library amounted to.
    LibraryReconciled {
        root: PathBuf,
        /// How many records the artifact store holds.
        records: usize,
        /// Records whose artifact was where the record said it would
        /// be, whether the fast path settled it or the hash confirmed
        /// it after the fast path missed.
        confirmed: usize,
        /// Records whose last-known path was brought to an artifact
        /// found elsewhere in the library.
        repaired: usize,
        /// Records whose artifact was edited since borax last saw it,
        /// and whose new hash was appended.
        changed: usize,
        /// Records left exactly as they were because the match was
        /// ambiguous.
        ambiguous: usize,
        /// Records whose artifact is nowhere in the library.
        missing: usize,
        /// Artifacts the run hashed. The fast path exists to keep this
        /// far below the artifact count, so a reader of the stream can
        /// see whether it did its job — and a second pass over an
        /// untouched library reports zero.
        hashed: usize,
    },
    /// The run is over. Always the last event.
    RunFinished { counts: Counts },
}

/// Why a file was left alone.
///
/// Every variant is a decision borax made deliberately; nothing here is
/// a crash. Serialized with a `kind` tag, nested under the event's
/// `reason` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SkipReason {
    /// Neither extraction pass found an identifier in the file.
    NoIdentifier,
    /// An identifier was found but no source holds it. `attempts`
    /// records what each source said, in the order they were asked.
    ///
    /// `found` is the identifier that was looked up, in the form the
    /// stream writes one, and `tier` the pass that read it. Carried
    /// because they are the part of this skip worth acting on: a
    /// reader deciding what to do about the file, at the terminal or
    /// over the log, needs to know which identifier nobody held — and
    /// one read from the text layer is likelier to be a reference's
    /// than one read from the file's own metadata.
    Unresolvable {
        found: String,
        tier: Option<String>,
        attempts: Vec<Attempt>,
    },
    /// The file's own metadata disagrees with the resolved record, so
    /// the record is probably about a different work.
    ///
    /// `similarity` is how close the two were, from 0.0 to 1.0, and is
    /// always below the threshold that would have cleared them. It is
    /// reported so the skip can be judged: a value near the threshold
    /// says the identifier is probably right and the metadata merely
    /// differs, while one near zero says the file and the record are
    /// about different works.
    Conflict {
        field: String,
        extracted: String,
        resolved: String,
        similarity: f64,
    },
    /// The name the template produced is taken, and the collision
    /// policy is to skip.
    TargetTaken { target: PathBuf },
    /// The file could not be read as a PDF at all.
    Unreadable { message: String },
    /// The record resolved, but the template rendered an empty name
    /// from it — the record is too sparse to name a file.
    Unnameable,
    /// The operator was shown the move and answered that it should not
    /// happen. Only an interactive run reports this.
    Declined,
    /// The rename itself failed, after the plan said it would not.
    RenameFailed { message: String },
    /// Bibliography output for the file could not be written.
    BibWriteFailed { message: String },
    /// The record resolved, but the template rendered nothing a
    /// citation key can be made of.
    Unciteable,
    /// Something borax did not write already sits where the file's
    /// sidecar would go, so the sidecar was not written.
    SidecarTaken { target: PathBuf },
    /// The move could not be recorded identifiably, so it was not
    /// made. Every rename in an apply-run log names the content that
    /// moved, and a line that could not say which file it was about
    /// would be a move the log cannot account for afterwards.
    Unrecordable { message: String },
    /// The library already holds this file, by content or by work.
    /// `existing_path` is where the artifact record says the file it
    /// duplicates sits, as a full path rather than the
    /// library-relative one the record stores, so the report names
    /// somewhere the reader can go and look.
    Duplicate {
        reason: DuplicateReason,
        existing_path: PathBuf,
    },
    /// The move was not made because the run writes no record and
    /// making it would leave an artifact record unable to name its
    /// artifact again.
    ///
    /// Only a run with the record gate off reports this. The record
    /// named here names the file's current path and holds no hash of
    /// the bytes the file has now, so a move that wrote nothing
    /// afterwards would take away the one path it can be found by
    /// while leaving nothing in it that matches the file's content.
    ///
    /// `id` is the artifact identity, which is the record's file name
    /// under `.borax/artifacts/`. The remedy named is
    /// `borax reconcile --rehash`, which records the file's current
    /// bytes and lets the move proceed under either setting; re-running
    /// without the gate is not named, because it does not always work.
    Stranding { id: String },
}

/// Something wrong with a library's own records.
///
/// A finding is a report about a library and never a rejected write:
/// `borax validate` repairs nothing and refuses nothing, and every
/// writer — borax's own CLI, a file manager, a text editor — is judged
/// by the same list. An orphan, an artifact borax cannot find and an
/// item nothing links to are counts rather than findings, so none of
/// them is here.
///
/// Serialized with a `kind` tag, nested under the event's `finding`
/// field, as [`SkipReason`] is under `reason`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Finding {
    /// An artifact record naming an item the library does not hold.
    DanglingItem { item: String },
    /// Two files of one store carrying one identity. `other` is the
    /// file the identity was first read from, so the pair is legible
    /// from the finding alone.
    DuplicateIdentity { id: String, other: PathBuf },
    /// A file whose record carries an identity its own name does not,
    /// including a name carrying no identity at all. `id` is what the
    /// record says, which is the authoritative one.
    NameDisagrees { id: String },
    /// An artifact record whose last-known path is not a
    /// library-relative path under the root.
    PathNotRelative { path: String },
    /// An artifact record with no hash history at all, which is a
    /// record that is evidence about nothing.
    EmptyHistory,
    /// An artifact record holding a hash that is not one borax writes.
    MalformedHash { hash: String },
    /// A history entry naming no run, so what it records cannot be
    /// attributed to anything the library did.
    HistoryEntryWithoutRun { hash: String },
    /// A file of one of the stores that does not parse as the record
    /// it claims to be, including an item whose verbatim source fields
    /// do not parse as JSON. One such file is a finding about itself
    /// and about no other.
    Unreadable { message: String },
}

/// What a reconcile made of one artifact record.
///
/// Every variant is about a record the run did not simply confirm:
/// what it repaired, what it found changed, and what it deliberately
/// left alone. Serialized with a `kind` tag, nested under the event's
/// `repair` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Repair {
    /// The artifact was found elsewhere in the library and the record's
    /// last-known path was brought to it. `from` is the path the record
    /// named, library-relative, so the move is legible from the event
    /// alone.
    Repaired { from: String },
    /// The artifact at the record's own path has bytes no hash in its
    /// history held: it was edited since borax last saw it, and `hash`
    /// was appended after the hashes already recorded.
    Changed { hash: String },
    /// The record matched more than one unclaimed artifact, or its one
    /// match is a match for another record too. Nothing was written:
    /// a stale path is repaired by the next pass, and an item link
    /// given to the wrong artifact is not detectable at all.
    /// `candidates` names what it matched, library-relative and in path
    /// order.
    Ambiguous { candidates: Vec<String> },
    /// The record's last-known path holds no file and no artifact in
    /// the library matches its history. The record is kept as it is,
    /// path included: a record outliving its artifact is the library
    /// saying it once held one.
    Missing,
    /// The repair was decided and the file could not be written, with
    /// `message` as the filesystem put it. The record is as it was, and
    /// the next reconcile decides the same thing again.
    Unwritten { message: String },
}

/// What an applying run's admission of one file came to, for the
/// admissions it has something to say about.
///
/// Every variant is about an admission that did not simply happen:
/// nothing is written for a file outside the library, nothing lands
/// when the store refuses the write, and a link only moves when the
/// operator re-identified the file. An admission that minted or
/// updated a record and left its link where it was produces no event
/// at all. Serialized with a `kind` tag, nested under the event's
/// `admission` field, as [`Repair`] is under `repair`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Admission {
    /// The operator re-identified a file that already had an artifact
    /// record, so the record was re-linked to the item for the record
    /// they settled it on.
    ///
    /// `id` is the artifact's own identity, which the re-link leaves
    /// alone; `from` is the item the record named before and `to` the
    /// item it names now. Both are named because the item left behind
    /// may now have nothing linking to it, which is an ordinary
    /// library state `borax validate` counts.
    Relinked {
        id: String,
        from: String,
        to: String,
    },
    /// The file the run settled is not in the library, so nothing was
    /// recorded for it.
    ///
    /// borax brings no file into a library: a file named as input from
    /// outside the tree is renamed where it sits, and so is one inside
    /// a subtree this library does not own — its own state directory,
    /// its item store, or a nested library. The rename stands and is
    /// reported; what this adds is that no artifact record and no item
    /// were written for it.
    Outside,
    /// The record could not be written, with `message` as the
    /// filesystem put it.
    ///
    /// The rename stands and is reported: a write to the store costs
    /// the file its record and never its rename. borax retries
    /// nothing within a run, and the next applying run over the file
    /// records it again.
    Unwritten { message: String },
}

/// A title a file claims for itself, and where it was read.
///
/// Claims are reported as the file makes them, including one the
/// conflict check dismissed as a producer's leftover: the check is a
/// heuristic, and a person reading the two titles is better placed to
/// judge which is the work's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// Where the title was read: the XMP packet, or the document
    /// information dictionary.
    pub from: ClaimOrigin,
    pub title: String,
}

/// Where a [`Claim`] was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimOrigin {
    /// The XMP packet's `dc:title`.
    Xmp,
    /// The document information dictionary's `Title`.
    Info,
}

/// A conflict an operator accepted, reported on the record they
/// accepted it for.
///
/// The fields are the ones [`SkipReason::Conflict`] carries, and carry
/// them unchanged: a reader of the run log sees what was overridden in
/// the vocabulary the skip would have used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overridden {
    pub field: String,
    pub extracted: String,
    pub resolved: String,
    pub similarity: f64,
}

/// One source's answer during resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub source: String,
    pub error: String,
}

/// One external table a run read, identified well enough to explain a
/// name it produced after the file has been edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableUsed {
    /// The name templates address it by.
    pub name: String,
    /// The file it was read from, as the run resolved it.
    pub path: PathBuf,
    /// A digest of the bytes it was read with.
    pub digest: String,
}

/// What the run did, in totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub resolved: usize,
    pub renamed: usize,
    pub skipped: usize,
    /// Files already carrying the name their record implies.
    pub named: usize,
    /// Distinct lookups that found no row.
    pub unmatched: usize,
    /// Input files the run left without a fate: the file an interactive
    /// run was ended at and every file after it. Zero for a run that
    /// reached the end of its inputs.
    ///
    /// Not counted from an event, since no event is emitted for a file
    /// nothing happened to; the run sets it when it stops early.
    pub unreached: usize,

    /// Findings reported about the library's own records. A run
    /// reporting any ends in partial success, on the same terms as one
    /// that skipped a file: something in what the run was asked about
    /// needs attention.
    pub findings: usize,
}

impl Counts {
    /// Fold `event` into the totals.
    ///
    /// Counts what happened, not what was planned: a preview run
    /// renames nothing and leaves `renamed` at zero however many moves
    /// it described. An event that is about the run rather than about a
    /// file changes nothing.
    pub fn observe(&mut self, event: &Event) {
        match event {
            Event::Resolved { .. } => self.resolved += 1,
            Event::Renamed { .. } => self.renamed += 1,
            Event::Skipped { .. } => self.skipped += 1,
            Event::AlreadyNamed { .. } => self.named += 1,
            Event::LookupMissed { .. } => self.unmatched += 1,
            Event::LibraryFinding { .. } => self.findings += 1,
            _ => {}
        }
    }
}

/// Which rendering of the event stream to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Prose for a person.
    Human,
    /// One JSON object per line.
    Json,
}

/// `event` rendered in `format`, or `None` when this format has nothing
/// to say about this event.
///
/// Only the human format is ever silent — [`Format::Json`] renders
/// every event, because a consumer that has to reconstruct a run cannot
/// do it from a stream with holes in it.
pub fn render(format: Format, event: &Event) -> Option<String> {
    match format {
        Format::Human => human_line(event),
        Format::Json => Some(json_line(event)),
    }
}

/// `event` as a single line of JSON.
///
/// The object carries `schema` set to [`SCHEMA`] alongside the event's
/// own fields, and contains no newline, so a consumer may split the
/// stream on `\n` before parsing.
pub fn json_line(event: &Event) -> String {
    serde_json::to_string(&Line {
        schema: SCHEMA,
        event,
    })
    .unwrap_or_else(|error| unrenderable(&error.to_string()))
}

/// One line of the JSON stream: the schema version, then the event's own
/// fields hoisted to the same level.
#[derive(Serialize)]
struct Line<'a> {
    schema: u32,
    #[serde(flatten)]
    event: &'a Event,
}

/// The stand-in line for an event that would not serialize.
///
/// A caller writing the stream has nowhere to put an error — losing the
/// line entirely would leave a consumer silently short one event, so the
/// failure is reported in the stream's own shape, under an `event` tag
/// no real event uses.
#[derive(Serialize)]
struct Unrenderable<'a> {
    schema: u32,
    event: &'static str,
    message: &'a str,
}

fn unrenderable(message: &str) -> String {
    serde_json::to_string(&Unrenderable {
        schema: SCHEMA,
        event: "unrenderable",
        message,
    })
    .unwrap_or_else(|_| format!(r#"{{"schema":{SCHEMA},"event":"unrenderable"}}"#))
}

/// `event` as a line of prose, or `None` when the event is not worth
/// saying aloud.
///
/// [`Event::RunStarted`] is the only silent one: a person watching a
/// rename wants the renames, not a restatement of the command they
/// just typed. The JSON stream keeps it regardless.
pub fn human_line(event: &Event) -> Option<String> {
    match event {
        Event::RunStarted { .. } => None,
        Event::Resolved {
            path,
            identifier,
            source,
            cached,
            ..
        } => Some(format!(
            "{}: resolved {identifier} via {source}{}",
            path.display(),
            if *cached { " (cached)" } else { "" }
        )),
        Event::Planned { path, target } => Some(format!(
            "{}: would rename to {}",
            path.display(),
            target.display()
        )),
        Event::Renamed { path, target, .. } => Some(format!(
            "{}: renamed to {}",
            path.display(),
            target.display()
        )),
        Event::Skipped { path, reason } => Some(format!(
            "{}: skipped, {}",
            path.display(),
            skipped_because(reason)
        )),
        Event::AlreadyNamed { path } => Some(format!("{}: already named", path.display())),
        Event::BibEntry { path, key, outcome } => Some(format!(
            "{}: bibliography entry {key} {outcome}",
            path.display()
        )),
        Event::Sidecar { path, target } => Some(format!(
            "{}: sidecar written to {}",
            path.display(),
            target.display()
        )),
        Event::ConfigSetting { key, value, origin } => Some(format!("{key} = {value}  # {origin}")),
        Event::CacheStatus {
            root,
            entries,
            bytes,
        } => Some(format!(
            "{}: {entries} entries, {bytes} bytes",
            root.display()
        )),
        Event::CacheCleared {
            root,
            entries,
            bytes,
        } => Some(format!(
            "{}: cleared {entries} entries, {bytes} bytes",
            root.display()
        )),
        Event::LookupMissed { table, input } => Some(format!("{table}: no row for {input:?}")),
        Event::LibraryStatus {
            root,
            artifacts,
            items,
            records,
            orphans,
            nested,
            identifiable,
        } => Some(format!(
            "{}: {artifacts} artifacts, {items} items, {records} records, {orphans} orphans{}{}",
            root.display(),
            match identifiable {
                None => String::new(),
                Some(identifiable) => format!(", {identifiable} identifiable"),
            },
            match nested.is_empty() {
                true => String::new(),
                false => format!(" (excluding nested libraries: {})", nested.join(", ")),
            }
        )),
        Event::LibraryFinding { path, finding } => {
            Some(format!("{}: {}", path.display(), what_is_wrong(finding)))
        }
        Event::LibraryValidated {
            root,
            findings,
            orphans,
            missing,
            unlinked,
        } => Some(format!(
            "{}: {findings} findings, {orphans} orphans, {missing} missing, {unlinked} unlinked",
            root.display()
        )),
        Event::LibraryRepair { path, repair, .. } => {
            Some(format!("{path}: {}", what_was_repaired(repair)))
        }
        Event::LibraryAdmission { path, admission } => Some(format!(
            "{}: {}",
            path.display(),
            what_was_admitted(admission)
        )),
        Event::LibraryReconciled {
            root,
            records,
            confirmed,
            repaired,
            changed,
            ambiguous,
            missing,
            hashed,
        } => Some(format!(
            "{}: {records} records, {confirmed} confirmed, {repaired} repaired, \
             {changed} changed, {ambiguous} ambiguous, {missing} missing, \
             {hashed} hashed",
            root.display()
        )),
        Event::RunFinished { counts } => Some(human_summary(counts, 0)),
    }
}

/// What a run amounts to, as the closing line of a human rendering.
///
/// `hidden` is how many already-named files the run passed over without
/// a line of their own, which only an interactive run does. Saying so
/// here is what keeps a terminal that showed less than the stream held
/// honest about it.
///
/// A zero count of unmatched lookups is left out rather than written as
/// a zero: the JSON summary carries it either way, and a run that
/// looked nothing up has nothing to say about tables it never
/// consulted. The same goes for files already named, renames not
/// reached, files passed over, and findings about a library the run
/// never validated.
pub fn human_summary(counts: &Counts, hidden: usize) -> String {
    format!(
        "{} resolved, {} renamed, {} skipped{}{}{}{}",
        counts.resolved,
        counts.renamed,
        counts.skipped,
        match (counts.named, hidden) {
            (0, _) => String::new(),
            (named, 0) => format!(", {named} already named"),
            (named, hidden) if hidden >= named => format!(", {named} already named (not shown)"),
            (named, hidden) => format!(", {named} already named ({hidden} not shown)"),
        },
        match counts.unmatched {
            0 => String::new(),
            unmatched => format!(", {unmatched} unmatched"),
        },
        match counts.unreached {
            0 => String::new(),
            unreached => format!(", {unreached} not reached"),
        },
        match counts.findings {
            0 => String::new(),
            findings => format!(", {findings} findings"),
        }
    )
}

/// The clause following `skipped,` in a human line.
fn skipped_because(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NoIdentifier => "no identifier found".to_string(),
        SkipReason::Unresolvable {
            found, attempts, ..
        } => {
            let said: Vec<String> = attempts
                .iter()
                .map(|attempt| format!("{}: {}", attempt.source, attempt.error))
                .collect();
            match said.is_empty() {
                true => format!("no source had a record for {found}"),
                false => format!("no source had a record for {found} ({})", said.join("; ")),
            }
        }
        SkipReason::Conflict {
            field,
            extracted,
            resolved,
            similarity,
        } => format!(
            "{field} disagrees {}% (file says {extracted}, record says {resolved})",
            (similarity * 100.0).round()
        ),
        SkipReason::TargetTaken { target } => format!("{} is taken", target.display()),
        SkipReason::Unreadable { message } => format!("unreadable ({message})"),
        SkipReason::Unnameable => "the record renders an empty name".to_string(),
        SkipReason::Declined => "declined".to_string(),
        SkipReason::RenameFailed { message } => format!("rename failed ({message})"),
        SkipReason::BibWriteFailed { message } => {
            format!("bibliography output failed ({message})")
        }
        SkipReason::Unciteable => "the record renders an empty citation key".to_string(),
        SkipReason::SidecarTaken { target } => {
            format!("{} exists and borax did not write it", target.display())
        }
        SkipReason::Unrecordable { message } => {
            format!("the move could not be recorded, so it was not made ({message})")
        }
        SkipReason::Duplicate {
            reason: DuplicateReason::Content,
            existing_path,
        } => format!("same bytes already archived at {}", existing_path.display()),
        SkipReason::Duplicate {
            reason: DuplicateReason::Work,
            existing_path,
        } => format!(
            "same work already archived at {} (different file)",
            existing_path.display()
        ),
        SkipReason::Stranding { id } => format!(
            "artifact record {id} holds no hash of these bytes, and this run writes none, so \
             moving it would strand the record; borax reconcile --rehash records them"
        ),
    }
}

/// What `finding` says is wrong with the file, as the clause following
/// that file's name in a human rendering.
///
/// The record's own reading of an identity is given rather than the
/// name's: a file whose name and contents disagree is named by the
/// line already, and what the record says is the authoritative half.
fn what_is_wrong(finding: &Finding) -> String {
    match finding {
        Finding::DanglingItem { item } => {
            format!("names item {item}, which the library has none of")
        }
        Finding::DuplicateIdentity { id, other } => format!(
            "carries identity {id}, which {} carries too",
            other.display()
        ),
        Finding::NameDisagrees { id } => format!("carries identity {id}, which its name does not"),
        Finding::PathNotRelative { path } => {
            format!("records {path:?}, which is not a library-relative path")
        }
        Finding::EmptyHistory => "has no hash history, so it is evidence about nothing".to_string(),
        Finding::MalformedHash { hash } => {
            format!("holds {hash:?}, which is not a hash borax writes")
        }
        Finding::HistoryEntryWithoutRun { hash } => {
            format!("records {hash} against no run")
        }
        Finding::Unreadable { message } => format!("unreadable ({message})"),
    }
}

/// `repair` as the end of a sentence whose subject is the artifact the
/// record is about, for the human rendering of
/// [`Event::LibraryRepair`].
fn what_was_repaired(repair: &Repair) -> String {
    match repair {
        Repair::Repaired { from } => format!("was recorded at {from:?} and is here now"),
        Repair::Changed { hash } => {
            format!("has changed since borax last saw it; {hash} recorded")
        }
        Repair::Ambiguous { candidates } => format!(
            "could be any of {}, so nothing was changed",
            candidates.join(", ")
        ),
        Repair::Missing => "is recorded and is nowhere in the library".to_string(),
        Repair::Unwritten { message } => format!("could not be put right ({message})"),
    }
}

/// `admission` as the clause following the file's name in the human
/// rendering of [`Event::LibraryAdmission`].
fn what_was_admitted(admission: &Admission) -> String {
    match admission {
        Admission::Relinked { id, from, to } => {
            format!("artifact {id} moved from item {from} to item {to}")
        }
        Admission::Outside => "outside the library, so nothing was recorded".to_string(),
        Admission::Unwritten { message } => {
            format!("renamed but not recorded ({message})")
        }
    }
}

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Warning,
    Error,
}

/// A message about the run rather than about a file.
///
/// Diagnostics go to stderr in both output formats and are never part
/// of the event stream, so `--json` stdout stays parseable even when a
/// run has plenty to complain about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    /// `warning: <message>` or `error: <message>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.level {
            Level::Warning => write!(f, "warning: {}", self.message),
            Level::Error => write!(f, "error: {}", self.message),
        }
    }
}
