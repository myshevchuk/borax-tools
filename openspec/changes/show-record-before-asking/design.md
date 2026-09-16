## Context

What the run knows about a file at the moment of the question, and
where it lives:

| Fact | Where it is | Reaches the stream today |
|---|---|---|
| identifier | `Event::Resolved.identifier` | yes |
| extraction pass | `FileRecord.tier` | yes, as `tier` |
| service | `FileRecord.source` | yes, but `None` → `"cache"` on a content-index hit |
| record | `Event::Resolved.record` | yes, whole |
| per-field provenance | `record.borax.provenance` | yes, inside the record |
| titles the file claims | local to `resolve_file` | no |
| proposed target | `PlannedRename::Rename.target` | only on `planned`/`renamed` |
| rendered target before suffixing | inside the planner | no |

The question is interaction rather than report (`add-interactive-rename`
D6), so the proposed target and the suffix note may be shown without
being events. Everything else in the description is a rendering of the
`resolved` event, which is what the `cli` requirement that human and
JSON output render one stream asks.

## Goals / Non-Goals

Goals:

- Every fact the operator needs to tell the right record from a
  plausible wrong one is on the screen when the question is.
- The description is a pure function, tested byte for byte.
- A `--json` consumer learns everything the description shows about the
  resolution.

Non-goals:

- Changing batch human output.
- Deciding anything differently. The description informs a question
  that already exists; it adds no choice.

## Decisions

### D1. The layout

```text
── 3 of 17 ──────────────────────────────────────────────────────────
file        50-Article Text-95-2-10-20240507.pdf
identifier  doi:10.15407/bioorganica2023.01.010, from an earlier run
record      Crossref
type        journal article
title       Applications of chiral sulfinyl auxiliaries in the asymmetric
            synthesis of fluorinated amines and amino acids
authors     Nataliya V. Lyutenko, Alexander E. Sorochinsky, Vadim A.
            Soloshonok
issued      2023
in          Ukrainica Bioorganica Acta 18(1), 10–21
file says   nothing read
new name    lyutenko2023_ApplicationsChiralSulfinyl.pdf
? Rename this file?
```

The record is the one the content index holds for that corpus file,
whose provenance attributes every field to Crossref while today's event
reports its source as `cache` (D4). On a live answer `identifier` would
read `from the text layer` and `file says` would list the titles read.

- A rule with the position opens each description, so a long session
  can be read back as a sequence of files.
- Labels are a fixed column; values wrap at the terminal width with a
  hanging indent. Nothing is truncated except the author list, which
  shows three names and a count, because a title cut short is the
  field most likely to hide the difference between two works.
- A field the record lacks is omitted, not shown empty.
- `identifier` is the identifier that was looked up, not whichever one
  the record happens to prefer, and says where it was found: `from
  embedded metadata`, `from the text layer`, `supplied` once change 4
  lands, or `from an earlier run` when the content index answered and
  no pass ran. Where the record carries identifiers the lookup did not
  use, they are the record's and are not what this line names.
- `record` names the services that supplied the record. On a live
  answer that is the service the resolver used; on a content-index
  answer it is the distinct sources in the record's provenance other
  than extraction itself, in the fixed order Crossref, OpenAlex, arXiv,
  DataCite, PubMed, sidecar, joined with `, `. Provenance naming only
  extraction, or nothing at all, reads `an earlier run`. `priority` is
  not that order: it is a different list per identifier type and leaves
  out services a record can still carry a field from.
- `publisher` is left out. It is the longest string a record holds,
  wraps to two lines as often as not, and separates two candidate
  records less often than the title, the authors, the year or the
  container do. A record's own JSON is a `--json` run away for anyone
  who wants the rest.
- `file says` lists every claimed title with where it was read (`XMP`,
  `document info`), or `nothing read` when the content index answered
  and the file was not opened. It is shown whatever the conflict check
  concluded, including for claims the check dismissed as placeholders:
  the operator is better placed than the heuristic to discard one.
- `new name` is the proposed target relative to the file's directory.
  When the planner suffixed it, a second line reads `(<rendered> is
  taken)`.

Colour is not part of the contract. Labels may be dimmed where the
terminal supports it; the text is complete without it.

The description is written to standard error, with the question and by
the same asker. It is part of what is being asked rather than part of
what the run reports, and the two streams already divide that way:
stdout carries the event stream, stderr carries the conversation and
the diagnostics. A run whose stdout is redirected — `borax rename lib/
> report.txt`, which stays interactive because the terminal it asks at
is stdin — therefore keeps its questions legible, which putting the
description on stdout would not.

The `resolved` line stays on stdout and is not replaced. The terminal
shows it and then the description; one is the report and the other is
the question, and the run log and a `--json` reader are unaffected by
either.

### D2. The description is a function of an event

```rust
pub fn describe(
    resolved: &Event,          // Event::Resolved
    proposal: &Proposal,       // target, and the rendered name when suffixed
    position: (usize, usize),
    width: usize,
) -> Vec<String>
```

`Proposal` is what `Planning::propose` returns, extended with the
unsuffixed rendering so D1's note has something to say. Taking the event
rather than a `FileRecord` is deliberate: it makes the requirement that
the description render the stream a property of the signature.

### D2a. The identifier the description names

`Event::Resolved.identifier` is `identifier_of(&record)`, which prefers
the record's DOI. For a file whose arXiv identifier was found in the
text layer and whose record carries both, that is the DOI — and the
description would attach "from the text layer" to an identifier no
pass ever saw.

The event gains `found`: the identifier the run looked up, as a string
in the same form the stream already uses (`doi:…`, `arXiv:…`), beside
the `tier` that says where it came from. `identifier` keeps its meaning
— what the record is filed under — so a consumer reading it is
unaffected.

The description names `found`. Where the record's own identifier
differs, that difference is the record's business and not evidence
about the file.

### D3. `claims` on the `resolved` event

```json
"claims": [
  {"from": "xmp", "title": "Applications of chiral sulfinyl compounds"},
  {"from": "info", "title": "Microsoft Word - manuscript.docx"}
]
```

In the order they were read, empty when the file was not opened.
`extract_from` already reads both; it keeps the origin beside each
title rather than flattening them. The conflict check is unchanged and
still receives the titles alone.

### D4. `source` on a content-index answer

Today `FileRecord.source` is `None` when the content index answers and
the event renders that as `"cache"`. The event instead carries the
services named by provenance, joined as D1 orders them, and `cached:
true` continues to say the content index answered. A record with no
service in its provenance keeps `"cache"`.

This touches batch output too, since the batch resolution line renders
`source`: `resolved doi:… via crossref (cached)` rather than `via cache
(cached)`. That is a correction of a line that named the lookup rather
than the source, not a change in what batch reports.

## Risks / Trade-offs

- **Twelve lines per question.** A session over a hundred files is long
  to scroll. The description is what makes each answer informed, and
  `skip-already-named-files` removes the files with nothing to decide.
- **Width detection.** A terminal that reports no width gets 80 columns.
  The description is for a person at a terminal, and an interactive run
  requires one.

### D7. The schema version is per release, not per change

`skip-already-named-files` takes the event schema to 2 for removing a
skip reason. This change adds `claims` and `found`, and changes what
`source` says when the content index answered — a change of meaning,
which by that change's own rule needs a version.

Both land in the same unreleased version, so they share schema 2: the
number tells a consumer that the stream it reads differs from the one
0.4.0 emitted, and one bump says that whether one change or three made
it so. What matters is that the boundary is a release a consumer can
pin, not a change it has no way to see.

The obligation that leaves is a check at release time: if 2 ships
before this change lands, this change takes 3.
