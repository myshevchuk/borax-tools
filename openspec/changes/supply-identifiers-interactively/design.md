## Context

`pipeline::resolve_file` is a straight line — content index, extraction,
resolution, conflict check — that returns `Resolved` or `Skipped` and
emits nothing itself. `run::resolved_record` emits the verdict event the
moment `resolve_file` returns, then hands the record on. That ordering
is what a batch run wants and what this change cannot keep: a conflict
emitted as `skipped` before the operator has seen it would have to be
followed, on an override, by `renamed` for the same file.

`borax_core::identifier` already parses every identifier kind the
resolver dispatches on — DOI, arXiv, PMID, ISBN — with the normalisation
extraction uses, and `borax_sources::dispatch::resolve` takes an
`Identifier` regardless of where it came from.

The content index is written in `resolve_file` after the conflict check
passes, under the file's hash, and read before extraction. A record in
it is served for that file under any name, with no extraction, no
service and no conflict check.

## Goals / Non-Goals

Goals:

- Every file the run could not identify on its own, and every file whose
  record it would not trust on its own, can be settled by the operator
  in the session, without leaving borax.
- Nothing the operator did not see is used: a supplied identifier's
  record is described and asked about before any move.
- Batch runs keep their contract exactly: they never accept a conflict,
  never take an identifier from anyone, never ask.
- The operator is asked about a file once, not on every run.

Non-goals:

- Finding identifiers for the operator (title search is deferred).
- Duplicates. A file the ledger reports as a duplicate is reported and
  not asked about; the ledger's verdict is about the collection, not
  about what the file is.

## Decisions

### D1. Which files get which question

| Situation | Choices |
|---|---|
| resolves, proposal is a move | rename · supply a different identifier · skip · quit |
| no identifier found (including no text layer) | supply an identifier · skip · quit |
| identifier found, no service holds it | supply a different identifier · skip · quit |
| conflict, proposal is a move | rename anyway · supply a different identifier · skip · quit |
| conflict, proposal is not a move | supply a different identifier · skip · quit |
| unreadable or encrypted, hash known | supply an identifier · skip · quit |
| already named, `--no-skip-named` | keep · supply a different identifier · quit |
| resolves, proposal is not a move | no question, as today |
| duplicate, or hash unknown | no question, as today |

A file whose hash is unknown cannot be renamed in an applying run
(`Unrecordable`), so offering it an identifier would lead to a question
whose only useful answer then fails; it is not asked.

"No service holds it" is only one way resolution fails.
`Unresolved::is_conclusive` already separates a confirmed absence from
the services being unreachable, rate-limited, or answering with
nonsense, and the question says which happened: a file nobody holds is
one to supply another identifier for, while services that could not
answer are worth asking again. The inconclusive case therefore offers
*try again* as its first choice, which re-runs the same lookup, and
the conclusive one does not offer it at all. Neither presents an
outage as evidence about the file.

**Where a supplied identifier leads.** Supplying is a loop, and each
turn ends in one of four places:

| After supplying | What happens |
|---|---|
| resolves, proposal is a move | described, then the move question, with *supply a different identifier* still on it |
| resolves, proposal is not a move | reported as that outcome — target taken, unnameable, already named — and the file's menu is put again |
| does not resolve | the attempts are shown and the file's menu is put again |
| operator escapes the input | the file's menu is put again, unchanged |

The file's menu is the one its *current* situation calls for, not the
one it started with. A file that resolved on its own and was offered a
move keeps that offer: supplying an identifier that then fails leaves
the original record still in hand, and the operator can accept it,
supply another, or skip. A file that never resolved has nothing to
fall back to, so its menu stays the unidentified one. Nothing about
the loop can turn a file that had a record into a file that has none.

"Rename anyway" names the proposed target in its label, so the one
choice that overrides a safety check says exactly what it will do.
Skip, not rename, is the default for a conflict: Enter must never be the
override.

### D2. Supplying an identifier

`Asker` gains `fn text(&mut self, prompt: &TextPrompt) -> Option<String>`;
`None` is Esc, which returns to the file's menu. The input is tried, in
order, as a DOI (any form `Doi::parse` accepts, so a pasted
`https://doi.org/…` works), an arXiv identifier, and — only with a
`pmid:` or `isbn:` prefix — a PMID or ISBN. The prefixes are required for
the last two because a bare run of digits is both.

Input that parses as nothing is refused in place with the kinds
accepted, and asked again. A parsed identifier is resolved with
`dispatch::resolve`. If no service holds it, the attempts are shown and
the file's menu is put again, with the unresolvable situation's choices;
supplying the same identifier again is how the operator retries after a
transient failure.

A resolved record is described (`show-record-before-asking`), planned
with `Planning::propose`, and asked about. Proposing claims nothing
(`add-interactive-rename` D3), so a file can go round this loop any
number of times and leave the plan exactly as it found it.

### D2a. The claims a comparison needs are read when they are needed

Comparing a supplied record against the file's own titles needs those
titles, and two paths through the pipeline do not have them: a file
the content index answered for was never opened, and `extract_from`
collects titles only after an identifier is found, so a file with no
identifier reaches the driver with none either.

Both are read when the comparison needs them rather than always: the
driver asks the library for the file's claimed titles at the moment an
operator supplies an identifier, and that read is the only new work
this change does on the ordinary path — none, since the ordinary path
never asks. Collecting titles moves out from under the
identifier-found branch so that a file with no identifier still has
them to be read.

A file that cannot be opened at all has no claims and no comparison;
the record is described and the question is put without that line,
which is what a file borax cannot read was always going to allow.

### D3. The conflict check still runs on a supplied record

The file's claims are compared with a supplied identifier's record as
with an extracted one. A disagreement does not block: the operator
picked the identifier, which is a stronger statement than the heuristic
can make. It is shown — both titles and the similarity, in the
description — and the question is the conflict situation's, with skip as
its default. A typo in a DOI that resolves to an unrelated paper is
exactly what this catches.

### D4. Accepting a conflict is a decision, not a guess

The `resolution` requirement "Ambiguity is skipped, never guessed"
exists to stop borax choosing a record it has evidence against. An
override in an interactive run is not borax choosing: the operator has
both titles and the similarity in front of them and chooses the record,
with the target named in the choice. The requirement is amended to say
so, and to keep its batch clause verbatim.

The alternative, refusing overrides and offering only a different
identifier, leaves the most common conflict — a manuscript whose title
changed before publication — with no remedy but supplying the very
identifier that was already found, which the supplied-record conflict
check would then flag again.

### D5. A file's verdict is emitted after its decision

An interactive run holds a file's verdict event until the file's
question is answered, then emits what the decision made of it:

| Outcome | Events |
|---|---|
| renamed on the extracted identifier | `resolved`, `renamed` |
| renamed on a supplied identifier | `resolved` (tier `supplied`), `renamed` |
| renamed over a conflict | `resolved` (with `overrode`), `renamed` |
| skipped after a conflict | `skipped` (reason `conflict`, as today) |
| skipped with no identifier | `skipped` (reason `no-identifier`, as today) |
| skipped after supplying | the original skip reason |
| kept an already-named file | `resolved`, `already-named` |

A skipped file is reported with the reason borax had, not with
`declined`: the operator's skip leaves the file exactly as a batch run
would have, for exactly that reason. `declined` stays the reason for
refusing a move borax proposed on its own. A file that never had a
reason of its own — one that resolved, was offered a move, and was
skipped after a supplied identifier failed — is `declined`, because
what the operator declined is the move that was on offer.

**An abandoned candidate leaves nothing anywhere.** A record the
operator supplied and then walked away from is not the file's
resolution, so it is not reported, not written to the content index
(D7), and not cited: no sidecar beside the file, no entry in the
master bibliography. Only the record a file's decision settled on
reaches any of them. This is a rule the driver has to hold
deliberately, because the batch path cites every file it resolves,
including one whose move was declined.

The prohibition on reporting a file twice is about its identification:
a file has one resolution and one fate, and a candidate that lost is
neither. It says nothing about the events that follow a fate — a
rename that succeeded and a sidecar that then failed are two true
things about one file — nor about the run log's record of a move
written before the move is made.

Records resolved from identifiers the operator then abandoned are not
reported. They were candidates, not outcomes, and the run log records
what happened to each file.

Holding a verdict delays nothing observable in a batch run, which does
not take this path. In an interactive run the description is already
the rendering of the held `resolved` event, so the operator sees the
same facts before the decision that the stream carries after it.

### D6. `tier: "supplied"` and `overrode`

The `resolved` event's `tier` is the extraction pass that found the
identifier. Its documentation already reserves `null` for an
identifier "the caller named", but `null` is also what a content-index
answer reports, so the stream could not tell the two apart. A supplied
identifier reports the explicit value `supplied`.

`overrode` carries the conflict's `field`, `extracted`, `resolved` and
`similarity` exactly as the `skipped` reason would have, so a reader of
the run log sees what was overridden in the vocabulary the skip would
have used.

**Where `supplied` lives.** `FileRecord::tier` is
`Option<borax_pdf::tiered::Tier>`, and every `Tier` variant names an
extraction pass over a file. A supplied identifier came from no pass,
so it is not a `Tier` and no variant is added to that enum: borax-pdf
knows nothing about operators. `FileRecord` carries a borax-level
`Provenance` instead — `Extracted(Tier)` or `Supplied`, with `None`
still meaning the content index answered and nothing was read or
asked. `resolved_event` renders it into the `tier` string, which keeps
the three cases the stream has to tell apart in one field: the pass's
own name, `supplied`, and `null`.

### D7. The answer is remembered in the content index, on rename only

After a rename carried out on a supplied identifier or over a conflict,
the record is written to the content index under the file's hash — the
same write an ordinary resolution makes. Every later run, batch
included, is then answered by the index for that file: no extraction, no
conflict check, no question. That is what "asked once" means, and it is
also what makes a batch re-run over a folder settled interactively
agree with the session.

The write happens only on rename. An operator who supplies an identifier
and then skips has not accepted the record, and a mistyped identifier
that resolved to the wrong paper must not be served for the file
forever after. An ordinary resolution writes on resolution because the
record passed the conflict check; a supplied one has only passed the
operator, and the operator's acceptance is the rename.

Where the index is the only memory, it forgets, and more easily than
clearing it: `FileCache::put` drops a write it cannot make, and a run
that falls back to an in-memory cache keeps nothing past the process.
So "asked once" is what the index makes likely, not what it
guarantees, and the specification says so — an answer that was not
kept means the file is asked about again, which is the safe way to
lose it.

`borax cache --clear` removes the entries, and `--no-cache` bypasses
the read though it still writes. The file's name, sidecar and ledger
entry still say what it is. Durable, curated identifications are
deferred.

Nothing is ever removed from the index by this change. An ordinary
resolution writes its record before the operator is asked anything, so
a file that already had one keeps it when a supplied candidate is
abandoned: the promise is that the rejected candidate is not stored,
not that the file is left unidentified.

## Risks / Trade-offs

- **An override is persistent.** A conflict accepted by mistake is
  served by the content index on every later run. The rename itself is
  recorded in the run log with the conflict it overrode, and re-running
  with `--no-skip-named` offers the file for re-identification.
- **The resolution split touches the most-tested code in the crate.**
  The batch functions keep their signatures and are re-expressed in
  terms of the split parts, so the existing pipeline tests pin the
  refactor before any interactive behaviour is added.
- **Menus grow.** Four choices is the most any situation has, and the
  default is always the safe one.
