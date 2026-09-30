## Why

Resolution does not ask the library. `pipeline::standing` hashes a file,
asks the content index, and serves a hit as the file's record
(`from_index`, `indexed_record`). On a miss it extracts an identifier and
asks the services. Neither path reads the item store or the artifact
records. `resolve_events` passes no library to `resolve_batch`,
`resolved_record` (used by `bib`) passes none to `resolve_file`, and
`rename_events` gives `standing` an `Account` used only for the two
duplicate checks, and gives it that only when the `record` setting is on.

Once a file has been admitted to a library, its linked item is the
library's statement of what the file is. The content index is a cache:
the living `library` requirement "The library's text is authoritative and
is read directly" says so. But the index answers first, and it keeps the
record as it was when the index entry was written.

The second interactive review observed the consequence (reviews
`interactive-review.org`, "Engine finding: library correction ignored
during resolution"). The Klykov PDF was adopted into a library. Its
item's title was then prefixed with `REVIEW CORRECTION:`, keeping its
identity and DOI. `borax resolve --json` on the tracked PDF returned the
older title from the content index with `cached: true`, empty `claims`
and a null `tier`. The correction was still in the item during the run:
it was ignored, not overwritten.

The review agreed the direction ("Agreed direction: consult the library
first"): once an artifact is admitted, its linked item is the source of
truth. Inconsistent or unavailable library records are reported
explicitly, and fresh fallback information stays distinguishable from
unavailable library authority. The maintainer's roadmap schedules this
as change 11 in Phase 1 and rules that it needs a proposal. It is not a
restoration: no living requirement governs what resolution reads first.

## What Changes

- **A tracked file resolves from its library item.** The run's library
  is asked before the content index. A file is *tracked* when exactly one
  artifact record names the file's own path and holds the file's content
  hash anywhere in its history, and that record links to an item
  exactly one item file carries. The item's record is then the file's
  record. Nothing is
  extracted, no service or response cache is asked, the content index is
  neither read nor written, and no title comparison is made (design D1,
  D6, D9).
- **Paths are compared normalised.** Root and input are made absolute
  and resolved lexically before containment, exclusion and record
  matching, as the duplicate checks already do. `borax resolve
  paper.pdf` and `./sub/../paper.pdf` are consulted like the absolute
  spelling. The two helpers it rests on, `library_relative` and
  `excludes`, and discovery from a relative start directory were
  restored to the library's containment rule by separate fixes on this
  branch (design D1a).
- **Counting every history entry is an interim policy.** A rollback
  resolves from the item. A hash entry records no item, though, so bytes
  restored after a re-link answer with the *new* item. The proposal pins
  that behaviour with a test and hands the fix to change 15 (design D1,
  Risks).
- **Which library.** The run's library is the one discovered today: the
  nearest `.borax.toml` above the run's start directory, or
  `library-root`. Its stores are read once per run, and consultation
  reads that immutable snapshot, never the copy a rename run learns its
  admissions into. A file outside the library, or in a subtree it
  excludes, resolves as today. Outside any library nothing changes
  (design D8).
- **Which commands.** `resolve`, `rename` (batch and interactive,
  preview and apply, under any `record` setting) and `bib`. `--no-cache`
  does not bypass the library, because the library is not a cache.
  `--no-record` does not either: it turns off the account and its
  writes, and consulting the library is neither (design D7).
- **A file the library cannot answer for is reported, then resolved as
  if untracked.** One of these holds:
  - the file's bytes are in no history of a record at its path;
  - several records at its path hold them;
  - the record links no item;
  - the linked item is absent;
  - the linked item could not be read (its file, or an unlistable
    `items/`);
  - several item files carry the linked identity;
  - the file cannot be hashed;
  - no readable record names the path, but the artifact store has
    unreadable or unlistable files.

  The run reports which on the file's resolution event, then continues
  through the content index, extraction and the services as for any
  untracked file. A file with no record at its path in a store read
  whole, including a copy of an artifact's bytes elsewhere, is
  `untracked` and resolves as today. The run warns once when the
  artifact store has faults (design D1, D2).
- **A store that cannot be listed is not an empty store.**
  `store_files` treats only a missing directory as empty. Any other
  listing or entry failure becomes a store fault. As a consequence,
  `validate` reports an unlistable store as an `unreadable` finding
  instead of certifying an empty library. `status`, `reconcile` and
  `adopt` are unchanged (design D2a).
- **A rename keeps what the library said.** Batch and interactive
  settlement, retries after an outage, and the description of a failed
  file all carry the consultation (design D11).
- **The event says what the library said.** `resolved`, and `skipped`
  when it reports a file's resolution, gain a `library` field. It is
  `null` when the library was not consulted, and otherwise an object
  tagged by `kind`: `tracked`, `untracked`, or one of the eight problem
  kinds. A record the library supplied reports `tier: "library"`,
  `cached: false`, no claims, and the services its provenance names as
  `source`, or `library` when the provenance names none. The human line
  gains ` (from the library)` or `; the library could not answer: …`,
  and the interactive description gains the same information
  (design D3, D5).
- **The event schema stays at version 3.** Every change is an addition:
  a new field and new values in `tier` and `source`. No existing field
  changes meaning. In particular `tier: null` still means exactly "the
  content index answered", and `cached` still means exactly that. The
  argument against the `cli` requirement "JSON Lines output is
  first-class", and the one residual risk, are in design D4.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED "A file its library tracks resolves from its library item":
    the lookup order, what makes a file tracked, which library, which
    commands and flags, and that a library answer opens nothing, checks
    nothing and leaves the content index alone.
  - ADDED "A library that cannot answer for a file says so": the eight
    problem kinds, the fallback, unreadable and unlistable stores, and
    that none of this is a skip, a finding or an exit-status change.
  - ADDED "A resolution says whether its library answered": the
    `library` field, `tier: "library"`, and the human rendering.
  - MODIFIED "Responses are cached locally": `--no-cache` forces live
    queries for every file the library does not answer for. The
    "Renamed file, same content" scenario now covers a file the library
    tracks under its new name.
  - MODIFIED "A resolution reports the evidence it was checked against":
    a library answer carries no claims. Its source is read from the
    item's provenance, as for a content-index answer, and reports
    `library` where the provenance names no service.
  - MODIFIED "Ambiguity is skipped, never guessed": an accepted conflict
    recorded in a library is answered by the item it was linked to on
    later runs, and that item is not judged either.
  - MODIFIED "An operator's answer about a file is remembered": inside a
    library the answer is also kept by the item the rename linked the
    file to, and a later run resolves from that item.
- `rename`: ADDED "An interactive question says whether the library
  answered": the description names a library answer as such, and states
  a library problem.
- `bib-output`: MODIFIED "A sidecar is derived citation output and
  governs no decision": borax's knowledge of a tracked file also comes
  from its library item.

Requirements checked and left unchanged:

- `cli` "JSON Lines output is first-class": design D4 argues that this
  change is an addition under its rule.
- `cli` "Exit codes distinguish partial success": a library problem is
  neither a skip nor a finding. How callers detect unresolved library
  conditions belongs to change 5 in the roadmap.
- `library` "The library's text is authoritative and is read directly":
  this change applies it to resolution. The requirement already says
  every answer about a library is read from its files, and that clearing
  the content index changes no answer about a library.
- `library` "An applying run records what it admits" and "Reconciliation
  repairs a stale path by a bounded walk": both still append a file's
  hash to the record at its path. Design's Risks section covers how that
  interacts with this change. Deciding which bytes are accepted belongs
  to change 15.
- `ledger`, all three requirements: the content-duplicate check still
  runs before any resolution, the library consultation included. The
  work check applies to a library answer as to any record. `--no-record`
  still suppresses both checks and every store write. The scenario
  "Re-running over an admitted file" holds: the file is reported already
  named without a service being queried.
- `rename` "An interactive question shows what the answer rests on": its
  rule of naming an identifier's origin "only where the run found it"
  already covers a library answer. The ADDED `rename` requirement adds
  the library-specific lines.
- `library` "A library is one directory tree": discovery is unchanged.
  Design D1a records an observed limitation of discovery from a
  relative start directory, which is left alone.
- `library` "Validation defines a well-formed library": its findings are
  listed "at minimum", so an unlistable store reported as an
  `unreadable` finding needs no delta (design D2a).

## Impact

- `crates/borax/src/library.rs`:
  - `Stores::consult(path, hash)` returns what the library says about
    one file, over normalised paths.
  - `Stores::record_faults` reports what the artifact store could not
    read.
  - `store_files` records a fault for every listing or entry failure
    other than a missing directory (design D2a, D10).
  - `Finding::Unreadable`'s docstring covers a store directory.
- `crates/borax/src/event.rs`: `LibraryAnswer`, a `library` field on
  `Resolved` and `Skipped`, and the human line suffixes. `SCHEMA` stays
  3.
- `crates/borax/src/pipeline.rs`: `standing` takes the library and asks
  it after the content-duplicate check and before the content index.
  `Provenance::Library`, `FileRecord::library`, `Standing::library`,
  `verdict_event`. `resolve_batch` takes the library. `sources_of`
  reports `library` for an item whose provenance names no service.
- `crates/borax/src/run.rs`:
  - `resolve_events` and `bib_events` read the run's library once and
    pass it on.
  - `rename_events` reads it under every `record` setting and consults
    an immutable snapshot. The account, and the learning behind it,
    stays behind the setting as today.
  - `alone`, `asked`, `skipped`, retry and `Settled::Skip` carry the
    answer (design D11).
  - The artifact-store warning.
- `crates/borax/src/describe.rs`: `whence("library")` claims no origin.
  The record line reads `from the library`, and a `library` line states
  a problem.
- Tests: existing literals of `Event::Resolved`, `Event::Skipped` and
  `FileRecord` gain the new field. Existing calls to `standing` and
  `resolve_batch` gain the library argument. Existing library-context
  tests that expect a content-index answer on a file an earlier run
  recorded are audited (tasks 5.4).
- Documents a person reads, written by the doc writer (tasks 7):
  `docs/manual.org`, `CHANGELOG.md`. `openspec/STATE.md` records the
  built state.
- No new dependency. No configuration key or flag changes.

## Deferred

Everything below belongs to a later roadmap change. This proposal does
not design any of it.

- **Library identity and index references** (changes 12, 13). The
  library is found by its marker, as today. The content index is left
  exactly as it is, including for tracked files (design D6).
- **Refresh as candidate** (change 14). After this change, `--no-cache`
  gets a tracked file no fresh lookup. How a person asks the services
  about a tracked file and accepts the answer is change 14's design.
  Supplying an identifier in an interactive run remains the one way to
  re-identify a tracked file.
- **History entries bound to items** (change 15, Phase 4). Until
  then, a rollback after a re-link answers with the new item (design
  D1, Risks).
- **Acceptance of changed bytes** (change 15). Reconciliation's
  "Edited in place" step and an applying run's record update both write
  unrelated bytes into an artifact's history today. After this change,
  the item answers for bytes once they are written. Change 15's scope
  needs to cover both writers. Design's Risks section explains.
- **The sectioned schema** (Phase 3, change 9). `tier: "library"` and
  the `library` field are what Phase 3 migrates into `record_retrieval`
  and whatever library section it settles. No Phase 3 name is fixed
  here.
- **Exit status for library conditions** (change 5).
- **Several libraries in one run.** A file in a library other than the
  run's own is not consulted (design D8).
