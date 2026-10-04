## Why

Change 9 (`sectioned-resolved-event`, archived) reports what a resolution
found out about a file in seven sections. It reserved an eighth,
`identifier_input`, between `extraction` and `lookup`, and left one
value interim. Everything an operator types at the identifier prompt
still reaches no event. The engine at `a62839b` loses it at four points:

- **Refused text.** `run::supplied_identifier`
  (`crates/borax/src/run.rs`, line 2390) loops on the prompt until a
  text parses. Each refused text is quoted back to the operator and
  then dropped. `identifier::supplied`
  (`crates/borax-core/src/identifier.rs`, line 457) returns
  `Option<Identifier>`, so even the refusal cannot say why the text
  failed. The parser does know: it recognises a `pmid:` or `isbn:`
  prefix, it tells an ISBN that fails its checksum from one that is
  malformed (`IdentifierError::Checksum`), and it recognises the DOI and
  arXiv prefixes and the `10.` every DOI begins with.
- **Supplied identifiers that led nowhere.** A supplied identifier that
  no service holds is shown in the next question's report block and is
  then forgotten (`run::asked`, the `Err(unheld)` arm of
  `Answer::Supply`).
- **Candidates set aside.** A record a supplied identifier reached and
  the operator then passed over is not reported anywhere. The driver's
  docstring says so on purpose: "a candidate reaches no event stream".
  Skipping such a file replays the file's own held verdict
  (`run::skipped`, line 2283). The round-two interactive review (the
  maintainer's notes, outside the repository) recorded the result: the
  operator supplied `10.1039/c9cc02492`, which no service held, then
  `10.1039/c9cc02492a`, which returned a record, and then answered Skip.
  The skip reported the file's original no-identifier result and said
  nothing about the candidate.
- **The file's own lookup, once a supplied record is accepted.**
  `pipeline::resolve_supplied` (`crates/borax/src/pipeline.rs`, line
  786) builds the candidate's evidence on top of the file's own and
  replaces its `lookup` and `match_check`. When the operator accepts the
  candidate, the `resolved` event carries the operator's lookup, and the
  file's own failed lookup is gone. That is the case supplying
  identifiers was built for: a DOI read from the file, with `.author`
  appended, that no service holds.

A retry loses evidence too. When the services fail to answer, the
operator can ask them again. `pipeline::unheld_evidence` and
`resolve_supplied`, given the file's own extraction pass as the origin,
build the new lookup and drop the one before it. The driver then makes
that the file's own evidence (`run::asked`, the `Answer::Retry` arm).
An outage followed by a retry therefore keeps only the second attempt
list, and the stream never learns the first one happened. The living
requirement "A resolution keeps every service it asked, in order" says
a retry replaces the file's lookup, so this is the specified behaviour.
The maintainer has asked for it to change here.

Acceptance has a related gap. `FileRecord::acceptance` (`pipeline.rs`,
line 111) knows two values for a record that was used: `overridden` and
`automatic`. A record the operator supplied and renamed from with no
title conflict therefore reports `automatic`. Change 9's design D7
names this value as interim, and the maintainer accepted it on the
condition that this change replaces it before 0.9.0 ships. The question's
description has the same gap. It renders `run::described`, which passes
a candidate through `pipeline::accept` before anything is accepted. So a
candidate whose titles agree is described as `automatic`, and one
whose titles conflict as `overridden`, while the operator is still
deciding.

This is change 10 of the roadmap's Phase 3. The Phase 3
resolution-schema design discussion (maintainer's notes, outside the
repository) assigns it these tasks:

- add operator input and pending acceptance to schema 4, additively,
  before 0.9.0;
- add `pending`, `accepted` and `rejected` for operator candidates;
- leave even a title-agreeing candidate pending until the operator
  accepts it;
- keep evidence inside each file's verdict event, with no prompt or
  answer events, no machine-readable stdin protocol and no title
  search.

## What Changes

- **The `identifier_input` section.** Every `resolved` event and every
  resolution `skipped` event carries it, between `extraction` and
  `lookup`. It is `supplied` when the operator typed anything at the
  identifier prompt for the file. Otherwise it is not attempted, with
  reason `not-asked` (no question was put about the file, which covers
  every batch run, `resolve` and `bib`), `not-supplied` (asked, nothing
  typed), or `content-duplicate`. Design D1.
- **Submissions.** `supplied` lists every text the operator typed, in
  order, numbered from 1 within the file. Each entry carries its `raw`
  text and its `syntax` result: `parsed` with the normalised identifier,
  or `rejected` with a reason. Each entry also carries its own outcome:
  its `lookup`, `record_retrieval`, `match_check`, `acceptance`, and the
  `record` it reached. The one exception is the submission whose record
  the event itself reports. `used` names that submission, and its
  outcome is the event's own sections, stated once. Design D2.
- **What a used submission displaced.** When the event's record came
  from a submission, `displaced` carries the file's own `lookup`,
  `record_retrieval`, `match_check` and record. These are the facts the
  operator's record replaced in the event's sections. The file's own
  failed lookup therefore survives an accepted correction (design D2).
- **Syntax reasons from the parser.** `identifier::supplied` returns
  `Result<Identifier, SuppliedError>`. The reasons are `unrecognised`,
  `invalid` with the form the text named (`doi`, `arxiv`, `pmid` or
  `isbn`), and `checksum` (an ISBN). Every text that parses today parses
  to the same identifier. The operator's refusal message is unchanged
  (design D3).
- **No repair.** A truncated DOI such as `10.1039/c9cc02492` is
  well-formed, so it parses and is looked up exactly as typed. Nothing
  completes or extends it. This pins current behaviour (design D3).
- **Candidate decisions.** The top-level `acceptance` gains `pending`
  and `accepted`. A record reached from a submission is `pending` until
  the operator answers, whatever its title check concluded. Once
  renamed from, it is `accepted`, or `overridden` when the rename went
  over its conflict. `accepted` replaces change 9's interim `automatic`
  for a supplied record. A submission that was set aside reports
  `rejected` on its entry: the operator answered Skip while it was on
  offer, or supplied another identifier that parsed. An entry that
  reached nothing to accept is not attempted, with reason `unparsed`,
  `no-record` or `no-move`. Design D4.
- **No reported verdict is `pending`.** Only the event a question's
  description is rendered from carries `pending`. Every verdict is
  reported after the operator's answer (design D4).
- **Every round of the file's own lookup.** A lookup the operator asks
  for again becomes the file's current `lookup`, as today. The lookups
  before it are kept on that section as `earlier`: an ordered list,
  oldest first. Each round is `attempted` with its `attempts`, or
  `no-eligible-service`. The rounds repeat no identifier and no origin,
  because those are the lookup's own. `earlier` is absent from a lookup
  made once, and from every lookup of a supplied identifier. Whether
  Retry is offered is still decided by the current round alone
  (`LookupEvidence::is_conclusive`). The human lines and the
  description show the current round only, as they do today. Design
  D12.
- **Human lines.** A `resolved` line or a resolution `skipped` line
  whose file had a candidate rejected ends with `; candidate rejected:
  <identifier>`, or `; candidates rejected: <a>, <b>` for more than one.
  The clause is escaped with the rest of the line. A final skip after a
  candidate therefore reads differently from the file's own skip
  (design D6).
- **The description.** A candidate on offer gets one more line:
  `candidate   pending; skipping leaves the file as it was`. Its
  conflict line is shown while it is pending, as today. Each rejected
  candidate is named on a `rejected` line. Nothing else in the
  description changes (design D7).
- **Engine surface.** The following are added:
  - `Evidence::identifier_input`;
  - `IdentifierInput` and its submission types in `evidence.rs`;
  - `FileRecord::accepted`;
  - `earlier` on `LookupEvidence::Attempted` and on the event's
    `LookupStep::Attempted` and `NoEligibleService`, with a
    `LookupRound` type on each side;
  - the `Unattempted` reasons `NotAsked`, `NotSupplied`, `Unparsed` and
    `NoMove`;
  - `IdentifierKind` and `SuppliedError` in `borax-core`.

  `FileRecord::acceptance` gives `Pending` and `Accepted` as well.
  `run::described` stops calling `accept`. The interactive driver
  records each submission and puts the input, as it stands, on every
  event it builds (design D5).

The schema version stays 4. `identifier_input` and `lookup.earlier` are
new keys, and `pending` and `accepted` are new values. A consumer that ignores what it
does not know reads them unchanged, and the `cli` requirement "JSON Lines
output is first-class" asks no bump for an addition. One value changes
meaning: a supplied record renamed with no conflict reported `automatic`
and now reports `accepted`. That would need a bump if a release had
carried it. None has. Schema 4 and the interim value exist only on
`main` after 0.8.0, which shipped schema 3. The maintainer accepted the
interim value on exactly this condition.

Explicitly out of scope:

- prompt or answer events, a machine-readable stdin protocol, and title
  search;
- when Retry is offered: the current round alone still decides it;
- `run-finished` counters, exit status, and which services are asked;
- the wording of the prompt's refusal;
- a file the operator quit at, which is reported nothing, so its
  submissions are reported nowhere.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED:
    - "A resolution reports the identifiers an operator supplied"
    - "Each submission reports what it came to"
    - "Supplied text that names no identifier says why"
  - MODIFIED:
    - "An operator can supply an identifier": the syntax result, the
      looked-up-as-typed rule, and pending until answered
    - "Resolution events carry their evidence in sections": eight
      sections and four new reasons
    - "A resolution says why a step was not taken": the operator-input
      step, a submission's steps, and their reasons
    - "A resolution reports its title check and whether its record was
      accepted": `pending` and `accepted`
    - "A resolution skip names its cause and states each fact once": one
      scenario counted "the seven sections"
    - "A resolution keeps every service it asked, in order": a retry
      keeps the lookups before it, instead of replacing them
    - "Each service asked is reported with its outcome": the `earlier`
      rounds
    - "Human resolution lines name the work, where it came from, and why
      a file was skipped": the rejected-candidate clause
- `rename`:
  - ADDED:
    - "A file's verdict keeps every identifier its operator supplied"
    - "An interactive description says what became of the operator's
      candidates"
  - MODIFIED:
    - "An interactive run asks about files it could not settle": a
      skipped file carries its submissions, and "exactly as it was"
      described the file's situation, not its report
    - "A candidate the operator abandoned leaves nothing behind": an
      abandoned candidate is now reported, as evidence and never as the
      resolution
    - "An interactive question shows what the answer rests on": a
      candidate's pending state belongs to the question

Requirements checked and left unchanged:

- `resolution`:
  - "A resolution says whether its library answered", whose paragraph
    covers "a verdict reached by asking the services again". It still
    holds: a retry keeps the library section as it was.
  - "An operator's answer about a file is remembered". A rejected
    candidate is still never written to the content index, and task 5.1
    tests it.
  - "A resolution reports where its record was retrieved". An operator's
    lookup still reports the service. A submission's own
    `record_retrieval` follows the same rule.
  - "What became of remembering an operator's answer is reported". The
    `content-index-write` event is unchanged.
- `rename`:
  - "A file's verdict follows the operator's decision". An abandoned
    record is still never reported as the file's resolution. It appears
    only inside a submission.
  - "An interactive description says whether the file's titles were
    read".
  - "Quitting an interactive run leaves the rest untouched".
- `cli`:
  - "A question describes whichever verdict it is asking about". A
    conflict is still shown "whether the conflict is being asked about
    or was accepted", and a pending candidate's conflict is the first
    case.
  - "JSON Lines output is first-class". This change is additive except
    for the unreleased interim value, as stated above.
  - "Non-interactive contexts never prompt".
- `extraction`: nothing. Extraction's results and titles are unchanged.

## Impact

- `crates/borax-core/src/identifier.rs`: `IdentifierKind`,
  `SuppliedError`, and `supplied` returning a `Result`.
- `crates/borax/src/event.rs`:
  - `Sections::identifier_input`;
  - `IdentifierInputStep`, `Submission`, `SubmissionOutcome`,
    `SyntaxStep`, `SubmissionAcceptance` and `Displaced`;
  - `Acceptance::Pending` and `Acceptance::Accepted`;
  - `earlier` on `LookupStep::Attempted` and `NoEligibleService`, and
    the `LookupRound` type;
  - the rejected-candidate clause in `human_line`.
- `crates/borax/src/evidence.rs`:
  - `Evidence::identifier_input`, with `IdentifierInput`, `Submission`,
    `SubmissionOutcome`, `CandidateDecision`, `Displacement` and
    `Displaced`;
  - `earlier` on `LookupEvidence::Attempted`, and `LookupRound`;
  - the four `Unattempted` reasons;
  - the projection.
- `crates/borax/src/pipeline.rs`:
  - `resolve_supplied` and `unheld_evidence` carry the earlier rounds
    forward on a lookup of the file's own identifier;
  - `FileRecord::accepted`, which `accept` sets;
  - `FileRecord::acceptance` gives `Pending` and `Accepted`;
  - `identifier_input` defaults to `not-asked`, and to
    `content-duplicate` on a content duplicate.
- `crates/borax/src/run.rs`:
  - the driver records each submission, the refused ones included;
  - it marks a candidate rejected or not offered;
  - it puts the input on every event it builds;
  - `described` no longer calls `accept`;
  - `skipped` rebuilds the held verdict with the input;
  - the docstrings that say a candidate reaches no stream, or that a
    retry replaces the file's lookup, are corrected. The retry path's
    code is otherwise unchanged, because `pipeline` carries the rounds
    forward.
- `crates/borax/src/describe.rs`: the `candidate` and `rejected` lines,
  and the conflict line on a pending candidate.
- Tests:
  - compile-only edits where tests build `Evidence`, `Sections`,
    `FileRecord`, `LookupEvidence::Attempted` or `LookupStep` literals;
  - deliberate rewrites where tests assert the section key set, exact
    schema-4 JSON, or `supplied(...)` returning `None`.

  Design D11 lists each file.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`: the interactive session's skip, retry and
    supplying passages, and the Run logs JSONL passages;
  - `CHANGELOG.md`: the Unreleased schema-4 entry.
- `openspec/STATE.md` (implementer):
  - a paragraph for this change;
  - the interim-value sentence in the change-9 paragraph is closed;
  - "An abandoned candidate leaves nothing anywhere" in the
    `supply-identifiers-interactively` paragraph is corrected;
  - "a retry replaces it" in the `expose-resolution-attempts` paragraph
    is corrected.
- No new dependency, configuration key, flag or event.

## Deferred

- **A frontend protocol.** Prompt and answer events, a machine-readable
  stdin protocol, and title search stay deferred beyond this change. The
  `pending` value exists so that one can be reported. This change emits
  it on no event.
