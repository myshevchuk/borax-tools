# State of borax-tools

A running assessment of where the project actually is, kept apart from
the specifications because they describe intent and this describes
reality. Read it before planning a change or cutting a release; update it
whenever it stops being true, and at the latest before every version
bump.

Last reviewed: 2026-10-18, at 0.9.0 with `reduce-content-index` merged
and `index-references-libraries` implemented on its branch.

## What is built

The `add-core-pipeline` change is implemented and archived, so the living
specifications in `openspec/specs/` describe working code rather than a
plan. That covers the whole path from a file to a renamed file: tiered
PDF extraction, identifier normalization, resolution against Crossref,
OpenAlex and arXiv with on-disk caching and per-service pacing, the
CSL-JSON record model, the template engine, collision-aware rename
planning, and BibTeX output to a master file or to sidecars.

`stream-per-file-events` is implemented on top of that. A
run writes each event when it happens rather than assembling the whole
stream first, and `rename` and `bib` work one file at a time, so a
file's verdict and its fate are adjacent. The decisions are unchanged:
`borax_core::rename::Planner` is the batch planner's own state made
drivable one input at a time, and the batch entry point is a fold over
it.

`add-ledger-and-run-logs` is implemented and archived on top of both. A
collection now keeps account of what it has admitted: a ledger at
`.borax/ledger.jsonl` beside the nearest `.borax.toml`, duplicate
checking in `rename` by content and by work, `borax ledger rebuild` to
derive the ledger back from the files and sidecars themselves, a run
log per run. `journal.rs` is deleted and nothing reads a v0.1.0
`renames.jsonl`.

`remove-undo` is implemented on top of all three. `borax undo` is gone,
along with its engine, its event vocabulary, and the latest-apply-log
selector that fed it. Nothing replays a run any more: an applied rename
is undone, when it has to be, by reading the run log, which is unchanged
and still mandatory and pre-flushed for `rename --apply`. Re-running a
corrected template is the ordinary answer, since files are identified by
content rather than by name.

`add-external-tables` is implemented and archived on top of all four,
and ships in 0.4.0. A template can now consult a file the user curates
outside borax: configuration declares named lookup tables by path and
by which of their columns supply keys and values, and a
`lookup("<table>")` filter substitutes what one holds for whatever
reached it. Matching goes through the fold `slug` always performed,
now public and stated normatively, because the point of the change is
that one curated file answers the same way for borax and for the other
tool that reads it. A table's values are literal text, or template
fragments compiled when the table loads, which is how a row rather
than a template decides whether a journal's volume belongs in the
name; a fragment may not itself look anything up, so rendering stays
total and terminating. Five publication fields — `volume`, `issue`,
`pages`, `firstpage`, `publisher` — and the two affix filters that
keep an absent segment from leaving its separator behind arrived with
it. The failures are all in preflight beside template compilation, a
miss renders empty and is reported once per distinct table and input
and counted in the summary, and `run-started` names every table read
by path and content digest.

`scope-cli-flags-per-subcommand` is implemented and archived on top of
all five, and ships in 0.4.0 beside it. The flag surface is
per-subcommand: each command declares the settings that can change what
it reports, writes or moves, so a subcommand's `--help` describes that
subcommand and naming an inapplicable setting is an unknown argument
rather than a silent no-op. What that costs is position, spent
deliberately — a setting flag follows its subcommand, and `--json` alone
is still accepted on either side, since an argument propagates down from
a parent and never up from a child. `--template` is gone with it, which
leaves the three open-ended tables — `templates`, `citation-keys`,
`tables` — configuration-file-only without exception. `borax config`
keeps every flag, because an override there is the question it answers
rather than a no-op, so the thirty-odd flag-layering tests kept their
subjects.
Configuration itself is untouched: layering, precedence, origins and
`borax config` output are what they were, and a file or an environment
variable still sets every key whatever command runs. A command now
compiles only the template tables it renders from, so a filename
template that will not compile ends a `rename` and no longer ends a
`bib`.

That `rename` and `bib` resolve serially is now visible in the command
line rather than only in the code: `--concurrency` is declared on
`resolve` and on `config`, and naming it on a `rename` is refused
instead of accepted and ignored. The open decision below is unchanged
— what would put the flag back on `rename` is an answer about ordering,
not about the flag.

Tests ran green on Linux, macOS and Windows as of 0.4.0, including
integration tests over real PDFs and Windows coverage of the path
shapes discovery and run-log placement depend on; CI now checks Linux
alone (see Live risks). A scheduled job exercises the real
APIs, so schema drift at a source surfaces without a user finding it
first.

`add-interactive-rename` is implemented on top of all six and is the
first of four changes making the rename workflow interactive. A rename
run with a terminal on stdin and human output asks about each move it
would make — rename, skip, or quit — and an accepted answer moves that
file there and then; `--batch` gives the preview that used to be the
default, and `--apply` selects a batch run by itself, so every existing
invocation means what it meant. A declined proposal claims no name, so
it costs the next file nothing, and quitting leaves the rest untouched
and counted. The planner gained the split this rests on: deciding a
target and claiming it are two steps, and a batch plan is the two in
sequence.

Two things came with it that outlive the session. `rename.batch` is
read from the run's own configuration rather than per input directory,
because a run is one session with one operator. And every applying run
now writes a move to its log before making it: the whole-plan pre-flush
the specifications promised stopped being possible when
`stream-per-file-events` made a run decide one file at a time, and the
implementation had been writing each event after its move ever since.
That divergence is closed rather than inherited.

`inquire` is the first dependency borax has taken for the terminal
itself. It is the one part of the change the suite does not cover: the
adapter translates a question into a menu and its answer back, and it
was verified by hand against the real-PDF corpus through a pty.

`skip-already-named-files` is implemented on top of it, and closes two
defects this file recorded as known. A file carrying the name its
record implies is an outcome of its own rather than a skip, so a
collection in order exits 0; an interactive run passes over such files
silently unless `--no-skip-named` asks for them; and a file the ledger
admitted is no longer reported as a duplicate of itself, the lookups
walking past their own entry so a second copy elsewhere is still
found.

Two more corrections came with it, both of them things the duplicate
check had been hiding rather than anything this change broke. A
rendered subdirectory is filed from the collection root, so a filed
collection is left alone instead of being nested one level deeper on
every run and a paper whose journal changes moves across rather than
down; outside a collection there is no root and filing stays relative
to the file's own directory. And a collision suffix that lands on the
name a file already carries is already-named rather than a move the
filesystem would refuse.

The names claimed under one base now belong to the run rather than to
one directory, which is what keeps a preview and an applying run
agreeing when two directories file into one journal. The event schema
version is 2, the first bump since the stream existed: removing a skip
reason is not something a consumer can ignore.

`show-record-before-asking` is implemented on top of those, and is the
third of the four interactive changes. A question is now preceded by
the evidence its answer rests on: the identifier the run looked up and
where it was found, the services that supplied the record, what the
record says, the titles the file claims for itself, and the name the
file would take. It is written where the question is, on standard
error, so a run whose stdout is redirected still asks with its evidence
attached.

Two things it had to settle are worth remembering. The identifier the
stream reported was the one the record is filed under, which is not
always the one that was looked up — an arXiv identifier resolves to a
record carrying a DOI — so `resolved` now carries both, and a
content-index answer, which looked nothing up, names no origin at all.
And everything the description quotes from a record or a file is
escaped: a PDF's title is written by whoever made the file, and an
escape sequence in one could have redrawn the question above a menu
whose first choice is Rename.

`supply-identifiers-interactively` completes the four. A question is
now put to every file the run could not settle on its own — no
identifier, no service holding one, unreadable, a title conflict — and
not only to the files that resolved, which is where the hit rate
actually went. The operator can give a file an identifier by hand,
accept a record over a conflict, ask the services again after an
outage, or keep the name an already-named file has. A record accepted
this way is written to the content index on the move, as its
identifier, since `reduce-content-index`, so later runs, batch
included, are answered without asking.

The measured problem it addresses: on a slice of eight files from the
real corpus, two resolve unaided. Of the six that do not, three carry
no identifier, one is a patent, one has a title conflict, and one
carries a DOI with `.author` appended — an author manuscript, which no
service holds. That last is the case the whole change was proposed
for, and it is now a DOI typed once.

Four things it settled are worth remembering. A file's verdict is held
until its fate is settled, so a file re-identified by hand is reported
once, from the record that settled it; that is a longer hold than
`stream-per-file-events` allowed, and the requirement was amended
rather than contradicted. `supplied` is not an extraction tier —
borax-pdf names passes over a file and knows nothing about operators —
so `FileRecord` carries a borax-level `Provenance`. An abandoned
candidate is never indexed or cited, which the driver has to hold
deliberately because the batch path cites every file it resolves;
since `operator-input-evidence` it is reported, as a rejected
submission in the file's verdict and never as its resolution. And the index write happens on
rename alone, because a mistyped identifier that resolved to the wrong
paper must not be served for that file forever after.

Its review gate found two defects worth recording for what they say
about the suite rather than about the change. A record the operator
supplied or accepted over a conflict never went through the ledger's
work check, so the collection could take a second copy of a paper it
already held — the one thing the ledger exists to prevent. And
quitting an interactive run left the output hold open, so the summary
and any bibliography line after it were buffered and dropped: a run
that ended with no output at all. That one predates this change, and
survived because the quit test runs in JSON, where a hold does
nothing. Both are fixed and both now have tests that fail without the
fix. The lesson is that the interactive human-output paths are the
thinnest-covered part of the suite, because most dispatch tests assert
on events rather than on what a terminal was shown.

This change also amended three requirements it inherited, in its own
group 0, rather than leaving the archive holding both halves of a
contradiction — the first change in the project to need that, and the
pattern to copy when a stacked change contradicts one below it.

`add-library-store` is implemented and archived on top of all of
these, ships in 0.6.0, and replaces the collection's
accounting with a library store. A library is the tree under the nearest
`.borax.toml`, or under the configured `library-root`; its boundary is
lexical, a symlink is neither an artifact nor an orphan, and a nested
`.borax.toml` is a library of its own that the outer one's commands do
not see into. It keeps two stores of plain TOML files read directly,
with no index: `items/<key>.<uuid>.toml`, one item per work holding its
record, and `.borax/artifacts/<uuid>.toml`, one artifact record per file
holding its item link, its library-relative path, its size and
modification time, and its hash history. An item and an artifact record
each carry a v7 UUID minted once, and the `id` inside a file is what
every reference names, never the file name.

Four commands read and keep it. `status` counts what a tree holds and
opens no document, so a directory borax has never seen is reported on
with no preceding step; `--identify` adds an extraction pass and still
queries nothing. `validate` reports what is wrong with the records
themselves and repairs nothing. `reconcile` brings records back to
files moved out of band, by size and modification time first and by
hash after, in four ordered steps that let two swapped files both
repair; an ambiguous match is left alone and named. `adopt` records,
offline, each orphan the content index already knows, and holds back
one whose bytes a record already has, since that is a moved artifact
for `reconcile` rather than a new one. An applying rename records what
it moves or finds already named, item first so an interruption leaves
an unlinked item rather than a dangling link, and every store write is
an atomic whole-file replacement of a document edited through
`toml_edit`.

Both duplicate checks now answer from the artifact store, and from the
library as the run has left it so far rather than as it stood when the
run began: the first version read a snapshot, and the hand
verification on the real-PDF corpus caught it admitting a byte-identical
pair twice and warning falsely about paths to reconcile. A preview
learns what it would admit on the same terms. `--no-record` turns both
checks and every write off, and refuses the one move that would strand
a record naming the file with no hash of its bytes.

The ledger is retired. `.borax/ledger.jsonl` is neither read, written
nor deleted, `borax ledger` and the `ledger-rebuilt` event are gone,
`ledger`/`--ledger` became `record`/`--record` and `collection-root`
became `library-root`, with no aliases. No migration is owed before
1.0.0: `adopt` or an applying rename populates the store, and the old
file can be deleted by hand. The `ledger` capability keeps its name in
`openspec/specs/` although it now specifies the store's duplicate
checks and record gate, because a capability rename is not something
archiving expresses; renaming it is a spec-only change of its own. The
event schema version is 3.

This change also settles what a sidecar is: citation output for other
tools, which governs no decision borax makes. Nothing reads a sidecar
to build library state, and `adopt` deliberately does not.

What the next change in the stack adds on top is the organizational
half the store was built to carry: views, nodes and memberships, with
the filesystem-backed view among them. That view derives from the
recorded size and modification time, which is why a reconcile refreshes
them on every record it confirms, including after a bare `touch`.
Template filing becomes assignment to that view once views exist, and
the `rename` filing requirement was left untouched here for that
reason.

`fit-summary-to-command` is implemented on top and ships in 0.7.0.
The human rendering of `run-finished` is chosen per command through
`Command::summary`: `rename` keeps its line byte for byte, `resolve`
and `bib` close on `N resolved, N skipped`, and the library commands,
`config` and `cache` close on their own report line. A skipped,
unreached or findings total is still named where a shape would say
nothing, so a partial-success exit never ends on a clean-looking
terminal. `human_line` renders `run-finished` as nothing, because the
event does not name the command. JSON and run logs are unchanged and
the event schema version is still 3. It is the first change of the
roadmap drawn from the interactive reviews of 0.6.0, which lives
outside the repository.

`consult-library-first` is implemented on top of that, as the second,
and ships in 0.7.0 beside it.
`resolve`, `rename` and `bib` read a start-of-run snapshot of the
run's library (`Stores::consult`) before the content index. A file
whose recorded path holds a record with its bytes in any history
entry, linking one readable item, resolves from that item with no
extraction, no service and no content-index traffic, whatever
`--no-cache` or `--no-record` say. Any other answer from a library
that records the path is a `LibraryAnswer` problem carried on the
fallback's `resolved` or `skipped` event, never a skip of its own.
`store_files` now treats only a missing directory as an empty store,
so `validate` reports an unlistable one. Schema 3 is kept: the
`library` field and the `library` values of `tier` and `source` are
additions, and Phase 3 of the roadmap migrates them. Two restorations
landed with it: override discovery climbs from a normalised start, and
`library_relative` and `excludes` normalise both sides, so relative
input spellings find and are admitted into their library. The two
items it left open for the roadmap's change 15 — unrecognised bytes at
a recorded path appended to its history, and history entries recording
no item — are closed by `separate-change-from-acceptance`, below.

`report-extraction-per-file` is implemented on top, ships in 0.8.0,
and opens Phase 2 of the roadmap. `status --identify` writes a
`library-extraction` event per surveyed artifact as its extraction
finishes, carrying `found` with the identifier and pass, or one of
`no-text-layer`, `text-without-identifier`, `encrypted` and
`unreadable`; `identifiable` is counted from the same results.
`pipeline::extraction` is the reusable per-file operation, built on
`from_file` so status and resolution run the same passes, and
`pipeline::extraction_of` maps each extractor outcome to its own kind
with no wildcard arm. It records the boundary change 5 follows: the
command selects the subjects (`status_events` over
`survey.artifacts`), an inspection gives one result per subject and
writes nothing, and the renderers present typed events, with any
totals taken over the same results. Schema 3 is kept.

`list-untracked-missing-unlinked` is implemented on top, as the second
user of that boundary, ships in 0.8.0 beside it, and closes Phase 2. A
`library-condition` event names each counted object: `status` writes
one per orphan as soon as the survey is read and before any document
opens, and `validate` writes one per orphan, missing record (with its
artifact id and its own record file, which tells apart records sharing
an identity) and unlinked item after its findings, with every total
counted from `Validation::conditions`. `adopt` reports an orphan the
content index cannot answer for as `Adoption::Unindexed`, so each
orphan it sees gets exactly one event. Conditions are never findings,
and exit status is unchanged: the living requirements call these
healthy states and a missing record never clears, so scripts read the
stream; an opt-in failure flag is the deferred route. Schema 3 is
kept.

`expose-resolution-attempts` is implemented on top, ships in 0.9.0,
and opens Phase 3 as its change 8. It is engine-only: no event, human line, count or
exit status changes, and schema 3 is kept. `FileRecord` and `Standing`
each carry an `Evidence` (`crates/borax/src/evidence.rs`) with one
section per step: the library consultation, the content index's read
(hit, miss, bypassed, unavailable with the hashing error, or not
attempted) and write, extraction's result in the five-kind vocabulary
beside the titles (read, failed, or not attempted), the lookup with
its identifier, its origin (an extraction pass or the operator) and
every service attempt in order with a structured outcome and, for the
found one, whether the response cache or the network answered, and the
title check (agreed, conflict, insufficient with its cause, or not
attempted). Every step that did not run says why, from one
`Unattempted` vocabulary. The schema-3 fields `source`, `tier`,
`found`, `claims`, `cached` and `library` are methods derived from it,
with `source`, `tier` and `cached` all read from `Evidence::retrieval`;
`Standing::evidence` equals the evidence of the record its verdict is
about, the refused candidate's included. `Source::fetch` returns
`Fetched`, `dispatch::Resolved` keeps the failures before the
answering source, and `Cache::put`, `ContentIndex::put` and
`pipeline::remember` return their write's result, which never fails a
run. The interactive driver keeps the file's own evidence apart from a
candidate's: a retry becomes its current lookup and, since
`operator-input-evidence`, keeps the rounds before it; a supply never
touches it. A record an
operator reached, or accepted over its own conflict through
`pipeline::accept`, records its content-index write as awaiting
acceptance; the write itself is made at the move, and `remember`'s
result is held beside the move's outcome. `sectioned-resolved-event`
renders all of it.

`sectioned-resolved-event` is implemented on top, ships in 0.9.0, as
Phase 3's change 9 with change 3 folded in, and makes the single bump: the event
schema version is 4. `resolved` no longer carries `found`, `cached`,
`source`, `tier`, `claims` or `overrode`. It and every `skipped` event
that is a resolution verdict carry seven sections in pipeline order —
`library`, `content_index`, `extraction`, `lookup`,
`record_retrieval`, `match_check`, `acceptance` — which
`Evidence::sections` projects from the engine's evidence onto
event-side serde types in `event.rs`; the engine types stay
unserialised, so the round-trip test keeps holding. A step not taken
is `{"status":"not-attempted","reason":R}` with `R` from
`Unattempted::as_str`, which gained `cache-bypassed` for a network
answer no response cache stood in front of. The verdict skips are the
four extraction failures (`no-text-layer`, `text-without-identifier`,
`encrypted`, `unreadable`), `unresolvable`, `conflict`, and
`duplicate` of either reason (`SkipReason::is_resolution_verdict`);
their reasons are slim dispatch keys, apart from `unreadable`'s
message and `duplicate`'s two fields, and a conflict skip carries the
refused record as `candidate`. Every other skip carries `path` and
`reason` alone. A record an operator reached and then moved is
followed, after `renamed` and any `library-admission`, by a
`content-index-write` event reporting the write `remember` made, which
counts toward nothing; it cannot ride on `renamed`, which is logged
before the move. The human `resolve` line names the work and where the
record was retrieved (`… to "<title>" (<authors>, <year>) via
<services>, from <where>`), and everything after `<path>: ` on a
`resolved`, `skipped` or `content-index-write` line is escaped once, in
`human_line`. The interactive description reads the sections: it
states the titles' state where it used to say `nothing read`, and it
shows the file's titles on a verdict whose extraction or lookup
failed. The interim `acceptance` `automatic` it gave a record an
operator supplied with no conflict is replaced by `accepted` in
`operator-input-evidence`, before any release carried it.

`operator-input-evidence` is implemented on top, ships in 0.9.0 and
closes Phase 3 as its change 10, and keeps schema 4: what it adds is
additive, and the one change of meaning (`automatic` to `accepted` for
a supplied record) lands before
any release carries schema 4, which the `cli` requirement "JSON Lines
output is first-class" now states as its rule. Every `resolved` event
and resolution `skipped` event carries an eighth section,
`identifier_input`, between `extraction` and `lookup`: `supplied`
with every text typed at the identifier prompt, in order, or not
attempted for `not-asked` (every batch run, and an interactive file
settled without a question), `not-supplied` or `content-duplicate`.
Each submission carries its `raw` text, its `syntax` (`parsed`, or
`rejected` for `unrecognised`, `invalid` or `checksum` with the form
it named, from `identifier::supplied`, which now returns
`Result<Identifier, SuppliedError>`), and its own outcome, except the
one `used` names, whose outcome is the event's own sections;
`displaced` then holds the file's own lookup, retrieval, title check
and record, so a correction no longer erases the file's own failed
lookup. A truncated DOI is looked up as typed. A supplied record is
`pending` until answered (only the event a description is rendered
from carries it), then `accepted`, `overridden` over a conflict, or
`rejected` on its submission when the operator skips or a later
record takes its place on offer; the human line gains `; candidate
rejected: <id>` and the description `candidate` and `rejected` lines.
The driver restores the `rename` requirement that a supply leading
nowhere leaves the file as it was: a candidate on offer stays on
offer, and a later record with no move puts back what was on offer
before it. A retry keeps the earlier rounds of the file's own lookup
in `lookup.earlier`, oldest first; whether Retry is offered still
reads the current round alone. `FileRecord` gained `accepted`, set by
`pipeline::accept`, and `Unattempted` gained `not-asked`,
`not-supplied`, `unparsed` and `no-move`. A rejected candidate is
still never written to the content index.

`separate-change-from-acceptance` is implemented on top, as change 15
and the first of Phase 4, and keeps schema 4. Bytes at a record's path
that no record's history holds become its history only once accepted:
automatically when the identifier extraction finds in them is one the
record's linked item carries (`library::accepted_by`, comparing each
kind with its own field, an arXiv identifier without its version, and
an ISBN-10 never against an ISBN-13), by an identifier the operator
supplied for the file in a rename, or by `borax reconcile --accept
<path>`. Reconciliation's step 3 is "changed in place": it extracts
only from a file that reaches it, under the run's extraction settings,
which `reconcile` now takes (`--page-limit`), and a change it does not
accept is a `pending` repair that writes nothing, size and
modification time included, so every reconcile reports it again and
`library-reconciled` counts it. An `--accept` path at which nothing was
pending is a `skipped` event of reason `not-pending`, a partial
success. An applying rename settles the gate before the move: accepted
bytes are appended to the held record and reported `changed` (then
`relinked` if the link moved); unaccepted bytes it moves are minted as
a new artifact and reported `separated`, the old record left to become
`missing`; unaccepted bytes left in place write nothing and are
reported `pending`, whatever the hold. A preview foresees the same and
treats every path it moves a file away from as holding nothing.
`HashEntry` gained an optional `item`, written only when present, so a
0.9.0 record reads and stays byte-identical until borax appends to it.
`ArtifactRecord::item_for` gives the item a record answers with for
given bytes: the link for the newest entry's, otherwise the item the
newest entry holding them names, or the link when it names none.
`ArtifactRecord::append` restates the newest bytes under the link as
read before appending other bytes, where that link was corrected by
hand, and `admit` and `Stores::take_in` append only for a decision: an
ordinary admission of held bytes appends nothing, so restored bytes
keep answering with the item they were accepted under.
`Stores::consult` checks the link first, so every 0.9.0 answer keeps
its kind, then answers `tracked` with `accepted_under`, or the new
`accepted-item-unavailable`; `record_retrieval`'s `item` names the
item whose record was returned. The work check passes over the item
the own record answers with for the incoming bytes as well as its link.
Neither `status` nor `validate` reports a pending change, and an
entry's item is never a finding. A supplied identifier that leads to no
move is set aside by the interactive driver, so a supplied identifier
can accept or separate a file but never leave it pending in place.

`library-identity` is implemented on top, as change 12 and the second
of Phase 4, and keeps schema 4. A library's identity is a v7 UUID in
`.borax/library.toml` (`library::IDENTITY_FILE`), the file exactly
`id = "<uuid>"` and read as `borax_core::library::LibraryIdentity`:
unknown keys are ignored, and an `id` that is absent, not a string or
not canonical UUID text does not parse, whatever its version. A copy
that carries `.borax/` is the same library; nothing inside the library
reads the identity, and an absent file is healthy. `library::identity`
reads it as `Absent`, `Present` or `Faulty`, a dangling symlink or a
directory included in `Faulty`. Every item and artifact-record write
goes through a `library::StoreWriter`, which calls `ensure_identity`
immediately before each write: `admit` in the rename driver (one
writer per run) and in `adopt`, and `rewrite` in `reconcile`. So the
first store write mints the identity, and a run that writes nothing —
`status`, `validate`, `resolve`, `bib`, a preview, `--no-record`, an
untouched or `--rehash`-only reconcile, an adoption that records
nothing, an admission `rewrite` renders unchanged — mints nothing. A
reconcile that refreshes a touched record's size and time does mint.
The mint never replaces: `borax_sources::store::create_atomically`
writes an exclusively created temporary and hard-links it into place,
falling back to an exclusive create where linking fails for a reason
other than `AlreadyExists`, and a lost race reads the winner's file. A
failed mint fails the guarded write, reported as the `unwritten`
admission, adoption or repair it already was, naming the identity
file. A `Faulty` file is neither minted over nor a reason to refuse the
write; the run warns once on stderr (`library::identity_fault_warning`,
through `Aftermath::identity`, `Reconciliation::identity_fault` and
`Adoptions::identity_fault`, written after `Aftermath::library`), and
`validate` reports it as `unreadable`, ahead of the item and record
findings. `library-status` and `library-validated` gain `id` after
`root`, omitted when there is no readable identity, and each human
report line then ends `; library <uuid>`. Change 13 must read the
identity when it stores a reference rather than assume one: a
successful admission does not imply a `Present` identity. Since
`index-references-libraries` it does: the rename driver reads
`library::identity` immediately before each reference write, `adopt`
and `reconcile` once after their own store writes, and only `Present`
writes; `Absent` and `Faulty` write and report nothing, and nothing
mints for a reference. Its review
turned up a restoration: `reconcile` over a directory with no library
root rewrote stale records there, against the `library` requirement
that such a run writes no library state, and would have minted an
identity too. It is now refused before it starts, as `adopt` is.

`reduce-content-index` is implemented on top, as change 13a and the
third of Phase 4, and moves the event stream to schema 5. The content
index lives under the data directory (`store::data_root`:
`XDG_DATA_HOME`, then `HOME/.local/share` on Unix, `APPDATA`, then
`XDG_DATA_HOME` on Windows, with `borax/v2` appended), apart from the
response cache, so `borax cache --clear` removes service responses
only; with no data directory the index is held in memory, unreported.
It keeps identification evidence and no record. An entry is the
directory `content/<hash>/`: `extracted.json`, written by a resolution
that extracted an identifier and confirmed it by a lookup, and
`operator.json`, an identifier with origin `supplied` or `overridden`,
written by `pipeline::remember` at the move and by `reconcile`. Each
file has one writer and is replaced whole, nothing reads one to write
the other, an operator fact answers before an extracted one, and an
unreadable operator fact answers with nothing. `v1` entries under the
cache root are read by nothing and go with `cache --clear`. An index
answer is looked up through the run's sources, response cache first,
with `lookup.origin` `content-index`, no extraction and no title
check, and is `unresolvable` when no service holds it; Retry of such a
lookup goes through `pipeline::resolve_indexed_again`, keeping its
origin and earlier rounds, checking no title and writing nothing.
History entries carry `identifier` and `origin` (`extracted`,
`supplied`, `overridden`, `adopted`) after `item`, both or neither, and
an admission whose decision carries evidence appends unless the newest
entry already holds the same bytes, item and evidence, so
re-identifying the newest bytes now appends one entry. `reconcile`
restores to `operator.json` every operator decision the library's
histories hold and the index does not (`library::operator_decisions`,
the record read last winning a disagreement), each reported as
`content-index-write` before the totals; nothing else restores, and
`status` and `validate` still write nothing. `adopt` reads the index's
identifier, then the response cache under `cache_root` in
`dispatch::priority` order whatever `sources` says, records the entry
as `adopted`, and writes nothing to the index; evidence no cached
response answers is the new `unanswered`, and `recorded` gains
`retrieval`. `content_index.read` `hit` carries `evidence`,
`content-index-write` gains `hash` and `entry`, and `record_retrieval`
loses `content-index`. A hand check over a copy of the real corpus
traced resolve, clear, an interactive supply, deletion of the data
directory then reconcile, and adoption into a second library.

`index-references-libraries` is implemented on top, as change 13b and
the fourth of Phase 4, and adds to schema 5 without a bump. An entry
now also holds references to libraries, each its own file
`content/<hash>/refs/<name>.json` holding `library`, `root`,
`artifact` and `item`, its name `sha256-` of library, root and artifact
joined by NUL, so a changed item keeps its file; each is replaced
whole, writing one touches no evidence fact, and an unreadable one is
skipped. `IndexStore` gains `list_facts` and `remove_fact`. Writers are
an applying rename after each admission that returned a record
(`Admitted::answers_with` is the item), `adopt` for each orphan it
records (`Adoptions::owed`), and `reconcile` for every hash of every
history (`library::references_owed`), after its operator restorations;
`references::keep` writes nothing for a key holding the same item,
rewrites one holding another or unreadable, and otherwise writes the
reference and then removes the references of the same library and
artifact at roots that are missing or hold no library or another one,
reported as `repaired_from`. Nothing prunes a reference otherwise, so
a deleted copy's stays `missing`. For a file its run's library does
not answer for, references are read before the evidence
(`pipeline::standing_in`, one `references::Libraries` per run reading
each root once, by identity and item store, never the marker or the
artifact record): one reachable reference answers, several only when
their items carry one preferred identifier, and a reference answer is
`hit` with `references` and no `evidence`, `record_retrieval`
`library-reference`, no lookup, no extraction and no evidence on an
admission. Disagreeing references fall to the evidence, and every
reference read is reported with its status and cause, on the human
line and in the interactive description as `reference` and
`references` lines. `adopt`'s lookup settles references before the
evidence; a record reached through one is `recorded` with retrieval
`library-reference` and no evidence on its history entry, and
`unanswered` may carry no `identifier` and gains `references`. The
reference writes are `content-index-write` entries of kind
`reference`, which render nothing unless they fail. A hand check over
copies of a library built from the real corpus traced reconcile
restoring `refs/`, an outside resolve answered from the library, an
`rsync` copy reconciled in both places agreeing, the copy moved and
reported `missing`, a reconcile at the moved root naming
`repaired_from`, and after `cache --clear` an adoption into a third
library recorded through the reference.

Its review found that a stale-root repair could remove a reference
another run had rewritten, or whose root held the library again,
between the listing and the removal. `keep` now re-reads each stale
reference and its root from disk just before removing it, and leaves
one that changed. The window between that check and the removal
remains, since no filesystem offers a portable compare-and-delete; a
reference lost in it is restored by the next `reconcile` in the
library it names.

## Not built yet

- **The optional `pdfium` backend.** The pure-Rust `PdfSource` is the
  only extraction backend. The second one was always conditional on
  evidence that the first is insufficient, and that evidence has not
  appeared: the fixture corpus and the live contract tests passed on
  all three platforms without it, as of 0.4.0, when CI still ran them
  there. Picking a binding crate and a
  prebuilt-binary pipeline for three platforms is the cost being
  avoided; revisit only if real PDFs start failing extraction.

  This is task 6.7 of `add-core-pipeline`, and that change was archived
  with the task still open — 59 of 60 — rather than pretending it was
  done. Nothing in the living specifications depends on it: the
  `extraction` spec constrains behaviour (tiering, page bounds, typed
  failures, offline operation) and never names a backend, so which
  engine reads the PDF is an implementation choice the specs leave
  free.
- **Searching for an identifier by title.** This is now the single
  largest remaining lever on the hit rate, and the natural next change.
  `supply-identifiers-interactively` made every unidentified file
  answerable, but the operator supplies the answer: borax asks and
  waits, having looked for nothing. A file with no identifier usually
  does have a title, and Crossref and OpenAlex both take one — so the
  same question could offer candidates to choose from instead of an
  empty prompt, and the choosing is already built.

  What it needs that does not exist: a title query on the `Source`
  trait, a way to rank and present several candidates, and a rule for
  when a match is close enough to offer at all. What it can reuse:
  the whole supply loop, the description, and the content-index memory
  — a chosen candidate is remembered exactly as a supplied identifier
  is. Named out of scope in the `add-core-pipeline` proposal, and
  deferred again by `supply-identifiers-interactively` deliberately,
  so that supplying by hand works before searching is layered on it.
- **Book series.** The pattern external tables were built for applies
  to book series as much as to journals — a code for the series, the
  volume within it, the first page — and nothing can render it, because
  `Record` has no `collection-title`. There is no value for a
  `[series]` field to read and so no string for a table to fold, which
  is why the field does not exist rather than existing and rendering
  empty. What it needs is that record field: `collection-title` on the
  model, both source readers populating it with their cassettes
  re-recorded to show they do, and then one field variant and one more
  row in the user's table. `add-external-tables` was deliberately built
  so that is the whole of the work, and deferred it because it reaches
  into `record-model` and `resolution` rather than because it is hard.
- **Views, nodes and memberships.** The next change in the library
  stack; see the `add-library-store` paragraphs above. An item and an
  artifact record are one edge today and nothing organizes them.
- **Everything the design names as a later change**: no library index
  (SQLite and FTS5 wait on a measurement against the direct reads),
  search or watch mode; no OCR or interactive candidate picker; no XMP
  write-back, Zotero interop or Emacs package; no bibliography format
  other than BibTeX. The file's bytes are never modified — borax renames
  and never edits.

## Known defects

- **Retry is offered for an identifier no configured service
  supports.** A lookup with no eligible service has no attempts, so
  `LookupEvidence::is_conclusive` calls it inconclusive and the menu
  offers `Try the services again` as it does after an outage, though
  retrying cannot help: no service has been added in between. Each
  retry adds another `no-eligible-service` round to `lookup.earlier`.
  `operator-input-evidence` left when Retry is offered out of scope;
  a fix is its own change.

- **On Windows, `borax cache --clear` removes the run logs of runs
  outside a library.** The run-log state root (`runlog::state_root`)
  and the response cache root (`store::cache_root`) are both
  `LOCALAPPDATA\borax\v1` there, and clearing the cache removes that
  whole directory. That goes against the `cli` requirement that `cache`
  clears the response cache and the `run-logs` requirement that such a
  run leaves a readable record. `reduce-content-index` found it while
  choosing where the index lives, put the index under `APPDATA` out of
  its reach, and deferred the collision itself: the fix is a
  restoration of its own. Not verified on Windows, which CI does not
  run.

- **Adoption's `unreadable` and `unwritten` messages reach the terminal
  unescaped.** The `library-adoption` human line escapes its path, but
  `Adoption::Unreadable` and `Adoption::Unwritten` print the
  filesystem's message as it came, so a control character in one is
  acted on by the terminal rather than shown.

  This is what is left of the defect "Human output other than the
  description passes metadata through unescaped".
  `show-record-before-asking` closed it for the description, and
  `sectioned-resolved-event` closed it for every `resolved`, `skipped`
  and `content-index-write` line, the metadata-conflict skip line among
  them, by escaping the whole clause after `<path>: ` in one place,
  `human_line`. The `library-extraction` and `library-condition` lines
  already escaped what they print. Closing the remainder is a small
  restoration of its own.

- **`adopt` and `reconcile` report after the pass, not as they go.**
  `run::adopt_events` and `run::reconcile_events` collect every
  per-object event and write them after the last file is hashed,
  contrary to the `cli` requirement "A run reports as it goes".
  Restoring it means passing a sink into `library::adopt` and
  `library::reconcile`; it is a restoration and needs no proposal.

- **A DOI keeps a closing Unicode bracket.** `Doi::parse`
  (`crates/borax-core/src/identifier.rs`, line 103) trims only ASCII
  closers from a candidate's end, and the text scan cuts a candidate
  at ASCII whitespace alone. HAL cover pages print the DOI as
  `⟨10.1039/D0CS01430C⟩.`, so the trailing `.` goes and `⟩` (U+27E9)
  stays: the real corpus's `Manuscript(revised).pdf` extracts
  `doi:10.1039/d0cs01430c⟩`, which no service holds. Likely every HAL
  deposit with a cover page. The `extraction` requirement "Extracted
  identifiers are validated and normalized" already requires
  surrounding punctuation stripped, so the fix is a restoration and
  needs no proposal. Found by `status --identify` over the corpus once
  it printed per-file results.

- **A sidecar is never moved with its file, so a rename can orphan
  one.** `write_sidecar` writes beside the path the file has when it is
  reached, and no code path renames or removes an existing sidecar when
  its file's name changes. So renaming an already-renamed file — a
  template change, say — writes the new sidecar and leaves the old one
  beside the vacated name.

  It loses no data: the orphan is borax's own output and its content is
  still correct about the record, only about a path nothing occupies.
  It predates the ledger and run-log work and is not a regression from
  it. `remove-undo` closed the second way in, where a file moved back to
  its original name left its sidecar behind under the vacated one.

  What a sidecar is has been settled by `add-library-store`: derived
  citation output that governs no decision, and nothing reads one back.
  So the fix is no longer blocked on a question of identity. What is
  left is choosing between moving a sidecar with its file on rename and
  regenerating it and removing the old one, which is a change of its
  own, not a patch.

- **A PDF parser's panic prints a trace.** `borax-pdf` catches a panic
  in the parsers and reports the file as unreadable, but Rust's default
  panic hook has already written the trace to standard error by then.
  `status --identify` over the real-PDF corpus prints two of them. The
  run is unaffected and exits 0; the output reads as a crash when it is
  not one.

- **Names are compared case-sensitively on every Unix target.**
  `crates/borax/src/paths.rs` answers wrongly on a case-insensitive
  volume, macOS's default included, and collision detection,
  already-named reporting and the library boundary all go through it.
  Deferred by `add-library-store` because fixing it reaches all three.

- **Every capability spec opens with a placeholder Purpose.** All ten
  files in `openspec/specs/` carry the same line the archive tool
  writes and asks to have replaced:

  ```markdown
  ## Purpose
  TBD - created by archiving change <name>. Update Purpose after archive.
  ```

  It has been there since `add-core-pipeline` was archived and has
  never been filled in for any capability. Nothing depends on it and
  no behaviour is wrong; the cost is that each spec states what it
  requires without ever saying what the capability is *for*, so a
  reader infers the scope from the requirements and can only guess
  where a new requirement belongs.

  Worth one sweep across all ten rather than a line at a time. Filling
  in only the specs a change happens to touch is what has kept it
  outstanding: it makes the newly written ones the odd files out, which
  is a worse state than uniform placeholders, so each change has
  reasonably left it alone.

## Open decisions

- **How `rename` could ever honour `concurrency`.** `resolve` runs its
  batch on a bounded pool; `rename` is serial, and
  `stream-per-file-events` made that harder to change by choosing live
  per-file reporting. The two pull against each other: resolving
  concurrently means files finish in whatever order the network
  answers, so a concurrent `rename` would either report out of order or
  buffer completions to restore input order — which is the buffering
  that change removed. Nothing is broken today, and the answer is not
  obvious enough to guess at. `scope-cli-flags-per-subcommand` did not
  settle it; it stopped `rename` from advertising the flag, so the
  question is now asked by its absence rather than by a setting that
  read as available and did nothing. Answering it moves `--concurrency`
  into the shared resolution group, which is one line at the flatten
  site.
- **Whether `pdfium` is ever worth it.** See above; the decision is
  evidence-gated rather than open in the usual sense.

## Live risks

- **CI runs and releases on Linux alone.** The test matrix in
  `.github/workflows/ci.yml` was cut to `ubuntu-latest` during 0.5.0,
  and after 0.9.0 the release build was cut to the Linux musl target
  too; the macOS and Windows entries of both are commented out, so
  restoring them is uncommenting. 0.9.0 is the last release with macOS
  and Windows binaries. Nothing was done to stop the code supporting
  them — they are simply neither checked nor shipped, so a
  platform-specific break will reach a user building from source rather
  than a run. The first push of the
  interactive work found exactly one such break, a test that wrote a
  rendered path with a slash where Windows spells it with a backslash,
  and that class of mistake is the one to watch for: compare `PathBuf`
  values, which treat both separators alike, and build any expected
  *string* with the platform's own separator.

- **The public surface is untested by users.** The JSONL event schemas
  are declared the stable integration contract, and no external consumer
  has yet built against them. Their weaknesses are therefore unknown,
  which is the main reason the project is in `0.y.z` and not approaching
  `1.0.0`. Do not promise compatibility until something has actually
  consumed the stream.
- **Source schema drift** at Crossref, OpenAlex or arXiv. Mitigated, not
  eliminated: the readers ignore unknown fields and type missing ones as
  absent, and the scheduled live contract tests compare real responses
  against the cassettes. A silent change in the *meaning* of a field
  would still pass.
- **BibTeX emission is lossy in both directions.** Provenance fields and
  sidecars carry the full record, so nothing is lost to borax; a
  `.bib` round-trip through another tool is where information goes
  missing.
- **The template grammar invites scope creep** toward a general
  expression language. The grammar is specified and versioned; anything
  beyond the specified filter set needs a spec change rather than an
  implementation.
