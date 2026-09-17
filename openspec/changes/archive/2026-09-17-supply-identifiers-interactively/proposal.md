## Why

On real folders, most of the files borax fails on are not borax's
failures. They are free preprints and author manuscripts, which print
no DOI, carry no identifier in their metadata, and were never meant to
be citable as they stand. In a twelve-file sample of the real-PDF
corpus, four were skipped `no identifier found`, and that proportion is
what holds the hit rate near half.

The person running borax usually knows, or can find in seconds, what
such a file is: the arXiv listing it was downloaded from, the DOI of the
published version. Today that knowledge has nowhere to go. The file is
skipped, and the only remedy is to rename it by hand, outside the
ledger, the run log and the naming templates — everything borax exists
to keep consistent.

The second kind of failure borax cannot fix alone is the metadata
conflict. A file whose embedded title disagrees with its record is
skipped because the record might be for a different work, and it is
right to be: a confident wrong rename is the failure this tool exists
to rule out. But the disagreement is often a manuscript title that
changed before publication, and a person looking at both titles can
tell in a glance what the similarity heuristic cannot.

`add-interactive-rename` built a session that can stop and ask, and
`show-record-before-asking` puts the record in front of the operator.
This change lets the operator answer the two questions only they can.

## What Changes

- **A file without an identifier can be given one.** When an interactive
  run finds no identifier in a file, or finds one no service holds, it
  shows what it tried and asks: supply an identifier, skip, or quit.
  A supplied identifier is parsed like an extracted one — a DOI in any
  form extraction accepts, an arXiv identifier, or a prefixed PMID or
  ISBN — and resolved through the same services in the same order.
- **Every question offers a different identifier.** The rename question
  gains "supply a different identifier", for the case the description
  exposes: a text-layer DOI that belongs to a cited reference.
- **A conflicting record can be accepted by hand.** When a record
  resolves but the file's title disagrees with it, an interactive run
  shows both titles and their similarity and offers: rename anyway,
  supply a different identifier, skip, or quit. Batch runs still skip,
  always.
- **A supplied record is shown before it is used.** A supplied
  identifier's record is described and put to the operator like any
  other; the conflict check still runs against it, and a disagreement is
  shown rather than enforced, since the operator chose the identifier.
- **The answer is remembered.** A rename carried out on a supplied
  identifier or over a conflict writes the record to the content index
  under the file's hash, as an ordinary resolution does. The next run —
  interactive or batch — finds the file already identified and does not
  ask again. A skipped answer leaves nothing behind.
- **Already-named files can be re-identified.** In an interactive run
  with `--no-skip-named`, an already-named file is asked about — keep
  its name, supply a different identifier, or quit — so a file once
  named from a wrong identifier can be put right.
- **The stream records who decided.** A `resolved` event from a supplied
  identifier reports `supplied` as its extraction tier, and one accepted
  over a conflict carries the conflict it overrode. A file's verdict is
  reported once, after its decision, so no file is reported skipped and
  then renamed.

## Capabilities

### Modified Capabilities

- `resolution`: the ambiguity requirement permits an operator, and only
  an operator, to accept a conflicting record in an interactive run;
  new requirements for supplied identifiers and for remembering an
  operator's answer.
- `rename`: interactive questions for unidentified, unresolvable,
  conflicting and (with `--no-skip-named`) already-named files; a
  file's verdict follows its decision.

## Impact

- `crates/borax/src/pipeline.rs`: resolution is split so that a file's
  hash, claims and extraction outcome are available to a caller that
  wants to ask before deciding, and so an identifier can be resolved for
  a file without extraction. The batch entry points keep their
  signatures and outcomes.
- `crates/borax/src/run.rs`: the interactive driver gains the situations
  above; it re-proposes a file after a new identifier resolves.
- `crates/borax/src/session.rs`: `Answer` gains `Supply`, `Override`
  and `Keep`; the `Asker` trait gains a text-input method; the `inquire`
  adapter uses `inquire::Text` for it.
- `crates/borax/src/event.rs`: `resolved` gains `overrode` (the
  conflict, when one was accepted); `tier` gains the value `supplied`.
- `crates/borax/src/describe.rs`: renders unresolved attempts, both
  titles of a conflict with their similarity, and a supplied identifier.
- Documents a person reads: `docs/manual.org` (`borax rename`, a section
  on supplying identifiers and on what the content index remembers),
  `README.md`, `CHANGELOG.md`, `openspec/STATE.md`.

## Deferred

- **Searching by title.** A preprint with no identifier usually has a
  title, and Crossref and OpenAlex can be searched by it. Offering the
  best candidates in the same menu would turn most of these questions
  into a single choice instead of a lookup the operator does elsewhere.
  It needs a search client per service, a candidate ranking and its own
  guard against a plausible wrong match — a change of its own, which
  this one makes room for in the menu.
- **A durable record of answers.** The content index is a cache:
  `borax cache --clear` and `--no-cache` forget what the operator told
  it. The sidecar and the ledger keep what a renamed file is, and
  `ledger rebuild` recovers it, but a later run does not consult them
  for identification. A curated identifier table, in the manner of
  `add-external-tables`, is the durable answer if forgetting turns out
  to matter.
- **Supplying identifiers to `resolve` and `bib`.** Both could use it;
  both are batch commands today, and neither has a session to ask from.
