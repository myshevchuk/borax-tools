# Design: operator-input-evidence

## Context

Everything below was read from source at `a62839b` (change 9 merged).
This change carries out change 10 of the roadmap's Phase 3, as the
Phase 3 resolution-schema design discussion (maintainer's notes,
outside the repository) assigns it:

1. **No second bump.** Operator input and pending acceptance are added
   to schema 4, before 0.9.0 ships.
2. **Candidate decisions.** `acceptance` gains `pending`, `accepted` and
   `rejected` for operator candidates. Even when the titles agree, an
   operator candidate stays pending until it is accepted.
3. **Frontend boundary.** Evidence and history live inside each file's
   verdict event. There are no prompt or answer events, no
   machine-readable stdin protocol, and no title search.
4. **Rejected candidates are never associated with the file's hash.**
   This is already true. This change keeps it true and tests it.

Change 9 left two things for this change. Its design D1 reserved
`identifier_input` between `extraction` and `lookup`. Its design D7
made `automatic` the interim `acceptance` of a record an operator
supplied with no conflict. The maintainer accepted that value on
condition that this change replaces it before 0.9.0.

**Settled by the maintainer on 2026-10-04**, after the first draft of
this change. These are decisions, not open questions:

1. **`displaced` is accepted as designed** (D2).
2. **Retry history is part of this change.** Every lookup of the file's
   own identifier is kept, in order. Whether Retry is offered is still
   decided by the latest lookup alone (D12). The first draft had left
   this to a later proposal.
3. **`pending` is never emitted in this change.** It appears only on
   the event a question's description is rendered from (D4).
4. **`rejected` covers both ways a candidate is set aside**: Skip while
   it is on offer, and a later submission whose record replaces it on
   offer (D4, as amended after the critic gate).
5. **Quit at a candidate reports nothing about that file**, its
   submissions included (D4, D5).
6. **The change from `automatic` to `accepted` is allowed without a
   version bump**, because no release carries schema 4 (D4).

**Amended after the critic gate** (findings accepted by the
coordinator):

- **A supply that does not resolve keeps a candidate on offer.** This is
  a restoration of the living `rename` text "SHALL leave the file
  exactly as it was, with the record and the choices it already had".
  Today's driver breaks it when the record on offer is itself a
  candidate (D4, D5).
- **A collision notice leaves a candidate pending.** Rename or Rename
  anyway on a candidate whose work the library already holds is
  answered with a notice, and the question is put again (D4).
- **A retried record accepted over its conflict is `overridden`**, not
  `automatic` (D4).
- **The schema-version rule is made explicit about unreleased schemas**
  by modifying the `cli` requirement "JSON Lines output is first-class"
  (D4, D10).
- **D11 splits the `earlier` edits** into constructor sites and pattern
  sites.

What the engine does today, at the points this change touches:

- `identifier::supplied` (`crates/borax-core/src/identifier.rs`, line
  457):
  - returns `Option<Identifier>`;
  - tries `Doi::parse`, then `ArxivId::parse`;
  - accepts a PMID or an ISBN only behind its prefix;
  - returns `None` for anything else.

  Underneath it, `IdentifierError` tells `Invalid` from `Checksum`.
  `supplied` discards that difference.
- `run::supplied_identifier` (`crates/borax/src/run.rs`, line 2390)
  asks until a text parses or the prompt returns `None`. `None` means an
  empty line, Esc, or an interrupted terminal: the inquire adapter
  (`session.rs`, `Asker::text`) trims the line and returns `None` for
  an empty one. Each refused text is quoted back and dropped.
- `run::asked` (line 1836) keeps the file's own evidence (`evidence`),
  its own record (`own`), and whatever is on offer (`offer`, with
  `candidate` saying whether that is a supplied record). The Supply
  arm's outcomes are these:
  - **A record.** `resolve_supplied` returns `Ok(file)`. That record
    goes on offer.
  - **No record.** `resolve_supplied` returns `Err(unheld)`. A report
    block is shown and the file's own record goes back on offer
    (`offer = own.clone()`, near line 2079). That holds even when the
    record on offer was a candidate, so a candidate is dropped by a
    supply that leads nowhere, against the living `rename` requirement
    (D4, "Restored").
  - **No move.** A candidate whose decision is not a move is set aside
    by `run::elsewhere` before any question is put about it, and the
    file's own record goes back on offer (near line 1922).
  - **A collision notice.** Rename, Rename anyway or Keep on a record
    the operator reached (`!on.kept`) is checked against the library
    first (near line 2026). Where the library already holds a file of
    its work and the operator has not been told, the driver shows the
    collision, sets `told`, and puts the question again without
    accepting anything.
- `run::skipped` (line 2283) replays the held verdict. On a skip, a
  candidate the operator saw is reported nowhere.
- `run::described` (line 2327) renders a candidate's would-be
  `resolved` event through `accepted(offer)`, which is
  `pipeline::accept`. A candidate whose titles conflict is therefore
  described as `overridden` before anyone has overridden anything.
  `describe` (`describe.rs`, line 174) shows the `conflict` line only
  on `overridden`.
- `pipeline::resolve_supplied` (`pipeline.rs`, line 786) builds the
  candidate's evidence from the file's own. It keeps `library`,
  `content_index.read` and `extraction.result`. It replaces `titles`
  (read now), `lookup`, `match_check` and `content_index.write`
  (awaiting acceptance).
- `FileRecord::acceptance` (line 111) gives `Overridden` when
  `overridden` is set and `Automatic` otherwise.
- The Retry arm of `run::asked` calls `resolve_supplied` (on success)
  or `unheld_evidence` (on failure) with the file's own extraction pass
  as the origin, and makes the result the file's own evidence. Both
  functions build a new `LookupEvidence::Attempted` from the new
  attempts alone, so the lookup the retry follows is dropped. A lookup
  of an identifier no configured service supports fails with no
  attempts. `LookupEvidence::is_conclusive` is `false` for it, so Retry
  is offered for it as for an outage. This change leaves that as it is.

## D1. `identifier_input`: where it sits and what it says without input

**Decision.** `identifier_input` is a section of every `resolved` event
and every resolution `skipped` event. It sits between `extraction` and
`lookup`. It is a single step, so it is tagged by `status` (change 9's
D1):

| `status` | Fields | When |
|---|---|---|
| `supplied` | `submissions`, `used`, `displaced` (D2) | the operator submitted at least one text at the identifier prompt for this file |
| `not-attempted` | `reason`: `content-duplicate`, `not-asked` or `not-supplied` | nothing was submitted |

The reasons:

- **`content-duplicate`.** The file is a content duplicate. No question
  is ever put about one, in any run (the `rename` requirement "An
  interactive run asks about files it could not settle"). This keeps
  the content duplicate's rule "every section not attempted with
  reason `content-duplicate`" true for the new section.
- **`not-asked`.** No question was put about the file. This covers:
  - every verdict of a batch `rename`, of `resolve` and of `bib`;
  - in an interactive run, a file that was settled or reported without
    a question: one passed over under `rename.skip-named`, one whose
    content hash is unknown, a target taken, a record that renders no
    name.
- **`not-supplied`.** At least one question was put about the file and
  no text was submitted. That includes a prompt abandoned with an empty
  line or Esc.

**The batch answer.** In a batch run, `identifier_input` is
`{"status":"not-attempted","reason":"not-asked"}`, except on a content
duplicate. Change 9's D1 says a step that did not run is never omitted,
and this change makes no exception. A consumer then reads the same keys
on every resolution event. A passed-over file in an interactive run is
also "reported exactly as a batch run reports it" (`run::About`), and
here that holds byte for byte.

**Rejected: an upstream reason** (`library-answered`,
`content-index-hit`, `extraction-failed`). Those steps do not stop an
operator from supplying an identifier. A library answer, an
index hit and an extraction failure are each offered Supply. So none of
them is why input was not taken. A content duplicate is the exception,
because it is never asked about.

**Rejected: one reason for both `not-asked` and `not-supplied`.**
Whether a person saw the file and declined to say what it is, or never
saw it, is a fact about the decision. Merging the two would lose it.

## D2. Submissions, and how the verdict relates to them

**Decision.** `supplied` carries three keys:

- **`submissions`.** Every text the prompt returned for the file, in the
  order typed. A text that parsed and one that did not are both
  submissions. An abandoned prompt returns nothing and submits nothing.
  Each entry has:
  - `submission`: its number, counting from 1 within the file;
  - `raw`: the text as the prompt returned it. The terminal adapter
    trims its edges, and JSON encoding escapes it;
  - `syntax`: its parse result (D3);
  - its own outcome (D4): `lookup`, `record_retrieval`, `match_check`,
    `acceptance`, and `record` when it reached one. One entry is the
    exception: the one `used` names carries only the first three keys.
- **`used`.** The number of the submission whose identifier the event's
  own `lookup` looked up, or `null`.
- **`displaced`.** When `used` is a number, the file's own steps that
  the used submission replaced in the event's sections: `lookup`,
  `record_retrieval`, `match_check`, and `record` when the file's own
  resolution reached one. `null` when `used` is.

These invariants hold, and tests pin them:

1. `used` is a number exactly when the event's `lookup.origin` is
   `operator`. That submission's `syntax` is then `parsed`, and its
   `identifier` equals the event's `lookup.identifier`.
2. The event's `lookup`, `record_retrieval`, `match_check`, `acceptance`
   and `record` are then the used submission's outcome. Its entry
   repeats none of them.
3. Every other entry carries its whole outcome.
4. `displaced` is non-null exactly when `used` is.
5. Submission numbers run 1, 2, … with no gaps, in `submissions` order.
   The used entry need not be last. A candidate stays on offer when a
   text typed after it is refused, when an identifier typed after it
   reaches no record, or when one reaches a record that leads to no
   move (D4). Each of those submissions follows it.

**Why a pointer, and why the used entry is bare.** The section follows
the pipeline. `identifier_input` produces the identifier `lookup` looks
up, as `extraction` does: change 9 reports `extraction.result.identifier`
and then `lookup.identifier`. The steps after `identifier_input` in the
event are therefore the used submission's steps. Repeating them inside
its entry would state the lookup, the retrieval, the title check and
the whole record twice on one event. The maintainer's decision 2
("facts are stated once") and change 9's D7 both reject that. Every
other submission never reached the event's sections, so its entry is
the only place its outcome can be.

**Why `displaced`.** `resolve_supplied` replaces the file's own `lookup`
and `match_check` with the candidate's. Without `displaced`, accepting
a correction erases the file's own failed lookup. That is the case
supplying identifiers was proposed for: a DOI with `.author` appended,
which no service holds. The same goes for the living `rename` scenario
"A reference's DOI caught": the record the file's own pass reached,
which the operator replaced, would vanish too. `displaced` holds exactly
what the event's sections stopped saying about the file's own
resolution:

- `lookup`;
- `record_retrieval`, which is `Evidence::retrieval` of the file's own
  evidence;
- `match_check`;
- the record, resolved or refused.

Two other replaced facts are left out, deliberately:

- **The titles.** `resolve_supplied` re-reads them from the same file.
  The only state it can replace is "not read because the content index
  or the library answered", and `content_index.read` and `library`
  still say that.
- **The file's own `content_index.write`.** The index entry it left is
  overwritten by the move's write, which the `content-index-write`
  event reports.

**Rejected: uniform, self-contained entries** (the used one included).
These are the simplest to read, but they state the used submission's
lookup, retrieval, check and whole record twice.

**Rejected: a current input plus a history list.** The "current" input
is not the last one (invariant 5). Splitting submissions of one kind
across two keys would also make a consumer merge them to read their
order.

**Rejected: the event's sections always describing the file's own
resolution, with the operator's record only in its submission.** That
changes the meaning of change 9's `lookup` (`origin` `operator` would
vanish from it), its `match_check` and its `record_retrieval` on an
accepted supply. It is not an addition, and it would need a version
bump. The maintainer accepted one change of meaning (D4), not this
one.

**Rejected: no `displaced`.** The file's own failed lookup would leave
the stream at the moment the operator fixes it, which is when it
explains the fix.

## D3. The syntax result, and what the parser can tell apart

**Decision.** `identifier::supplied` returns
`Result<Identifier, SuppliedError>`. Every text that parses today parses
to the same `Identifier`, and every text refused today is refused.
`SuppliedError` names why, using only distinctions the parser already
draws:

| `SuppliedError` | `syntax` | Raised when |
|---|---|---|
| `Unrecognised` | `{"status":"rejected","reason":"unrecognised"}` | the text names no form: no DOI marker, no `arXiv:`, `pmid:`, `isbn:`, `isbn-10:` or `isbn-13:` prefix, and it does not parse as a bare arXiv identifier. This includes prose, blank text, and a bare run of digits, which `supplied` refuses because it could be a PMID or an ISBN |
| `Invalid { kind }` | `{"status":"rejected","reason":"invalid","expected":K}` | the text names a form and fails that form's syntax (`IdentifierError::Invalid`) |
| `Checksum { kind }` | `{"status":"rejected","reason":"checksum","expected":"isbn"}` | an ISBN that is well formed and fails its check digit (`IdentifierError::Checksum`) |

`K` is `IdentifierKind::as_str`: `doi`, `arxiv`, `pmid` or `isbn`. Which
form a refused text names is decided by the first match, in this order:

1. A DOI marker: `doi:` or one of the four resolver addresses
   `Doi::parse` strips (`http(s)://doi.org/`, `http(s)://dx.doi.org/`),
   compared case-insensitively.
2. `arXiv:`, the prefix `ArxivId::parse` strips.
3. `pmid:`.
4. `isbn:`, `isbn-10:` or `isbn-13:`.
5. A body that, after trimming, begins with `10.`. That is
   `Doi::parse`'s first syntactic test once the prefixes are gone.

A text that parses reports `{"status":"parsed","identifier":I}`, where
`I` is the `Identifier`'s `Display` (`doi:…`, `arXiv:…`, `pmid:…`,
`isbn:…`). That is the normalised input, written as change 9 writes
`extraction.result.identifier` and `lookup.identifier`.

Examples, each a test (task 1.1):

| Text | Result |
|---|---|
| `see email from Anna`, `not-an-identifier`, `` (empty), `   `, `12345678`, `9781593278281` | `Unrecognised` |
| `doi:not-a-doi`, `https://doi.org/abc`, `10.12/x` (three-digit registrant), `10.1234` (no suffix), `10.1234/` | `Invalid { Doi }` |
| `arXiv:12345` | `Invalid { Arxiv }` |
| `pmid:0`, `pmid:abc` | `Invalid { Pmid }` |
| `isbn:123` | `Invalid { Isbn }` |
| `isbn:9781593278282` | `Checksum { Isbn }` |
| `10.1039/c9cc02492` | `Ok(Doi("10.1039/c9cc02492"))` |

**No repair.** `10.1039/c9cc02492` is the round-two truncated DOI (the
published one ends in `a`). It is well formed, so it parses and is
looked up exactly as typed. Its lookup's attempts are what the
services say about that DOI. Nothing appends a character, retries a
variant, or proposes the DOI it might have been. This pins current
behaviour (task 1.1 for the parser, task 5.1 for the lookup).

**The refusal is unchanged.** `run::supplied_identifier` still says
`"<text>" is not a DOI, an arXiv identifier, or a pmid: or isbn:
number.` The reason reaches the stream. The prompt's wording is not
this change's brief, which asks for minimal human-wording changes.

**`raw` is never blank in practice.** The prompt treats an empty line
as declining, so a blank text never reaches `supplied` from a terminal.
`supplied("")` is still `Unrecognised`, so the function stays total.

**Rejected: a `bare-number` reason.** The parser has no test for bare
digits. It refuses them only because no other form matches. Adding the
test would be new syntax analysis. `unrecognised` covers the case, and
the refusal already names the prefixes.

**Rejected: reasons for each failure point inside `Doi::parse`**
(registrant length, missing slash, empty suffix). They would tie the
stream to that function's internals. `invalid` with `expected` `doi` is
what a consumer can act on.

**Rejected: flagging a likely truncation.** Nothing in the parser can
tell a truncated DOI from a short one. Any heuristic would be a guess,
which is what this tool does not make.

## D4. Candidate decisions

**Decision: two places, two vocabularies.** The event's own
`acceptance` says how the event's record was used. A submission's
`acceptance` says what the operator decided about the candidate that
submission reached.

The event's `acceptance`:

| `status` | When |
|---|---|
| `not-applicable` | the verdict is a skip (unchanged) |
| `automatic` | a record was used, no conflict was overridden to use it, and it was not reached from an operator's submission (unchanged in meaning, narrowed by this change) |
| `overridden` | an operator accepted the record over `match_check`'s conflict: the file's own record or a supplied one (unchanged) |
| `accepted` | **new.** An operator accepted the record reached from their submission (`identifier_input.used`), with no conflict overridden. This replaces change 9's interim `automatic` |
| `pending` | **new.** The record reached from the operator's submission is on offer and unanswered, whatever its title check concluded |

A submission's `acceptance`, on every entry except the used one:

| `status` | When |
|---|---|
| `rejected` | its record was on offer and stopped being: the operator answered Skip, or a later submission reached a record that replaced it on offer |
| `not-attempted`, `unparsed` | the text did not parse |
| `not-attempted`, `no-record` | it parsed, and no service held the identifier or none could be asked |
| `not-attempted`, `no-move` | its record leads to no move (a taken target, no usable name, or the file's current name), so the driver set it aside without asking (`run::elsewhere`) |

What does not reject a candidate, which stays on offer and `pending`:

- a refused text;
- an abandoned prompt;
- a later identifier that reaches no record (closed `no-record`);
- a later identifier whose record leads to no move (closed `no-move`);
- a collision notice (below).

Quit does not reject one either. It is settled (Context, item 5): the
file is reported nothing, so its submissions are reported nowhere.

**Restored: a supply that does not resolve keeps the candidate.** The
living `rename` requirement "An interactive run asks about files it
could not settle" says a supplied identifier that does not resolve
"SHALL leave the file exactly as it was, with the record and the
choices it already had". Today's driver reverts to the file's own
record (`offer = own.clone()`), and so drops a candidate that was on
offer. This change restores the requirement: the record on offer
before the supply, candidate or own, is on offer after it. The
requirement is the authority, so this is a restoration inside this
change, not a new rule. The first draft had instead rejected the
candidate as soon as a later text parsed, which contradicted the
requirement.

**Decided here: a later record with no move keeps the candidate too.**
The living text does not say what is on offer after a candidate whose
record leads to no move. Today `elsewhere` reverts to the file's own
record. This change reverts to the record on offer before that supply.
That treats a supply that led nowhere the same whether no service held
it or its record had no move, and it keeps the rule one sentence long:
a candidate is rejected only by Skip, or by a later record that takes
its place on offer. Where no candidate was on offer, the record before
the supply is the file's own, so today's behaviour is unchanged
(`a_resolved_candidate_that_led_nowhere_leaves_the_files_own_record_to_cite`
still holds).

**A collision notice is `pending` → `pending`.** Rename or Rename
anyway, given on a candidate whose work the library already holds a
file of, settles nothing the first time. The driver shows the
collision, as the living `rename` requirement asks, and puts the
question again with the candidate still on offer and still `pending`.
The answer given after the notice settles it like any other:

- Rename or Rename anyway accepts it, and the move files the file as
  another artifact of that item;
- Skip rejects it;
- a later submission rejects it if its record replaces the candidate on
  offer, and leaves it pending otherwise;
- Quit reports nothing.

The notice is shown once per file (`told`), as today, so the second
answer is never met with it again.

**`pending` is never a reported verdict** (settled, Context item 3).
An interactive run reports a file only once an answer has settled it
(`rename` "A file's verdict follows the operator's decision"). The
answers that settle a candidate on offer:

- Rename or Rename anyway accepts it, except the first time on a work
  the library already holds, which only brings the notice;
- Skip rejects it, and so does a later record that replaces it on
  offer;
- Quit reports nothing.

Every other answer leaves it on offer and `pending`.

`pending` therefore appears only on the event a question's description
is rendered from. It is still a stream value, because that event is a
`resolved` event and the description is a rendering of it. A later
frontend protocol would report it as it is, and this change emits it on
no event.

**`overridden` for a supplied record over a conflict.** Change 9
defines `overridden` as "an operator accepted the record over the
conflict `match_check` holds", and that covers a supplied record.
Reporting such a record as `accepted` would change `overridden`'s
meaning, which is not additive. The event still says the record was
the operator's, in `lookup.origin` and `identifier_input.used`.

**A retried record is `automatic`, or `overridden` over its
conflict.** A retry is the file's own lookup made again, and its origin
is `extracted`. D12 keeps its earlier rounds, but it stays the file's
own lookup, so it is never `pending` or `accepted`. A retried record
that cleared the title check and is renamed from reports `automatic`. A
retry can also reach a record whose title conflicts (the Retry arm
then holds a `conflict` skip, near line 2107). If the operator renames
over that conflict, `accept` sets `overridden`, and the record reports
`overridden` like any record accepted over its conflict. The interim
value the maintainer named was the supplied record's.

**The one change of meaning, and the version** (settled, Context item
6). Change 9 made a
supplied record renamed with no conflict report `automatic`, and this
change makes it report `accepted`. The `cli` requirement "JSON Lines
output is first-class" asks a bump for "a field whose meaning changes".
As written, its phrase "a consumer that reads the stream correctly
today" could take in a build of `main` between two changes. That rule
protects a consumer of a released stream, and no release
carries schema 4: 0.8.0 shipped schema 3, and changes 9 and 10 both land
before 0.9.0. The maintainer accepted the interim value on exactly this
condition (change 9's D7). Everything else here is additive, so the
version stays 4.

**The `cli` requirement is modified rather than read around.** The
alternative was to argue in the delta that the requirement is not
contradicted, because it speaks of releases. But its wording is about
what a consumer reads "today", and change 9 relied on the release
reading without writing it down. This change modifies "JSON Lines
output is first-class" to say:

- the version separates schemas as released;
- a schema not yet carried by any release may change without a further
  bump;
- the first release that carries a schema fixes it.

That is the rule changes 9 and 10 have both followed. Writing it down
lets the next unreleased change check against it, rather than against
an interpretation.

**Settled: one `rejected` for both ways** (Context item 4), rather than
telling them apart as `skipped` and `superseded`. The entries' order
already tells it. A rejected entry followed by a submission whose
record went on offer was set aside by that submission. The
last rejected entry was set aside by the final answer.

**Rejected: `accepted` for every supplied record the operator accepted,
conflict included.** That changes what `overridden` means.

**Rejected: `rejected` as the event's `acceptance` on a final skip.**
The skip's sections are the file's own verdict, which is
`not-applicable`. The rejected candidate was never the file's verdict,
so its rejection belongs to its submission.

## D5. The engine

**`borax-core`** (D3): `IdentifierKind`, `SuppliedError`, and `supplied`
returning `Result<Identifier, SuppliedError>`. `IdentifierError` and the
four types' `parse` functions are unchanged.

**`Evidence::identifier_input`.** `Evidence` gains the section between
`extraction` and `lookup`, as an engine type, `IdentifierInput` (D9).
The types derive no serde, like the rest of `evidence.rs`.
`Evidence::sections` projects the new section, total and with no
wildcard arms, as change 9's D2 requires:

- a submission's `record_retrieval` is the service of its lookup's
  found attempt, `service-cache` or `network`, or `null`;
- `displaced.record_retrieval` is the `RecordRetrieval` it holds.

**Defaults.** `pipeline::standing` and `resolve_file` give every
evidence `IdentifierInput::NotAttempted(Unattempted::NotAsked)`. The one
exception is a content duplicate, whose `Evidence::not_attempted(
ContentDuplicate)` gives `NotAttempted(ContentDuplicate)`. The index-hit
and library-answer constructors build on `Evidence::not_attempted(
reason)`, and they must still end up with `NotAsked`. How that is
arranged is the implementer's choice. `resolve_supplied` and
`unheld_evidence` copy `prior.identifier_input`. The driver overwrites
it (below).

**`Unattempted`** gains four reasons:

- `NotAsked` (`not-asked`);
- `NotSupplied` (`not-supplied`);
- `Unparsed` (`unparsed`);
- `NoMove` (`no-move`).

Every reason in the stream still comes from `Unattempted::as_str`.

**`FileRecord::accepted: bool`.** `pipeline::accept` sets it on every
record it is given. It still sets `overridden` exactly when
`match_check` is a conflict. `FileRecord::acceptance` becomes:

| `overridden` | lookup origin | `accepted` | `acceptance()` |
|---|---|---|---|
| true | any | any | `Overridden` |
| false | `Operator` | true | `Accepted` |
| false | `Operator` | false | `Pending` |
| false | extracted, or no lookup | any | `Automatic` |

The batch path never calls `accept`, and never has an operator origin.

**The driver** (`run::asked`):

- It keeps the file's submissions as they are made, numbered from 1,
  and remembers whether any question was put.
- `supplied_identifier` records each refused text as a submission. Its
  outcome is lookup, match check and decision all not attempted for
  `Unparsed`. The function can take the list, or return the refused
  texts beside the parsed one; that is the implementer's choice. The
  refusal is still shown and the prompt put again.
- Before resolving a parsed text, the driver remembers the record on
  offer, which is a candidate or the file's own (`before`). Then:
  - **`Err(unheld)`.** The new submission is closed as
    `NotAttempted(NoRecord)`. Its lookup is `unheld_evidence`'s, and
    its match check is not attempted for `NoRecord`. The offer is
    restored to `before`: a candidate on offer stays on offer, open and
    `pending`. This replaces `offer = own.clone()` in the Supply arm's
    `Err` branch. It is the restoration D4 describes, so it changes
    what the driver does and needs its own red test (task 5.1).
  - **`Ok(file)` whose record leads to no move.** `elsewhere` reports
    it, closes the new submission as `NotAttempted(NoMove)`, and
    restores the offer to `before`, where today it reverts to the
    file's own.
  - **`Ok(file)` whose record leads to a move.** The new record goes on
    offer, open. Only now is a candidate in `before` closed as
    `Rejected`, taking its lookup, match check and record.

  "Leads to a move" is what the loop already tests before asking
  (`candidate && !matches!(decision, Some(PlannedRename::Rename { .. }))`).
  The rejection is therefore made where that test passes, not where
  `resolve_supplied` returns.
- Skip with a candidate on offer closes it as `Rejected`.
- The collision notice (Rename, Rename anyway or Keep with `!on.kept &&
  !told` and a held work) closes nothing. The candidate stays open and
  `pending`, and the next answer is handled as any answer is.
- Every event the driver builds carries the input as it stands at that
  moment:
  - the event a description renders;
  - the held verdict a skip replays;
  - the record a move carries out;
  - the file's own `resolved` event before a `declined` skip.

  That input is one of three things:
  - `Supplied` with the submissions, when there are any. `used` names
    the open submission and holds the file's own lookup, retrieval,
    match check and record as `displaced`, when the record is that
    submission's.
  - `NotAttempted(NotSupplied)` when a question was put and nothing was
    submitted.
  - The pipeline's default otherwise.
- `described` stops passing a candidate through `accept`. A candidate
  is described with `acceptance` `Pending`, and with its conflict when
  it has one.
- `skipped` rebuilds the held verdict from the file's own evidence with
  the input, through `resolution_skip` or `resolved_event`. Replaying
  the held event verbatim would leave out the submissions.
- Quit returns `Settled::Stop` as today, and nothing is reported.

A record accepted from a submission is otherwise unchanged:
`remember` writes it at the move, `content-index-write` follows, and
`reidentified` reads the lookup's origin. A rejected candidate is never
passed to `remember`, so it never reaches the content index. The driver
already holds that line, and task 5.1 tests it.

**Docstrings to correct** (the implementer's; they are prose inside
code):

- `run::asked`: "the only channel left, since a candidate reaches no
  event stream";
- `run::skipped`: "A candidate the operator supplied is nowhere in
  either answer";
- `run::described`: "what the operator is shown before deciding is what
  the stream carries after". That is now true except for `acceptance`,
  which is `pending` before and settled after;
- `describe::Candidate`: "a candidate the file's decision did not settle
  on reaches nobody through the event stream";
- `run::asked`'s comment "A retry is the file's own lookup and replaces
  it", and the Retry arm's comment on the file's own resolution: a
  retry becomes the current round and keeps the ones before it (D12);
- `pipeline::resolve_supplied` ("on top of the file's existing evidence
  … the lookup, with every attempt") and `pipeline::unheld_evidence`:
  how the earlier rounds are carried forward (D12);
- `LookupEvidence`, and `is_conclusive`'s "Only an inconclusive lookup
  is worth making again": it reads the current round;
- `Acceptance`, `Sections`, and `Evidence`'s module docstring.

## D6. Human lines

**Decision.** A `resolved` line and a resolution `skipped` line whose
`identifier_input` holds any submission with `acceptance` `rejected` end
with one more clause. It comes after the library clause, if any:

```text
; candidate rejected: <identifier>
; candidates rejected: <identifier>, <identifier>
```

Each `<identifier>` is a rejected submission's `syntax.identifier`, in
submission order. The clause falls inside the text `human_line` already
escapes after `<path>: `. No other line changes. Two lines are
affected:

- **The round-two final skip.**
  `paper.pdf: skipped, no identifier found in its metadata or the pages
  read; candidate rejected: doi:10.1039/c9cc02492a`. Without a
  candidate, the line is unchanged.
- **A file whose own record resolved.** Skip with a candidate on offer
  reports the file's own `resolved` event, carrying the clause, and
  then `skipped, declined`. A non-verdict skip carries no sections, so
  it carries no clause.

A submission that reached no record, or one set aside for no move, is
not named. The operator was told about it in the report block of the
question that followed. The clause exists so that a skip after a
candidate does not read as the file's own skip.

**Rejected: naming every submission.** It lengthens every line after a
correction session to restate what the operator saw a moment earlier.
The stream keeps all of it.

## D7. The description

**Decision.** Three changes, all read from the event, so the
description stays a rendering of it:

- **`conflict`** is shown when `match_check` is `conflict` and
  `acceptance` is `overridden` or `pending`. A candidate with a
  conflict therefore shows the same line it shows today, now for the
  right reason.
- **`candidate`.** One line when `acceptance` is `pending`:

  ```text
  candidate   pending; skipping leaves the file as it was
  ```

  It tells the operator what a final skip after this candidate reports.
- **`rejected`.** One line per submission whose `acceptance` is
  `rejected`, in submission order. Each names the identifier whole, as
  the `identifier` line does:

  ```text
  rejected    doi:10.1039/c9cc02492a
  ```

Order on a resolution: `… file says`, `conflict`, `rejected` lines,
`candidate`, `new name`. On a failed verdict, the `rejected` lines come
last, after `library`. Every value is escaped, as every description
value is.

The `rename` requirement "An interactive question shows what the answer
rests on" holds the description to what the file's `resolved` event
carries. It lists what "belongs to the question rather than to the
resolution". A candidate's pending state is added to that list. It is
true only while the question is open, and the verdict reported after
the answer carries `accepted`, `overridden`, or the candidate's
`rejected` instead.

**Rejected: a line for every submission.** The report block above the
description already says what became of the last one, and repeating
the history in every question would bury the record being decided on.

## D12. Every round of the file's own lookup

This section is numbered after D11 because it was added after the
first draft. It sits here because the field table and the interface
below include it.

**Decision.** The file's own `lookup` keeps the current lookup exactly
as change 9 reports it. It gains `earlier`, an ordered list of the
lookups of the same identifier made before the current one, oldest
first. Each entry is a round:

| Round `status` | Fields | When |
|---|---|---|
| `attempted` | `attempts`: non-empty `[ATTEMPT]`, as change 9 reports them | at least one service was asked in that round |
| `no-eligible-service` | | no configured service supported the identifier in that round |

A round carries no `identifier` and no `origin`. A retry looks up the
file's own identifier again from the same pass, so both are the
enclosing lookup's, and stating them per round would state them twice.

`earlier` is present on `attempted` and `no-eligible-service` lookups,
and only where at least one earlier round exists. It is absent:

- from a lookup made once, which is every lookup no operator retried;
- from every lookup of an identifier an operator supplied: in a
  submission, and in the event's own `lookup` when `used` is set.

That absence is not a step that did not run, so change 9's D1 rule
"never omitted" does not apply. It follows `stored` on a found attempt
and `record` on a submission, which are likewise present exactly when
there is something to report. Omitting the empty case keeps every event
in which nobody retried byte-identical to change 9's, so the addition is
invisible to any consumer that never meets a retry.

**What decides Retry.** `LookupEvidence::is_conclusive` reads the
current round's `attempts` only, exactly as today. An outage, then a
retry that every service answers `not-found`, is conclusive. The
question put again therefore offers no further retry, whatever the
earlier round says.

**The engine.**

- `LookupEvidence::Attempted` gains `earlier: Vec<LookupRound>`, where
  `LookupRound { attempts: Vec<ServiceAttempt> }`. Empty `attempts`
  means no eligible service, the same convention `Attempted` already
  uses.
- `pipeline::resolve_supplied` and `pipeline::unheld_evidence` build
  the new lookup's `earlier`. It is `prior`'s `earlier` followed by
  `prior`'s current round, when all of these hold:
  - `origin` is `Origin::Extracted(_)`;
  - `prior.lookup` is `Attempted`;
  - `prior.lookup`'s identifier equals the one being looked up.

  Otherwise `earlier` is empty. An operator's supplied identifier
  therefore never inherits the file's rounds.
- `found_lookup` and `unheld_lookup` take the `earlier` they are given.
  `pipeline::standing` and `resolve_file` make lookups with no earlier
  rounds.
- The driver is unchanged apart from docstrings. Its Retry arm already
  passes the file's own evidence as `prior` and keeps what comes back
  as the file's own, so the rounds accumulate across any number of
  retries.
- A later accepted correction carries the rounds into `displaced.lookup`
  (D2), because they are part of the file's own lookup.
- `Evidence::retrieval` and `LookupEvidence::found` read the current
  round only. An earlier round never found a record: Retry is offered
  only where no record is on offer.

**The projection.**

- `Evidence::sections` maps each `LookupRound` to
  `{"status":"attempted","attempts":[…]}`, or to
  `{"status":"no-eligible-service"}` when its attempts are empty.
- The current round maps as change 9's D5 says.

**Human output is unchanged.** The `unresolvable` skip line and the
description's `no record` block show the current round only, as today.
The operator saw each earlier round when it happened: the question put
again after a failed retry opens with the `tried again` report block
(`describe::reported`). The skip line states the verdict, which is the
current round's. The history is evidence for a reader of the stream.
Showing it in the human output would restate on every line what the
terminal has already printed.

**Rejected: one `attempts` list spanning the rounds, each attempt with
a `round` number.** It changes every attempt object, or leaves `round`
on some attempts and not others. It also makes "the current lookup's
attempts", which decides Retry and the `unresolvable` line, something a
consumer must filter for. A no-eligible-service round would have no
attempt to carry its number.

**Rejected: earlier rounds as whole `lookup` objects.** Each would
repeat the identifier and the origin.

**Rejected: `earlier` always present, empty when there is no history.**
It is uniform, but it changes every attempted lookup in every event
and every exact-JSON test, and it adds a key that is empty on all but a
handful of events.

## D8. Field table

This table is normative for spelling. The spec deltas require the
behaviour. Change 9's D3 tables stand for every other section.

### `identifier_input`

| `status` | Fields | When |
|---|---|---|
| `supplied` | `submissions`: non-empty `[SUBMISSION]` in the order typed; `used`: a `submission` number or `null`; `displaced`: DISPLACED or `null`, non-null exactly when `used` is | at least one text was submitted (D1) |
| `not-attempted` | `reason`: `content-duplicate`, `not-asked`, `not-supplied` | nothing was submitted |

Key order: `status`, `submissions`, `used`, `displaced`.

### SUBMISSION

| Key | Value | Present |
|---|---|---|
| `submission` | 1, 2, … within the file | always |
| `raw` | the text as the prompt returned it | always |
| `syntax` | SYNTAX | always |
| `lookup` | change 9's `lookup` step, `origin` `operator`; or `{"status":"not-attempted","reason":"unparsed"}` | every entry but the used one |
| `record_retrieval` | `{"kind":"service-cache","service":S}`, `{"kind":"network","service":S}`, or `null` | every entry but the used one |
| `match_check` | change 9's `match_check` step; not attempted for `unparsed` or `no-record` where there is no record | every entry but the used one |
| `acceptance` | `{"status":"rejected"}`, or not attempted for `unparsed`, `no-record` or `no-move` | every entry but the used one |
| `record` | the full CSL-JSON record the submission reached | exactly when the entry's `record_retrieval` is not `null` |

### SYNTAX

| `status` | Fields |
|---|---|
| `parsed` | `identifier`: the normalised identifier |
| `rejected` | `reason`: `unrecognised`, `invalid` or `checksum`; `expected`: `doi`, `arxiv`, `pmid` or `isbn`, present exactly when `reason` is `invalid` or `checksum` |

### DISPLACED

| Key | Value |
|---|---|
| `lookup` | the file's own `lookup` step |
| `record_retrieval` | where the file's own record came from (any change-9 kind), or `null` |
| `match_check` | the file's own `match_check` step |
| `record` | the file's own record, resolved or refused; present exactly when `record_retrieval` is not `null` |

### `lookup.earlier` (D12)

On an `attempted` or `no-eligible-service` lookup of the file's own
identifier, present only when non-empty. It comes after `attempts`, or
after `origin` on `no-eligible-service`.

| ROUND `status` | Fields |
|---|---|
| `attempted` | `attempts`: non-empty `[ATTEMPT]` |
| `no-eligible-service` | |

### `acceptance` (the event's)

`not-applicable`, `automatic`, `overridden`, `accepted`, `pending` (D4).
Only the status, as before.

### `Unattempted` reasons added

`not-asked`, `not-supplied`, `unparsed`, `no-move`.

## D9. The interface the tests are written against

```rust
// crates/borax-core/src/identifier.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdentifierKind { Doi, Arxiv, Pmid, Isbn }
impl IdentifierKind {
    pub fn as_str(self) -> &'static str;           // "doi" | "arxiv" | "pmid" | "isbn"
}
/// Why text a person typed names no identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppliedError {
    Unrecognised,
    Invalid { kind: IdentifierKind },
    Checksum { kind: IdentifierKind },
}
impl SuppliedError {
    pub fn reason(self) -> &'static str;           // "unrecognised" | "invalid" | "checksum"
    pub fn expected(self) -> Option<IdentifierKind>;
}
impl fmt::Display for SuppliedError {}            // wording is the implementer's
impl std::error::Error for SuppliedError {}
pub fn supplied(input: &str) -> Result<Identifier, SuppliedError>;

// crates/borax/src/event.rs
pub struct Sections {
    pub library: LibraryStep,
    pub content_index: ContentIndexSection,
    pub extraction: ExtractionSection,
    pub identifier_input: IdentifierInputStep,     // new, in this position
    pub lookup: LookupStep,
    pub record_retrieval: Option<RetrievedFrom>,
    pub match_check: MatchCheckStep,
    pub acceptance: Acceptance,
}
// Tagged "status", kebab-case.
pub enum IdentifierInputStep {
    Supplied {
        submissions: Vec<Submission>,
        used: Option<u32>,                         // null when None
        displaced: Option<Box<Displaced>>,         // null when None
    },
    NotAttempted { reason: String },
}
pub struct Submission {
    pub submission: u32,
    pub raw: String,
    pub syntax: SyntaxStep,
    /// Flattened: its keys sit beside `syntax`. `None` exactly for the
    /// submission `used` names, whose keys are then absent.
    pub outcome: Option<Box<SubmissionOutcome>>,
}
pub struct SubmissionOutcome {
    pub lookup: LookupStep,
    pub record_retrieval: Option<RetrievedFrom>,   // null when None
    pub match_check: MatchCheckStep,
    pub acceptance: SubmissionAcceptance,
    pub record: Option<Box<Record>>,               // omitted when None
}
// Tagged "status", kebab-case.
pub enum SyntaxStep {
    Parsed { identifier: String },
    Rejected { reason: String, expected: Option<String> },  // `expected` omitted when None
}
// Tagged "status", kebab-case.
pub enum SubmissionAcceptance { Rejected, NotAttempted { reason: String } }
pub struct Displaced {
    pub lookup: LookupStep,
    pub record_retrieval: Option<RetrievedFrom>,   // null when None
    pub match_check: MatchCheckStep,
    pub record: Option<Box<Record>>,               // omitted when None
}
pub enum Acceptance { NotApplicable, Automatic, Overridden, Pending, Accepted }  // Copy, Eq
// Tagged "status", kebab-case. Change 9's variants each gain `earlier`.
pub enum LookupStep {
    Attempted {
        identifier: String,
        origin: IdentifierOrigin,
        attempts: Vec<ServiceAnswer>,
        earlier: Vec<LookupRound>,                 // omitted when empty
    },
    NoEligibleService {
        identifier: String,
        origin: IdentifierOrigin,
        earlier: Vec<LookupRound>,                 // omitted when empty
    },
    NotAttempted { reason: String },
}
// Tagged "status", kebab-case.
pub enum LookupRound {
    Attempted { attempts: Vec<ServiceAnswer> },
    NoEligibleService,
}

// crates/borax/src/evidence.rs
pub struct Evidence {
    pub library: Consultation,
    pub content_index: IndexEvidence,
    pub extraction: ExtractionEvidence,
    pub identifier_input: IdentifierInput,         // new
    pub lookup: LookupEvidence,
    pub match_check: MatchCheck,
}
pub enum IdentifierInput {
    Supplied { submissions: Vec<Submission>, used: Option<Displacement> },
    NotAttempted(Unattempted),
}
pub struct Submission {
    pub number: u32,
    pub raw: String,
    pub syntax: Result<Identifier, SuppliedError>,
    /// `None` exactly for the submission `used` names.
    pub outcome: Option<SubmissionOutcome>,
}
pub struct SubmissionOutcome {
    pub lookup: LookupEvidence,
    pub match_check: MatchCheck,
    pub record: Option<Record>,
    pub decision: CandidateDecision,
}
pub enum CandidateDecision { Rejected, NotAttempted(Unattempted) }
pub struct Displacement { pub submission: u32, pub displaced: Displaced }
pub struct Displaced {
    pub lookup: LookupEvidence,
    pub retrieval: Option<RecordRetrieval>,
    pub match_check: MatchCheck,
    pub record: Option<Record>,
}
pub enum LookupEvidence {
    Attempted {
        identifier: Identifier,
        origin: Origin,
        attempts: Vec<ServiceAttempt>,
        earlier: Vec<LookupRound>,                  // new: oldest first
    },
    NotAttempted(Unattempted),
}
/// One earlier lookup of the same identifier; no attempts means no
/// configured service supported it.
pub struct LookupRound { pub attempts: Vec<ServiceAttempt> }
pub enum Unattempted { /* change 9's eleven, */ NotAsked, NotSupplied, Unparsed, NoMove }
// All derive Debug, Clone, PartialEq, as their neighbours do.

// crates/borax/src/pipeline.rs
pub struct FileRecord {
    pub record: Record,
    pub hash: Option<ContentHash>,
    pub evidence: Evidence,
    pub overridden: bool,
    pub accepted: bool,                             // new
}
impl FileRecord {
    pub fn acceptance(&self) -> Acceptance;         // D5's table
}
pub fn accept(file: FileRecord) -> FileRecord;      // sets `accepted`; `overridden` on a conflict
```

The Rust names above are fixed, because tests construct values with
them. The serde attributes are the implementer's: flatten, tags,
`skip_serializing_if`, defaults and boxing. They must produce the JSON
of D8 and the illustrative lines, and keep `Event`'s round trip.

A flattened `Option<Box<SubmissionOutcome>>` deserializes to `None` when
its keys are absent, through serde's flatten of an `Option`. That is
the expected route. If it does not round-trip with
`record_retrieval: null` and an `f64` similarity inside, the fallback
is a hand-written `Serialize`/`Deserialize` for `Submission`. The
fallback keeps the Rust shape above, so no test changes for it.

## D10. How the specifications change

ADDED requirements:

- `resolution`:
  - the section's shape: "A resolution reports the identifiers an
    operator supplied";
  - each entry's outcome and the `used`/`displaced` relation: "Each
    submission reports what it came to";
  - the parser's reasons: "Supplied text that names no identifier says
    why".
- `rename`:
  - the interactive guarantees, with the round-two session as a
    scenario: "A file's verdict keeps every identifier its operator
    supplied";
  - the description lines: "An interactive description says what
    became of the operator's candidates".

MODIFIED requirements, each restated whole, with every scenario name
kept except the one renamed below:

- `resolution`:
  - "An operator can supply an identifier": looked up as typed, the
    refusal recorded, and pending until answered;
  - "Resolution events carry their evidence in sections": eight
    sections, the new reasons, and why the version stays 4;
  - "A resolution says why a step was not taken": the new steps and
    reasons;
  - "A resolution reports its title check and whether its record was
    accepted": `pending` and `accepted`;
  - "A resolution skip names its cause and states each fact once": its
    scenario "A taken target carries no sections" counted "the seven
    sections";
  - "Human resolution lines name the work, where it came from, and why
    a file was skipped": the rejected-candidate clause;
  - "A resolution keeps every service it asked, in order": a retry
    becomes the current lookup and keeps the ones before it, in place
    of "SHALL replace the lookup retained for the file". Whether a
    lookup is conclusive is judged on the current one;
  - "Each service asked is reported with its outcome": the `earlier`
    rounds.
- `rename`:
  - "An interactive run asks about files it could not settle": a skip
    carries its submissions, and "exactly as it was" becomes "in the
    situation it was in";
  - "A candidate the operator abandoned leaves nothing behind": reported
    as evidence only;
  - "An interactive question shows what the answer rests on": the
    pending state belongs to the question.
- `cli`:
  - "JSON Lines output is first-class": the version separates released
    schemas, and a schema no release has carried may change without a
    further bump (D4).

Each requirement's first line carries its SHALL. One MODIFIED
requirement carries a `<!-- drops: … -->` marker: "A resolution keeps
every service it asked, in order". Its scenario "Asking again replaces
the file's own lookup" is renamed "Asking again keeps every round of
the file's own lookup", because the old name states the behaviour this
change removes. The marker names that rename and nothing else. The
requirement's prose was checked by hand in place of the script. Every
other MODIFIED requirement keeps all its living prose and scenario
names, and `scripts/check-spec-deltas.py` confirms that.

The search behind the list covered `openspec/specs/`. It looked for
`acceptance`, `automatic`, `seven`, `supplied`, `abandon`, `exactly as
it was`, "reaches no", `again`, `retry` and `replace`, and the titles
the brief named. Only the two requirements above said a retry replaces
the file's lookup. Three hits are left alone:

- `cli` "A question describes whichever verdict it is asking about"
  already requires a conflict "being asked about" to be shown. A
  pending candidate's conflict is that case.
- `rename` "A file's verdict follows the operator's decision" forbids
  reporting an abandoned record "as the file's resolution". That stays
  true.
- `resolution` "A resolution says whether its library answered" names
  "a verdict reached by asking the services again" among those that
  carry the library's answer. That stays true, because a retry keeps
  the `library` section.

## D11. Existing tests: what changes and how

Change 9's rules apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. None is.

| File | Edit |
|---|---|
| `crates/borax-core/tests/identifier.rs` | (b) the four `supplied(...).is_none()` tests (`…_bare_digits_…`, `…_prose_…`, `…_an_empty_string`, `…_whitespace_only_input`) assert `Err(SuppliedError::Unrecognised)`. The `.unwrap()` calls compile unchanged |
| `crates/borax/tests/event.rs` | (a) `Evidence` literals (4) take `identifier_input`. `FileRecord` literals (4) take `accepted: false`. (b) `Sections` literals (18) take `identifier_input` not attempted for `not-asked`, or for `content-duplicate` on the content duplicate, as the pipeline would give them. The key-set and key-order tests near lines 619–720 list `identifier_input` between `extraction` and `lookup`. Every exact schema-4 JSON text in the file gains the matching `identifier_input` at that position |
| `crates/borax/tests/pipeline.rs` | (a) `Evidence` literals (11) and `FileRecord` literals (4) as above. The `sections(Acceptance::Automatic)` calls are unchanged |
| `crates/borax/tests/describe.rs` | (a) `Sections` literals (10) take `identifier_input`. (b) the `Fixture`'s `tier` `"supplied"` arm models a candidate being asked about, so it becomes D2's shape: `identifier_input` `supplied` with one used submission, and `acceptance` `Pending`. Its one test (`a_supplied_identifier_says_supplied_where_a_pass_would_be_named`) asserts only the `identifier` line, and that assertion is unchanged |
| `crates/borax/tests/dispatch.rs` | (b) the `sections_for` helper (near line 450) gains `identifier_input` not attempted for `not-asked` in every arm, because its callers compare whole batch events. Its `tier: Some("supplied")` arm has no caller at `a62839b`. Give it D2's shape (`supplied`, one used submission, `acceptance` `Accepted`), or remove it. (a) the other `Sections` literals (4). Any assertion on a candidate's whole description gains the `candidate` line |
| `crates/borax/tests/renaming.rs` | (a) `Sections` literals (2) and the `FileRecord` literal (1) |
| `crates/borax/tests/per_file.rs` | (a) `Sections` literals (2) |
| `crates/borax/tests/bib.rs` | (a) the `FileRecord` literal (1) |
| `crates/borax/tests/end_to_end.rs` | none required. The presence check near line 1340 is extended by task 8.1, not rewritten |

**`earlier` (D12), kind (a) throughout.** A struct literal and a
struct pattern take different edits. `earlier: vec![]` is an expression
and does not compile in a pattern. The sites at `a62839b`, by line:

| File | Constructors: add `earlier: vec![]` | Exact patterns: add `..` | Patterns already ending in `..`: no edit |
|---|---|---|---|
| `tests/pipeline.rs` | `LookupEvidence::Attempted` 1381, 1406, 1426, 1529, 1610, 1632, 1659, 1806, 4310; `LookupStep::NoEligibleService` 1436 | `LookupStep::Attempted` 1390; `LookupEvidence::Attempted` 724, 3131, 4085, 4420, 4732 | 1416, 1540, 1852, 1863, 3305, 4018, 4191, 4278, 4568, 4603, 4654 |
| `tests/event.rs` | `LookupEvidence::Attempted` 42; `LookupStep::Attempted` 94, 1127, 1175; `LookupStep::NoEligibleService` 1404 | none | 2064, 2482 |
| `tests/describe.rs` | `LookupStep::Attempted` 133, 252, 1224; `LookupStep::NoEligibleService` 1800 | none | none |
| `tests/dispatch.rs` | `LookupStep::Attempted` 498 (the `sections_for` helper) | none | 5386, 11257, 15744, 16058, 16132, 16336 |
| `tests/per_file.rs` | `LookupStep::Attempted` 287 | none | none |

That is 21 constructors and 6 exact patterns. An exact pattern takes
`..`, not `earlier, ..`. It is a shape-only edit, and binding `earlier`
to assert on it would change what the test expects (kind (b)), which
no existing test is asked to do. Line numbers drift as earlier groups
edit a file. The test stage reports a site it cannot find, rather than
guessing.

The same split applies in `crates/borax/src`, which the implementer
edits:

- `evidence.rs`: the two exact `LookupEvidence::Attempted` patterns
  in `sections` (near lines 136 and 144) bind `earlier`, because the
  projection uses it. The two `LookupStep` constructors there take it.
  Its other patterns end in `..`;
- `pipeline.rs`: the constructors in `found_lookup` and
  `unheld_lookup`;
- `describe.rs`: the or-pattern `LookupStep::NoEligibleService { identifier,
  origin }` in `looked_up` (near line 313) takes `..`.

No exact-JSON text changes, because an empty `earlier` is omitted. The
existing retry tests in `dispatch.rs`
(`an_outage_then_not_found_on_retry_stops_offering_a_retry`,
`a_retry_after_an_outage_keeps_the_extraction_pass_as_tier_not_supplied`,
and the two `interactive_retry_…` tests) assert the current round
through `..` patterns, so they pass unchanged.

The counts come from `rg -c` for `(?<![A-Za-z])Evidence \{`,
`(?<![A-Za-z])Sections \{` and `overridden:` at `a62839b`. A count the
test stage finds different is reported, not forced.

## Not changed, deliberately

- Which steps run, which services are asked and in what order, what is
  cached and when anything is written.
- When Retry is offered, and what a retry looks up (D12 only keeps the
  earlier rounds).
- The menus, their order, the prompt and its refusal.
- `run-finished`, `Counts`, exit status, and every event other than
  `resolved` and resolution `skipped`. Those two gain one key.
- What a quit reports.

## Risks / Trade-offs

- **The used entry is bare.** A consumer iterating submissions must
  read the event's sections for the used one. The invariant is simple
  (`used` names it, and `lookup.origin` is `operator`), it is stated in
  the requirement, and a test pins it. The alternative states every
  fact of the event's record twice.
- **Event size.** A rejected candidate's record and a displaced record
  are carried whole. They appear only after an operator has corrected a
  file, which is rare, and the conflict skip's `candidate` already set
  this precedent.
- **`pending` reaches no emitted event.** That is settled (Context item
  3), and it is the frontend boundary working as intended (D4). The value is tested on the description
  event, where it is rendered.
- **The interactive human paths are the thinnest-covered part of the
  suite** (`STATE.md`). Groups 5 to 7 test through the dispatch
  harness, both the JSON stream and what the terminal is shown.

## Illustrative schema-4 lines

These are pretty-printed. In the stream, each is one line. Records are
abbreviated with `…`. Every one assumes a run with no library.

**The round-two session's final skip.** The operator typed
`not-an-identifier` (refused), `10.1039/c9cc02492` (no service holds
it), and `10.1039/c9cc02492a` (Crossref returned a record), and then
answered Skip:

```json
{"schema":4,"event":"skipped","path":"paper.pdf","reason":{"kind":"text-without-identifier"},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"miss"},"write":{"status":"not-attempted","reason":"extraction-failed"}},
 "extraction":{"result":{"status":"text-without-identifier"},"titles":{"status":"read","claims":[]}},
 "identifier_input":{"status":"supplied","submissions":[
   {"submission":1,"raw":"not-an-identifier","syntax":{"status":"rejected","reason":"unrecognised"},
    "lookup":{"status":"not-attempted","reason":"unparsed"},"record_retrieval":null,
    "match_check":{"status":"not-attempted","reason":"unparsed"},
    "acceptance":{"status":"not-attempted","reason":"unparsed"}},
   {"submission":2,"raw":"10.1039/c9cc02492","syntax":{"status":"parsed","identifier":"doi:10.1039/c9cc02492"},
    "lookup":{"status":"attempted","identifier":"doi:10.1039/c9cc02492","origin":"operator",
              "attempts":[{"service":"crossref","outcome":{"status":"not-found"}},
                          {"service":"openalex","outcome":{"status":"not-found"}}]},
    "record_retrieval":null,
    "match_check":{"status":"not-attempted","reason":"no-record"},
    "acceptance":{"status":"not-attempted","reason":"no-record"}},
   {"submission":3,"raw":"10.1039/c9cc02492a","syntax":{"status":"parsed","identifier":"doi:10.1039/c9cc02492a"},
    "lookup":{"status":"attempted","identifier":"doi:10.1039/c9cc02492a","origin":"operator",
              "attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}]},
    "record_retrieval":{"kind":"network","service":"crossref"},
    "match_check":{"status":"insufficient-evidence","reason":"no-titles"},
    "acceptance":{"status":"rejected"},
    "record":{"type":"article-journal",…,"DOI":"10.1039/c9cc02492a"}}],
  "used":null,"displaced":null},
 "lookup":{"status":"not-attempted","reason":"extraction-failed"},
 "record_retrieval":null,
 "match_check":{"status":"not-attempted","reason":"extraction-failed"},
 "acceptance":{"status":"not-applicable"}}
```

Human: `paper.pdf: skipped, no identifier found in its metadata or the
pages read; candidate rejected: doi:10.1039/c9cc02492a`

**The event the third question's description rendered**, before the
Skip. Submissions 1 and 2 are as above, and submission 3 is the used
one:

```json
{"schema":4,"event":"resolved","path":"paper.pdf","identifier":"doi:10.1039/c9cc02492a","record":{…},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"miss"},"write":{"status":"not-attempted","reason":"awaiting-acceptance"}},
 "extraction":{"result":{"status":"text-without-identifier"},"titles":{"status":"read","claims":[]}},
 "identifier_input":{"status":"supplied","submissions":[{…1…},{…2…},
   {"submission":3,"raw":"10.1039/c9cc02492a","syntax":{"status":"parsed","identifier":"doi:10.1039/c9cc02492a"}}],
  "used":3,
  "displaced":{"lookup":{"status":"not-attempted","reason":"extraction-failed"},"record_retrieval":null,
               "match_check":{"status":"not-attempted","reason":"extraction-failed"}}},
 "lookup":{"status":"attempted","identifier":"doi:10.1039/c9cc02492a","origin":"operator",
           "attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}]},
 "record_retrieval":{"kind":"network","service":"crossref"},
 "match_check":{"status":"insufficient-evidence","reason":"no-titles"},
 "acceptance":{"status":"pending"}}
```

Its description (layout as today; the `candidate` line is new):

```text
── 1 of 1 ───────────────────────────────────────────────────────────────────
file        paper.pdf
identifier  doi:10.1039/c9cc02492a, supplied
record      Crossref
type        journal article
title       …
file says   no title in its metadata
candidate   pending; skipping leaves the file as it was
new name    …
```

**The same session, answered Rename instead.** The event is the one
above, with `"acceptance":{"status":"accepted"}`. It is followed by
`renamed` and `content-index-write`, as change 9's D8 orders them.

**A correction of an `.author` DOI, accepted.** The file's own lookup
survives in `displaced`:

```json
{"schema":4,"event":"resolved","path":"manuscript.pdf","identifier":"doi:10.1021/jacs.4c01234","record":{…},
 …,"extraction":{"result":{"status":"found","identifier":"doi:10.1021/jacs.4c01234.author","tier":"text-layer"},
                 "titles":{"status":"read","claims":[]}},
 "identifier_input":{"status":"supplied",
   "submissions":[{"submission":1,"raw":"10.1021/jacs.4c01234","syntax":{"status":"parsed","identifier":"doi:10.1021/jacs.4c01234"}}],
   "used":1,
   "displaced":{"lookup":{"status":"attempted","identifier":"doi:10.1021/jacs.4c01234.author","origin":"extracted",
                          "attempts":[{"service":"crossref","outcome":{"status":"not-found"}},
                                      {"service":"openalex","outcome":{"status":"not-found"}}]},
                "record_retrieval":null,
                "match_check":{"status":"not-attempted","reason":"no-record"}}},
 "lookup":{"status":"attempted","identifier":"doi:10.1021/jacs.4c01234","origin":"operator","attempts":[…found…]},
 "record_retrieval":{"kind":"network","service":"crossref"},
 "match_check":{"status":"insufficient-evidence","reason":"no-titles"},
 "acceptance":{"status":"accepted"}}
```

**A supplied record accepted over its conflict** (Rename anyway): the
same shape, with `match_check` `conflict` and
`"acceptance":{"status":"overridden"}`. **The file's own conflict
overridden**, with nothing supplied: change 9's line, with
`"identifier_input":{"status":"not-attempted","reason":"not-supplied"}`.

**An outage, a retry, and an answer** (D12). Crossref and OpenAlex
were unavailable, the operator chose Try the services again, and
OpenAlex answered:

```json
{"schema":4,"event":"resolved","path":"paper.pdf",…,
 "identifier_input":{"status":"not-attempted","reason":"not-supplied"},
 "lookup":{"status":"attempted","identifier":"doi:10.1039/c5ay00042d","origin":"extracted",
           "attempts":[{"service":"crossref","outcome":{"status":"unavailable","message":"HTTP 503"}},
                       {"service":"openalex","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}],
           "earlier":[{"status":"attempted","attempts":[
             {"service":"crossref","outcome":{"status":"unavailable","message":"HTTP 503"}},
             {"service":"openalex","outcome":{"status":"unavailable","message":"HTTP 502"}}]}]},
 "record_retrieval":{"kind":"network","service":"openalex"},
 …,"acceptance":{"status":"automatic"}}
```

The human line is change 9's `resolved` line, unchanged.

**Any batch resolution:** change 9's line, with
`"identifier_input":{"status":"not-attempted","reason":"not-asked"}`
between `extraction` and `lookup`. On a content duplicate, the reason is
`content-duplicate`.
