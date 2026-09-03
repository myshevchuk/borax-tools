# State of borax-tools

A running assessment of where the project actually is, kept apart from
the specifications because they describe intent and this describes
reality. Read it before planning a change or cutting a release; update it
whenever it stops being true, and at the latest before every version
bump.

Last reviewed: 2026-09-03, before cutting 0.4.0.

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

Tests run green on Linux, macOS and Windows, including integration
tests over real PDFs and Windows coverage of the path shapes discovery
and run-log placement depend on. A scheduled job exercises the real
APIs, so schema drift at a source surfaces without a user finding it
first.

## Not built yet

- **The optional `pdfium` backend.** The pure-Rust `PdfSource` is the
  only extraction backend. The second one was always conditional on
  evidence that the first is insufficient, and that evidence has not
  appeared: the fixture corpus and the live contract tests pass on all
  three platforms without it. Picking a binding crate and a
  prebuilt-binary pipeline for three platforms is the cost being
  avoided; revisit only if real PDFs start failing extraction.

  This is task 6.7 of `add-core-pipeline`, and that change was archived
  with the task still open — 59 of 60 — rather than pretending it was
  done. Nothing in the living specifications depends on it: the
  `extraction` spec constrains behaviour (tiering, page bounds, typed
  failures, offline operation) and never names a backend, so which
  engine reads the PDF is an implementation choice the specs leave
  free.
- **Fuzzy title matching**, for files carrying no usable identifier.
  Named out of scope in the `add-core-pipeline` proposal. Today such a
  file is reported unresolvable and skipped, which is correct but leaves
  a class of documents borax cannot help with.
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
- **Everything the design names as a later change**: no library index,
  search or watch mode; no OCR or interactive candidate picker; no XMP
  write-back, Zotero interop or Emacs package; no bibliography format
  other than BibTeX. The file's bytes are never modified — borax renames
  and never edits.

## Known defects

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

  Fixing it properly means deciding what a sidecar's identity is — an
  output regenerated per run, or a companion that follows its file —
  and the answer governs re-renames and `ledger rebuild`'s scan alike.
  That is a change of its own, not a patch.

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
