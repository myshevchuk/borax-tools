## Why

`borax status` and `borax validate` count three library conditions and
name none of the objects behind them. `library::survey` keeps the
orphans' paths and `status_event` reports only `survey.orphans.len()`.
`library::validate` computes its `missing` and `unlinked` totals with
`.len()` and `.count()` and keeps no list at all, so the `Validation`
it returns cannot say which record or which item it counted
(`crates/borax/src/library.rs`). A person reading
`7 artifacts, 0 items, 2 records, 5 orphans` or
`0 findings, 5 orphans, 1 missing, 1 unlinked` has no way to get from
the number to the files, short of reading `.borax/artifacts/` by hand.

`borax adopt` has the same gap one level down. For an orphan the
content index holds nothing about, `library::adopt` reaches
`let Some(record) = lookup(&hash) else { continue; };` and writes no
adoption for it. The `Event::LibraryAdoption` doc comment says so:
such an orphan is "left an orphan silently, and the count of orphans
in `Event::LibraryAdopted` is where it shows". The remaining-orphan
count is the only sign that a file was missed, and it does not say
which file.

The maintainer's roadmap schedules this as change 5 of Phase 2, the
second user of the select/inspect/render boundary that
`report-extraction-per-file` recorded (its design D1). Every count in
scope must be traceable to the object it counts. The maintainer
accepted the adoption-miss report into this change on 2026-10-01. No
living requirement asks for per-object reports of these conditions,
so this needs a proposal rather than a restoration.

## What Changes

- **Exit status does not change (design D1). Maintainer, please
  confirm.** A missing artifact record and a reconcile's ambiguous or
  missing result still exit 0, as do orphans and unlinked items. The
  interactive review noted that a script cannot detect these from the
  exit status alone, and accepted that no exit code changes. This
  design weighed a distinct exit code and rejected it, because the
  living `library` requirements define all of these as states of a
  well-formed library, not failures. A script detects them from the
  structured stream instead: from the new per-object events, or from
  the counts the totals events already carry. The `cli` requirement
  "Exit codes distinguish partial success" is not modified.
- **One new event names each condition.** `library-condition` carries
  a library-relative `path` and a `condition` tagged by `kind`:
  `orphan`, `missing` (with the artifact record's `id` and its
  `record` file under `.borax/artifacts/`) or `unlinked` (with the
  item's `id`). The record file keeps two records that share an
  identity, which validation tolerates, from producing identical
  events. Each kind is named after the count it adds to (design D3).
- **`status` names every orphan it counts.** One `library-condition`
  event per orphan, in survey order, is written before any document
  is opened, so it comes ahead of `--identify`'s `library-extraction`
  events and of `library-status`. `status` reports no missing or
  unlinked count, so it names none either (design D4, D5). Plain
  `status` still opens no document.
- **`validate` names every orphan, missing record and unlinked item
  it counts.** The events come after the findings and before
  `library-validated`, whose three counts are now taken over those
  events (design D5).
- **`adopt` reports each orphan it leaves untracked because the
  content index holds nothing for it.** This is a new `Adoption` kind,
  `unindexed`. Every orphan the run reaches now gets exactly one
  `library-adoption` event, so the remaining-orphan count equals the
  number of those events that are not `recorded` (design D6).
- **Human output writes one line per condition and per unindexed
  orphan.** The lines use the house `<path>: <clause>` form, and each
  condition line begins with the word its count uses. Paths are
  escaped through `describe::escaped`. The existing count lines keep
  their wording (design D7).
- **Conditions stay distinct from findings.** No condition is a
  `library-finding`, a skip, or a counted finding. `validate`'s
  complete check table is unchanged, and so is the rule that a
  finding, and only a finding, makes `validate` exit partial (design
  D8).
- **The event schema stays at version 3.** The new event tag, the new
  `Condition` vocabulary and the new `unindexed` adoption kind are
  additions (design D9).

Explicitly out of scope:

- **Default inspections, verbosity and cost flags.** `status` lists
  every orphan in human output, as `status --identify` lists every
  artifact. Neither has a quieter mode, and neither gets a flag that
  limits listing. Both belong to the roadmap's deferred Phase 2
  policy. An opt-in mode that fails on conditions is also a policy
  flag, and it is deferred with them (design D1).
- **Folding `validate` into `status`, regrouping help, and renaming
  "orphans" or "identifiable" in human prose.** These are later
  changes. The report lines of `status`, `validate` and `adopt` keep
  their wording. Only the new per-object lines have wording of their
  own.
- **Content verification of tracked files.** Hashing every artifact,
  and comparing extracted identifiers with items, are later work.
  `orphan` keeps `library::orphans`' test: no record names the path.
- **`reconcile`.** Its per-record `library-repair` events already name
  every ambiguous and missing record, and the exit-status decision
  leaves it alone.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library`: ADDED "Library conditions are named where they are
  counted". `status` and `validate` name each orphan, missing artifact
  record and unlinked item they count as a `library-condition` event.
  The requirement defines the three kinds and how each object is
  identified. Each count equals the number of events of its kind, and
  every event precedes the totals. The requirement also fixes the
  human line and its escaping. A condition is never a finding or a
  skip, it does not affect the exit status, and a caller detects it
  from the stream. Its scenarios include two records of one identity
  that are both missing, each named by its own record file.
- `library`: MODIFIED "borax reports a library it has never seen".
  `status` names each orphan before it opens any document. The orphan
  count equals the number of orphan events. All existing paragraphs
  and all ten scenarios are kept. "Two hundred files borax has never
  seen" now also names the orphans. New scenarios cover naming before
  opening, and a recorded artifact that is not named.
- `library`: MODIFIED "Validation defines a well-formed library". The
  minimum findings list is kept word for word. The paragraph on
  orphans, missing artifacts and unlinked items now says they are
  named per object as well as counted, and are never findings. All
  four scenarios are kept. New scenarios cover naming each condition,
  a missing artifact on its own exiting 0, and findings left unchanged
  when conditions are added beside them.
- `library`: MODIFIED "borax adopt records what the library already
  holds". An orphan the content index cannot answer for is reported as
  `unindexed`, and every orphan the run reaches is reported exactly
  once. An orphan is `unindexed` only when it is readable and no record
  holds its bytes, because only then is the index asked. All five
  scenarios are kept. "What adoption leaves alone" now states the
  report. "Adoption after the cache is cleared" keeps its wording and
  adds a clause limited to readable orphans whose bytes no record
  holds. A new scenario covers accounting for every orphan.

Requirements checked and left unchanged:

- `cli` "Exit codes distinguish partial success". No exit code changes
  (design D1). `session::outcome_for` still sums skipped, unreached and
  findings.
- `cli` "JSON Lines output is first-class". Each addition falls under
  its rule (design D9).
- `cli` "A run reports as it goes". `status` writes its orphan events
  as soon as the survey is read, before the first document is opened.
  `validate` does no per-object work between reading the library and
  writing its events. `adopt` still hashes every orphan before it
  writes any of its events, and this change leaves that alone (see
  "Not changed" in the design).
- `cli` "A run's human summary fits its command". `status`, `validate`
  and `adopt` still end on their own report line, and
  `library-condition` adds no skip, finding or unreached total. The
  fallback summary line therefore never fires.
- `library` "An artifact is a document in the tree, and one without a
  record is a worklist item". "An orphan SHALL be reported and
  counted, and SHALL NOT be a validation finding" stays true. The new
  requirement says how it is reported.
- `library` "An item is a logical work with a minted identity".
  "nothing reports it as incomplete" stays true. An `unlinked` line
  states that no artifact record links the item. It is not a finding,
  and its wording does not call the item incomplete (design D7).
- `library` "Reconciliation repairs a stale path by a bounded walk".
  Unchanged. Its `missing` is a different test from `validate`'s
  (design D4).

## Impact

- `crates/borax/src/event.rs`:
  - adds `Condition` (`Orphan`, `Missing { id, record }`,
    `Unlinked { id }`), `Event::LibraryCondition { path, condition }`
    and `Adoption::Unindexed`;
  - adds a human line for each, escaped through `describe::escaped`;
  - the `LibraryAdoption` arm now escapes its path for every kind;
  - the docs of `LibraryAdoption` and `LibraryValidated` are updated;
  - `SCHEMA`, `Counts::observe` and `Summary` are unchanged.
- `crates/borax/src/library.rs`:
  - adds `orphan_events`;
  - `Validation` replaces its three count fields with
    `conditions: Vec<(String, Condition)>`, plus the methods
    `orphans()`, `missing()` and `unlinked()`;
  - `validate` builds the list from the same filters it counts with
    today;
  - `validation_events` writes findings, then conditions, then totals;
  - `adopt` records `Adoption::Unindexed` where it used to `continue`;
  - `survey`, `Survey`, `status_event`, `missing`, `orphans` and
    `reconcile` keep their signatures.
- `crates/borax/src/run.rs`: `status_events` writes `orphan_events`
  before the extraction loop. `validation_events` and `adopt_events`
  are unchanged.
- `crates/borax/src/session.rs`: unchanged.
- Tests: existing tests that assert the whole event sequence, an event
  count, or the human lines of `status`, `validate` or `adopt` over a
  library holding a condition are updated to the new contract. So are
  five `library.rs` tests that read `Validation`'s count fields.
  Tasks 3.1, 4.1 and 5.1 name them. The existing tests that pin each
  validation finding are not edited.
- Documents a person reads, written by the doc writer (task 6):
  - `docs/manual.org`: the `borax status`, `borax validate` and
    `borax adopt` sections, and the JSONL schema paragraph, including
    how a script detects conditions without an exit code;
  - `CHANGELOG.md`.
- `openspec/STATE.md` records the built state and the exit-status
  decision.
- No new dependency, configuration key or flag.

## Deferred

- **A way to fail on conditions.** If a script needs the exit status to
  reflect missing or ambiguous records, the route is an opt-in policy
  flag. It belongs with the deferred Phase 2 policy, not a new default
  exit code (design D1).
- **Listing limits.** Limiting or summarising the per-object lines on
  a large library is a verbosity question for the same deferred
  policy.
- **`status` reporting missing and unlinked.** That belongs with
  folding `validate` into `status`.
- **Writing `adopt` and `reconcile` events as they occur.** Both hash
  files one at a time and write no per-object event until the last
  file is done. Task 6.3 records this in `openspec/STATE.md` as a
  deviation from "A run reports as it goes", to be restored
  separately.
