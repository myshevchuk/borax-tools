# Design: separate-change-from-acceptance

## Context

Everything below was read from source at `6d9305b` (release 0.9.0 is
`76ed2fa`; the two commits after it touch CI and documentation only).
This change carries out change 15 of the roadmap's Phase 4, as the
Phase 4 library-authority design record (maintainer's notes, outside
the repository) assigns it.

**Settled by the maintainer.** These are decisions, not open questions:

1. Reconciliation and an applying rename always observe and report
   bytes at a recorded path that match no record's history.
   Observation does not by itself accept them as the same artifact.
2. They accept the bytes automatically only when the new bytes extract
   an identifier that the linked item carries. An annotated paper that
   still carries its DOI qualifies. Otherwise the change stays pending,
   and the hash is not appended, until the operator accepts it.
3. Each history entry records the item it was accepted under. That
   closes the open re-link case in `openspec/STATE.md`.
4. No migration and no on-disk format-version transition before 1.0.0.

**Defaults the record sets, kept here:** a pending change writes
nothing to the artifact record and is reported on every run until
accepted, the way a missing record is. Only files whose bytes changed
at a recorded path pay for an extraction.

**Owned by this proposal:**

- how the operator accepts (D4);
- the condition and event vocabulary for a pending change (D3, D9);
- whether `status` reports pending changes (D3);
- what an applying rename does with unaccepted bytes (D5);
- how a pre-change history entry is read (D7);
- what identifier comparison means (D2).

What the engine does today, at the points this change touches:

- `library::reconcile` (`crates/borax/src/library.rs`, line 1905) runs
  four steps. Step 3 (near line 2002) takes a record still unresolved
  whose path holds an unclaimed artifact matching no record's history,
  claims it, and decides `Decided::Changed { hash }`. Step 4 (near line
  2049) pushes a `HashEntry` and refreshes size and modification time.
  `reconcile` takes no documents and reads no item.
- `run::reconcile_events` (`crates/borax/src/run.rs`, line 3676) passes
  the root, `rehash`, and the run's timestamp. `Command::Reconcile`
  (`crates/borax/src/cli.rs`, line 281) has `path`, `rehash` and
  `run_log`. The `cli` requirement "A subcommand accepts only the
  settings it consumes" gives `reconcile` `--rehash` alone.
- `library::admit` (line 2337) keeps the held record's link unless the
  file was re-identified or the record links none. It appends the
  file's hash when it is not the history's newest (line 2414), and so
  appends a duplicate entry for bytes restored to an earlier version.
  `Stores::take_in` (line 1043) mirrors it for the run's account and a
  preview's.
- `run::recorded` (line 2972) admits a file after its `renamed` or
  `already-named` event, under the `held` record `recorded_at` found
  before the move (line 1454). `run::reidentified` (line 3070) is true
  for a lookup whose origin is the operator, or a record accepted over
  a conflict.
- `Stores::consult` (line 840) answers `Tracked` with the record's
  link for any hash in its history.
- `HashEntry` (`crates/borax-core/src/library.rs`, line 359) has `hash`,
  `run`, `timestamp` and `tool_version`. `history_table` (library.rs
  line 1653) writes those four, and `append_history` (line 1673) appends
  entries past the ones a document holds and never rewrites one.
- `pipeline::extraction` (`crates/borax/src/pipeline.rs`, line 298)
  opens one file, runs the passes resolution runs, and returns an
  `event::Extraction`. A resolved file's own result is
  `Evidence::extraction.result`, `ExtractionStep::Ran(Extraction)` or
  `NotAttempted` when the content index answered first.
- The arXiv reader stores the feed's `id`
  (`crates/borax-sources/src/arxiv.rs`, line 198), which carries the
  newest version at lookup time. `ArxivId` keeps the version apart and
  exposes `id()` without it.

## D1. Where acceptance is decided, and on what evidence

**Decision.** Let P be a record's last-known path, R the record, and B
the bytes at P, where no record's history holds B's hash. B is
*accepted* as R's artifact on one of three bases:

| Basis | Where | Condition |
|---|---|---|
| `extracted` | reconcile and rename | extraction of B finds an identifier that R's linked item carries (D2) |
| `supplied` | rename | the operator supplied an identifier for the file in this run, and R's linked item carries it |
| `operator` | reconcile | the operator named P with `--accept` (D4) |

Otherwise B is *unaccepted*: pending while it stays at P (D3), and a
new artifact once a rename moves it (D5).

`extracted` is tried first wherever extraction ran, because it is
evidence from the bytes themselves.

**Why extraction of the new bytes.** The settled rule says the new
bytes "extract an identifier". The extracted identifier is a fact about
B alone, it is the same in every run under the same settings, and it is
available to reconcile, which resolves nothing. Using it on both paths
means reconcile and rename give the same answer for the same bytes.

**Why a supplied identifier counts.** An operator who types, for this
file, an identifier the linked item carries has said that these bytes
are the work the record links. That is acceptance by the operator, made
where the operator is already being asked. The common case is a scan or
an annotated copy whose text yields nothing.

**Rejected: the resolved record's identifiers.** Those came from a
service or from the content index, not from B. An arXiv record carries
the published DOI, and a content-index entry keyed by B's hash may have
been written by another library or an earlier answer. On a
content-index hit no extraction runs at all, so reconcile and rename
would decide differently about the same bytes. The settled wording
names extraction.

**Rejected: any re-identification as acceptance.** Re-identifying a
file as another work says what work the file is. It does not say the
file is R's artifact. Accepting would put the replacement into R's
history, and every entry R holds from before this change would then
answer with the new item (D7), which is the wrong association this
change exists to remove. A re-identification to a work R's item does
not carry therefore separates the file on a move (D5).

**Rejected: the operator answering Rename.** The question asks about a
name, not about the artifact.

**Rejected: title or content similarity.** Both are heuristics, and the
title check exists for another purpose. The settled rule names no
heuristic.

**Rejected: a record that links no item accepting anything.** Nothing
could answer wrongly from it today. But the item would then be selected
from the new bytes, and every pre-change entry would answer with it.

## D2. What "the linked item carries the identifier" means

**Decision.**

- **The linked item.** R's `item`, carried by exactly one item file the
  store read, which is the test tracking already applies. A record
  linking no item, an item no file carries, an unreadable one, or one
  two files carry, carries nothing. Reconcile reads the item store once
  per run. An applying rename reads it fresh at admission, as `admit`
  does. A preview asks its account.
- **The kinds.** DOI, arXiv, PMID and ISBN, the four `Identifier` forms
  `library::identifiers` searches. An identifier is compared only with
  the item record's field of its own kind, as `identifier_of` does
  (`record.doi`, `record.borax.arxiv`, `record.pmid`, `record.isbn`).
  Extraction finds only DOIs and arXiv identifiers
  (`borax_pdf::scan::FoundIdentifier`), so an `extracted` basis is
  always one of those two. A `supplied` basis can be any of the four,
  since `identifier::supplied` parses all four. The comparison is
  defined for all four either way.
- **Normalisation.** The parser's own normal forms, which both sides
  already carry:
  - a DOI is lowercased, with its resolver prefixes stripped;
  - a PMID is an integer;
  - an ISBN is compact, with an uppercase `X`.
- **arXiv without its version.** Two arXiv identifiers match when their
  `ArxivId::id()` match. The item's identifier carries the version that
  was newest when it was looked up. The PDF's stamp carries the version
  of that PDF. Whole-value equality would leave the motivating case, an
  annotated preprint, pending whenever a later version existed at
  lookup. Two versions of one preprint are one work, so this joins
  nothing unrelated.
- **ISBN-10 is not converted to ISBN-13.** `Isbn` preserves the length
  it was given. Converting would be normalisation the parser does not
  do. An ISBN reaches the gate only as a supplied identifier, and the
  miss leaves the change unaccepted, which is the safe miss. A test
  pins it.
- **One identifier.** The extractor returns the first identifier its
  passes find. When that is a cited reference's DOI rather than the
  paper's, the change is pending. That is the conservative miss, and a
  pending change writes nothing.
- **Reading it back.** `Extraction::Found` carries the identifier as
  the text `Identifier`'s `Display` writes (`doi:…` or `arXiv:…`).
  `extracted_identifier` parses it with `identifier::supplied`, which
  accepts every form that `Display` writes. A test pins the round trip
  for all four kinds, so the function stays total if extraction ever
  finds more.

**Rejected: whole-value arXiv equality.** See above.

**Rejected: comparing `Display` strings.** It would tie the comparison
to rendering, and it cannot drop an arXiv version.

**Rejected: a new identifier helper in `borax-core`.** `identifier_of`
in `library.rs` is the comparison the item store already uses. `carries`
is that, plus the arXiv rule, beside it.

## D3. A pending change, and where it is reported

**Decision, in reconciliation.**

- Step 3 is renamed "Changed in place" and still claims the artifact,
  whatever it decides. The step order is unchanged, so two swapped files
  still both repair in step 2, and a second record naming the same path
  still finds the file claimed.
- A pending record is written nothing. That covers the hash and also
  the size and modification time. The next reconcile therefore misses
  the fast path again, hashes the file, and reports the change again. A
  change the fast path cannot see stays one only `--rehash` sees, as
  today.
- It is reported as one `library-repair` event of kind `pending`,
  carrying the hash, the record's link (absent when it has none) and
  the extraction result. It is counted under `pending` on
  `library-reconciled`, and it is not counted as confirmed.
- It is not a finding and not a skip, and it does not change the exit
  status. Reconciliation stays routine.
- Extraction runs only for records that reach step 3. A library nothing
  has touched opens no document, and a swap opens none either, since
  step 2 repairs it. The extraction settings are the run's own, as
  `status --identify` uses them.

**Decision, in resolution.** A file whose change is pending is still
answered `unrecognised-content` by its library, on every `resolve`,
`rename` and `bib`, as today. That is the observation on the resolution
path.

**`status` does not report pending changes, and neither does
`validate`.** By requirement `status` opens no document and hashes
nothing, and `validate` hashes nothing. A pending change is known only
after hashing and then extracting. Inferring "possibly changed" from
size and modification time would report every `touch` as a change. The
two places that know report it: reconcile, on every run, and the
resolution of the file.

**Rejected: a `library-condition` of kind `pending`.** Conditions belong
to `status` and `validate`, and are named before any document is
opened. The requirement "Library conditions are named where they are
counted" says reconcile writes none and traces its counts through its
own per-object events, which `pending` is.

**Rejected: recording the pending state on disk.** For example, a
`pending` table in the record. The default says a pending change writes
nothing. A marker would also be state that goes stale when the file
changes again, and would itself need reconciling.

**Rejected: refreshing size and modification time for a pending
record.** The fast path would then settle the record, and the change
would never be reported again.

## D4. How the operator accepts: `borax reconcile --accept <path>`

**Decision.** The recommendation stands as given.

- **Shape.** `--accept <path>` on `borax reconcile`, repeatable, with no
  new command and no prompt.
- **Paths.** Each path is taken as every path argument borax takes:
  made absolute against the working directory and normalised lexically,
  then made library-relative (`library_relative`). Repeats count once.
- **Fast path.** A record whose last-known path is named is hashed
  whatever the fast path says. So `--accept` reaches a change the fast
  path hides without `--rehash`.
- **Order.** The gate runs first. A named change whose bytes extract an
  identifier the item carries is accepted `extracted`, the stronger
  evidence.
- **Outcome per named path:**

  | At the path, after steps 1–3 | Outcome |
  |---|---|
  | a record whose change step 3 left pending | accepted `operator`: the hash is appended with the record's link as its `item` (none when it links none), size and modification time are refreshed, and the record is reported `changed` and counted as changed |
  | a record whose change the gate accepted | reported `changed` as `extracted`, and nothing more |
  | a record confirmed (its history holds the bytes) | nothing to do, and nothing reported |
  | anything else: no record names it, it is outside the library or excluded, its record was repaired elsewhere or is ambiguous or missing, the file there matches another record's history, or it could not be read | a `skipped` event, `path` as given, reason `not-pending` |

  A `not-pending` skip counts as skipped, so the run exits with the
  partial-success code and the human summary names the skip. A
  mistyped path must not read as success.
- **A selector, not a setting.** There is no configuration key and no
  `--no-accept`, like `--identify` and `--rehash`.
- **`reconcile` takes the extraction settings** (`--page-limit`). The
  `cli` requirement says a setting whose value can change what a
  command writes must be declared on it.

**Rejected: a new `borax accept` command.** Acceptance is a decision
made in step 3, after steps 1 and 2 have claimed what they claim. A
file at P that matches another record's history is that record's, not
P's change. A command of its own would have to repeat the walk, or
accept without it.

**Rejected: an interactive prompt in `reconcile`.** `reconcile` is not
interactive and has no session. The brief excludes a prompt here.

**Rejected: `--accept-all`.** It would recreate the defect in one flag.

**Rejected: accepting by artifact identity.** Paths are what an operator
types everywhere else, and what the human line leads with. The event
carries both.

**Rejected: accepting under an item named on the command line.** Saying
what work a file is belongs to `rename`, through re-identification.

**Rejected: reading the path as library-relative.** That would make
`--accept` the one path argument read against the library root. A
relative path that names a file from the working directory would then
silently name another one.

**Rejected: reporting `not-pending` as a warning on stderr.** The JSON
stream and the exit status would not show it.

## D5. The applying rename

Let `held` be the record naming the file's path before the move, as
today, and `hash` the file's hash.

**Decision.**

| Case | Written | Reported |
|---|---|---|
| no `held` | a minted record, as today | as today |
| `held`'s history holds `hash` | an update (D6) | `relinked` when the link moved, as today |
| not held, accepted (`extracted` or `supplied`) | the hash appended to `held` with the item the record links after admission, and an update | `changed`, then `relinked` when the link moved |
| not held, unaccepted, the run moved the file | a new artifact record at the target, with the item a mint selects; `held` is untouched | `separated`, naming the new record, its item, and `held` |
| not held, unaccepted, the file stays at its path (already named, or Keep) | nothing | `pending`, naming `held` and the hash |

The rest of the decision:

- **Where the gate's identifier comes from.** The extracted identifier
  is the file's own extraction result when it ran. When it did not,
  because the content index answered, `pipeline::extraction` runs on the
  file before the move. The supplied identifier is the one the file's
  lookup used when that lookup's origin is the operator. Both are
  settled before the move, where `held` is.
- **The hold.** A `pending` admission is shown whatever the hold, as an
  `unwritten` one is, so an interactive run passing over an
  already-named file still reports it.
- **`--no-record`.** The move is still refused by the existing
  stranding rule. Its reason now names `borax reconcile --accept` beside
  `--rehash`.
- **A preview.** It foresees the same outcome: a separated file as a
  mint, and a pending one as nothing learned. It emits no admission
  event, as today. Its account also treats every path it plans to move
  a file away from as holding nothing, unless it plans another file
  there. A separation leaves the held record at the vacated path. The
  preview's `exists` check (`run.rs`, near line 1313) adds foreseen
  targets but still sees the source on disk. Without this, a later copy
  of the held record's old bytes would be a content duplicate of that
  path in the preview, while the applying run passes over the path as
  stale and lets the copy proceed. The rule applies to every preview
  move. For an ordinary one it changes nothing, since the moved
  record's path follows the file there too.
- **Separation reuses the mint.** `admit` is called with no held record.
  The run's account learns the new record, and keeps `held` at its old
  path. A later file of the same run that matches `held` therefore
  meets a stale path, and the run warns that the library has paths to
  reconcile, which is true.

**Why separation on a move, and pending in place.** A pending change is
a state of a path. Once a rename takes the bytes away from P, there is
no change at P left to accept. R names a path that holds nothing, which
is what `missing` means, and its whole history is kept. If its bytes
turn up again, reconciliation repairs it by hash. The bytes at the
target are then a file no record names. Recording them as a new
artifact is what an applying run does for any such file, and it selects
their item from their own resolved record. No unaccepted bytes enter
any history, and no record is left with a wrong link. In the HPLC case,
the guide gets its own record and item, and the Klykov record reports
itself missing.

**Rejected: refusing the move until the change is accepted.** The HPLC
guide could then never be renamed unless the operator first accepted it
as the Klykov artifact. That is the wrong answer, and it would be the
only one on offer.

**Rejected: moving R's path with the file and leaving the hash out.** R
would follow bytes it never accepted. The guide would sit under the
guide's name as the Klykov artifact, pending indefinitely.

**Rejected: moving the file and writing nothing.** The rename resolved
and moved the file and would record it nowhere, which contradicts "An
applying run records what it admits".

**Rejected: a second record at the same path for a file left in
place.** Two records would name one path. `recorded_at` and `by_path`
answer with the first, and reconcile and consult would disagree about
which record the file belongs to.

## D6. Each entry's item, and how resolution reads it

**Decision.**

- **Every entry written after this change names `item`.** That is the
  item its bytes were accepted under:
  - for a mint, the item it selects;
  - for an acceptance in reconcile or rename, the item the record links
    afterwards;
  - for `--accept`, the record's link;
  - for a link change, the new item;
  - for a restatement, the record's link (see below).
- **`answers(H)`**, the item a record answers with for bytes H,
  `ArtifactRecord::item_for(H)`:
  - for the newest entry's bytes, the record's link;
  - for other bytes, the item named by the newest entry holding H, or
    the link when that entry names none;
  - a record that links no item answers `no-item` for every version, as
    today (D9).
- **Rule 1: a decision is recorded.** Some admissions decide that bytes
  H belong to item I:
  - accepting changed bytes;
  - a re-identification;
  - a re-link;
  - a record that linked no item gaining one.

  Each appends `(H, I)`, unless H is already the newest entry's hash and
  `answers(H) == I`, computed after the admission. The comparison is
  with `answers(H)`, not with whether the link changed. A restored
  version re-identified as the item the link already names (sequence h)
  is therefore appended, because until then it answered with the item
  its own entry names.
- **Rule 2: a hand correction is restated.** This is judged on the
  record **as read**, before the admission assigns a new link. If:
  - the record as read links an item L;
  - its newest entry names another item, or none; and
  - the admission is about to append an entry for bytes other than the
    newest entry's,

  then it first appends a restatement `(newest hash, L)`, stamped with
  the current run.
- **One helper, and the order it needs.** Both rules live in
  `ArtifactRecord::append(entry, link_as_read)`. Today `admit` (library.rs
  near line 2409) and `Stores::take_in` (near line 1071) assign the new
  link before pushing the entry. Each must capture the link as read
  before that assignment and pass it. Judged against the new link
  instead, Rule 2 would take borax's own re-link for a hand correction.
  In sequence (a) that gives `[A(X), B(Y), B(X), A(X)]`, and B wrongly
  answers X.
- **No append for an ordinary admission of bytes already held.** That
  admission decides nothing, so Rule 1 does not apply. The path, size
  and modification time are updated, and the link is kept, as today.
  Restored bytes stay an earlier entry and keep answering with the item
  they were accepted under.
- **Growth.** The history grows by at most one entry for each explicit
  operator re-identification, plus at most one restatement, and by
  nothing on an ordinary re-run. Alternating re-identifications (restore
  A and re-identify it as X, then restore B and re-identify it as Y,
  and so on) append one entry each, though only two distinct pairs
  exist. That is the record of what the operator decided, and when.

  A named test pins that ten ordinary re-runs over a restored file
  append nothing.

**The transitions the two rules defend**, each a test (task 6.1).
`A(X)` means an entry for bytes A naming item X, and `A(·)` an entry
naming none.

| Sequence | History afterwards | Answers |
|---|---|---|
| (a) `[A(X), B(Y)]`, link Y; restore A; re-identify A as X; restore B | `[A(X), B(Y), A(X)]`, link X. Rule 1: A not newest. Rule 2: newest B names Y, the link as read | A: X. B: Y |
| (b) `[A(X)]`; a person sets the link to Y; edited B accepted under Y; restore A | `[A(X), A(Y), B(Y)]`. Rule 2 restates A under Y | A: Y. B: Y |
| (c) `[A(X)]`; edited B accepted under X; re-identify B as Y; restore A; edit A to A′ carrying X's DOI | `[A(X), B(X)]`, link Y. Rule 1: B newest, and answers Y after the admission | A: X. B: Y. A′: pending, since Y does not carry X's DOI |
| (d) `[A(X), B(Y)]`, link Y; a person sets the link to Z; restore A; edited C accepted under Z | `[A(X), B(Y), B(Z), C(Z)]`. Rule 2: the correction was made while B was newest | A: X. B: Z. C: Z |
| (e) `[A(·)]`, link X; edited B accepted; re-identify B as Y; restore A | `[A(·), A(X), B(X)]`, link Y. Rule 2 pinned A at the acceptance, and Rule 1 skips B as newest, answering Y | A: X, not the interim Y. B: Y |
| (f) `[A(X)]`, link X; re-identify A as Y; later, edited B accepted under Y | `[A(X)]`, link Y; then `[A(X), A(Y), B(Y)]`, where Rule 2 restates A | A: Y throughout. B: Y |
| (g) `[A(X), B(Y)]`, link Y; restore A; ten ordinary renames | unchanged history, link Y | A: X. B: Y |
| (h) `[A(X), B(Y)]`, link Y; restore A; re-identify A as Y | `[A(X), B(Y), A(Y)]`, link Y. Rule 1: A not newest | A: Y. B: Y |

**Consequences for the duplicate checks.** Restored bytes answering
through an earlier entry's item, while the record keeps its link,
affects the work check (`Account::work_duplicate`, library.rs line 721)
on both of its sides:

- **The incoming file's own record.** Today that record excludes only
  its link. A restored version answering with X while the record links
  Y would meet X's other artifacts as a work duplicate on an ordinary
  batch rerun. **Decision:** exclude the item the own record answers
  with for the incoming bytes as well as its link. The incoming hash is
  already in hand, so this costs nothing. The `ledger` requirement is
  modified to say so.
- **The other side: which artifacts an item has.** This remains the
  records linked to it. An incoming PDF of Y can therefore be reported a
  work duplicate of a recorded path that holds a restored version
  answering with X. **Decision:** keep this, state it in the `ledger`
  requirement, and pin it with a test. Knowing what bytes another
  record's file holds would mean hashing that file, and the work check
  deliberately asks no more of a recorded path than whether it holds a
  file. The report is the conservative branch: a batch leaves the file
  where it is, and an interactive run asks.

Rejected for the other side:

- **Hashing every candidate artifact.** This adds a read per candidate
  to every work check, for a case that needs a restore after a re-link.
- **Counting entries' items as an item's artifacts.** An item would
  gain artifacts from versions nobody holds any more.

**Why the newest bytes answer with the link.** The link is what a
person edits. "The library's text is authoritative and is read
directly" makes a text editor a peer writer, so a link corrected by
hand must decide for the file as it stands.

**Why Rule 2.** A hand correction changes the link and writes nothing
else. Borax's own decision about the newest bytes is recorded the same
way when Rule 1 skips it (sequences c, e, f): the link carries it, and
Rule 2 restates it at the next append. Without a restatement, the correction would vanish for its bytes
the moment later bytes were accepted (sequence b). The restatement
records it at the first write borax makes after it. That is the
earliest moment borax can see the correction, and the last moment it
still applies to the newest bytes. It assumes a hand correction speaks
for the bytes that were newest when borax next writes. A person who
corrected the link and then restored an older version before any write
is a case the record cannot tell apart, and Rule 2 credits the
correction to the newest bytes (sequence d).

**Why Rule 1 appends even for an earlier version.** A link moved
without an entry ties the new item to whatever bytes are newest. In
sequence (a), that would make B answer X. A decision that leaves the
link where it was would otherwise be lost: in sequence (h), A would go
on answering X.

**Why bytes already held are not appended by an ordinary run.**
Appending a restored version would make it the newest, and so make it
answer with the link, undoing the fix on the run after an ordinary
rename (sequence g). The reason given for appending, that a record
should hold the bytes of the file it names, is already met.

What remains is that after a restore the newest entry is not the file's
bytes. Nothing depends on that. The fast path compares size and
modification time, which are the file's. Step 2 prefers a match on the
current hash only to choose between candidates. `strands` and the
content check read the whole history.

**Rejected: every entry answering with its own item, the newest
included.** A link corrected by hand would be ignored for the current
bytes.

**Rejected: Rule 1 keyed to the answered item before the admission**
(the first draft's `item_for(hash) != new item`). This loses sequence
(a): A already answers X, so nothing is appended, yet the link moves to
X and B's answer changes.

**Rejected: Rule 1 keyed to whether the link changed** (the second
draft). This loses sequence (h): the link stays Y and nothing is
appended, so A goes on answering X.

**Rejected: accepting the loss of hand corrections.** A text editor is
a peer writer by requirement. Losing its correction at the next edit
would make the hand-edited link a value that silently stops meaning
anything.

**Rejected: rewriting the newest entry's `item` to match a hand
correction.** History is append-only. `append_history` never rewrites
an entry, and the "A write cut off mid-way loses nothing" guarantee
assumes so.

**Rejected: the link following restored bytes on an ordinary run.** An
ordinary run would move a link, which the living requirement forbids,
and which would report as `relinked`, today a re-identification.

## D7. Entries written before this change

**Decision.**

- **The field is optional.** `HashEntry::item` is optional, and a
  missing value is not written. An entry without `item` answers, where
  it is the deciding entry (D6), with the record's current link. That is
  today's rule.
- **Nothing is rewritten.** `append_history` only appends. Every
  existing entry stays byte-identical, and a record nothing touches is
  never written. There is no migration and no format version, as the
  maintainer has ruled out both before 1.0.0, and user-owned files are
  protected.
- **Validation.** An entry naming no item is not a finding. Neither is
  an entry naming an item the library does not hold: that is the
  history of an item later deleted, and deleting an item nothing links
  to is an ordinary operation. If those bytes are presented again,
  resolution reports `accepted-item-unavailable` for them (D9). An
  entry whose `item` is
  not a canonical UUID makes the record one that does not parse, since
  `ItemId` refuses it, and it is reported as any unparseable record is.
- **The residue.** The first entry borax appends to such a record
  restates its newest bytes under the link (D6 Rule 2), which pins them
  to the item they answered with until then. Entries older than that
  newest one still answer with the current link, which is the old
  interim rule, because the record holds nothing better for them. A
  scenario states it.
- **`--accept` on a record linking no item** writes an entry naming
  none, which is read the same way.

**Rejected: a migration, or a format version.** Ruled out by the
maintainer.

**Rejected: a validation finding for entries with no item.** Every
existing library would fail validation for having been written by an
earlier release.

## D8. No origin on disk

**Decision.** This change records the item and nothing else. The design
record gives change 13 the identifier and its origin for each entry.
The basis of an acceptance is not needed on disk in order to report it.
The event reports it at the moment of acceptance (`accepted.by`), and
the entry's `run` names the run that reported it. The log of that run,
where one was written, holds the event. Adding an `origin` now would fix
its spelling before change 13 designs the evidence it belongs to.

## D9. Field table

Every new key, value and kind is an addition, and the schema stays 4.
Each released field this change touches keeps its **primary purpose**,
the thing a schema-4 consumer reads it for. Where the link and the item
answered with used to coincide and now can differ, the field follows
its purpose:

| Field | Primary purpose | After this change, and why that preserves it |
|---|---|---|
| `library` `tracked` `item` | which item the record links: the library's statement about the artifact | always the link. The item answered with, when different, is the new `accepted_under`. The link is checked first, so `item` is still carried by exactly one item file |
| `library` `no-item`, `dangling-item`, `unreadable-item`, `ambiguous-item`, and their `item` | what is wrong with the record's link | unchanged. They are emitted only for a problem with the link, for every version of the file. A problem with an earlier entry's item is the new `accepted-item-unavailable` |
| `record_retrieval` `library` `item` | source attribution: the item whose record the event carries | the item whose record was returned. For restored bytes accepted under X, that is X. A consumer that attributes the record's fields to this item stays right. Keeping the link here would attribute X's metadata to Y |
| `library-repair` `changed` | bytes appended to the record's history | unchanged; it gains `accepted` |
| `library-admission` `relinked` | a link moved by re-identification | unchanged |
| `library-reconciled` `changed` | records whose new hash was appended | unchanged |

**Rejected alternatives:**

- **`accepted_under` on `record_retrieval` too.** Once `item` there
  names the source, an extra key would repeat it. The link is in the
  `library` section, beside the record's answer, which is where a
  consumer asking about the link already reads.
- **`accepted_under` always present on `tracked`.** Every common-case
  line would change, and a consumer would have to compare two fields to
  learn whether anything unusual happened.
- **One meaning for every `item`** (all the link, or all the item
  answered with). Either keeps one field's coincidence and breaks the
  other field's purpose. All the link misattributes the record in
  `record_retrieval`. All the answered item changes `tracked`'s
  statement about the link.
- **Reusing `dangling-item` and its siblings for an entry's item.** A
  consumer would take `item` for the missing link.

Not every event is byte-identical to 0.9.0. `changed` repairs gain
`accepted`, reconciliation totals gain `pending`, and a restored
version's `record_retrieval` names the item it was accepted under.
Every key and kind added is an addition. Every value is the one its
field's purpose calls for.

### `library` answer (additions)

| `kind` | Fields |
|---|---|
| `tracked` | `artifact`, `item` (the link), and `accepted_under` (new, present only when the item answered with differs from the link) |
| `accepted-item-unavailable` (new) | `artifact`; `item`, the link; `accepted_under`; `cause`, which is `dangling`, or `unreadable` with `path` and `message`, or `ambiguous` with `files` |

### `record_retrieval` `library`

`artifact` and `item`, unchanged in shape. `item` is the item whose
record was returned: `accepted_under` when the `library` answer carries
it, and the link otherwise. `Evidence::retrieval` (`evidence.rs`, near
line 170) builds `RecordRetrieval::Library` from the `tracked` answer
this way.

### How `Stores::consult` answers, in order

1. Steps 1–4 are unchanged: no record names the path, the file is
   unhashable, no record holds the hash (`unrecognised-content`), or
   several do (`ambiguous`).
2. The holding record links no item: `no-item`, whichever version the
   file is.
3. The link is not carried by exactly one readable item file:
   `dangling-item`, `unreadable-item` or `ambiguous-item` about the
   link, as released, whichever version the file is.
4. `item_for(hash)` is the link: `tracked` with no `accepted_under`.
5. Otherwise, with X the entry's item: when X is carried by exactly one
   readable item file, the answer is `tracked` with `accepted_under` X
   and X's record; otherwise it is `accepted-item-unavailable` with the
   matching `cause`.

Checking the link first means that every answer 0.9.0 could have given
is given with the same kind and the same fields. A file whose link is
damaged is reported as damaged even where an earlier entry's item could
have answered. That is a record `validate` already has a finding about,
and reporting it is better than hiding it behind an older answer.

### `AcceptedBy` (under `accepted`)

| `by` | Fields | When |
|---|---|---|
| `extracted` | `identifier` (`doi:…` or `arXiv:…`, as extracted) | the gate's identifier basis, in reconcile or rename |
| `supplied` | `identifier` (as the lookup used it) | rename only: the operator's identifier, carried by the linked item |
| `operator` | none | reconcile only: `--accept` |

### `library-repair` `repair`

| `kind` | Fields |
|---|---|
| `changed` | `hash`, `accepted` (new key) |
| `pending` (new) | `hash`; `item`, the record's link, absent when none; `extraction`, the `library-extraction` vocabulary (`found` with `identifier` and `tier`, `no-text-layer`, `text-without-identifier`, `encrypted`, `unreadable` with `message`) |

### `library-reconciled`

`pending` is new: the number of `pending` repairs. It is written after
`changed`.

### `library-admission` `admission`

| `kind` | Fields |
|---|---|
| `changed` (new) | `id` (the artifact), `hash`, `accepted` (`extracted` or `supplied`) |
| `pending` (new) | `id` (the record naming the file's path), `hash` |
| `separated` (new) | `id` (the new artifact), `item` (its item), `from` (the record that did not accept the bytes) |

An admission emits, in order: `changed` when it accepted changed bytes,
then `relinked` when the link moved. Otherwise it emits exactly one of
`separated`, `pending`, `outside` or `unwritten`, or nothing. A write
that fails reports `unwritten` alone, since nothing was accepted.

### `skipped` `reason`

`not-pending` (new), carrying nothing beyond `kind`. It is not a
resolution verdict, so the event carries `path` and `reason` alone.

## D10. Human lines

Every value taken from a file name, a file or a reader is escaped with
`describe::escaped`, as the `library-extraction` line is.

- `library-repair` `changed`:
  - `extracted`: `<path>: has changed since borax last saw it; <hash>
    recorded, since it carries <identifier>, which its item carries`
  - `operator`: `<path>: has changed since borax last saw it; <hash>
    recorded, as the operator accepted it`
- `library-repair` `pending`: `<path>: has changed since borax last saw
  it, and the change is pending: <reason>; borax reconcile --accept
  accepts it`, where `<reason>` is:
  - `found` with an item: `it carries <identifier>, which item <item>
    is not known to carry`;
  - `found` with no item: `it carries <identifier>, and its record links
    no item`;
  - any failure: the clause the `library-extraction` line uses
    (`what_was_extracted`), for example `no identifier found; the pages
    read hold no text`.
- `library-reconciled`: `<root>: N records, N confirmed, N repaired, N
  changed, N pending, N ambiguous, N missing, N hashed`.
- `library-admission`:
  - `changed`, `extracted`: `<path>: artifact <id> accepted these bytes
    (<hash>), since they carry <identifier>, which its item carries`
  - `changed`, `supplied`: `<path>: artifact <id> accepted these bytes
    (<hash>), since its item carries <identifier>, which the operator
    supplied`
  - `pending`: `<path>: artifact <id> has not accepted these bytes
    (<hash>), so nothing was recorded; borax reconcile --accept accepts
    them`
  - `separated`: `<path>: artifact <from> did not accept these bytes,
    so they were recorded as artifact <id> of item <item>`
- `skipped` `not-pending`: `<path>: no change is pending here, so
  nothing was accepted`.
- The resolution line's `; the library could not answer: ` clause for
  `accepted-item-unavailable`: `artifact <artifact> holds these bytes
  as accepted under item <accepted_under>, which <is not in the
  library | could not be read from <path> (<message>) | is carried by
  <files>>`. A `tracked` answer with `accepted_under` leaves the line
  as a `tracked` answer does.
- `skipped` `stranding` (changed text): `artifact record <id> holds no
  hash of these bytes, and this run writes none, so moving it would
  strand the record; borax reconcile --rehash records them, or reports
  them pending for borax reconcile --accept`.

## D11. The interface the tests are written against

`borax-core`:

```rust
pub struct HashEntry {
    pub hash: ContentHash,
    pub run: RunId,
    pub timestamp: String,
    pub tool_version: String,
    /// The item these bytes were accepted under. `None` for an entry
    /// written before entries named items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemId>,
}

impl ArtifactRecord {
    /// D6. `None` when the history does not hold `hash`, or when the
    /// answer is a link the record does not carry.
    pub fn item_for(&self, hash: &ContentHash) -> Option<&ItemId>;

    /// D6 Rule 2, then a push. `link_as_read` is the record's link as
    /// it was read, captured by the caller before it assigns a new one.
    /// When `entry.hash` is not the newest entry's hash, `link_as_read`
    /// is `Some(L)`, and the newest entry names an item other than L or
    /// none, first push a restatement of the newest hash under L,
    /// stamped with `entry`'s run, timestamp and tool version. Then push
    /// `entry`. Rule 1, whether to call this at all, is the caller's:
    /// `admit` and reconcile decide it with `item_for` after the
    /// admission.
    pub fn append(&mut self, entry: HashEntry, link_as_read: Option<&ItemId>);
}
```

`borax::event`:

```rust
#[serde(tag = "by", rename_all = "kebab-case")]
pub enum AcceptedBy {
    Extracted { identifier: String },
    Supplied { identifier: String },
    Operator,
}
// Repair::Changed  { hash: String, accepted: AcceptedBy }
// Repair::Pending  { hash: String, item: Option<String>, extraction: Extraction }
//                  (`item` skipped when `None`)
// Event::LibraryReconciled gains `pending: usize`, after `changed`
// Admission::Changed   { id: String, hash: String, accepted: AcceptedBy }
// Admission::Pending   { id: String, hash: String }
// Admission::Separated { id: String, item: String, from: String }
// SkipReason::NotPending
// LibraryAnswer::Tracked { artifact, item, accepted_under: Option<String> }
// LibraryAnswer::AcceptedItemUnavailable {
//     artifact: String, item: String, accepted_under: String,
//     cause: AcceptedItemCause,
// }
// #[serde(tag = "cause", rename_all = "kebab-case")]
// enum AcceptedItemCause { Dangling, Unreadable { path: PathBuf, message: String },
//                          Ambiguous { files: Vec<PathBuf> } }
//     (flattened into the answer, so `cause` sits beside `artifact`)
// Every `accepted_under` is skipped when `None`.
// RetrievedFrom::Library and RecordRetrieval::Library keep their shape;
// `Evidence::retrieval` sets `item` to `accepted_under` when present.
```

`borax::library`:

```rust
/// D2: whether `record` carries `identifier`.
pub fn carries(record: &Record, identifier: &Identifier) -> bool;

/// D2: the identifier an `Extraction::Found` names; `None` otherwise.
pub fn extracted_identifier(extraction: &Extraction) -> Option<Identifier>;

/// D1, D2: `Extracted` when `extracted` is carried by the item `held`
/// links, else `Supplied` when `supplied` is, else `None`.
pub fn accepted_by(
    items: &ItemStore,
    held: &ArtifactRecord,
    extracted: Option<&Identifier>,
    supplied: Option<&Identifier>,
) -> Option<AcceptedBy>;

pub struct Reconciling<'a> {
    pub rehash: bool,
    /// The `--accept` paths, as given (D4).
    pub accept: &'a [PathBuf],
    /// What extraction makes of the file at a full path.
    pub extract: &'a dyn Fn(&Path) -> Extraction,
    pub run: RunId,
    pub timestamp: &'a str,
    pub tool_version: &'a str,
}

pub fn reconcile(root: &Path, reconciling: &Reconciling<'_>) -> Reconciliation;
// Reconciliation gains `not_pending: Vec<PathBuf>`: the `--accept`
// paths, as given, that came to a `not-pending` skip, in the order
// given.
```

`Account::work_duplicate(incoming, hash: Option<&ContentHash>,
identifiers)` gains the incoming file's hash. With it, the item the own
record answers with for those bytes is excluded beside its link. With
`None`, only the link is excluded, as today. `pipeline::second_copy`
gains the hash too, and both callers in `pipeline.rs` pass the file's.

The signatures of `admit`, `Admitting`, `Admitted`, `Stores::consult`
and `admission_event` are unchanged, and only their behaviour changes
(D5, D6). How the driver composes the new admission events is the
implementer's call. The tests reach them through `run::events_for`.

`borax::cli`: `Command::Reconcile` gains `extraction: ExtractionOptions`
and `accept: Vec<PathBuf>` (`#[arg(long, value_name = "PATH")]`).
`Command::reconcile(path, rehash)` keeps its signature, with `accept`
empty and the extraction options at their defaults. A new
`Command::reconcile_accepting(path, rehash, accept)` builds one with
paths.

## D12. How the specifications change

ADDED requirements:

- `library`:
  - "Changed bytes become an artifact's history only once accepted":
    D1, D2 and D3's reporting;
  - "An operator accepts a pending change by naming its path": D4.

MODIFIED requirements, each restated whole, with every living paragraph
and scenario name kept:

- `library`:
  - "An artifact record is authoritative state keyed by an artifact
    identity": each entry names its item, and D6's two rules tie every
    link change to its bytes (D6, D7). The scenario "An annotated
    artifact keeps its identity" gains "whose text still carries the DOI
    its item carries", and four scenarios are added;
  - "Reconciliation repairs a stale path by a bounded walk": step 3,
    pending and `--accept`. "An artifact annotated in place" and "A
    change the fast path cannot see" gain the identifier condition, and
    five scenarios are added;
  - "Validation defines a well-formed library": D7's paragraph and one
    scenario;
  - "An applying run records what it admits": D5 and D6. "A move that
    would strand a record is refused" and "A recorded move leaves the
    record describing the file" gain the identifier condition, "An
    operator corrects what a recorded file is" gains the appended
    entry, the explanation of why re-running without `--no-record` is
    not a remedy is corrected (it claimed a recording run refuses a
    move whose size or modification time changed, and none does), and
    six scenarios are added.
- `resolution`:
  - "A file its library tracks resolves from its library item": D6
    replaces the interim paragraph, and the link is checked first.
    "Bytes from before a re-link" keeps its name and now resolves from
    work A's item, and six scenarios are added;
  - "A library that cannot answer for a file says so": the released
    kinds keep their meaning (about the link), `accepted-item-unavailable`
    is added, `unrecognised-content` includes a pending change, and
    three scenarios are added;
  - "A resolution says whether its library answered": `accepted_under`
    on `tracked`, and the new kind, with three scenarios added;
  - "A resolution reports where its record was retrieved": the
    `library` retrieval's `item` is the item whose record was returned,
    with one scenario added and no key added.
- `ledger`: "Duplicate detection operates at two levels with distinct
  reasons". The work check excludes the item the own record answers
  with, the other side stays by link (justified in the requirement),
  and a preview treats a vacated source path as holding nothing. Three
  scenarios are added.
- `cli`: "A subcommand accepts only the settings it consumes".
  `reconcile` accepts the extraction settings, `--rehash` and
  `--accept`, and `--accept` is a selector. Two scenarios are added.

Each requirement's first line carries its SHALL. No MODIFIED
requirement drops a scenario or a paragraph, so no `<!-- drops: -->`
marker is used, and `scripts/check-spec-deltas.py` checks every one.

The search behind the list covered `openspec/specs/`. It looked for
`edited`, `appended`, `append`, `current hash`, `newest`, `interim`,
`re-link`, `history`, `--rehash` and `reconcile`. These hits are left
alone:

- `ledger`, the content check over the whole history, and the scenario
  "An admitted file annotated afterwards". Its file carries its item's
  identifier, so it is accepted.
- `library`, "Library conditions are named where they are counted". It
  already sends reconcile's counts to its own events.

## D13. Existing tests: what changes and how

The rules of the earlier changes apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed.

The search was `rg` over `crates/borax/tests`,
`crates/borax-core/tests` and `crates/*/src` for `edited`, `changed`,
`annotated`, `append`, `Repair::Changed`, `LibraryReconciled {`,
`HashEntry {`, `reconcile(`, `work_duplicate(`,
`LibraryAnswer::Tracked {`, `RetrievedFrom::Library {`,
`RecordRetrieval::Library`, `Admitted {`, `Account {` and
`--page-limit`. The `Tracked` counts are `rg -c` matches, constructors
and patterns together. `RetrievedFrom::Library`,
`RecordRetrieval::Library`, `Admitted` and `Account` keep their shape,
so their sites need no edit. There are no unit tests
under `src` for these modules.

| File | Test or site | Edit |
|---|---|---|
| `crates/borax-core/tests/library.rs` | the `hash_entry` helper (line 34) | (a) `item: None` |
| `crates/borax/tests/library.rs` | every `reconcile(root, …)` call (23) | (a) through a test helper building `Reconciling` with no accepted paths and an extractor that answers `Unreadable` and counts its calls |
| | `HashEntry {` literals (2: the `hash_entry` helper at line 88, and line 2242) | (a) `item: None` |
| | the six `account.work_duplicate(…)` calls (lines 4102–4332) | (a) pass `None` as the hash, which keeps today's link-only exclusion |
| | `LibraryAnswer::Tracked` sites (7) | (a) constructors take `accepted_under: None`, and exact patterns take `..` |
| | `reconciliation_events_emits_one_repair_event_then_the_totals` (line 2970) | (a) `pending: 0` in its totals literal |
| | `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded` (line 2203) | (b) the record links an item carrying a DOI, and the extractor finds that DOI for `paper.pdf`. `Changed` carries `accepted` `Extracted`, and the appended entry carries `item` |
| | `a_change_the_fast_path_cannot_see_is_missed_without_rehash_and_found_with_it` (line 2259) | (b) the same fixture change. The plain half is unchanged |
| | `consult_reports_tracked_for_a_rollback_to_an_older_history_entry_interim_policy` (line 4414) | kept: its entries name no item, so the link still answers. Only the docstring and the assertion message change, to name the pre-change-entry rule (D7) |
| | `two_records_whose_files_swapped_paths_both_repair_without_gaining_history_or_changing_links`, the touch, move and ambiguity tests, and every `admit_…` test | kept (`Admitting` is unchanged). `admit_relinks_to_the_supplied_records_item_when_reidentified` asserts nothing about the history. Its bytes are the newest and answer with the new item after the admission, so under D6 Rule 1 nothing is appended (task 6.1 asserts both branches) |
| `crates/borax/tests/dispatch.rs` | `HashEntry {` literals (4): `lib_hash_entry` (line 7151), `hash_entry_for` (line 10085), line 14979, and `seed_artifact_record` (line 15510) | (a) `item: None` |
| | `LibraryAnswer::Tracked` sites (4) | (a) as above |
| | the two direct `library::reconcile(…)` calls (near lines 11079 and 13122) | (a) take a `Reconciling` with no accepted paths. Both repair by hash in step 2 and should not reach step 3; the test stage reports it if one does |
| | `LibraryReconciled` literals near lines 9669, 9737 and 9814 | (a) `pending: 0` |
| | `reconcile_rehash_flag_reaches_the_library` (near line 9755) | (b) the record links an item carrying a DOI (`write_lib_item`), and `FakeDocuments` registers the file with `pdf_with_embedded_doi`. The rehashed `Changed` carries `accepted`, and its totals literal takes `pending: 0` |
| | `no_record_refuses_a_move_when_the_edit_in_place_hash_is_stale` (line 11110) | (b) `held` links an item carrying `10.1000/task-7.3a-refusal`, so the closing move's bytes are accepted as `extracted`. The refusal half is unchanged |
| | the human totals line near line 14403 | (b) gains `0 pending` |
| | `an_applying_run_writes_the_edited_files_current_hash_into_the_update` (line 11234) | kept: its item carries the DOI its fake PDF embeds, so the bytes are accepted, and it asserts the record, not the absence of events |
| | `an_accepted_interactive_rename_is_recorded_exactly_as_an_apply_run_records_it` (line 3563) | (b) The two runs write two libraries, and each mints its own item, so the histories no longer compare equal as a whole. The test compares them with each entry's `item` set aside, and separately asserts that every entry of each history names that run's own item (`accepted.item` and `applied.item`). The fact it pins, that both runs record the same history, is unchanged |
| | `an_applying_run_writes_an_artifact_record_for_the_file_it_moved`, and the two re-link tests (lines 10257 and 16111) | kept. None compares an entry's item or the history's length |
| `crates/borax/tests/pipeline.rs` | `record_at`'s `HashEntry` (line 3050) | (a) `item: None` |
| | `LibraryAnswer::Tracked` sites (9) | (a) as above |
| | `RecordRetrieval::Library` constructors in `a_tracked_files_evidence_marks_every_later_section_library_answered` (line 4335) and `a_tracked_file_resolves_from_its_item_with_no_extraction_no_source_and_no_index_traffic` (line 6064) | kept. The type keeps its shape, and in both fixtures the file holds its record's newest bytes, so the item returned is the link, as each asserts |
| `crates/borax/tests/describe.rs` | `LibraryAnswer::Tracked` sites (2) | (a) as above |
| `crates/borax/tests/binary.rs` | the `HashEntry` literal (line 401) | (a) `item: None` |
| `crates/borax/tests/cli.rs` | `reconcile_refuses_page_limit_as_an_unknown_argument` (line 1479) | (c) deleted. It asserts the surface this change replaces, and task 8.1's `reconcile_accepts_page_limit` replaces it. The parse tests comparing against `Command::reconcile(…)` are unchanged |
| `crates/borax/tests/event.rs` | `LibraryAnswer::Tracked` sites (3) | (a) as above. An absent `accepted_under` is not serialised, so no exact JSON text changes |
| | `all_events` and `all_plain_skips` | extended with the new variants by task 3.1, so the existing round-trip tests cover them. This is an addition, not an edit of an expectation |

Line numbers drift as earlier groups edit a file. The test stage reports
a site it cannot find, or a count that differs, rather than guessing.

## Not changed, deliberately

- The step order of reconciliation, its claims, its handling of
  ambiguity and missing records, and its fast path, except that a path
  named by `--accept` is hashed.
- Which files an applying run admits, the content check, the pre-move
  stranding check, and the item a mint selects. The work check changes
  only in excluding the item the own record answers with (D6).
- `resolve` and `bib`, apart from which item answers for bytes that are
  not a record's newest.
- `status`, `validate` and `adopt`.
- The interactive question and its description.

## Risks / Trade-offs

- **Pre-change entries keep the interim rule.** A library recorded
  before this change still answers restored bytes with the current
  link. It cannot do better without inventing the item, and the rule
  fades as entries are written.
- **Restored bytes renamed by an ordinary run.** They are named from the
  item they were accepted under (X), while the record keeps its link
  (Y). `validate` then counts X as unlinked when nothing else links it.
  Deleting X as unlinked would make those bytes answer
  `accepted-item-unavailable`. An incoming PDF of Y can meet that path
  as a work duplicate (D6). This needs a restore after a re-link,
  followed by a rename. It is rare, and every outcome is reported
  rather than wrong.
- **Rule 2 writes one extra entry.** The first acceptance in a record
  whose link was corrected by hand, or which was written before this
  change, restates the newest bytes first. That is one more entry,
  append-only, and it is what keeps the correction.
- **A hand correction made while an older version was restored** is
  credited to the newest bytes (D6, sequence d). The record cannot tell
  which bytes a person meant.
- **Extraction misses leave legitimate edits pending.** A reference's
  DOI found first, an ISBN-10 against an ISBN-13, or a scan, each stay
  pending until `--accept`. Pending writes nothing and is reported every
  run, so the cost is a command and not a wrong link.
- **The rename path opens a file the content index answered for.** It
  does so only at a recorded path whose bytes no history holds. A test
  pins that a content-index hit for bytes the record holds still opens
  nothing.
- **Separation is not announced in the question.** An operator
  answering Rename on a replaced file learns afterwards, from the
  `separated` line, that the file became a new artifact (Deferred).

## Illustrative schema-4 lines

```json
{"schema":4,"event":"library-repair","id":"0190c3a2-…","path":"klykov2019.pdf","repair":{"kind":"changed","hash":"sha256-…","accepted":{"by":"extracted","identifier":"doi:10.1039/c9ay00001a"}}}
{"schema":4,"event":"library-repair","id":"0190c3a2-…","path":"klykov2019.pdf","repair":{"kind":"pending","hash":"sha256-…","item":"0190c3a3-…","extraction":{"kind":"found","identifier":"doi:10.1000/buffer-guide","tier":"text-layer"}}}
{"schema":4,"event":"library-repair","id":"0190c3a2-…","path":"scan.pdf","repair":{"kind":"pending","hash":"sha256-…","item":"0190c3a3-…","extraction":{"kind":"no-text-layer"}}}
{"schema":4,"event":"library-repair","id":"0190c3a2-…","path":"klykov2019.pdf","repair":{"kind":"changed","hash":"sha256-…","accepted":{"by":"operator"}}}
{"schema":4,"event":"resolved","path":"/lib/klykov2019.pdf","…":"…","library":{"status":"consulted","answer":{"kind":"tracked","artifact":"0190c3a2-…","item":"0190c3c0-…","accepted_under":"0190c3a3-…"}},"record_retrieval":{"kind":"library","artifact":"0190c3a2-…","item":"0190c3a3-…"}}
{"schema":4,"event":"skipped","path":"/lib/klykov2019.pdf","reason":{"kind":"text-without-identifier"},"…":"…","library":{"status":"consulted","answer":{"kind":"accepted-item-unavailable","artifact":"0190c3a2-…","item":"0190c3c0-…","accepted_under":"0190c3a3-…","cause":"dangling"}}}
{"schema":4,"event":"library-reconciled","root":"/lib","records":3,"confirmed":1,"repaired":0,"changed":1,"pending":1,"ambiguous":0,"missing":0,"hashed":2}
{"schema":4,"event":"skipped","path":"typo.pdf","reason":{"kind":"not-pending"}}
{"schema":4,"event":"library-admission","path":"/lib/Smith2024.pdf","admission":{"kind":"changed","id":"0190c3a2-…","hash":"sha256-…","accepted":{"by":"supplied","identifier":"doi:10.1000/x"}}}
{"schema":4,"event":"library-admission","path":"/lib/klykov2019.pdf","admission":{"kind":"pending","id":"0190c3a2-…","hash":"sha256-…"}}
{"schema":4,"event":"library-admission","path":"/lib/Buffers2020.pdf","admission":{"kind":"separated","id":"0190c3b0-…","item":"0190c3b1-…","from":"0190c3a2-…"}}
```

A record after an acceptance. The first entry was written before this
change:

```toml
id = "0190c3a2-…"
item = "0190c3a3-…"
path = "klykov2019.pdf"
size = 482113
modified_millis = 1790000000000

[[history]]
hash = "sha256-…"
run = "2026-09-30T10:00:00Z"
timestamp = "2026-09-30T10:00:00Z"
tool_version = "0.9.0"

[[history]]
hash = "sha256-…"
run = "2026-10-07T09:00:00Z"
timestamp = "2026-10-07T09:00:00Z"
tool_version = "0.10.0"
item = "0190c3a3-…"
```
