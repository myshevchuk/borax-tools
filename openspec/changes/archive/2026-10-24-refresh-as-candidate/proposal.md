## Why

A library item is the authority for the files it answers for, and
nothing can ask the services about one again:

- **A tracked file never reaches a service.** `pipeline::standing_in`
  (`crates/borax/src/pipeline.rs`, line 733) answers a tracked file from
  its item through `library_record` (line 346) and asks nothing else.
  It does so "whatever `ResolveConfig::cache` says, since the library
  is not a cache" (line 480). `--no-cache` (`crates/borax/src/cli.rs`,
  line 65) bypasses the response cache (`run::polite`,
  `crates/borax/src/run.rs`, line 4394) and the content index's read
  (`pipeline::index_read`, line 229; references are read only with the
  cache on, line 814), and so reaches only files no library answers
  for.
- **Nothing updates an item.** An item file is written once, when
  `library::select_item` mints it (`crates/borax/src/library.rs`, line
  3160). `admit` leaves an item already there as it is: "an item that
  was already there is not rewritten" (line 2997). `reconcile` and
  `adopt` rewrite or mint artifact records and mint items, and nothing
  else. Only a person editing the file changes an item.

So a service's later correction never reaches a library: a DOI's record
gaining its volume and pages, a preprint's record naming its published
version. Asking again cannot be allowed to replace the item either,
since the item may hold a correction a person made by hand, which is
exactly what the content index lost before change 11.

This is change 14 of the roadmap's Phase 4 ("library authority"), its
last change; release 0.10.0 follows it. The roadmap's done-when is
"fresh lookup cannot silently overwrite library corrections". The Phase
4 library-authority design record (maintainer's notes, outside the
repository) settles three points:

1. A fresh lookup produces a candidate that requires acceptance, and it
   never silently overwrites a library item.
2. Obtaining a fresh candidate and accepting it as a replacement are
   two acts, and the change keeps them apart.
3. This change owns the flag's spelling and the prompt.

The maintainer decided five more for this change, recorded with their
rejected alternatives in design D1 to D5:

- a new `--refresh` on `resolve`, `rename` and `bib`, apart from
  `--no-cache`, which keeps its meaning;
- for a file an item answers for, the item still answers, and a fresh
  lookup of its preferred identifier yields a candidate reported with
  what differs;
- nothing is written in a batch run or a preview, and in an interactive
  `rename` a differing candidate is put to the operator, accepting it
  replaces the item's record as a document edit through the
  `StoreWriter` seam, and declining leaves the item untouched;
- a candidate for another library's item, reached through a reference,
  is reported and never written, and a file answered by evidence or by
  extraction just has its lookup made fresh;
- schema 5 is added to, and is not bumped.

## What Changes

- **`--refresh`.** A per-invocation selector on `resolve`, `rename` and
  `bib`. It is not a setting: `config` refuses it, no configuration key
  sets it, and it has no `--no-` form. Every lookup a run given it makes
  asks the service. The response cache is not read, and each answer is
  stored in it as a network answer always is. Design D1 and D6.
- **A candidate beside the item.** Where a library item answers for a
  file, the run's own item or one a reference names, and the file
  resolves, a run given `--refresh` looks the item's preferred
  identifier up again. The file still resolves to the item's record.
  The fresh record is a candidate, compared with the item's record in
  thirteen fields: the record's CSL fields and the arXiv identifier,
  not provenance, confidence or source fields. Design D2 and D7.
- **Reporting it.** A `resolved` event of a refresh run whose record an
  item answered with carries a `refresh` key after `acceptance`. Its
  `status` is `current`, `differs` (with `differences`, `candidate` and
  an `acceptance` of its own), `unresolved`, `no-eligible-service`, or
  `not-attempted` for an item carrying no identifier. `run-started`
  carries `refresh: true` on such a run. The human `resolved` line gains
  a clause, and the interactive description gains `refresh`, `differs`
  and `decision` lines. Design D8 and D11.
- **Accepting it.** Only an interactive `rename` that records into the
  run's own library asks. An item is the run's own when the run's
  library answered with it, or when a reference names the run's library
  identity at the run's own root, normalised lexically. A reference to
  the same identity at another root is a copy of the library, and is
  another library's. A differing candidate puts a question first,
  before any question about the move, and even for a file that would
  otherwise be passed over. The choices are: keep the library's record
  (the default), replace it with the fresh record, or quit. Replacing
  writes at once through the run's `StoreWriter`, as a document edit of
  the item file:
  - only the differing fields and their provenance entries change;
  - the identity, the file name, every other key, unknown keys and
    comments survive;
  - the whole replacement is refused when a value it would replace
    holds, at any depth, a key the record model does not read (an
    `orcid` in an author table) or a comment, since replacing it would
    discard what the operator was never shown;
  - an item file whose record changed before the re-read made just
    before the write is not written.

  The re-read narrows the race with a concurrent edit and does not close
  it. An edit landing between the re-read and the atomic rename is
  overwritten, and borax takes no lock. Beyond that window nothing is
  lost silently. A `library-replacement` event reports every write, and
  the file goes on with the item as replaced. The run's view of its own
  root is refreshed after a replacement, and an item is asked about
  once per run. Design D3, D9 and D10.
- **Where nothing is written.** The candidate is reported and never
  written in `resolve`, `bib`, a batch `rename` (previewing or
  applying), a run with `--no-record`, and for another library's item.
  The acceptance names why. Design D3 and D4.
- **Schema 5, added to.** No release carries schema 5, and every line a
  run not given `--refresh` writes is unchanged. Added:
  - `run-started.refresh`;
  - `resolved.refresh`;
  - the `library-replacement` event;
  - four reasons: `no-identifier`, `another-library`, `not-recording`
    and `already-asked`.

  No value already in schema 5 changes meaning. Design D5 and D8.
- **Interface.** Design D12.
  - `borax_sources::cache::Cached::refreshing`.
  - A new `borax::refresh` module.
  - `ResolveConfig::refresh` and `FileRecord::refresh`.
  - `Command::refreshing` and `Command::refreshes`.
  - `library::replace_record`, and `Stores::root`, `Stores::identity`,
    `Stores::item_file` and `Stores::learn_item`.
  - `refresh::whose` and `references::Libraries::forget`.
  - `Answer::KeepRecord` and `Answer::Replace`.
  - `Event::LibraryReplacement`, and `refresh` on `Event::Resolved` and
    `Event::RunStarted`.

Explicitly out of scope:

- accepting a candidate outside an interactive `rename`, or field by
  field (Deferred);
- refreshing an item no file of the run answers for (Deferred);
- the known defect that Retry is offered for an identifier no service
  supports, which a refresh never offers Retry for anyway.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED:
    - "A refresh looks a file's library item up again and never
      replaces it": the selector, which files get a candidate, the
      comparison, where nothing is written
    - "A refresh is reported beside the verdict": the `refresh` key, its
      statuses and its acceptance
  - MODIFIED:
    - "Responses are cached locally": `--refresh` beside `--no-cache`
    - "A file its library tracks resolves from its library item": the
      fresh lookup is not the file's record, the question it adds, and
      an item the run replaced
    - "A resolution keeps every service it asked, in order": the refresh
      lookup's attempts
    - "A resolution says why a step was not taken": the refresh and its
      acceptance as steps, and four reasons
    - "Resolution events carry their evidence in sections": the
      additions to schema 5
    - "Human resolution lines name the work, where it came from, and why
      a file was skipped": the refresh clause and the
      `library-replacement` line
- `library`:
  - ADDED: "An item's record is replaced only when an operator accepts a
    refresh candidate"
  - MODIFIED: "A library's first store write mints its identity": the
    replacement is a store write, and a refresh run mints exactly when it
    makes one
- `rename`:
  - ADDED: "An interactive run asks whether a differing refresh
    candidate replaces the library's record"
  - MODIFIED:
    - "Quitting an interactive run leaves the rest untouched": a
      replacement already made stands, and was reported
    - "An interactive run passes over already-named files": a file
      with a refresh to report is not passed over
    - "An interactive question shows what the answer rests on": the
      refresh lines, and a pending candidate as the question's own
    - "An interactive run asks about files it could not settle": the
      already-named file's one question
    - "A candidate the operator abandoned leaves nothing behind": what a
      quit still reports
    - "An interactive question says whether the library answered":
      what the questions offer now changes under `--refresh`
- `cli`, MODIFIED: "A subcommand accepts only the settings it consumes":
  `--refresh` on `resolve`, `rename` and `bib`

Requirements checked and left unchanged:

- `cli`:
  - "JSON Lines output is first-class": applied. Schema 5 is unreleased,
    and every change here is an addition.
  - "Configuration keys and CLI flags share one schema": `--refresh` is
    declared not configurable, so it has no key. `refresh` in a file is
    an unknown key, as `identify` is.
  - "Boolean options are negatable from the command line": `--refresh`
    is not a configurable boolean, so it has no `--no-refresh`.
  - "The apply gate is never configurable": `--refresh` authorises no
    write. A replacement needs an operator's answer.
  - "Non-interactive contexts never prompt": a refresh in a batch run
    reports and asks nothing.
  - "A run reports as it goes": the `library-replacement` event is
    written when the write is made. A file whose refresh question was
    put is not a passed-over file.
  - "A question describes whichever verdict it is asking about": the
    refresh question describes the file's `resolved` verdict.
  - "A run's human summary fits its command": a file shown because of
    its refresh is not counted as passed over.
- `resolution`:
  - "Network use is polite and bounded": a refresh lookup goes through
    the run's paced sources. `resolve` still reports in input order.
  - "A resolution says where its record came from", and "A resolution
    reports where its record was retrieved": the record a file resolves
    to still involves no service attempt. The refresh lookup's attempts
    are the candidate's, and `record_retrieval` keeps naming the item.
  - "Each service asked is reported with its outcome": about the
    `lookup` section, which a refresh does not touch. The refresh's
    attempts take the same shape (ADDED requirement).
  - "A resolution reports its title check and whether its record was
    accepted": the verdict's own `acceptance` stays `automatic` for an
    item's record. The candidate's decision is the refresh's own
    `acceptance`, as a submission's is.
  - "A resolution reports the evidence it was checked against": the
    refresh compares records and reads no title from the file.
  - "An operator's answer about a file is remembered": a refresh writes
    no evidence. Its scenario "Asked once" describes a run not given
    `--refresh`, which the MODIFIED "Responses are cached locally" now
    says of every such statement.
  - "The content index keeps identification evidence, one fact per
    file", and "A library reference answers for a file before the
    entry's evidence": a refresh changes no identification and no
    reference. A reference's answer is still the item's record.
- `library`:
  - "An item is a logical work with a minted identity": the record
    stays text a person reads, diffs and edits, and a replacement is an
    edit of that text.
  - "An item file's name carries a legible key and its identity": a
    replacement renames nothing.
  - "The library's text is authoritative and is read directly": the
    replacement edits the authoritative text, and "A text editor is a
    peer" still holds.
  - "An applying run records what it admits": a preview and a
    `--no-record` run write no item. A replacement is no admission,
    appends no history entry and moves no link.
- `ledger`, "Duplicate detection operates at two levels with distinct
  reasons": its rule that checks answer from "the library as the run
  has left it so far" is why the run's account learns a replacement
  (design D10).
- `rename`:
  - "An interactive run asks before each move": "unless another
    requirement gives it one" covers the refresh question.
  - "A file's verdict follows the operator's decision": the verdict
    still follows the decisions, and the replacement event is not a
    verdict.
  - "A file's verdict keeps every identifier its operator supplied": a
    verdict reached from a supplied identifier also keeps the file's
    refresh.
- `extraction`, `external-tables`, `templates`, `bib-output`,
  `record-model`, `run-logs`: unchanged.

## Impact

- `crates/borax-sources/src/cache.rs`: `Cached::refreshing`, and
  `fetch` honouring it.
- `crates/borax/src/refresh.rs` (new):
  - `Field`, `Difference`, `differences`, `replaced`;
  - `Fresh`, `Decision`, `Whose`, `Refresh`;
  - `refresh` and `whose`.
- `crates/borax/src/cli.rs`: `refresh` on `Command::Resolve`, `Rename`
  and `Bib`, with `Command::refreshing` and `Command::refreshes`.
- `crates/borax/src/pipeline.rs`:
  - `ResolveConfig::refresh` and `FileRecord::refresh`;
  - `standing_in` attaching a refresh;
  - `resolved_event` projecting it.
- `crates/borax/src/evidence.rs`: `Unattempted` gains `NoIdentifier`,
  `AnotherLibrary`, `NotRecording` and `AlreadyAsked`.
- `crates/borax/src/event.rs`:
  - `Event::Resolved::refresh` and `Event::RunStarted::refresh`;
  - `Event::LibraryReplacement`;
  - `RefreshStep`, `DifferenceStep`, `RefreshAcceptance`;
  - the human lines.
- `crates/borax/src/library.rs`:
  - `replace_record`, with its annotation check;
  - `Stores::root` and `Stores::identity`, with `Stores::read` reading
    the identity file, writing nothing;
  - `Stores::item_file` and `Stores::learn_item`.
- `crates/borax/src/references.rs`: `Libraries::forget`.
- `crates/borax/src/session.rs`: `Answer::KeepRecord` and
  `Answer::Replace`, with their menu labels.
- `crates/borax/src/describe.rs`: the `refresh`, `differs` and
  `decision` lines.
- `crates/borax/src/run.rs`:
  - `polite` with a refreshing cache;
  - `resolving` taking the selector;
  - `run-started.refresh`;
  - the rename driver's refresh question, write, run-wide memory and
    passing over.
- Tests: design D15 lists every existing test that changes, and how.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`;
  - `CHANGELOG.md`.
- `openspec/STATE.md` (implementer).
- No new dependency, configuration key or warning. One new flag.

## Deferred

- **Accepting outside an interactive rename.** `resolve` and `bib` are
  never interactive, and a batch `rename` has nobody to ask. A
  dedicated acceptance command, such as accepting every differing
  candidate of a run log or naming items to accept, would be a change
  of its own.
- **Accepting field by field.** Accepting takes every differing field.
  An operator who wants to keep one hand correction and take the rest
  declines and edits the item file, or accepts and edits the
  correction back.
- **Refreshing items with no file in the run.** A refresh reaches only
  the items that answer for the files a run is given. An item with no
  artifact, or whose files are elsewhere, is never refreshed.
- **A record of when an item was refreshed.** The run log holds every
  `library-replacement` event. The item itself carries no timestamp.
