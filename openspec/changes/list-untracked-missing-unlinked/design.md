# Design: list-untracked-missing-unlinked

## Context

Everything below was read from source at `dcf4e8c`, which is `main`
after `report-extraction-per-file` was merged.

- `library::survey` makes one walk of the tree and one read of each
  store (`contents`). It returns a `Survey` whose `orphans:
  Vec<PathBuf>` lists, in survey order, the artifacts no record names
  by path (`library::orphans`). `status_event` reports
  `survey.orphans.len()`. The survey opens no document.
- `run::status_events` surveys `reported_root`. With `--identify` it
  writes one `library-extraction` per artifact as each extraction
  finishes, then `library-status`. Without it, it writes only
  `library-status`.
- `library::validate` reads `contents` once. Its findings are
  `item_findings` followed by `record_findings`. It keeps three
  counts:
  - `orphans`: `survey.orphans.len()`;
  - `missing`: `library::missing(root, &records, exists).len()`, where
    `exists` is `!excludes(root, path) && path.is_file()`, so a record
    whose path lies in a nested library, `items/` or `.borax/` counts
    as missing;
  - `unlinked`: the items for which `records.by_item(&item.id)` is
    empty, counted over `ItemStore::iter`, which gives one entry per
    item file. Two files carrying one identity count twice.

  `validation_events` writes one `library-finding` per finding, then
  `library-validated`. No list behind the three counts survives.
- `ArtifactStore::iter` and `ItemStore::iter` yield entries in read
  order, which is path order of the store files. `ItemStore::files`,
  which is private, pairs each item with the file it was read from.
- `library::adopt` walks the orphans in survey order. For each one it
  hashes the file (`Unreadable` on failure) and checks whether a
  record or an earlier adoption already holds the hash (`Held`). It
  then asks `lookup`. On `None` it `continue`s and records nothing.
  `adoption_events` maps `adoptions` to `library-adoption` events and
  computes `library-adopted.orphans` as `adoptions.orphans - adopted`.
  An orphan whose path `library_relative` cannot express would also be
  skipped silently. `orphans` never yields one, because it filters on
  `library_relative`.
- `run::validation_events`, `reconcile_events` and `adopt_events` call
  the library function and then write every event it returned. Only
  `status_events` writes as it goes.
- `session::outcome_for` returns `Partial` when `skipped + unreached +
  findings` is nonzero, and `Success` otherwise. `Counts::observe`
  counts `library-finding` and no other library event.
  `Command::summary` maps `status`, `reconcile` and `adopt` to
  `Silent`, and `validate` to `Validation`.
- `event::human_line` writes `LibraryAdoption` as `"{path}: …"` with
  the path unescaped. `LibraryExtraction` escapes its path, identifier
  and message through `describe::escaped`, which writes each control
  character as `\xNN`.
- The living `library` spec states that orphans, missing artifacts and
  unlinked items "SHALL be reported as counts rather than as findings"
  ("Validation defines a well-formed library"). It states that
  reconciliation "SHALL NOT be an error path". It states that an item
  with no artifact is reported and "nothing reports it as incomplete".

## D1. Exit status: unchanged, and conditions are detected from the stream

**Decision.** No exit code changes. `session::outcome_for` keeps
counting only skipped, unreached and findings. The following all
still exit 0:

- a `validate` whose only conditions are missing records, orphans or
  unlinked items;
- a `reconcile` that leaves records ambiguous or missing;
- a `status` that names orphans;
- an `adopt` that reports `unindexed` orphans.

The `cli` requirement "Exit codes distinguish partial success" is not
modified.

A caller that needs to act on a condition detects it from `--json`
output. Each condition now has a per-object event, and the counts
were already on the totals events. For example:

```sh
borax validate --json | jq -e 'select(.event == "library-validated") | .missing == 0'
borax reconcile --json | jq -e 'select(.event == "library-reconciled") | .ambiguous + .missing == 0'
```

Each `jq -e` exits 1 when the condition is present. The per-object
events name which objects are affected:
`select(.event == "library-condition" and .condition.kind ==
"missing")`, or `library-repair` with `repair.kind` `ambiguous` or
`missing`. The manual documents this recipe (task 6.1).

**Why.**

1. **The living requirements define these as healthy states.**
   "Validation defines a well-formed library" calls none of the three
   "a malformed library": an orphan is work to do, a missing artifact
   is history the library deliberately keeps, and an unlinked item is
   an ordinary item for a work with no file. Reconciliation "SHALL NOT
   be an error path", and an ambiguous record is preserved on
   purpose. An exit code that treats any of these as a failure would
   contradict those requirements. It would not be a change to `cli`
   alone.
2. **A missing record never goes away by itself.** "A record outlives
   its artifact" is a scenario, not a defect. A library from which
   one file was ever deleted keeps that record for as long as the
   library exists. With a nonzero code for missing records, `borax
   validate && …` would fail permanently on a library the
   specification calls well formed. The only way to clear it would be
   to delete the record, which is the history the requirement keeps
   deliberately.
3. **Reusing the partial code would merge two questions.** `validate`
   exits 2 when there is a finding. If conditions also exited 2, a
   script could not tell "the records are malformed" from "there is
   work to do", and that difference is what the findings/conditions
   split exists to keep. A third code avoids the merge but still runs
   into reasons 1 and 2.
4. **The review accepted this.** The interactive review observed that
   a script cannot detect missing or ambiguous conditions from the
   exit status alone. It accepted no exit-code change during the
   review. This change removes the reason the observation mattered:
   before it, a script could detect *that* a condition existed from
   the totals but not *which object* it was about. Now it can do both
   from the stream.

**Rejected: a dedicated exit code (for example 3) for unresolved
conditions on `validate`, `reconcile` and `status`.** See reasons 1
and 2. It would also make `status`, a report command, the only
command whose exit status depends on what it counted, rather than on
whether it ran.

**Rejected: the partial-success code for conditions.** See reason 3.

**Rejected: an opt-in flag such as `--fail-on missing,ambiguous`.**
This is the right shape if scripts turn out to need it. The caller,
not the library's definition, decides that a condition matters, and
the default stays honest. But it is a policy flag, and the roadmap
defers policy and verbosity flags together. It is recorded as the
route under "Deferred" in the proposal.

**What is flagged for the maintainer.** The decision is no change,
and it is at the top of the proposal's What Changes so it can be
confirmed or overruled before tests exist. Overruling it means
amending this proposal: modifying the `cli` exit-code requirement,
and the `library` paragraphs cited in reason 1.

## D2. The boundary, applied a second time

**Decision.** Change 5 follows `report-extraction-per-file` D1 with no
new abstraction.

1. **The command selects the subjects.**
   - `status`: `survey.orphans`.
   - `validate`: the orphans, the missing records and the unlinked
     items, using the filters `validate` already counts with.
   - `adopt`: the orphans, as today.

   Each selection is made once, from the one walk and the one store
   read the command already performs.
2. **The inspection gives one result per subject and writes nothing.**
   A condition is the result of a test the selection already ran. So
   the inspection here is the mapping from each selected object to
   `(path, Condition)`, placed relative to the root. It adds no object
   and drops none. For `adopt` the inspection is the existing
   per-orphan decision, which now has a result in every branch.
3. **Renderers present typed events.** `json_line` and `human_line`
   render `library-condition` and the new adoption kind. Every total
   is taken over the same list the events are written from:
   - `status`'s `orphans` over `survey.orphans`, as today;
   - `validate`'s three counts over `Validation::conditions`;
   - `adopt`'s remaining orphans over `Adoptions::adoptions`.

   Because the counts come from those lists, a total cannot disagree
   with its events.

There is still no `Inspection` trait. Change 4 extracted from files.
Change 5 reads conditions that already exist in the survey and store
results. The two share a shape (subjects in, one typed result out,
totals over the results) but no code. A trait would fix that shape
with two implementations that have nothing to reuse from each other.

## D3. The event

**Decision.** One event for all three conditions, with a `kind`-tagged
outcome. This follows `library-finding` and `library-repair`:

```rust
/// A library condition a run counted, named: an artifact no record
/// names, a record whose artifact the library does not have, or an
/// item no record links to.
Event::LibraryCondition {
    /// Library-relative and `/`-separated: the artifact for `orphan`,
    /// the record's last-known path as recorded for `missing`, the
    /// item file for `unlinked`.
    path: String,
    condition: Condition,
}

#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Condition {
    Orphan,
    /// `record`: the record's own file, library-relative, under
    /// `.borax/artifacts/`.
    Missing { id: String, record: String },
    Unlinked { id: String },
}
```

```json
{"schema":3,"event":"library-condition","path":"sub/new.pdf","condition":{"kind":"orphan"}}
{"schema":3,"event":"library-condition","path":"gone.pdf","condition":{"kind":"missing","id":"0190c3a2-…","record":".borax/artifacts/0190c3a2-….toml"}}
{"schema":3,"event":"library-condition","path":"items/milner1978.0190c3a3-….toml","condition":{"kind":"unlinked","id":"0190c3a3-…"}}
```

**Kinds are named after the count they add to.** The count fields
are `orphans`, `missing` and `unlinked`, so the kinds are `orphan`,
`missing` and `unlinked`. The number of `library-condition` events of
kind `orphan` equals the `orphans` total of the same run, and likewise
for the other two. That is the "every count can be traced" rule
written into the vocabulary.

`orphan` is used rather than `untracked`:

- Renaming "orphans" in prose is a later change, and that change may
  rename the count field at Phase 3's schema bump.
- Using a second word now would mean the event kind and the count it
  traces disagree for as long as schema 3 lasts.
- "Untracked" already means something narrower in `resolution`: no
  record holds the file's bytes, which `orphan`'s by-path test does
  not check (`report-extraction-per-file` D6).

**How each object is identified.**

| kind | `path` | other fields | Why |
|---|---|---|---|
| `orphan` | the artifact, library-relative | — | An orphan has no record, so it has no identity. Its path is the only handle, and it is the same handle `library-extraction` uses, so the two join. |
| `missing` | the record's last-known path, verbatim as stored | `id`: the record's artifact identity; `record`: the record file, library-relative under `.borax/artifacts/` | The identity is the reference that follows the record across runs, as `library-repair` carries it. The path says where the record expected the file. It is not rewritten, because it may be exactly what is wrong with the record. The record file is what makes the event name one record (see below). |
| `unlinked` | the item file, library-relative (`items/<key>.<uuid>.toml`) | `id`: the item's identity | The identity inside the file decides what the item is ("never by parsing a file name"). The file path is what a person opens, and it carries the creation-time key as a legible label. |

**Why a missing record carries its record file.** `ArtifactStore`
keeps one entry per record file, and `validate` must tolerate two files
carrying one artifact identity: that is a `DuplicateIdentity` finding,
not a reason to stop. `library::missing` judges each record on its own.
Two such files that both name `gone.pdf` would otherwise give two
identical events, and a consumer could not tell which file to open or
fix. Even with different paths, `id` alone does not say which store
file the condition is about. The record file is the same kind of handle
the unlinked item's file path is: where a person opens the thing the
condition is about. It also joins with `library-finding`, whose `path`
names the same store file (as a full path). The identity stays, because
it is the reference that follows the record across runs.

The record file is placed in the `condition`, not in `path`. `path` is
the last-known path, which is what a reader comparing against the tree
looks for, and which `library-repair` also reports for a missing
record.

**Rejected: a citation key or title for an unlinked item.**
- A key parsed from the file name would treat the name as identity,
  which the item requirement forbids. A key rendered now could differ
  from the label, which is exactly the staleness that requirement
  accepts.
- A title is text from outside borax. It would need escaping and
  would grow the line. The file path already shows the key.

**Rejected: three event tags (`library-orphan`, `library-missing`,
`library-unlinked`).** A consumer accounting for conditions would
have to join three tags, which the house style avoids. `validate`
emits all three kinds, and one filter on `event` should collect them.

**Rejected: lists on the totals events (`orphan_paths: […]` on
`library-status`).** Sibling D4 rejected this for extraction. The
totals event would grow with the library, and nothing could be
written until it was complete.

**Rejected: reusing `library-finding` with new `Finding` kinds.** That
would make every condition a counted finding. `Counts::observe`
counts the event, not the kind, so the conditions would make
`validate` exit partial. This is the merge D8 forbids.

## D4. Which command names which conditions

**Decision.**

| Command | Counts it reports | Conditions it names |
|---|---|---|
| `status` | artifacts, items, records, orphans | `orphan` |
| `validate` | findings, orphans, missing, unlinked | `orphan`, `missing`, `unlinked` (findings as today) |
| `adopt` | adopted, orphans left | through `library-adoption`, one per orphan (D6) |
| `reconcile` | records, confirmed, repaired, changed, ambiguous, missing, hashed | unchanged: `library-repair` already names every non-confirmed record |

**Why `status` names only orphans.** The rule is that every count in
scope can be traced. Among the conditions, `status` counts orphans
and nothing else. Its `artifacts`, `items` and `records` are
inventory, not conditions. `--identify` already traces `artifacts`
per file, and `identifiable` over the same files.

Naming missing records and unlinked items in `status` would leave two
bad options:
- write events for conditions `status` does not count, which breaks
  the "totals over the same results" rule in reverse; or
- add `missing` and `unlinked` to `library-status` and to its line,
  which changes the report line's wording and starts folding
  `validate` into `status`, a later change.

**Cost.** `status` keeps what it computes today. `survey` gains no
work, and in particular performs no stat of recorded paths, so
"`borax status` SHALL open no document" holds trivially. `validate`
already computes all three lists' worth of tests. It now keeps the
objects instead of only counting them.

**`missing` means different things in `validate` and `reconcile`.**
In `validate` it means a record whose last-known path holds no
artifact of this library: a stat, no hashing. In `reconcile` it means
a record no artifact in the library matches by hash. A file moved out
of band is `missing` to `validate` until the next reconcile repairs
it, and never `missing` to `reconcile`. This change uses each
command's existing definition and does not reconcile the two. The
`missing` condition kind belongs to `validate`, and `reconcile` keeps
`Repair::Missing`.

## D5. Order, streaming and the totals

**Decision.**

- **`status`:** survey, then one `library-condition` per orphan in
  survey order, then (with `--identify`) the `library-extraction`
  events, then `library-status`. The orphans are known once the survey
  is read, before any document is opened. "A run reports as it goes"
  therefore puts them first: holding them behind a pass over every
  file would delay lines that are already decided. Task 3.4 pins this.
  When the first document is opened, the writer already holds every
  orphan line.
- **`validate`:** the findings in their existing order, then the
  conditions, then `library-validated`. Conditions are grouped by kind
  in the order the totals line names them: orphans in survey order,
  missing records in store read order, unlinked items in item store
  read order. Findings come first because they decide the exit status
  and are what `validate` is for.
- **Totals:** `library-status.orphans` stays `survey.orphans.len()`,
  and `orphan_events` maps that same `Vec`. `library-validated`'s
  `orphans`, `missing` and `unlinked` come from
  `Validation::orphans()`, `missing()` and `unlinked()`, each counting
  `Validation::conditions` by kind. The filters that build the list
  are the ones that compute the counts today. So for every library,
  every total is the same number it is today. Task 4.2 asserts this
  on a library holding all three conditions.

The totals event stays last before `run-finished` for all three
commands.

## D6. The adoption miss

**Decision.** A new kind on the existing event, not a new event:

```rust
pub enum Adoption {
    Recorded { id: String, item: String },
    Held { id: String },
    Unreadable { message: String },
    Unwritten { message: String },
    /// The content index holds no record for the orphan's bytes, so
    /// there was nothing to adopt it from and it is still an orphan.
    Unindexed,
}
```

```json
{"schema":3,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"unindexed"}}
```

In `library::adopt`, the `None` branch of `lookup` pushes
`(relative, Adoption::Unindexed)` where it now `continue`s. With that,
every orphan the walk yields gets exactly one adoption. Every branch
of the loop records one, and `orphans()` only yields paths
`library_relative` can express. So
`library-adopted.orphans == adoptions.orphans - adopted` equals the
number of `library-adoption` events that are not `recorded`. That is
the "totals over the same results" rule. Task 5.1 asserts it.

**Why a kind and not a new event.** `library-adoption` is the
per-orphan report of an adoption run. Its doc comment carved out
exactly this case as the one it stayed silent about. Closing the gap
inside the event means a consumer accounts for every orphan with one
tag. A separate `library-condition` with kind `orphan` would report
the same file twice under two vocabularies on the same run.

**Why `unindexed`.** The kind names the cause, which is that the
content index holds nothing for those bytes. It does not name a state
every non-recorded kind shares ("still an orphan"). `unknown` would
describe borax's knowledge rather than where it looked.

`unindexed` is reported only for an orphan the index was asked about.
`adopt` decides `unreadable` (the hash failed) and `held` (a record
already holds the bytes) before it consults the index, and a recorded
path is never an orphan at all. So after `borax cache --clear`, the
orphans that are `unindexed` are the readable ones whose bytes no record
holds. That is all the extended "Adoption after the cache is cleared"
scenario claims; its original wording is otherwise kept.

## D7. Human rendering

**Decision.** One line per object in the house form `<path>: <clause>`.
Each condition line begins with the word its count uses on the totals
line, so a reader maps lines to counts by eye:

| Event | Line |
|---|---|
| `library-condition`, `orphan` | `<path>: orphan; no artifact record names it` |
| `library-condition`, `missing` | `<path>: missing; artifact record <id> (<record>) names this path and the library has no artifact here` |
| `library-condition`, `unlinked` | `<path>: unlinked; no artifact record links item <id>` |
| `library-adoption`, `unindexed` | `<path>: the content index holds no record of its bytes, so it is still an orphan` |

The adoption line ends like the existing `unreadable` and `unwritten`
adoption lines (`…, so it is still an orphan`). It names no remedy,
because which one applies (`resolve` and then `adopt`, or `rename
--apply`) depends on the operator's intent. The manual's "Choosing how
to record existing files" already covers that choice.

The `missing` line says "the library has no artifact here" rather than
"no file is here". `validate` counts a record whose path lies in a
nested library or under `.borax/` as missing even when a file stands
there.

The `unlinked` line states a fact about links and calls nothing
incomplete, which keeps the item requirement's scenario "An item with
no artifact" true.

**Escaping.** Every `path` is passed through `describe::escaped`:

- an orphan's or item file's path comes from a file name;
- a missing record's path comes from text a peer writer may have
  edited;
- a missing record's `record` comes from a file name under
  `.borax/artifacts/`, and is escaped too.

The `id`s are UUIDs borax parsed and renders itself, so they are
written as they are. JSON carries raw values.

The `LibraryAdoption` arm renders the path for every kind. Escaping it
for `unindexed` therefore escapes it for `recorded`, `held`,
`unreadable` and `unwritten` too. For a path with no control character
the existing lines are byte-for-byte unchanged. For one with a control
character, they stop passing it to the terminal. This is a narrow case
of the "Human output … passes metadata through unescaped" defect, and
it is closed for this line rather than widened. Messages on existing
lines (`Adoption::Unreadable`, `Unwritten`) are left as they are. The
full fix is the one-place escaping `STATE.md` describes.

**Why every object, by default.** The review asked which objects were
behind the counts. The sibling lists every artifact under
`--identify`. A flag to opt in would be a verbosity decision, and the
roadmap defers those. The report lines keep their wording and stay
last.

**Rejected: a closing line listing the paths (`5 orphans: a.pdf,
…`).** It would repeat the per-object lines and would no longer be a
report line with fixed wording.

## D8. Conditions are not findings: the validation gate

**Decision.** `Counts::observe` ignores `library-condition` and every
`library-adoption` kind through its existing wildcard arm. `Summary`
and `outcome_for` are unchanged. A condition is never a
`library-finding`. `validate`'s check table is unchanged, and its tests
are not edited:

- **item file:** readable and parseable, and its file name agrees with
  its identity;
- **artifact record:**
  - readable and parseable;
  - its file name agrees with its identity;
  - its last-known path is library-relative;
  - its hash history is nonempty;
- **hash entry:** a well-formed hash and a nonempty run id;
- **relationship:** the referenced item exists;
- **collection context:** no duplicate item identity and no duplicate
  artifact identity;
- **item source fields:** malformed source fields in a readable item
  are reported as `unreadable`;
- **store directory:** an unlistable store directory is reported as
  `unreadable`.

`item_findings` and `record_findings` are not touched. Task 4.2 adds a
regression test over one library holding one instance of every
finding kind beside all three conditions. It asserts that the
findings are exactly those the existing per-finding tests pin, that
the `findings` count equals the number of `library-finding` events,
and that no `library-finding` names a condition.

## D9. Schema: additions under version 3

**Decision.** `SCHEMA` stays 3. Under the `cli` rule, this change
consists of additions:

- a new event tag, `library-condition`;
- a new value of an existing tagged field, `adoption.kind` =
  `unindexed`;
- more `library-adoption` events on runs that previously left orphans
  unreported.

No existing field changes meaning:
- `library-adopted.orphans` still counts the orphans the run left;
- `library-validated`'s and `library-status`'s counts are the same
  numbers for every library (D5).

A consumer that ignores tags and kinds it does not know reads every
stream as before. The precedents are `consult-library-first`, which
added `library` values to `tier` and `source`, and
`report-extraction-per-file`, which added `library-extraction`.

What does change is the set of `library-adoption` events: previously
only orphans the run tried to adopt produced one. A consumer that
counted those events as "orphans the index answered for" now sees
`unindexed` among them, and should dispatch on `adoption.kind`, as it
already must to tell `recorded` from `held`. The changelog states
this (task 6.2).

## D10. The interface the tests are written against

```rust
// crates/borax/src/event.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Condition {
    Orphan,
    Missing { id: String, record: String },
    Unlinked { id: String },
}
// Event::LibraryCondition { path: String, condition: Condition }
// Adoption::Unindexed

// crates/borax/src/library.rs
/// The `library-condition` events naming each orphan of `survey`, in
/// survey order, each named relative to `survey.root` as
/// [`extraction_event`] names an artifact.
pub fn orphan_events(survey: &Survey) -> Vec<Event>;

pub struct Validation {
    pub root: PathBuf,
    pub findings: Vec<(PathBuf, Finding)>,
    /// Every condition, each with the library-relative path it is
    /// about: the orphans in survey order, then the missing records in
    /// read order, then the unlinked items in read order.
    pub conditions: Vec<(String, Condition)>,
}
impl Validation {
    pub fn orphans(&self) -> usize;
    pub fn missing(&self) -> usize;
    pub fn unlinked(&self) -> usize;
}
```

The following keep their signatures:
- `validate(&Path) -> Validation` and `validation_events(&Validation)
  -> Vec<Event>`;
- `survey`, `status_event`, `extraction_event`, `orphans` and
  `missing`;
- `adopt` and `adoption_events`, with `Adoptions` and
  `Adoptions::adopted`.

`run::status_events` stays private and is tested through `events_for`
and `dispatch`.

A path that `library_relative` cannot express falls back as
`extraction_event`'s does: `Path::display` with the platform separator
written as `/`. The fallback is shared through one private helper. A
missing record's `path` is the stored string itself and needs no
fallback. Its `record` comes from the record file through the same
helper.

`validate` builds the missing conditions over the private
`ArtifactStore::files`, which pairs each record with its file, applying
the same `exists` test `library::missing` applies. `library::missing`
keeps its signature and its callers.

## D11. How the specifications change

**Decision.** Four deltas to `library`, none to `cli`.

- ADDED "Library conditions are named where they are counted". This
  is the contract shared by `status` and `validate`:
  - the three kinds and how each object is identified;
  - counts equal to events per kind;
  - every event before the totals;
  - the human line and its escaping;
  - never a finding or skip, no effect on exit status, detection from
    the stream.

  It sits in its own requirement because two commands share it, as
  `extraction`'s ADDED requirement did in the sibling.
- MODIFIED "borax reports a library it has never seen". Its seven
  paragraphs are kept word for word. One paragraph is added: orphans
  are named before any document is opened, and the count equals the
  events. "Two hundred files" gains "names each of the 200 orphans" in
  its THEN. Two scenarios are added.
- MODIFIED "Validation defines a well-formed library". The minimum
  findings list is kept word for word. The counts paragraph becomes
  "named and counted rather than reported as findings". Three
  scenarios are added. Nothing is dropped.
- MODIFIED "borax adopt records what the library already holds". The
  sentence about an index miss gains the report. One paragraph is
  added: every orphan is reported once, and the remaining count equals
  the events. Two scenarios' THEN clauses are extended, and one
  scenario is added.

No `<!-- drops: -->` marker is used, because nothing is dropped.

## Not changed, deliberately

- `session::outcome_for`, `Counts`, `SCHEMA`, `Command::summary` and
  every exit code (D1).
- `item_findings`, `record_findings` and every `Finding` kind (D8).
- `reconcile`, `reconciliation_events` and `Repair`.
- The wording of the `library-status`, `library-validated` and
  `library-adopted` lines.
- `survey` and `Survey`: `status` computes nothing new.
- How `run::adopt_events` and `run::reconcile_events` write: each
  still collects its per-object events and writes them after the last
  file is hashed. That predates this change and deviates from "A run
  reports as it goes". Task 6.3 records it in `STATE.md` as a known
  defect. Fixing it means threading a sink into `library::adopt` and
  `library::reconcile`, which is a restoration of its own.

## Risks / Trade-offs

- **Plain `status` gets long on a library borax has never seen.** 200
  new PDFs print 200 orphan lines above the report. Before this
  change, `status` was a one-line answer, so this is the largest
  visible change. The way out is a verbosity flag, and the roadmap
  defers those. The maintainer may want to weigh it at the critic
  gate.
- **Test churn.** About a dozen existing tests assert exact `status`,
  `validate` or `adopt` event sequences or line lists over libraries
  holding orphans. Tasks 3.1, 4.1 and 5.1 name the ones known, and
  each is updated to the new contract in a red task, not by the
  implementer.
- **Positional consumers.** A script expecting `status --json` to
  print exactly two lines breaks, as it did with the sibling. The
  schema rule does not protect positional readers. The changelog
  announces the addition.
- **One `missing` word, two tests (D4).** A reader seeing `missing` on
  a `validate` condition and `missing` on a `reconcile` repair may
  expect them to agree, and they need not. The manual says which is
  which (task 6.1).
