## Why

A library records each artifact's bytes in a hash history, and a file
whose bytes are in that history resolves from the artifact's item
(change 11, `consult-library-first`). Two writers put bytes into that
history, and neither asks whether the bytes still belong to the work
the record links:

- **Reconciliation's step 3, "Edited in place".** `library::reconcile`
  (`crates/borax/src/library.rs`, the loop after the comment "Step 3",
  near line 2002) takes any unclaimed file at a record's own path whose
  hash no record holds as "that artifact edited since borax last saw
  it", and appends the hash (near line 2050). The living requirement
  "Reconciliation repairs a stale path by a bounded walk" says exactly
  that.
- **An applying rename's admission.** `library::admit` (near line 2414)
  appends the file's hash whenever it is not the history's newest, and
  `Stores::take_in` (near line 1074) mirrors it for the run's own
  account. The living requirement "An applying run records what it
  admits" says so too.

The round-two interactive review (the maintainer's notes, outside the
repository) recorded the result. Replacing the Klykov PDF with an
unrelated HPLC buffer guide made `borax reconcile` append the guide's
hash to the Klykov artifact. The link was unchanged, so from then on
the Klykov item answered for the guide's bytes. A trailing comment
added to the PDF got the same treatment. The engine cannot tell an edit
from a replacement.

The history has a second gap. A `HashEntry`
(`crates/borax-core/src/library.rs`, near line 359) records the hash,
the run, the timestamp and the borax version, and not the item the
bytes were accepted under. So bytes restored after an operator re-linked
the artifact answer with the new item. The living requirement "A file
its library tracks resolves from its library item" calls that an
interim rule, and `openspec/STATE.md` hands both items to this change
at the end of its `consult-library-first` paragraph.

This is change 15 of the roadmap's Phase 4, and the first of the four.
The Phase 4 library-authority design record (maintainer's notes,
outside the repository) settles three things this change must
preserve:

1. Reconciliation and an applying rename always observe and report
   bytes at a recorded path that match no record's history.
2. They accept those bytes as the same artifact automatically only when
   the new bytes extract an identifier that the linked item carries.
   Otherwise the change stays pending, and the hash is not appended,
   until the operator accepts it.
3. Each history entry records the item it was accepted under.

It also sets a default, which this change keeps: a pending change
writes nothing to the artifact record and is reported on every run
until accepted, the way a missing record is. Only files whose bytes
changed at a recorded path pay the cost of an extraction.

## What Changes

- **Acceptance is gated.** Bytes at a record's last-known path that no
  record's history holds become that record's history only once
  accepted. Reconciliation accepts them automatically when the
  identifier extraction finds in them is one the record's linked item
  carries, and otherwise leaves the change pending. Design D1, D2.
- **Identifier comparison.** DOI, arXiv, PMID and ISBN, each compared
  only with the item field of its own kind, in the normalised form the
  parser already produces. Extraction finds only DOIs and arXiv
  identifiers. A supplied identifier may be any of the four. An arXiv
  identifier is compared without its version. An ISBN-10 is not
  converted to an ISBN-13. The linked item
  must be carried by exactly one item file. Design D2.
- **A pending change writes nothing.** Not the hash, and not the
  refreshed size and modification time, so the next reconcile meets the
  same change and reports it again. Reconciliation reports it as a
  `library-repair` event of the new kind `pending`, carrying the hash,
  the linked item and what extraction made of the file.
  `library-reconciled` gains a `pending` count. Neither `status` nor
  `validate` reports pending changes: neither hashes a file. Design D3.
- **The operator accepts with `borax reconcile --accept <path>`.** The
  flag is repeatable and takes a path as every path argument is taken.
  A pending change at a named path is accepted under the record's
  linked item. A named path is hashed whatever the fast path says. A
  named path at which nothing was pending is reported as a `skipped`
  event with the new reason `not-pending`, unless its record already
  holds its bytes or the identifier accepted them in the same run.
  Design D4.
- **`reconcile` reads documents.** It takes the extraction settings
  (`--page-limit`) as `status` does, because what it accepts now
  depends on them. Design D4.
- **The applying rename applies the same gate.** A file at its record's
  path whose bytes the history does not hold is accepted when the
  identifier extracted from it, or an identifier the operator supplied
  for it, is one the linked item carries. Otherwise:
  - a file the run **moves** is recorded as a new artifact, with an
    item selected as for any mint, and the old record is left exactly
    as it was. Its path now holds nothing, so it is the `missing`
    record it has become (`separated`);
  - a file the run leaves where it is writes nothing, and the run
    reports the change as pending (`pending`).

  `--no-record` still refuses the move, and its reason now names
  `--accept` as well as `--rehash`. Design D5.
- **History entries name their item.** A new history entry carries
  `item`, the item its bytes were accepted under. An entry written
  before this change carries none and stays valid, readable and
  unrewritten. Design D6, D7.
- **Resolution answers with the item the bytes were accepted under.** A
  file whose bytes are its record's newest entry answers with the
  record's link, as today, so a link corrected by hand still decides.
  A file whose bytes are an earlier entry answers with the item that
  entry names, and with the link when it names none. Every decision
  about which item bytes belong to is tied to those bytes:
  - an admission that decides bytes H belong to item I appends `(H, I)`,
    unless H is already the newest and answers I afterwards;
  - before appending an entry for other bytes, an admission restates
    the newest bytes under the link as read, when a person corrected
    that link by hand.

  History grows by at most one entry per operator re-identification,
  and by nothing on an ordinary re-run.

  An ordinary admission of bytes the history already holds no longer
  appends a duplicate entry. The link is still checked first, so a
  record with no link, or a damaged one, answers as it did. Design D6.
- **Duplicate checks follow.** The work check no longer counts a file
  as a duplicate of the item its own record answers with for its bytes.
  It already passed over the record's link. A preview treats a path it
  moves a file away from as holding nothing, so preview and apply agree
  after a separation. Design D5, D6.
- **Events.** These are added, schema 4 kept (design D9):
  - `library-repair`:
    - `changed` gains `accepted`, tagged by `by`: `extracted` with the
      `identifier`, or `operator`;
    - the new kind `pending`;
  - `library-reconciled` gains `pending`;
  - `library-admission` gains:
    - `changed` (by `extracted` or `supplied`);
    - `pending`;
    - `separated`;
  - `skipped` gains the reason `not-pending`;
  - the `library` answer `tracked` gains `accepted_under`, present only
    when the item answered with is not the record's link. Its `item`
    stays the link. `record_retrieval` `library` keeps its shape, and
    its `item` keeps attributing the record to the item it came from;
  - the `library` answer gains the kind `accepted-item-unavailable`.
- **Human lines.** These say what was accepted and on what evidence,
  what is pending and how to accept it, and the totals count pending
  (design D10).

The schema version stays 4. Every key, value and kind above is new, and
no existing one changes its purpose. `tracked`'s `item` and the
released problem kinds keep describing the link. `record_retrieval`'s
`item` keeps naming the item whose record was returned (design D9). Fewer `changed` repairs and
fewer silent admissions are emitted for the same input, and each one
still means what it meant: bytes appended to a record's history. The
`cli` requirement "JSON Lines output is first-class" asks no bump for
that.

Explicitly out of scope:

- library identity (change 12);
- the reduced content index, and identification evidence (identifier
  and origin) per history entry (change 13). This change records no
  origin on disk: the basis of an acceptance is reported in the event,
  and the entry's `run` names the run that reported it (design D8);
- refresh (change 14);
- views and sidecars;
- an interactive acceptance prompt, and any warning in the rename
  question that renaming a file whose change is pending records it as a
  new artifact (Deferred).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library`:
  - ADDED:
    - "Changed bytes become an artifact's history only once accepted":
      the gate, the comparison, and where a pending change is reported
    - "An operator accepts a pending change by naming its path":
      `reconcile --accept`
  - MODIFIED:
    - "An artifact record is authoritative state keyed by an artifact
      identity": each entry names its item, pre-change entries name
      none, and every link change is tied to its bytes
    - "Reconciliation repairs a stale path by a bounded walk": step 3
      becomes "Changed in place", with the pending branch, `--accept`,
      and no refresh of a pending record
    - "Validation defines a well-formed library": an entry's item is
      never a finding
    - "An applying run records what it admits": the gate, separation,
      pending, entries' items, re-identification appending an entry,
      held bytes not appended again, the `--no-record` remedy, and a
      corrected explanation of why re-running without `--no-record` is
      not a remedy
- `resolution`, MODIFIED:
  - "A file its library tracks resolves from its library item": the
    item a record answers with for given bytes replaces the interim
    rule, and the link is checked first
  - "A library that cannot answer for a file says so":
    `accepted-item-unavailable` is added, and `unrecognised-content`
    includes a pending change. The released kinds keep their meaning
  - "A resolution says whether its library answered": `accepted_under`
    and the new kind
  - "A resolution reports where its record was retrieved": its `item`
    is the item whose record was returned
- `ledger`, MODIFIED: "Duplicate detection operates at two levels with
  distinct reasons": the work check excludes the item the own record
  answers with, the other side stays by link with the reason stated,
  and a preview treats a vacated path as holding nothing
- `cli`, MODIFIED: "A subcommand accepts only the settings it
  consumes": `reconcile` accepts the extraction settings and
  `--accept`, and `--accept` is a selector

Requirements checked and left unchanged:

- `library`:
  - "Library conditions are named where they are counted". It already
    says reconcile traces its counts through its own per-object events,
    and `pending` is one of them.
  - "borax adopt records what the library already holds". Adoption
    mints, and a minted entry names its item under the modified
    artifact-record requirement.
  - "borax reports a library it has never seen". `status` hashes
    nothing, so it has no pending changes to report (design D3).
  - "The library's text is authoritative and is read directly". A text
    editor stays a peer: a hand-corrected link decides for the current
    bytes (design D6).
- `ledger`, the scenario "An admitted file annotated afterwards". It
  still holds, because its identifier is carried. The content check's
  match over the whole history is unchanged.
- `cli`:
  - "Exit codes distinguish partial success". A `not-pending` skip is a
    skip.
  - "A run's human summary fits its command". Its rule for a total a
    later change lets a command produce covers reconcile's skips.

## Impact

- `crates/borax-core/src/library.rs`:
  - `HashEntry::item`, optional, written only when present;
  - `ArtifactRecord::item_for`, and `ArtifactRecord::append(entry,
    link_as_read)`, which restates a hand-corrected link. `admit` and
    `Stores::take_in` capture the link before assigning a new one.
- `crates/borax/src/library.rs`:
  - `carries`, `extracted_identifier` and `accepted_by`;
  - `Reconciling`, and `reconcile` taking it, with the gate in step 3,
    `Decided::Pending`, and the `not_pending` paths;
  - `history_table` writes `item`;
  - `Stores::consult` checks the link, then answers through
    `item_for`, with `accepted_under` and `accepted-item-unavailable`;
  - `Account::work_duplicate` takes the incoming hash;
  - `admit` and `Stores::take_in` append on the new terms;
  - `reconciliation_events` counts `pending`.
- `crates/borax/src/event.rs`:
  - `AcceptedBy`;
  - `Repair::Changed::accepted` and `Repair::Pending`;
  - `LibraryReconciled::pending`;
  - `Admission::Changed`, `Pending` and `Separated`;
  - `SkipReason::NotPending`;
  - `accepted_under` on `LibraryAnswer::Tracked`, and
    `LibraryAnswer::AcceptedItemUnavailable`;
  - the human lines, and the `Stranding` reason's text.
- `crates/borax/src/evidence.rs`: `Evidence::retrieval` builds
  `RecordRetrieval::Library` with the item whose record was returned
  (`accepted_under` when present). The type's projection onto
  `RetrievedFrom::Library` is unchanged in shape. The `Tracked` pattern
  there takes the new field.
- `crates/borax/src/run.rs`:
  - `reconcile_events` passes the documents, the extraction settings
    and the accepted paths, and emits a `skipped` event per
    `not_pending` path;
  - the rename driver settles the gate before the move, separates or
    reports pending, and emits the new admission events;
  - a pending admission is released through the hold as an unwritten
    one is;
  - a preview's `exists` treats a path it moved a file away from as
    holding nothing.
- `crates/borax/src/pipeline.rs`: `second_copy` and the work check pass
  the file's hash.
- `crates/borax/src/cli.rs`: `Command::Reconcile` gains `extraction` and
  `accept`, with `Command::reconcile` unchanged and
  `Command::reconcile_accepting` added.
- Tests: design D13 lists every existing test that changes, and how.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`: "What a run does", `borax rename`,
    `borax reconcile`, the library-model passage on artifact records,
    and the Run logs library events;
  - `CHANGELOG.md`: an Unreleased entry.
- `openspec/STATE.md` (implementer):
  - a paragraph for this change;
  - the "Still open, and handed to the roadmap's change 15" sentences
    in the `consult-library-first` paragraph closed.
- No new dependency, configuration key or event type. `reconcile` gains
  a flag and a setting it did not take before.

## Deferred

- **Recording pending bytes as a new artifact in place.** A pending
  change whose file stays at its record's path can be accepted, or
  moved by a rename (which separates it), and nothing else. A verb that
  says "this is a different file" without moving it is left until it is
  asked for.
- **Telling the operator before a rename separates a file.** The
  question's description shows the library's `unrecognised-content`
  answer as today, and does not say that answering Rename records the
  file as a new artifact.
- **Origins on disk.** Change 13 adds an identifier and its origin to
  each entry. This change reports the basis of each acceptance in the
  event alone.
