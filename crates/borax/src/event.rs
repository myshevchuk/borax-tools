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
use borax_core::record::{Record, Source as FieldSource};
use serde::{Deserialize, Serialize};

use crate::describe::escaped;

/// The version of the event schema, emitted on every JSON line.
///
/// Consumers pin it: within a major version of borax the shape of an
/// event with a given `event` tag does not change, and a new schema
/// version is how a breaking change announces itself.
pub const SCHEMA: u32 = 4;

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
    /// A file's resolution: the record it reached, and what every step
    /// of reaching it found out.
    Resolved {
        path: PathBuf,
        /// The record's preferred identifier ([`Record`]'s DOI, else its
        /// arXiv id, PMID or ISBN) in the form the stream writes one
        /// (`doi:…`, `arXiv:…`), or empty when the record carries none.
        /// What was looked up, which can differ, is in the `lookup`
        /// section.
        identifier: String,
        /// The whole canonical record, so a consumer of the JSON stream
        /// has what the run resolved rather than only what it looked up.
        /// `borax resolve` exists to emit records; an identifier alone
        /// would send a caller back to the network for what borax
        /// already held. Who wrote each field is in
        /// `record.borax.provenance`.
        ///
        /// Boxed because events accumulate in a `Vec` for the whole
        /// run, where every event pays the size of the largest variant.
        record: Box<Record>,
        /// The resolution's evidence, one section per step, written as
        /// eight keys beside `record`.
        #[serde(flatten)]
        sections: Box<Sections>,
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
    Skipped {
        path: PathBuf,
        reason: SkipReason,
        /// The resolution's evidence, written as eight keys beside
        /// `reason`: `Some` exactly when the skip is the file's
        /// resolution verdict ([`SkipReason::is_resolution_verdict`]),
        /// and absent from the line otherwise. Its absence is how a
        /// consumer tells a skip made after the file resolved.
        #[serde(flatten)]
        sections: Option<Box<Sections>>,
        /// The record the title check refused: `Some` exactly when
        /// `reason` is [`SkipReason::Conflict`], and absent from the line
        /// otherwise. `record_retrieval` says where it came from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate: Option<Box<Record>>,
    },
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
    /// What extraction made of one artifact a `status --identify` run
    /// inspected.
    ///
    /// One per surveyed artifact, in survey order, each written once
    /// that artifact's extraction is done and all of them before the
    /// run's [`Event::LibraryStatus`]. Neither a skip nor a finding: it
    /// describes a file, and counts toward no total of the run's.
    LibraryExtraction {
        /// The artifact, library-relative and `/`-separated, as
        /// [`Event::LibraryAdoption`] names one.
        path: String,
        extraction: Extraction,
    },
    /// What a library holds, counted from its tree and from its two
    /// stores.
    ///
    /// `identifiable` is `None` when the run was not asked for it: a
    /// count nobody asked for and a count of zero are different
    /// answers, and only `--identify` opens a document. When it was
    /// asked for, it is the number of the run's
    /// [`Event::LibraryExtraction`] events whose result
    /// [`Extraction::is_found`].
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
    /// A library condition a run counted, named: an artifact no record
    /// names, a record whose artifact the library does not have, or an
    /// item no record links to.
    ///
    /// One per object behind the `orphans` count of
    /// [`Event::LibraryStatus`], and behind the `orphans`, `missing` and
    /// `unlinked` counts of [`Event::LibraryValidated`], all of them
    /// before that totals event. Neither a skip nor a finding: a
    /// condition is an ordinary library state, and counts toward no
    /// total of the run's.
    LibraryCondition {
        /// Library-relative and `/`-separated: the artifact for an
        /// orphan, the record's last-known path as recorded for a
        /// missing record, and the item file for an unlinked item.
        path: String,
        condition: Condition,
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
    /// What became of writing a record an operator reached — supplied,
    /// asked for again, or accepted over a conflict — to the content
    /// index, once its file was moved.
    ///
    /// Follows the file's [`Event::Renamed`] and any
    /// [`Event::LibraryAdmission`], and is emitted for no other file:
    /// the resolution's own `content_index.write` reads
    /// `awaiting-acceptance` for exactly these. `write` is
    /// [`WriteStep::Written`] or [`WriteStep::Failed`]; a failure is
    /// evidence and never a failure of the rename. Counts toward no
    /// total.
    ContentIndexWrite {
        /// The file, at the path it holds after the move.
        path: PathBuf,
        write: WriteStep,
    },
    /// What validating a library amounted to: how many findings were
    /// reported, and the three counts that are not findings.
    ///
    /// Each of `orphans`, `missing` and `unlinked` is the number of the
    /// run's [`Event::LibraryCondition`] events of the matching kind.
    LibraryValidated {
        root: PathBuf,
        findings: usize,
        orphans: usize,
        /// Records whose last-known path holds no artifact of this
        /// library. History the library deliberately keeps, not a
        /// malformed record.
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
    /// What an adoption made of one orphan.
    ///
    /// Every orphan the run found produces exactly one, so the count of
    /// orphans in [`Event::LibraryAdopted`] is the number of these
    /// whose adoption is not [`Adoption::Recorded`].
    LibraryAdoption {
        /// The orphan, library-relative, as [`Event::LibraryRepair`]
        /// names an artifact.
        path: String,
        adoption: Adoption,
    },
    /// What adopting into a library amounted to. Always the last event
    /// of an adoption before the run's own.
    LibraryAdopted {
        root: PathBuf,
        /// Orphans the run gave an artifact record.
        adopted: usize,
        /// Orphans left after the run: those the content index had no
        /// record for, and those the run could not adopt.
        orphans: usize,
    },
    /// The run is over. Always the last event.
    RunFinished { counts: Counts },
}

/// Why a file was left alone.
///
/// Every variant is a decision borax made deliberately; nothing here is
/// a crash. Serialized with a `kind` tag, nested under the event's
/// `reason` field.
///
/// The first six variants and [`SkipReason::Duplicate`] are resolution
/// verdicts ([`SkipReason::is_resolution_verdict`]): the facts behind
/// them are in the event's sections, so the reason names the cause and
/// carries no detail beyond an unreadable file's message and a
/// duplicate's two fields. Every other variant is a skip made after the
/// file resolved, and carries what it needs itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SkipReason {
    /// Extraction found no identifier, and no page it read held text.
    NoTextLayer,
    /// Extraction found no identifier in the file's metadata or in the
    /// text its pages held.
    TextWithoutIdentifier,
    /// The file cannot be read without a password.
    Encrypted,
    /// The file could not be opened or parsed as a PDF, with `message`
    /// as the reader gave it.
    Unreadable { message: String },
    /// An identifier was looked up and no service supplied a record for
    /// it, or no configured service could be asked about it. The
    /// `lookup` section says which, and what each service answered.
    Unresolvable,
    /// The file's own titles name a different work from the record its
    /// identifier reached, so the record was refused. The `match_check`
    /// section holds the two titles and how alike they were, and the
    /// event's `candidate` holds the refused record.
    Conflict,
    /// The name the template produced is taken, and the collision
    /// policy is to skip.
    TargetTaken { target: PathBuf },
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
    /// The library already holds this file, by content or by work: a
    /// resolution verdict, whose sections are not attempted for a
    /// content duplicate and are the evidence of the record the file
    /// resolved to for a work duplicate. `existing_path` is where the artifact record says the file it
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

impl SkipReason {
    /// Whether this skip is a file's resolution verdict, and so carries
    /// the resolution's sections: the four extraction failures,
    /// [`SkipReason::Unresolvable`], [`SkipReason::Conflict`] and
    /// [`SkipReason::Duplicate`] of either reason.
    pub fn is_resolution_verdict(&self) -> bool {
        match self {
            SkipReason::NoTextLayer
            | SkipReason::TextWithoutIdentifier
            | SkipReason::Encrypted
            | SkipReason::Unreadable { .. }
            | SkipReason::Unresolvable
            | SkipReason::Conflict
            | SkipReason::Duplicate { .. } => true,
            SkipReason::TargetTaken { .. }
            | SkipReason::Unnameable
            | SkipReason::Declined
            | SkipReason::RenameFailed { .. }
            | SkipReason::BibWriteFailed { .. }
            | SkipReason::Unciteable
            | SkipReason::SidecarTaken { .. }
            | SkipReason::Unrecordable { .. }
            | SkipReason::Stranding { .. } => false,
        }
    }
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
    /// A file of one of the stores that cannot be read or does not
    /// parse as the record it claims to be, including an item whose
    /// verbatim source fields do not parse as JSON, or a store
    /// directory that cannot be listed. One such file is a finding
    /// about itself and about no other; a directory is a finding about
    /// every record it would have listed.
    Unreadable { message: String },
}

/// What a library said about one file a run resolved.
///
/// [`LibraryAnswer::Tracked`] is the library answering for the file,
/// and [`LibraryAnswer::Untracked`] is the library having no record of
/// it. Every other variant is a library that could not say which: the
/// file was resolved as if untracked, and the variant says why the
/// library could not answer, with the evidence a reader needs to go
/// and look.
///
/// Identities are canonical UUID text. Lists are in the order the
/// stores were read, and paths are full paths. Serialized with a `kind`
/// tag, nested under the event's `library` field, as [`SkipReason`] is
/// under `reason`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LibraryAnswer {
    /// The artifact record `artifact` names the file's path and holds
    /// its bytes, and links the item `item`, which exactly one item
    /// file carries. The item's record is the file's.
    Tracked { artifact: String, item: String },
    /// No artifact record names the file's path, and the artifact store
    /// was read whole.
    Untracked,
    /// The records naming the file's path hold none of the file's
    /// bytes in their histories: the file changed since the library
    /// last recorded it.
    UnrecognisedContent { artifacts: Vec<String> },
    /// Several records naming the file's path hold its bytes.
    Ambiguous { artifacts: Vec<String> },
    /// The one record holding the file links no item.
    NoItem { artifact: String },
    /// The one record holding the file links an item the item store
    /// does not hold.
    DanglingItem { artifact: String, item: String },
    /// The one record holding the file links an item the item store
    /// does not hold, and `path` could not be read: an item file whose
    /// name claims that item, or the item store's directory itself.
    /// `message` is the reader's.
    UnreadableItem {
        artifact: String,
        item: String,
        path: PathBuf,
        message: String,
    },
    /// The one record holding the file links an item several item
    /// files carry: `files`.
    AmbiguousItem {
        artifact: String,
        item: String,
        files: Vec<PathBuf>,
    },
    /// Records name the file's path, and the file could not be hashed,
    /// so none of them can be confirmed as its record.
    Unhashable { artifacts: Vec<String> },
    /// No readable record names the file's path, and the artifact store
    /// has files it could not read, one of which might. `listed` is
    /// `false` when the store directory could not be listed at all, and
    /// `unreadable` counts the record files that could not be read, 0
    /// when `listed` is `false`.
    UnreadableRecords { listed: bool, unreadable: usize },
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

/// What an adoption made of one orphan.
///
/// Serialized with a `kind` tag, nested under the event's `adoption`
/// field, as [`Repair`] is under `repair`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Adoption {
    /// The orphan now has an artifact record, `id`, linked to the item
    /// `item`: one the library already held for an identifier of the
    /// cached record, or one minted from that record.
    Recorded { id: String, item: String },
    /// The orphan's bytes are already in the history of the artifact
    /// record `id`, so it was left an orphan.
    ///
    /// Such a file is that record's artifact moved, or a copy of it,
    /// and a second record for the same bytes would leave `borax
    /// reconcile` unable to tell which is which. Reconciling first is
    /// what settles the first case; the second stays an orphan.
    Held { id: String },
    /// The orphan could not be read, with `message` as the filesystem
    /// put it, so the content index was never asked about it.
    Unreadable { message: String },
    /// The index answered and the record could not be written, with
    /// `message` as the filesystem put it. The orphan is still one, and
    /// the next adoption tries it again.
    Unwritten { message: String },
    /// The content index holds no record for the orphan's bytes, so
    /// there was nothing to adopt it from and it is still an orphan.
    Unindexed,
}

/// Which library condition a [`Event::LibraryCondition`] names.
///
/// Each kind is named after the count it adds to. Serialized with a
/// `kind` tag, nested under the event's `condition` field, as
/// [`Repair`] is under `repair`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Condition {
    /// An artifact no artifact record names by path.
    Orphan,
    /// The artifact record `id`, stored in the file `record`
    /// (library-relative, under `.borax/artifacts/`), names a path
    /// that holds no artifact of this library.
    ///
    /// A file standing there inside a nested library, the item store
    /// or `.borax/` does not count as one.
    Missing { id: String, record: String },
    /// The item `id` is linked to by no artifact record.
    Unlinked { id: String },
}

/// What extraction made of one file: the identifier and the pass that
/// found it, or the way extraction failed.
///
/// Serialized with a `kind` tag, nested under the event's `extraction`
/// field. No two failures share a variant: a file with no text to read
/// and a file whose text holds no identifier are different answers, as
/// are a file locked by a password and one that is not a PDF at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Extraction {
    /// A pass found an identifier. `identifier` is in the form the
    /// stream writes one (`doi:…`, `arXiv:…`), and `tier` names the
    /// pass that read it (`embedded-metadata` or `text-layer`).
    Found { identifier: String, tier: String },
    /// The metadata held no identifier, and no page the text pass read
    /// held text, including when it read none.
    NoTextLayer,
    /// The metadata held no identifier, and the text the pages held
    /// had none either.
    TextWithoutIdentifier,
    /// The document cannot be read without a password.
    Encrypted,
    /// The file could not be opened or parsed as a PDF, with `message`
    /// as the reader gave it.
    Unreadable { message: String },
}

impl Extraction {
    /// Whether an identifier was found: the results `identifiable`
    /// counts.
    pub fn is_found(&self) -> bool {
        matches!(self, Extraction::Found { .. })
    }
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

/// What every step of a file's resolution found out, as a resolution
/// event carries it: one key per step, in the order the steps run.
///
/// A step that did not run is never left out. It is
/// `{"status":"not-attempted","reason":R}` wherever it sits, with `R`
/// one of [`crate::evidence::Unattempted`]'s names.
///
/// Built from the engine's evidence by
/// [`crate::evidence::Evidence::sections`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sections {
    /// What the run's library said about the file, or why it was not
    /// asked.
    pub library: LibraryStep,
    /// What reading the content index came to, and what became of
    /// writing the record reached to it.
    pub content_index: ContentIndexSection,
    /// What extraction found, and the titles the file claims.
    pub extraction: ExtractionSection,
    /// Every text an operator typed at the identifier prompt for the
    /// file, or why none was taken.
    pub identifier_input: IdentifierInputStep,
    /// The identifier looked up and every service asked about it. When
    /// `identifier_input` names a used submission, this is that
    /// submission's lookup.
    pub lookup: LookupStep,
    /// Where the record the verdict is about was retrieved from, or
    /// `null` when the verdict reached no record. On a conflict skip
    /// this is the refused candidate's.
    pub record_retrieval: Option<RetrievedFrom>,
    /// What the title check concluded about the record reached.
    pub match_check: MatchCheckStep,
    /// Whether the record was used, and how: automatically, by an
    /// operator over the title check, or as an operator's own record.
    pub acceptance: Acceptance,
}

/// What an operator typed at the identifier prompt for a file.
/// Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum IdentifierInputStep {
    /// At least one text was submitted. `submissions` are every one,
    /// in the order typed. `used` is the number of the submission whose
    /// identifier the event's own `lookup` looked up, or `null`;
    /// `displaced` is non-null exactly when `used` is, and holds the
    /// file's own steps that submission's outcome replaced in the
    /// event's sections.
    Supplied {
        submissions: Vec<Submission>,
        used: Option<u32>,
        displaced: Option<Box<Displaced>>,
    },
    /// Nothing was submitted: `not-asked`, `not-supplied` or
    /// `content-duplicate`.
    NotAttempted { reason: String },
}

/// One text an operator submitted at the identifier prompt.
///
/// Every submission but the used one carries its own outcome, whose
/// keys sit beside `syntax`. The used submission's outcome is the
/// event's own `lookup`, `record_retrieval`, `match_check`, `acceptance`
/// and record, so its entry carries `submission`, `raw` and `syntax`
/// only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Submission {
    /// The submission's number, counting from 1 within the file.
    pub submission: u32,
    /// The text as the prompt returned it.
    pub raw: String,
    /// Whether the text parsed, and to what.
    pub syntax: SyntaxStep,
    /// Flattened: its keys sit beside `syntax`. `None` exactly for the
    /// submission `used` names, whose keys are then absent.
    #[serde(flatten)]
    pub outcome: Option<Box<SubmissionOutcome>>,
}

/// What a submission that was not used came to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubmissionOutcome {
    /// The lookup of the identifier it parsed to, with origin
    /// `operator`, or not attempted for `unparsed`.
    pub lookup: LookupStep,
    /// Where its record was retrieved from, or `null` when it reached
    /// none.
    pub record_retrieval: Option<RetrievedFrom>,
    /// What the title check concluded about its record.
    pub match_check: MatchCheckStep,
    /// What the operator decided about its record.
    pub acceptance: SubmissionAcceptance,
    /// The record it reached: present exactly when `record_retrieval`
    /// is not `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<Box<Record>>,
}

/// Whether a submitted text parsed. Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SyntaxStep {
    /// The text named `identifier`, normalised and written as the
    /// stream writes one (`doi:…`, `arXiv:…`, `pmid:…`, `isbn:…`).
    Parsed { identifier: String },
    /// The text named no identifier: `reason` is `unrecognised`,
    /// `invalid` or `checksum`, and `expected` names the form the text
    /// named (`doi`, `arxiv`, `pmid` or `isbn`), present exactly when
    /// `reason` is `invalid` or `checksum`.
    Rejected {
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<String>,
    },
}

/// What an operator decided about the record a submission reached.
/// Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SubmissionAcceptance {
    /// Its record was on offer and stopped being: the operator answered
    /// Skip, or a later submission's record took its place.
    Rejected,
    /// It reached nothing to accept: `unparsed`, `no-record` or
    /// `no-move`.
    NotAttempted { reason: String },
}

/// The file's own steps that a used submission replaced in the event's
/// sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Displaced {
    /// The file's own lookup, every round of it.
    pub lookup: LookupStep,
    /// Where the file's own record came from, or `null` when its own
    /// resolution reached none.
    pub record_retrieval: Option<RetrievedFrom>,
    /// The file's own title check.
    pub match_check: MatchCheckStep,
    /// The file's own record, resolved or refused: present exactly when
    /// `record_retrieval` is not `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<Box<Record>>,
}

/// What a run's library said about a file. Serialized with a `status`
/// tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum LibraryStep {
    /// The library was asked, and gave `answer`.
    Consulted { answer: LibraryAnswer },
    /// The library was not asked: `no-library`, `outside-library` or
    /// `content-duplicate`.
    NotAttempted { reason: String },
}

impl LibraryStep {
    /// The library's answer, or `None` when it was not asked.
    pub fn answer(&self) -> Option<&LibraryAnswer> {
        match self {
            LibraryStep::Consulted { answer } => Some(answer),
            LibraryStep::NotAttempted { .. } => None,
        }
    }
}

/// The content index's two steps for a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentIndexSection {
    /// Reading the index by the file's hash.
    pub read: IndexReadStep,
    /// Writing the record reached under the file's hash.
    pub write: WriteStep,
}

/// What reading the content index came to. Serialized with a `status`
/// tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum IndexReadStep {
    /// The index held a record for the file's hash.
    Hit,
    /// The index held none.
    Miss,
    /// The run turned the cache off, so the index was not read.
    Bypassed,
    /// The file could not be hashed, with `message` as the hashing
    /// error. Reported whether or not the cache was turned off.
    Unavailable { message: String },
    /// The index was not read, because a content duplicate or the
    /// library settled the file first.
    NotAttempted { reason: String },
}

/// What a write to a store came to. Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum WriteStep {
    /// The write was made.
    Written,
    /// The write failed, with `message` as the store gave it. Never a
    /// failure of anything else.
    Failed { message: String },
    /// No write was made, for the earliest reason in pipeline order.
    NotAttempted { reason: String },
}

/// Extraction's two steps for a file, independent of each other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionSection {
    /// What the extraction passes found.
    pub result: ExtractionResultStep,
    /// The titles the file's own metadata claims.
    pub titles: TitlesStep,
}

/// What extraction found in a file, in [`Extraction`]'s vocabulary.
/// Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ExtractionResultStep {
    /// A pass found `identifier` (`doi:…`, `arXiv:…`); `tier` names the
    /// pass (`embedded-metadata` or `text-layer`).
    Found { identifier: String, tier: String },
    /// No identifier, and no page read held text.
    NoTextLayer,
    /// No identifier in the metadata or in the text the pages held.
    TextWithoutIdentifier,
    /// The file needs a password.
    Encrypted,
    /// The file could not be opened or parsed, with the reader's
    /// `message`.
    Unreadable { message: String },
    /// Extraction did not run, because something earlier answered.
    NotAttempted { reason: String },
}

/// The titles a file claims, in exactly one of three states.
/// Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum TitlesStep {
    /// The file was opened, and `claims` are every title it claims, in
    /// the order read: empty when it claims none.
    Read { claims: Vec<Claim> },
    /// The file could not be opened, with the open error's `message`.
    Failed { message: String },
    /// The file was not opened.
    NotAttempted { reason: String },
}

/// The lookup made for a file, or why none was. Serialized with a
/// `status` tag.
///
/// The variant and its fields describe the current round. `earlier` is
/// every round of the same lookup made before it, oldest first, when an
/// operator asked for the file's own identifier to be looked up again;
/// it is absent from the line when there is none, which is always the
/// case for an identifier an operator supplied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum LookupStep {
    /// `identifier`, which came from `origin`, was looked up, and
    /// `attempts` are the services asked, in order: never empty, and a
    /// found attempt is always the last.
    Attempted {
        identifier: String,
        origin: IdentifierOrigin,
        attempts: Vec<ServiceAnswer>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        earlier: Vec<LookupRound>,
    },
    /// `identifier`, which came from `origin`, was to be looked up, and
    /// no configured service supports its kind.
    NoEligibleService {
        identifier: String,
        origin: IdentifierOrigin,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        earlier: Vec<LookupRound>,
    },
    /// Nothing was looked up.
    NotAttempted { reason: String },
}

/// One earlier round of a file's own lookup. Serialized with a
/// `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum LookupRound {
    /// The services asked in that round, in order: never empty.
    Attempted { attempts: Vec<ServiceAnswer> },
    /// No configured service supported the identifier in that round.
    NoEligibleService,
}

/// Where an identifier that was looked up came from.
///
/// For [`IdentifierOrigin::Extracted`], the pass that read it is the
/// `extraction` section's `tier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentifierOrigin {
    /// The file's own, read by extraction, including when an operator
    /// asked for the lookup to be made again.
    Extracted,
    /// An operator supplied it.
    Operator,
}

/// One service asked about an identifier, and how it answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceAnswer {
    /// The service, by its lowercase name (`crossref`, `openalex`, …).
    pub service: String,
    pub outcome: ServiceOutcome,
}

impl ServiceAnswer {
    /// The answer as a person reads it: `<service>: <what it said>`, in
    /// the wording of [`borax_sources::source::SourceError`]'s messages
    /// for a failure (`not found`, `unavailable: …`, `rate limited`,
    /// `malformed response: …`) and `found` for a success.
    pub(crate) fn said(&self) -> String {
        let what = match &self.outcome {
            ServiceOutcome::Found { .. } => "found".to_string(),
            ServiceOutcome::NotFound => "not found".to_string(),
            ServiceOutcome::Unavailable { message } => format!("unavailable: {message}"),
            ServiceOutcome::RateLimited => "rate limited".to_string(),
            ServiceOutcome::Malformed { message } => format!("malformed response: {message}"),
        };
        format!("{}: {what}", self.service)
    }
}

/// How a service answered. Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ServiceOutcome {
    /// The service supplied the record, from `retrieval`. `stored` is
    /// what became of keeping a network answer in the response cache:
    /// present exactly when `retrieval` is [`FetchedFrom::Network`],
    /// and absent from the line otherwise.
    Found {
        retrieval: FetchedFrom,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stored: Option<WriteStep>,
    },
    /// The service does not hold the identifier.
    NotFound,
    /// The service could not be reached, or answered with a server
    /// error, as `message` says.
    Unavailable { message: String },
    /// The service asked the run to slow down.
    RateLimited,
    /// The service answered with something that is not a record, as
    /// `message` says.
    Malformed { message: String },
}

/// How a found answer reached the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FetchedFrom {
    /// The service's response cache; no request was sent.
    ServiceCache,
    /// The service, over the network.
    Network,
}

/// Where the record a verdict is about was retrieved from. Serialized
/// with a `kind` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RetrievedFrom {
    /// The file's library item, through the artifact record `artifact`.
    Library { artifact: String, item: String },
    /// The content index, under the file's hash.
    ContentIndex,
    /// `service`'s response cache.
    ServiceCache { service: String },
    /// `service`, over the network.
    Network { service: String },
}

/// What the title check concluded. Serialized with a `status` tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum MatchCheckStep {
    /// A title the file claims names the record's work.
    Agreed,
    /// The file's titles name another work: `field` is the field
    /// compared, `extracted` the file's value, `resolved` the
    /// record's, and `similarity` how alike they were, from 0.0 to 1.0
    /// and below the threshold that clears them. Reported on a conflict
    /// skip and on a record an operator accepted over it alike.
    Conflict {
        field: String,
        extracted: String,
        resolved: String,
        similarity: f64,
    },
    /// Too little to judge: `record-untitled`, `no-titles` or
    /// `no-evidence`.
    InsufficientEvidence { reason: String },
    /// The check was not made.
    NotAttempted { reason: String },
}

/// Whether a verdict's record was used. Serialized with a `status` tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Acceptance {
    /// The verdict is a skip, whose record, if it reports one, was not
    /// used.
    NotApplicable,
    /// A record was used, no title-check conflict was overridden to use
    /// it, and it was not reached from an operator's submission.
    Automatic,
    /// An operator accepted the record over the conflict `match_check`
    /// holds, whether it was the file's own or one they supplied.
    Overridden,
    /// The record reached from an operator's submission is on offer and
    /// unanswered, whatever its title check concluded. Only the event a
    /// question's description is rendered from carries it; no reported
    /// verdict does.
    Pending,
    /// An operator accepted the record reached from their submission,
    /// with no conflict overridden.
    Accepted,
}

/// The services that supplied `record`, as the human line and the
/// interactive description name them.
///
/// The service `retrieval` names, where a service's response cache or
/// the network answered. Otherwise every service `record`'s per-field
/// provenance names other than extraction, in one fixed order —
/// Crossref, OpenAlex, arXiv, DataCite, PubMed, then `sidecar` — and
/// none when it names none of them. A store is never named as though it
/// were a service.
pub(crate) fn services_of<'a>(
    record: &Record,
    retrieval: Option<&'a RetrievedFrom>,
) -> Vec<&'a str> {
    if let Some(RetrievedFrom::ServiceCache { service } | RetrievedFrom::Network { service }) =
        retrieval
    {
        return vec![service.as_str()];
    }
    const ORDER: [(FieldSource, &str); 6] = [
        (FieldSource::Crossref, "crossref"),
        (FieldSource::OpenAlex, "openalex"),
        (FieldSource::Arxiv, "arxiv"),
        (FieldSource::DataCite, "datacite"),
        (FieldSource::PubMed, "pubmed"),
        (FieldSource::Sidecar, "sidecar"),
    ];
    ORDER
        .iter()
        .filter(|(source, _)| record.borax.provenance.values().any(|had| had == source))
        .map(|(_, name)| *name)
        .collect()
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
/// The two framing events are silent. [`Event::RunStarted`] would
/// restate the command a person just typed. [`Event::RunFinished`]
/// closes the run with a line whose shape depends on the command, which
/// the event alone does not name, so a rendering that knows the command
/// writes it through [`human_summary`] instead. The JSON stream keeps
/// both regardless.
pub fn human_line(event: &Event) -> Option<String> {
    match event {
        Event::RunStarted { .. } => None,
        Event::Resolved {
            path,
            identifier,
            record,
            sections,
        } => Some(format!(
            "{}: {}",
            path.display(),
            escaped(&format!(
                "{}{}{}",
                resolved_as(identifier, record, sections),
                unanswered(sections.library.answer()),
                rejected_candidates(Some(sections))
            ))
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
        Event::Skipped {
            path,
            reason,
            sections,
            ..
        } => Some(format!(
            "{}: skipped, {}",
            path.display(),
            escaped(&format!(
                "{}{}{}",
                skipped_because(reason, sections.as_deref()),
                unanswered(
                    sections
                        .as_deref()
                        .and_then(|sections| sections.library.answer())
                ),
                rejected_candidates(sections.as_deref())
            ))
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
        Event::LibraryExtraction { path, extraction } => Some(format!(
            "{}: {}",
            escaped(path),
            what_was_extracted(extraction)
        )),
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
        Event::LibraryCondition { path, condition } => Some(format!(
            "{}: {}",
            escaped(path),
            what_the_condition_is(condition)
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
        Event::LibraryAdoption { path, adoption } => {
            Some(format!("{}: {}", escaped(path), what_was_adopted(adoption)))
        }
        Event::ContentIndexWrite { path, write } => match write {
            WriteStep::Failed { message } => Some(format!(
                "{}: {}",
                path.display(),
                escaped(&format!(
                    "the content index could not keep this answer ({message})"
                ))
            )),
            WriteStep::Written | WriteStep::NotAttempted { .. } => None,
        },
        Event::LibraryAdopted {
            root,
            adopted,
            orphans,
        } => Some(format!(
            "{}: {adopted} adopted, {orphans} orphans",
            root.display()
        )),
        Event::RunFinished { .. } => None,
    }
}

/// Which closing line a command's human rendering ends with.
///
/// Chosen per command by [`crate::cli::Command::summary`] and rendered
/// by [`human_summary`]. The JSON stream does not depend on it: every
/// command closes that stream with the same [`Event::RunFinished`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Summary {
    /// `rename`: resolved, renamed and skipped, then files already
    /// named, unmatched lookups, files not reached and findings, each
    /// when nonzero.
    Renaming,
    /// `resolve` and `bib`: resolved and skipped, then unmatched
    /// lookups, files not reached and findings, each when nonzero.
    Resolution,
    /// `validate`: no line of its own, since its findings are on the
    /// [`Event::LibraryValidated`] line, unless files were skipped or
    /// not reached.
    Validation,
    /// `status`, `reconcile`, `adopt`, `config` and `cache`: no line of
    /// their own, unless files were skipped or not reached or findings
    /// were reported.
    Silent,
}

/// What a run amounts to, as the closing line of a human rendering in
/// the shape `summary` asks for, or `None` when that shape has nothing
/// to say about `counts`.
///
/// [`Summary::Renaming`] and [`Summary::Resolution`] always give a
/// line, and always state resolved and skipped, zero included.
/// Unmatched lookups, files not reached and findings are written only
/// when nonzero, and in that order.
///
/// [`Summary::Silent`] and [`Summary::Validation`] give `None` unless a
/// total that makes the run a partial success is nonzero — skipped, not
/// reached, or findings — and then give just those totals, so the
/// terminal never ends a partial run without saying why.
/// [`Summary::Validation`] leaves findings out, because the
/// [`Event::LibraryValidated`] line already states them.
///
/// `hidden` is how many already-named files the run passed over without
/// a line of their own, which only an interactive run does, and is read
/// only by [`Summary::Renaming`]: its `already named` clause says how
/// many of them the terminal did not show.
pub fn human_summary(summary: Summary, counts: &Counts, hidden: usize) -> Option<String> {
    let clause = |count: usize, what: &str| (count != 0).then(|| format!("{count} {what}"));
    let skipped = format!("{} skipped", counts.skipped);
    let unmatched = clause(counts.unmatched, "unmatched");
    let unreached = clause(counts.unreached, "not reached");
    let findings = clause(counts.findings, "findings");
    let clauses: Vec<Option<String>> = match summary {
        Summary::Renaming => vec![
            Some(format!("{} resolved", counts.resolved)),
            Some(format!("{} renamed", counts.renamed)),
            Some(skipped),
            match (counts.named, hidden) {
                (0, _) => None,
                (named, 0) => Some(format!("{named} already named")),
                (named, hidden) if hidden >= named => {
                    Some(format!("{named} already named (not shown)"))
                }
                (named, hidden) => Some(format!("{named} already named ({hidden} not shown)")),
            },
            unmatched,
            unreached,
            findings,
        ],
        Summary::Resolution => vec![
            Some(format!("{} resolved", counts.resolved)),
            Some(skipped),
            unmatched,
            unreached,
            findings,
        ],
        Summary::Validation => vec![clause(counts.skipped, "skipped"), unreached],
        Summary::Silent => vec![clause(counts.skipped, "skipped"), unreached, findings],
    };
    let said: Vec<String> = clauses.into_iter().flatten().collect();
    (!said.is_empty()).then(|| said.join(", "))
}

/// The clause following `skipped,` in a human line, unescaped.
///
/// A resolution verdict's clause is read from `sections` where its
/// reason names no detail: the identifier and the services' answers for
/// [`SkipReason::Unresolvable`], the two titles for
/// [`SkipReason::Conflict`]. The four extraction failures are worded as
/// `status --identify` words them, so a skip and a survey say the same
/// thing about the same file.
fn skipped_because(reason: &SkipReason, sections: Option<&Sections>) -> String {
    match reason {
        SkipReason::NoTextLayer => what_was_extracted(&Extraction::NoTextLayer),
        SkipReason::TextWithoutIdentifier => what_was_extracted(&Extraction::TextWithoutIdentifier),
        SkipReason::Encrypted => what_was_extracted(&Extraction::Encrypted),
        SkipReason::Unreadable { message } => format!("unreadable ({message})"),
        SkipReason::Unresolvable => match sections.map(|sections| &sections.lookup) {
            Some(LookupStep::Attempted {
                identifier,
                attempts,
                ..
            }) => {
                let said: Vec<String> = attempts.iter().map(ServiceAnswer::said).collect();
                match said.is_empty() {
                    true => format!("no source had a record for {identifier}"),
                    false => format!(
                        "no source had a record for {identifier} ({})",
                        said.join("; ")
                    ),
                }
            }
            Some(LookupStep::NoEligibleService { identifier, .. }) => {
                format!("no configured service could be asked about {identifier}")
            }
            Some(LookupStep::NotAttempted { .. }) | None => {
                "no source had a record for its identifier".to_string()
            }
        },
        SkipReason::Conflict => match sections.map(|sections| &sections.match_check) {
            Some(MatchCheckStep::Conflict {
                field,
                extracted,
                resolved,
                similarity,
            }) => format!(
                "{field} disagrees {}% (file says {extracted}, record says {resolved})",
                (similarity * 100.0).round()
            ),
            _ => "the file's own title names another work".to_string(),
        },
        SkipReason::TargetTaken { target } => format!("{} is taken", target.display()),
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

/// The clause following a `resolved` line's path, unescaped and
/// without its library problem: the identifier, the work the record
/// describes, the services that supplied it and where it was retrieved
/// from, each left out where there is nothing to say.
fn resolved_as(identifier: &str, record: &Record, sections: &Sections) -> String {
    let mut clause = format!("resolved {identifier}");
    if let Some(work) = work_of(record) {
        clause.push_str(&format!(" to {work}"));
    }
    let services = services_of(record, sections.record_retrieval.as_ref());
    if !services.is_empty() {
        clause.push_str(&format!(" via {}", services.join(", ")));
    }
    if let Some(retrieval) = &sections.record_retrieval {
        clause.push_str(&format!(
            ", from {}",
            match retrieval {
                RetrievedFrom::Library { .. } => "the library",
                RetrievedFrom::ContentIndex => "the content index",
                RetrievedFrom::ServiceCache { .. } => "the response cache",
                RetrievedFrom::Network { .. } => "the network",
            }
        ));
    }
    clause
}

/// The work `record` describes, as `"<title>" (<authors>, <year>)`,
/// leaving out a title that is absent or blank and whichever of the
/// authors and the year the record lacks, or `None` when it has none of
/// the three.
///
/// Authors are named by family name: one as `Smith`, two as `Smith and
/// Jones`, and three or more as `Smith et al.`
fn work_of(record: &Record) -> Option<String> {
    let title = record
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .map(|title| format!("\"{title}\""));
    let authors = match record.authors.as_slice() {
        [] => None,
        [one] => Some(one.family.clone()),
        [first, second] => Some(format!("{} and {}", first.family, second.family)),
        [first, ..] => Some(format!("{} et al.", first.family)),
    };
    let year = record.issued.as_ref().map(|issued| issued.year.to_string());
    let credit: Vec<String> = authors.into_iter().chain(year).collect();
    let credit = (!credit.is_empty()).then(|| format!("({})", credit.join(", ")));
    let work: Vec<String> = title.into_iter().chain(credit).collect();
    (!work.is_empty()).then(|| work.join(" "))
}

/// The clause a resolution's human line ends with when its library
/// could not answer for the file, and nothing otherwise.
fn unanswered(library: Option<&LibraryAnswer>) -> String {
    match library.and_then(why_unanswered) {
        Some(what) => format!("; the library could not answer: {what}"),
        None => String::new(),
    }
}

/// The clause a resolution's human line ends with when an operator
/// rejected a candidate for the file: `; candidate rejected: <id>`, or
/// `; candidates rejected: <a>, <b>` in submission order, and nothing
/// otherwise.
fn rejected_candidates(sections: Option<&Sections>) -> String {
    let rejected = sections.map(rejected_identifiers).unwrap_or_default();
    match rejected.as_slice() {
        [] => String::new(),
        [one] => format!("; candidate rejected: {one}"),
        many => format!("; candidates rejected: {}", many.join(", ")),
    }
}

/// The identifiers of the submissions an operator rejected, in
/// submission order.
///
/// Shared by the human line and the interactive description.
pub(crate) fn rejected_identifiers(sections: &Sections) -> Vec<&str> {
    let IdentifierInputStep::Supplied { submissions, .. } = &sections.identifier_input else {
        return Vec::new();
    };
    submissions
        .iter()
        .filter_map(
            |submission| match (&submission.syntax, &submission.outcome) {
                (SyntaxStep::Parsed { identifier }, Some(outcome))
                    if outcome.acceptance == SubmissionAcceptance::Rejected =>
                {
                    Some(identifier.as_str())
                }
                _ => None,
            },
        )
        .collect()
}

/// Why the library could not answer for a file, as a clause whose
/// subject is the library's own records, or `None` when `answer` is
/// [`LibraryAnswer::Tracked`] or [`LibraryAnswer::Untracked`] and it
/// did answer.
///
/// Shared by the human line and the interactive description, so the
/// terminal and the batch output state a problem in the same words.
pub(crate) fn why_unanswered(answer: &LibraryAnswer) -> Option<String> {
    let artifacts = |ids: &[String]| match ids {
        [id] => format!("artifact {id}"),
        ids => format!("artifacts {}", ids.join(", ")),
    };
    let paths = |paths: &[PathBuf]| {
        paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some(match answer {
        LibraryAnswer::Tracked { .. } | LibraryAnswer::Untracked => return None,
        LibraryAnswer::UnrecognisedContent { artifacts: ids } => format!(
            "the library records {} at this path, but not these bytes",
            artifacts(ids)
        ),
        LibraryAnswer::Ambiguous { artifacts: ids } => format!(
            "{} artifact records claim this file: {}",
            ids.len(),
            ids.join(", ")
        ),
        LibraryAnswer::NoItem { artifact } => {
            format!("artifact {artifact} is linked to no item")
        }
        LibraryAnswer::DanglingItem { artifact, item } => {
            format!("artifact {artifact} links to item {item}, which the library does not hold")
        }
        LibraryAnswer::UnreadableItem {
            artifact,
            item,
            path,
            message,
        } => format!(
            "artifact {artifact} links to item {item}, which could not be read from {}: \
             {message}",
            path.display()
        ),
        LibraryAnswer::AmbiguousItem {
            artifact,
            item,
            files,
        } => format!(
            "artifact {artifact} links to item {item}, which {} item files claim: {}",
            files.len(),
            paths(files)
        ),
        LibraryAnswer::Unhashable { artifacts: ids } => format!(
            "the file could not be hashed, so {} recorded at this path cannot be confirmed",
            artifacts(ids)
        ),
        LibraryAnswer::UnreadableRecords { listed: false, .. } => {
            "the library's artifact records could not be listed, so it cannot say whether it \
             tracks this file"
                .to_string()
        }
        LibraryAnswer::UnreadableRecords {
            listed: true,
            unreadable,
        } => format!(
            "{unreadable} artifact record {} could not be read, so the library cannot say \
             whether it tracks this file",
            if *unreadable == 1 { "file" } else { "files" }
        ),
    })
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

/// `extraction` as the end of a sentence whose subject is the artifact,
/// for the human rendering of [`Event::LibraryExtraction`], with the
/// identifier and the reader's message escaped.
///
/// A `tier` naming neither pass is said to be from the file, as the
/// interactive description says it.
fn what_was_extracted(extraction: &Extraction) -> String {
    match extraction {
        Extraction::Found { identifier, tier } => format!(
            "identifier {} {}",
            escaped(identifier),
            match tier.as_str() {
                "embedded-metadata" => "from embedded metadata",
                "text-layer" => "from the text layer",
                _ => "from the file",
            }
        ),
        Extraction::NoTextLayer => "no identifier found; the pages read hold no text".to_string(),
        Extraction::TextWithoutIdentifier => {
            "no identifier found in its metadata or the pages read".to_string()
        }
        Extraction::Encrypted => "encrypted, so no identifier could be read".to_string(),
        Extraction::Unreadable { message } => format!("unreadable ({})", escaped(message)),
    }
}

/// `adoption` as the end of a sentence whose subject is the orphan,
/// for the human rendering of [`Event::LibraryAdoption`].
fn what_was_adopted(adoption: &Adoption) -> String {
    match adoption {
        Adoption::Recorded { id, item } => {
            format!("adopted as artifact {id} of item {item}")
        }
        Adoption::Held { id } => format!(
            "holds bytes artifact {id} already records, so it was left an orphan; \
             run borax reconcile if the file was moved"
        ),
        Adoption::Unreadable { message } => {
            format!("could not be read ({message}), so it is still an orphan")
        }
        Adoption::Unwritten { message } => {
            format!("could not be recorded ({message}), so it is still an orphan")
        }
        Adoption::Unindexed => {
            "the content index holds no record of its bytes, so it is still an orphan".to_string()
        }
    }
}

/// `condition` as the end of a sentence whose subject is the path it is
/// about, for the human rendering of [`Event::LibraryCondition`], with
/// a missing record's file escaped.
///
/// Each clause begins with the word the totals line counts it under.
fn what_the_condition_is(condition: &Condition) -> String {
    match condition {
        Condition::Orphan => "orphan; no artifact record names it".to_string(),
        Condition::Missing { id, record } => format!(
            "missing; artifact record {id} ({}) names this path and the library \
             has no artifact here",
            escaped(record)
        ),
        Condition::Unlinked { id } => format!("unlinked; no artifact record links item {id}"),
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
