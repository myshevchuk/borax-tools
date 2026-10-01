# Design: report-extraction-per-file

## Context

Everything below was read from source at `83d08b1` (0.7.0).

- `run::status_events` takes the library to report on from
  `reported_root`. That is the library root when a marker or
  `library-root` set one, and otherwise the directory the command was
  given. It surveys that root with `library::survey`. With
  `--identify`, it runs
  `pipeline::from_file(path, documents, &extraction).is_ok()` over
  `survey.artifacts` and counts the passes. It emits one event,
  `library::status_event(&survey, identifiable)`, which is
  `Event::LibraryStatus`. The extraction settings are the run's own
  (`resolving(configs.run().config()).extraction`), because a library
  command has no per-file directory.
- `library::survey` walks the tree and reads both stores without
  opening any document. `Survey::artifacts` holds every artifact under
  the root, sorted by path. Nested libraries, symlinks, sidecars,
  `.borax/` and `items/` are already left out by `walk` and
  `ownership`.
- `pipeline::from_file` opens the file through `Documents::open`,
  reads the title claims, and runs `borax_pdf::tiered::extract`. It
  returns `Result<(Extracted, Vec<Claim>), ExtractionError>`. On
  failure the claims are dropped. `Extracted` carries a
  `FoundIdentifier` and a `Tier`, either `EmbeddedMetadata` or
  `TextLayer`.
- `ExtractionError` has four variants. `Documents::open` reports
  `Unreadable { message }` and `Encrypted`. A parser panic is caught
  and reported as `Unreadable`. `tiered::extract` reports
  `NoTextLayer` when every page the text pass read was only ASCII
  whitespace, including when it read no page at all (no pages, or
  `page_limit` zero). It reports `NoIdentifierFound` when text was
  present. A `page_text` error passes through unchanged.
- `pipeline::skipped_for` maps those four onto two skip reasons:
  `Unreadable` and `Encrypted` become `unreadable`, and `NoTextLayer`
  and `NoIdentifierFound` become `no-identifier`. This change leaves it
  alone.
- The other library events about one object are `library-finding`
  (`path`: the store file, full path), `library-repair` and
  `library-adoption` (`path`: library-relative `String`, `/`-separated,
  with a `kind`-tagged outcome under a field named for the event), and
  `library-admission` (`path`: full path). `validate`, `reconcile` and
  `adopt` each write their per-object events before their totals
  event.
- `event::human_line` matches every `Event` variant exhaustively.
  `Counts::observe` counts `resolved`, `renamed`, `skipped`,
  `already-named`, `lookup-missed` and `library-finding`, and nothing
  else. `Command::summary` maps `status` to `Summary::Silent`.
- `describe::escaped` writes control characters as `\xNN`. The
  interactive description, the session's question and the echo of
  pasted input use it. No line `event::human_line` writes does.
  `openspec/STATE.md` records the unescaped title in the
  metadata-conflict skip line as a known defect.

## D1. The command selects, an inspection examines, a renderer presents

**Decision.** This change sets out a three-part boundary, which change
5 (`list-untracked-missing-unlinked`) will follow:

1. **The parent command selects the subjects.** For this change that
   is `status_events`, and the subjects are `survey.artifacts`. The
   selection is made once, before any inspection runs. It is the only
   place that decides which files or records the run reports on.
2. **An inspection operates on the subjects it is given, with library
   context.** For this change that is `pipeline::extraction`, applied
   to one artifact at a time. An inspection yields one result per
   subject, in the order given. It never adds a subject, never drops
   one, and writes nothing. It may read library context, such as the
   root, the stores or the survey, to examine a subject or to place
   its result. It may not use that context to change the selection.
   In this change, the only library context used is the root, which
   `library::extraction_event` uses to make each path library-relative.
3. **Renderers present structured results.** Each result is one typed
   event, rendered by `json_line` and `human_line`. A renderer never
   computes a result again. A total is calculated from the same
   results the per-object events carry. Per-object events come before
   the command's totals event, and the totals event stays last before
   `run-finished`.

Two rules apply to the whole boundary. A per-object result is not a
skip and not a finding unless a requirement says it is, so adding an
inspection does not change a command's exit status. And a command run
without the flag that asks for an inspection does exactly what it does
today, so plain `status` still opens no document.

This is a boundary between existing functions, not a framework. There
is no `Inspection` trait, no registry and no generic driver. Change 4
needs one inspection with one caller, and a trait with one
implementation would fix a shape before change 5 has shown what it
needs. Change 5 adds its per-object events in the same three places:
the command selects (`orphans`, missing records and unlinked items,
which `survey` and `validate` already compute), a function produces
per-object results, and `event.rs` renders them.

**Rejected: a generic `inspect(subjects, &[&dyn Inspection])`
driver.** It would be the only abstraction in the crate with exactly
one user. It also decides how inspections compose and how their costs
are announced, and the roadmap defers those questions with the default
and cost flags.

**Rejected: letting the inspection walk the library.** If extraction
were given the root, it would need its own idea of which files count,
and that could drift from the survey's. Taking the subjects from the
caller is what makes "never widens the selection" true by
construction rather than by matching rules.

## D2. The result vocabulary, and how `borax-pdf`'s errors map onto it

**Decision.** The reported result is a `kind`-tagged enum in
`event.rs`, next to `SkipReason`, `Finding`, `Repair`, `Adoption` and
`LibraryAnswer`:

| `kind` | Fields | From | Meaning |
|---|---|---|---|
| `found` | `identifier`, `tier` | `Ok(Extracted)` | an identifier was found; `identifier` in the stream's form (`doi:…`, `arXiv:…`, as `resolved.found` writes it), `tier` the pass (`embedded-metadata` or `text-layer`, as `Tier::as_str` writes it) |
| `no-text-layer` | — | `NoTextLayer` | no identifier in the metadata, and no page the text pass read held text, including when it read none |
| `text-without-identifier` | — | `NoIdentifierFound` | no identifier in the metadata, and text was read but held none |
| `encrypted` | — | `Encrypted` | the document cannot be read without a password |
| `unreadable` | `message` | `Unreadable { message }` | the file could not be opened or parsed as a PDF; `message` is the reader's own |

`pipeline::extraction_of` performs the mapping as an exhaustive
`match` with no wildcard arm. If `borax-pdf` ever gains a fifth
failure, for example from a second backend, the code will not compile
until someone decides how that failure is reported. The vocabulary
belongs to borax, not to `borax-pdf`: renaming a variant in the PDF
crate must not rename a token in the stream. `event.rs` therefore does
not import `borax-pdf`, as it does not today.

**The two controlled cases decide the boundary.** A blank page with an
embedded title is `no-text-layer`. Readable prose with no identifier,
also with an embedded title, is `text-without-identifier`. In both
cases the title holds no identifier. The result answers one question:
did extraction find an identifier, and if not, why not.

A title enters that answer only through the passes this change keeps.
`borax_pdf::scan::scan_info` scans the Info dictionary's values for
identifiers in a fixed order: every custom key, then `subject`,
`keywords`, `title` and `author`. A blank PDF whose Info `Title` is
`10.1234/example` therefore yields `found` with `embedded-metadata`
today, and still does after this change. The XMP `dc:title` is not
scanned by `scan_xmp`, which looks for identifier elements. So the
rule is narrower than "a title is never an identifier". A title counts
only through an identifier the metadata pass recognises in it, and a
title holding none changes nothing. Changing which metadata fields the
pass scans would change the passes the living `extraction`
requirements fix, and is not this change's business.

A title holding no identifier is evidence about the work, not an
identifier. Reporting it belongs with Phase 3's retained file
evidence, which also covers the claims `from_file` drops on failure
today. So the result carries no
claims, not even on success. If it carried claims only for a `found`
result, it would look like the evidence Phase 3 means to keep while
missing exactly the cases that need it.

**Why `text-without-identifier` and not `no-identifier`.** The skip
reason `no-identifier` covers both `NoTextLayer` and
`NoIdentifierFound`. If the extraction result reused that token for
the narrower case, the same word in one run log would mean two
different things depending on which event carried it. A consumer
filtering on `no-identifier` would then match a subset of extraction
results along with every such skip. Distinct tokens avoid that. Phase
3 can adopt these names when it restores the distinctions in skip
reasons, or choose others under its schema bump, and nothing here
constrains that choice.

**Rejected: reuse `SkipReason`.** That would bring back the collapse
this change exists to remove, and it would make an extraction result
look like a skip to anything that matches on `reason`.

**Rejected: report `ExtractionError`'s `Display` text as the result.**
Text is not a vocabulary. The `Display` wording is the PDF crate's to
change at any time.

**Rejected: report the scanned page range on `no-text-layer` and
`text-without-identifier`.** The page limit is the run's setting and
is the same for every file. A per-file field would repeat it on every
line. If a reader needs it, Phase 3's "scanned scope" evidence is
where it belongs.

## D3. Where the operation lives, and its signature

**Decision.** Two functions in `crates/borax/src/pipeline.rs`, next to
`from_file`:

```rust
/// What extraction made of one file, in the reported vocabulary.
pub fn extraction_of(result: &Result<Extracted, ExtractionError>) -> Extraction;

/// Run the extraction passes over the file at `path` and report the
/// result. Opens only `path`, asks no service, reads and writes no
/// cache, and never fails: every failure is a result.
pub fn extraction(path: &Path, documents: &dyn Documents, config: &ExtractionConfig) -> Extraction;
```

`extraction` is `from_file(path, documents, config)` with its claims
dropped and the result passed through `extraction_of`. Because
`status --identify` and `resolve` call the same function, they cannot
run different passes. That is what the living requirement's "the same
extraction passes `resolve` runs" asks for. Reading the claims costs a
title lookup on a document already parsed, which is not worth a second
code path to avoid.

`pipeline.rs` is the module for turning a file into a record. It
already owns `Documents`, `from_file` and the mapping to skip reasons,
and it is where the three intended reusers will look:

- Phase 3's resolution evidence will call `extraction_of` inside
  `standing`, on the result it already has.
- A validate-in-status inspection will call `extraction` over the
  files it selects.
- A public extraction command will call `extraction` from `run.rs`, as
  `status` does.

The operation is per file. The caller's loop is the selection, so no
batch function exists for a selection to leak through.

**Rejected: put it in `library.rs`.** `survey` opens no document, and
"`borax status` SHALL open no document" is easiest to keep when the
module that holds the survey never touches `Documents`. Extraction is
also not library state: a future public command will extract from
files that belong to no library.

**Rejected: a batch `extract_each(paths, …) -> impl Iterator`.** A
lazy map can be written in one line at the call site. A function for
it would only make sense as the generic driver D1 rejects.

**Rejected: a new `inspect` module.** With one inspection, it would
hold two functions that belong next to `from_file`. Change 5's
inspections read the stores, and their natural home is `library.rs`.
A shared module would separate both from their neighbours to express
a grouping that exists only in this design.

## D4. The event

**Decision.**

```rust
/// What extraction made of one artifact a `status --identify` run
/// inspected.
Event::LibraryExtraction {
    /// The artifact, library-relative and `/`-separated, as
    /// `library-adoption` names one.
    path: String,
    extraction: Extraction,
}
```

```json
{"schema":3,"event":"library-extraction","path":"sub/a.pdf","extraction":{"kind":"found","identifier":"doi:10.1234/x","tier":"text-layer"}}
{"schema":3,"event":"library-extraction","path":"b.pdf","extraction":{"kind":"no-text-layer"}}
{"schema":3,"event":"library-extraction","path":"c.pdf","extraction":{"kind":"unreadable","message":"…"}}
```

The shape follows `library-adoption { path, adoption }` and
`library-repair { …, path, repair }`: one event per object, a
library-relative path, and a `kind`-tagged outcome under a field named
for the event. A consumer dispatches on `event` and then on
`extraction.kind`, as it already does for the other two.

**Why the `library-` prefix.** The `path` is relative to the root that
`library-status` names on the same run. That makes it a library-scoped
path, and every event with such a path carries the prefix. A future
public command extracting from as-given paths should emit an event of
its own that reuses the `Extraction` value. It should not reuse a tag
whose `path` would then mean two different things. What is reusable
is the operation and the outcome type, not this event.

**Rejected: one list on `library-status`.** An `extractions` array on
the totals event is also an addition. But nothing could be written
until the last file had been extracted, which breaks "A run reports as
it goes", and the totals event would grow with the library.

**Rejected: two tags, such as `identifier-extracted` and
`extraction-failed`.** A consumer would have to join two tags to
account for every file. The house style is one event per object with
a tagged outcome.

**Rejected: emit `resolved` or `skipped`.** `resolved` claims a
record. `skipped` is counted, so it would make `status --identify`
exit partially on any failed file. That is an exit-status change,
which is out of scope, and it would also mix an extraction outcome
into the resolution skip queue.

## D5. Order, streaming and the totals

**Decision.** For each artifact in `survey.artifacts` order,
`status_events` calls `extraction`, emits that artifact's
`library-extraction` event straight away, and adds one to a running
count when the result is `found`. After the last artifact it emits
`library-status` with that count as `identifiable`. Without
`--identify` there are no per-file events, `identifiable` is `None`,
and no document is opened. The function shapes do not change: the
count is calculated from the results that were emitted, so it agrees
with them by construction, and a test checks this anyway (task 3.1).

`library-status` stays the last event before `run-finished`. That
keeps the human report line last, as "A run's human summary fits its
command" requires, and matches `validate`, `reconcile` and `adopt`,
which also write per-object events before their totals.

Extraction stays serial, as it is today. Running files concurrently
would mean either writing results out of order or buffering them, and
`stream-per-file-events` decided against buffering. Speed is a cost
question, and the roadmap defers cost questions.

The ordering tests (tasks 3.1 to 3.4) would also pass if the events
were collected and written after the loop. Task 3.5 pins the
streaming separately. It checks the writer when each document is
opened, as the existing liveness test in `streaming.rs` checks it
when each file is hashed.

## D6. Paths: library-relative, and not marked tracked or untracked

**Decision.** `path` is the artifact's library-relative path,
calculated by `library::library_relative(&survey.root, artifact)`. That
gives `/` separators on every platform, the same as `library-adoption`
and `library-repair` and the same as the paths artifact records store.
`library-status` names `root` on the same run, so `root` plus `path`
gives the file. Every surveyed artifact lies under the root, so
`library_relative` answers for all of them. If it ever returned
`None`, `extraction_event` falls back to the artifact's path as
`Path::display` writes it, with the separators converted to `/`. It
does not panic.

The event does not say whether the artifact is tracked.

- In `resolution`, "tracked" means that a record names the path *and*
  holds the file's bytes. Checking that needs every artifact hashed,
  which `status --identify` does not pay for today. Adding that cost
  is a decision about cost, and the roadmap defers it.
- A cheaper "a record names this path" flag, the survey's own
  not-an-orphan test, would give "tracked" a second, weaker meaning
  next to the resolution one.
- Change 5 owns per-object reports of untracked PDFs. It will name
  them by the same library-relative path, so a reader can join the two
  sets of events. The join shows only whether an artifact record names
  each identified file's path, which is `library::orphans`' test. It
  does not show that the file at that path is the one the record
  describes. `Stores::consult` checks that by hash. A PDF replaced at
  a recorded path is not an orphan, but `consult` answers
  `unrecognised-content` for it. The join therefore answers half of
  the review's "are the five tracked files the five identifiable
  ones": it confirms registration by path. Checking the content stays
  deferred, along with the cost of hashing every artifact.
- Comparing the extracted identifier with the linked item is
  resolution-evidence or validate-in-status work.

**Rejected: the path as given (absolute, `PathBuf`).** `skipped` and
`resolved` name paths as the user gave them, because their subjects
are command-line inputs. The subjects here are the library's own
artifacts, and the library's own events name those library-relative.

## D7. Human rendering

**Decision.** One line per artifact, for every result kind, using the
house per-object form `<path>: <clause>`:

| Result | Line |
|---|---|
| `found`, `embedded-metadata` | `<path>: identifier <identifier> from embedded metadata` |
| `found`, `text-layer` | `<path>: identifier <identifier> from the text layer` |
| `found`, any other `tier` | `<path>: identifier <identifier> from the file` |
| `no-text-layer` | `<path>: no identifier found; the pages read hold no text` |
| `text-without-identifier` | `<path>: no identifier found in its metadata or the pages read` |
| `encrypted` | `<path>: encrypted, so no identifier could be read` |
| `unreadable` | `<path>: unreadable (<message>)` |

The origin clauses are the interactive description's wording
(`describe::whence`): "from embedded metadata", "from the text layer",
and "from the file" as the fallback. A person who has seen one has seen
the other. `unreadable (<message>)` is the wording of the `unreadable`
skip and of the `unreadable` finding. The two failures with no
identifier both begin `no identifier found`, so a reader sees them as
two cases of one outcome. The report line follows, unchanged.

**Why every file and not only the failures.** The review's complaint
was that two files failed and neither was named. Listing only failures
would fix that. But the review's second question, which files were
identified, would still need JSON to answer. The roadmap's done-when
condition is that per-file results reach both renderers. A person
types `--identify` to ask what extraction makes of the library, and the
command already pays for a pass over every file. `reconcile` stays
silent about confirmed records because nobody asked about them, and
here somebody did. A quieter mode would be a verbosity flag, and
those are deferred.

**Escaping.** The path, the identifier and the message are each
passed through `describe::escaped` before formatting. The path comes
from a file name, the message from the PDF parser reading the file's
bytes, and the identifier from the file's text after normalisation. None
of them is borax's own text. `STATE.md` records the unescaped
metadata-conflict line as a defect to be fixed in one place. This new
line is written to that standard from the start, so the defect does
not grow. The lines that already exist are left as they are. JSON
carries the raw values, because JSON is escaped by its own encoding.

**Rejected: a closing line naming the failures (`2 files yielded no
identifier: a.pdf, b.pdf`).** It would repeat what the per-file lines
say. It would also be a summary line for `status`, which "A run's
human summary fits its command" rules out, and the report line would
no longer be last.

## D8. Not a skip, not a finding

**Decision.** `Counts::observe` ignores `library-extraction` through
its existing wildcard arm. `run-finished` counts nothing for the event,
`Command::summary` still maps `status` to `Silent`, and
`session::outcome_for` sees no new total. A `status --identify` run in
which every artifact fails extraction exits 0, as it does today.

An extraction failure describes a file. It does not report a decision
the run declined, and the library is no less well formed because of it.
Whether callers should detect conditions in a library through the exit
status is a question the roadmap gives to change 5's design.

## D9. Schema: an addition under version 3

**Decision.** `SCHEMA` stays 3. The `cli` rule changes the version when
an event or reason is removed or renamed, or a field changes meaning.
It does not change the version for an addition. `library-extraction` is
a new event tag. A consumer that ignores tags it does not know reads
every `status --identify` stream exactly as before: the same
`library-status` event, carrying the same `identifiable` with the same
meaning, followed by `run-finished`. No field on any existing event
changes, and no skip reason changes. The one difference such a consumer
could notice is extra lines, and the rule already accepts that for an
addition. Earlier additions kept the version on the same terms: the
`supplied` tier and the `found` and `claims` fields under schema 2,
and `consult-library-first`'s `library` field under schema 3.

## D10. The interface the tests are written against

```rust
// crates/borax/src/event.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Extraction {
    Found { identifier: String, tier: String },
    NoTextLayer,
    TextWithoutIdentifier,
    Encrypted,
    Unreadable { message: String },
}
impl Extraction {
    /// Whether an identifier was found: the results `identifiable`
    /// counts.
    pub fn is_found(&self) -> bool;
}
// Event::LibraryExtraction { path: String, extraction: Extraction }

// crates/borax/src/pipeline.rs
pub fn extraction_of(result: &Result<Extracted, ExtractionError>) -> Extraction;
pub fn extraction(path: &Path, documents: &dyn Documents, config: &ExtractionConfig)
    -> Extraction;

// crates/borax/src/library.rs
/// The `library-extraction` event reporting `extraction` for the
/// surveyed artifact at `artifact`, named relative to `survey.root`.
pub fn extraction_event(survey: &Survey, artifact: &Path, extraction: Extraction) -> Event;
```

`status_event(&Survey, Option<usize>)` and `survey(&Path)` keep their
signatures. `run::status_events` is private and is tested through
`events_for` and `dispatch`, as it is today.

## D11. How the specifications change

**Decision.** There are two deltas:

- `library`: MODIFIED "borax reports a library it has never seen". It
  repeats the three existing paragraphs word for word, and adds
  paragraphs for the per-file result, the selection, the event and its
  order, the agreement between the count and the results, the human
  line with escaping, and the rule that a result is neither a skip nor
  a finding. All three existing scenarios are kept by name. "What is
  identifiable is asked for" keeps its THEN clause and adds that each
  artifact is reported. The new scenarios cover the cases the roadmap
  names: a success with its pass, the blank page with a title,
  readable text without an identifier, encrypted versus unreadable,
  the selection boundary, the exit status, and escaping. There is no
  `<!-- drops: -->` marker, because nothing is dropped.
- `extraction`: ADDED "Extraction reports one result for each file it
  is given". The roadmap asks for an operation that later changes can
  reuse. Its contract belongs in the capability those changes will
  read, not inside the `status` requirement. The contract is one
  result per given file, the given order, no file added, five kinds
  that are never merged, a title counted only through an identifier
  the metadata pass finds in it, and
  library context that cannot widen the selection. The requirement
  says plainly that it does not govern skip reasons, so it cannot be
  read as requiring Phase 3's restoration now.

"Extraction failures are typed and non-fatal" is not modified. It
already requires the four modes to be told apart, and this change
satisfies it for `status`. Rewriting it to excuse the skip reasons'
collapse would change an authority to fit the code, which is the
opposite of what Phase 3 is scheduled to do.

## Not changed, deliberately

- `skipped_for` and every skip reason in `resolve`, `rename` and
  `bib`.
- `from_file`'s signature and its dropping of claims on failure.
- The wording of the `library-status` line, including "identifiable"
  and "orphans".
- `status` without `--identify`: one event, and no document opened.
- The panic trace a parser panic prints to standard error, which
  `STATE.md` records. A `library-extraction` event reports the file as
  `unreadable`, and the trace still appears above it.

## Risks / Trade-offs

- **Long human output on a large library.** A library of 2,000 PDFs
  prints 2,000 lines under `--identify`. That follows from choosing one
  line per file (D7). The way out is a verbosity flag, which the
  roadmap defers. The cost of the pass itself is the same as before.
- **A consumer that counted `status` events.** A script that expects
  `status --identify --json` to print exactly two lines will now see
  more. The schema rule (D9) does not protect positional readers, and
  the changelog announces the addition.
- **The controlled blank-page case is a fake, not a fixture.** The
  committed corpus's `no-text-layer.pdf` is a rasterised page with no
  title, so the blank page with an embedded title is tested through
  `FakePdf` in `dispatch.rs` and `pipeline.rs`. The real backend is
  covered end to end on `no-text-layer.pdf` and on `no-identifier.pdf`,
  which does carry an Info `Title` (task 3.3). Generating a new corpus
  fixture needs ghostscript, which neither CI nor this change requires.
