# Design: consult-library-first

## Context

How a file reaches its record today. Everything below was read from
source at `1c23be9`.

- `pipeline::standing` hashes the file once (`from_index`). Given an
  `Account`, it runs the content-duplicate check. It then serves a
  content-index hit (`indexed_record`: `cached: true`, `tier: None`,
  `found: None`, no claims, `source: None`). Otherwise it opens the
  file (`from_file`), asks the services (`from_sources`) and compares
  titles (`disagreement`). A fresh success is written to the content
  index even under `--no-cache`. `admissible` then runs the work check.
- `resolve_file` is `standing(..).verdict` with no account.
  `resolve_batch` maps it over the paths with `map_bounded` and restores
  input order.
- `run::resolve_events` calls `resolve_batch` with no library.
  `run::resolved_record`, used by `bib_events`, calls `resolve_file`
  with no library. `run::rename_events` receives the run's `Stores`
  from `preflight`. `preflight` reads them only when
  `configs.run().config().record` is on and `adapters.collection_root`
  is set. A per-file `Account` built from them is passed to `standing`
  for the duplicate checks and nothing else.
- `run::execute` discovers `collection_root` once per run, from the
  run's start directory, through `config::library_root`. This is the
  nearest `.borax.toml`, or `library-root` when that is set. Every
  command sees the same root.
- `library::Stores` holds the `ItemStore` and `ArtifactStore` read by
  `Stores::read`. Both readers record an unreadable or unparsable file
  as a `StoreFault` and keep reading. Outside validation
  (`item_findings`, `record_findings`), nothing reads `faults`. The
  `ledger` requirement's "the run warns once naming the number of
  unreadable records" has no implementation in `run.rs`.
- `Account::is_incoming` and `own_record` compare a record's path with
  an incoming path lexically, using the platform's name rules and
  resolving no symlinks. `ArtifactRecord::holds` searches the whole
  history.
- `event::Event::Resolved` carries `identifier`, `record`, `source`,
  `found`, `claims`, `tier`, `overrode` and `cached`.
  `Event::Skipped` carries `path` and `reason`. `SCHEMA` is 3. The human
  line is `<path>: resolved <identifier> via <source>[ (cached)]`.
- `describe::whence` maps every `tier` it does not know to `from the
  file`. `record_from` appends `from an earlier run` when `cached`.
- `run::inputs` does not make input paths absolute, so an input may be
  `paper.pdf` while `collection_root` is absolute. Two restorations
  committed on this branch before the tests (`6bd1ea4`, `1d2954a`) make
  that harmless: `config::nearest_override` climbs from the lexically
  normalised start, so `./paper.pdf` finds a marker above the working
  directory, and `library::library_relative` and `library::excludes`
  normalise both sides as `library::contains` does, so admission places
  a relative input in the tree.
- `library::store_files` returns an empty list for *every* `read_dir`
  error, and `listing.flatten()` plus the `metadata().is_ok_and(..)`
  filter silently drop entries that fail. An unreadable
  `.borax/artifacts/` or `items/` therefore reads as an empty store.
- `ItemStore::by_id` returns the first item carrying an identity.
  Several item files may carry one, which validation reports as
  `duplicate-identity`.
- `Stores::take_in`, reached through `learn` and `foresee`, pushes an
  in-memory item carrying the *admitting* record whenever the store
  lacks the linked item. `admit` itself writes no item file when it
  keeps a held record's link (`kept`), so on disk the link stays
  dangling while the run's in-memory `Stores` now holds an item for it.

The review evidence: a corrected item title was ignored in favour of
the content index. Reconciliation appended an unrelated replacement's
hash to an artifact's history (reviews `interactive-review.org`, "Engine
finding" sections).

## D1. What makes a file tracked

**Decision.** A file is *tracked* by the run's library when both hold:

1. Among the artifact records whose last-known path is the file's own
   path, compared as `Account::is_incoming` compares them, exactly one
   holds the file's content hash somewhere in its history.
2. That record links to an item, and exactly one item the item store
   read carries that identity.

The library then answers with that item's record. Every other outcome
is either *untracked* or one of the problems in D2:

| What the store holds for the file | Outcome |
|---|---|
| No record names its path; the artifact store was read whole | `untracked` |
| No record names its path; the artifact store has faults (D2a) | `unreadable-records` |
| Records name its path; file has no hash | `unhashable` |
| Records name its path; none holds the hash | `unrecognised-content` |
| Several records at its path hold the hash | `ambiguous` |
| One record at its path holds the hash; links no item | `no-item` |
| …links an item the store lacks; the item store could not be listed, or an item file whose name claims that UUID failed to read | `unreadable-item` |
| …links an item the store lacks; no such fault | `dangling-item` |
| …links an identity several item files carry | `ambiguous-item` |
| …links an item exactly one item file carries | `tracked` |

Checks run in the order of the table. Paths are compared after the
normalisation in D1a. Only the records naming the
file's path are examined, so a copy of an artifact's bytes at another
path is `untracked`. So is a recorded artifact moved out of band before
`borax reconcile` repaired its record, because no record names its new
path. That matches the library's own vocabulary. `borax status` counts
both as orphans (library "An artifact is a document in the tree…").
Reconciling is the remedy the library capability already names for a
moved artifact.

Any history entry counts, not only the newest. This is an **interim
policy**, not a claim that every entry was accepted as the work the
record now links to. An entry records that the library once accepted
those bytes as this *artifact*. It does not record which *item* the
artifact was linked to at the time: a hash entry carries no item, and
re-identification re-links the whole record while keeping its whole
history (library "An applying run records what it admits"). So:

- A rollback to an earlier version of an annotated PDF resolves from its
  item. This is the case the policy is for.
- An artifact holding work A's bytes, later replaced at its path by
  work B's bytes and re-identified by the operator as B, has history
  `[A, B]` and links item B. If A's bytes are restored at that path,
  D1 answers B. That is wrong, and this change pins it as the interim
  behaviour (tasks 1.1) rather than silently depending on it.

Nothing is written either way, so no answer here makes any entry
current. Binding hash entries to the item they were accepted under, or
refusing historical entries recorded before a re-link, is an acceptance
decision. It belongs to change 15 and the Phase 4 discussion (Risks).
The review's caution ("do not automatically treat every historical hash
as current") is respected only in that nothing is written.

**Rejected: the path alone (`own_record`).** It serves the item for
bytes the library has never accepted at that path. The review's
replacement test is exactly that case. `HPLC/ACE_Guide…` copied over the
Klykov PDF would have resolved as Klykov, and nothing would have said
the library had not seen those bytes.

**Rejected: the hash alone (any record holding it).** A byte-identical
copy is an orphan by the library's own definition, not the artifact. One
hash can be held by several records linking different items: two
recorded files of identical content are two artifacts, and nothing
requires them to link one item. Picking among them is a guess. The
`ledger` content check already reports a copy to a rename run as a
content duplicate, before any resolution.

**Rejected: the newest hash only.** A rollback would fall back to the
content index or extraction, although the library has accepted those
bytes as that artifact. Treating every history entry as an identity is
also what the `ledger` content check does ("Matching a hash anywhere in
a record's history"). Its cost is the re-link case above, which it
fixes only by also breaking the annotated-rollback case. Choosing
between the two is acceptance policy, which is why the interim keeps the
ledger's rule and names the gap.

**Rejected: the first item file carrying the linked identity**
(`ItemStore::by_id` as it stands). Two item files carrying one identity
are a validation finding (`duplicate-identity`). Which of them is the
work's record is exactly what the library cannot say, and read order is
file-name order, which a person can change with `mv`. The answer is
`ambiguous-item` (D2).

**Rejected: resolving ambiguity by preferring one record** (the first
read, or the one whose newest hash matches). Reconciliation declines to
guess in the same situation ("a wrong link is worse than a stale path").
Resolution declines on the same grounds and reports the ambiguity.

## D1a. Paths are compared normalised

**Decision.** Consultation normalises both the root and the input path
with `paths::lexical`, the normalisation `Account::is_incoming` already
uses: made absolute against the working directory, `.` dropped, `..`
resolved lexically, no symlink resolved. It then asks containment and
exclusion of the normalised pair, and matches records by
`is_incoming`'s comparison. `borax resolve paper.pdf`,
`borax resolve ./sub/../paper.pdf` and the absolute spelling are all
the same file to the consultation. A path that cannot be normalised (a
relative path with no working directory) is not consulted, `library:
null`.

`library_relative` and `excludes` already normalise both sides (the
restoration `1d2954a`, which brought admission back in line with the
library's lexical-containment rule), so consultation relies on them
rather than normalising separately. Discovery climbs from the
normalised start (the restoration `6bd1ea4`), so the root is absolute
for every spelling of the input.

## D2. What the library cannot answer, and what happens then

**Decision.** Eight problems, all named in D1's table:

- `unrecognised-content`: the library records an artifact at this path,
  but not these bytes. The file was edited, or replaced, since borax
  last recorded it. Which of the two is change 15's question.
- `ambiguous`: more than one record at this path holds these bytes.
- `no-item`: the one record links no item. A record borax writes always
  links one, so this is a hand-edited or damaged record.
- `dangling-item`: the linked item is not in the store. Validation
  reports this as the `dangling-item` finding.
- `unreadable-item`: the linked item is not in the store, and the item
  store's faults include either the item store directory itself (it
  could not be listed, D2a) or an item file whose name claims the
  item's UUID (`name_uuid`). It carries that fault's path and the
  reader's message. An unlistable `items/` is never reported as
  `dangling-item`: a store that could not be read cannot say an item is
  absent. An unreadable item file whose name does not claim the UUID
  cannot be attributed, and the outcome is `dangling-item`.
- `ambiguous-item`: the linked identity is carried by more than one item
  file. It carries those files, in read order.
- `unreadable-records`: no readable record names the file's path, but
  the artifact store has faults, so an unreadable record might. See D2a.
- `unhashable`: records name the path, but the file could not be hashed,
  so no record can be confirmed. A file that cannot be hashed and whose
  path no record names is plainly `untracked`.

On any of these, resolution continues exactly as for an untracked file:
content index (unless `--no-cache`), extraction, services, conflict
check. The fallback writes the content index as any fresh resolution
does. The problem goes on the file's resolution event, `resolved` or
`skipped`, whichever the fallback produced. The three situations the
roadmap asks to keep apart are then separate in one object:

- a library answer: `tier: "library"`, `library.kind: "tracked"`;
- fallback information with library authority unavailable: an ordinary
  `tier`/`cached` with `library.kind` naming the problem;
- an ordinary resolution of an untracked file: `library.kind:
  "untracked"`, or `library: null` outside any library.

A problem is not a skip and not a finding, and it does not change the
exit status. Whether an unresolved library condition should change the
exit status is change 5's decision, and this change does not make it
early.

An artifact record file that cannot be read or parsed costs its own
artifact and nothing else, as the `ledger` requirement already says for
duplicate detection. A tracked file whose own record reads cleanly is
still tracked. But the fault cannot be attributed to a path, so a file
that would otherwise be `untracked` is reported `unreadable-records`
instead (D2a). The run warns once on stderr (D5). For a rename run this
also delivers the warning the `ledger` requirement already asks for and
the source does not write (Context).

**Rejected: skip the file on a library problem.** "Ambiguity is skipped,
never guessed" is about a record that disagrees with the file. Here the
fallback record is checked against the file exactly as an untracked
file's is, so it is no less trustworthy. Skipping would also stop every
annotated artifact on `unrecognised-content` until a reconcile. That
would contradict the `ledger` scenario "An admitted file annotated
afterwards", which requires no question and no work-duplicate report
for it.

**Rejected: report problems as a separate per-file event.** A consumer
would have to join two events by path to know whether a `resolved`
record came from fallback, and the `resolved` event itself would look
like an ordinary resolution. The done-when condition asks for the
opposite. Phase 3 also plans per-outcome sections inside `resolved` and
`skipped`, so the report belongs there too.

**Rejected: treat `unrecognised-content` silently as untracked.** That
is the case the review's replacement finding is about. Saying nothing
at the one moment the library's record and the file part company would
hide it until an applying run or a reconcile writes the new hash (see
Risks).

## D2a. A store that cannot be listed is not an empty store

**Decision.** `store_files` treats only `ErrorKind::NotFound` on the
directory as an empty store, since a library that has recorded nothing
has no store directory. Every other failure is a `StoreFault`, and
reading continues with whatever can be read:

- the directory cannot be listed: one fault whose `path` is the
  directory and whose `message` is the error;
- an entry cannot be iterated: one fault on the directory, since the
  entry's own path is unknown;
- an entry's metadata cannot be read: one fault on the entry's path.

For consultation, the artifact store's faults are unattributable. If
the store has any, a file that no readable record names is
`unreadable-records`, not `untracked`. The event carries `listed:
false` when the directory itself could not be listed, and otherwise the
count of unreadable files.

**Why a distinct kind rather than `untracked` plus a note.** `untracked`
means the library has no authority over the file, and consumers will
read it that way. `unreadable-records` means the library could not find
out whether it has authority. That is the "unavailable library
authority" the roadmap's done-when condition asks to keep separate from
ordinary fallback. A flag on `untracked` would let a consumer that
checks `kind` alone read an unavailable library as a clean miss.

**What else the `store_files` change touches**, read from source:

- `validate`: `item_findings` and `record_findings` map every fault to
  a `library-finding` of kind `unreadable` at the fault's path. An
  unlistable store directory therefore becomes a finding, and `validate`
  exits with the partial-success code where today it reports a clean,
  empty library. This is **in scope** as the direct consequence.
  - The living requirement "Validation defines a well-formed library"
    lists its findings "at minimum", so no spec delta is needed.
  - One test pins it (tasks 1.1).
  - `Finding::Unreadable`'s docstring is widened from "a file" to "a
    file or directory of one of the stores".
- `status`, `reconcile`, `adopt`, and the rename account: none reads
  `faults`. Their counts and decisions over an unlistable store are
  what they are today, computed from an empty store. Reporting that in
  those commands is out of scope.

**Rejected: keep `store_files` and detect an unlistable store in
consultation alone.** Two readers of one directory would then disagree
about whether it is empty, and `validate` would go on certifying a store
it never read.

## D3. The event representation

**Decision.** The new values are these, and every one is an addition.

`resolved` and `skipped` gain `library`, always written:

- `null`: the library was not consulted. Either the run has no library,
  or the file lies outside it or in a subtree it excludes, or the
  resolution ended before the consultation (a content duplicate). It is
  also `null` on every `skipped` event that is not the file's resolution
  verdict (`target-taken`, `declined`, `rename-failed`, `stranding`,
  `unrecordable`, `unnameable`, `unciteable`, `bib-write-failed`,
  `sidecar-taken`). The file's `resolved` event already carries the
  consultation.
- Otherwise an object tagged by `kind`, kebab-case, as `reason`,
  `finding`, `repair` and `admission` are:

```json
{"kind": "tracked", "artifact": "<uuid>", "item": "<uuid>"}
{"kind": "untracked"}
{"kind": "unrecognised-content", "artifacts": ["<uuid>"]}
{"kind": "ambiguous", "artifacts": ["<uuid>", "<uuid>"]}
{"kind": "no-item", "artifact": "<uuid>"}
{"kind": "dangling-item", "artifact": "<uuid>", "item": "<uuid>"}
{"kind": "unreadable-item", "artifact": "<uuid>", "item": "<uuid>",
 "path": "/full/path/items/key.<uuid>.toml", "message": "…"}
{"kind": "ambiguous-item", "artifact": "<uuid>", "item": "<uuid>",
 "files": ["/full/path/items/a.<uuid>.toml", "/full/path/items/b.<uuid>.toml"]}
{"kind": "unhashable", "artifacts": ["<uuid>"]}
{"kind": "unreadable-records", "listed": true, "unreadable": 2}
{"kind": "unreadable-records", "listed": false, "unreadable": 0}
```

Identities are canonical UUID text. `artifacts` and `files` are in store
read order. `path` and `files` are full paths, as
`duplicate.existing_path` is, so a reader can open them. For an
unlistable `items/`, `unreadable-item`'s `path` is the `items/`
directory. `unreadable` counts unreadable artifact-record files, and is
0 when `listed` is false.

`library` reports what the library said, not where the record came
from. The record's origin is `tier`:

| Field | On a library answer | Meaning, unchanged |
|---|---|---|
| `tier` | `"library"` (new value) | where the identifier came from: a pass, `supplied`, `library`, or `null` for the content index |
| `cached` | `false` | whether the content index answered |
| `claims` | `[]` | titles read from the file; the file was not opened |
| `found` | the record's own identifier | the identifier looked up, or the record's own when nothing was looked up, as on a content-index answer |
| `source` | the item's provenance services, or `"library"` (new value) when it names none | the services that supplied the record |
| `overrode` | `null` | a conflict the operator accepted |
| `identifier`, `record` | the item's | the record's preferred identifier, the record |

The two facts can differ. An interactive run whose operator supplies an
identifier for a tracked file reports `tier: "supplied"` and
`library.kind: "tracked"`: the library answered, and the operator
re-identified the file (the `library-admission` `relinked` event
follows, as today).

**Rejected: `tier: null` on a library answer.** `null` means "the
content index answered" in the event documentation and the manual.
`supply-identifiers-interactively` design D6 added `supplied` rather
than reusing `null` for exactly this reason: two cases sharing `null`
cannot be told apart. Reusing it here would change what `null` means,
and that is a meaning change.

**Rejected: `cached: true` on a library answer.** `cached` means the
content index answered. Setting it for the library changes its meaning
and hides the very distinction this change exists to report.

**Rejected: `source: "library"` on every library answer.** `source` is
record provenance. The living requirement already reads it from
per-field provenance when nothing was looked up, and the review asks for
retrieval to be kept separate from provenance. Replacing Crossref with
`library` for tracked files would change what `source` says about the
same record. `show-record-before-asking` design D7 counted exactly that
kind of change as a meaning change. `library` appears in `source` only
where `cache` would otherwise appear: the provenance names no service.

**Rejected: a standalone `retrieval` field now** (`library`,
`content-index`, `lookup`). "Report retrieval as `library`" is met by
`tier: "library"`. A new retrieval field would repeat `cached` and
`tier`. It would lump service-cache and network retrieval into one value,
which Phase 3's `record_retrieval` then has to split, changing its
meaning. It would also fix a Phase 3 name in Phase 1. Phase 3 migrates
`tier: "library"` and `library` into its sections at its own bump.

**Rejected: one `library-status` string instead of a tagged object.**
The problems carry different evidence: the artifact, the item, the item
file and the reader's message. A tagged object is the shape every other
nested enum in the stream already has.

## D4. Schema: an addition under schema 3

**Decision.** `SCHEMA` stays 3. The `cli` requirement "JSON Lines output
is first-class" bumps the version "whenever a consumer that reads the
stream correctly today could read a later stream wrongly: an event or a
reason that is removed or renamed, or a field whose meaning changes",
and not "for an addition, which a consumer that ignores what it does not
know reads unchanged". Against that rule:

- **Nothing is removed or renamed.** No event, reason, field or value
  goes away.
- **Every new thing is an addition.** The `library` field is new. `tier:
  "library"` and `source: "library"` are new values of open string
  fields. `supplied` joined `tier` the same way, and the 0.5.0
  changelog records it among "additions" for which "the schema version
  remains 2". `add-interactive-rename` added the `declined` reason on
  the same terms. A consumer that ignores an unknown value sees a tier
  it does not know, not a known tier with a new meaning.
- **No existing field changes meaning.** D3's table checks each one.
  The value most at risk is `tier: null`, and it keeps its documented
  meaning because a library answer does not use it. `cached` still says
  only whether the content index answered.
- **The same file can produce a different event.** A tracked file that
  used to report `cached: true` now reports `tier: "library"` and
  perhaps a different title. That is a change in what borax decides,
  not in what any field means, and the changelog is where it is
  announced. The rule's test is a consumer that reads the stream
  correctly. Such a consumer reads this event correctly: the content
  index did not answer, the identifier came from the library, and the
  record is the library's.

**Residual risk, stated plainly.** Before this change, `cached: false`
coincided with "an identifier was looked up for this file in this run".
A consumer that treated `cached` as a two-way switch will read a library
answer as a lookup. That inference was never the field's meaning: the
event documents `cached` as whether the content index answered. `tier`
is the field that says how the identifier was reached, and it now says
`library`. The same kind of consumer was already exposed when
`supplied` joined `tier`. Before `1.0.0` a bump is cheap, but a version
that also moves for additions stops telling a consumer when to worry.
Phase 3 already carries the one intentional bump that will migrate this
representation (roadmap Phase 3 exit condition).

**Rejected: bump to 4 now.** The roadmap's Phase 3 gate plans "the
version transition after change 11" and prefers one bump in Phase 3.
Under the per-release rule (`show-record-before-asking` design D7),
bumping here would give 0.7.0 schema 4, and 0.9.0 would need 5 for a
change that is a removal. Consumers would be asked to re-pin twice when
only the second is a break.

## D5. Human rendering

**Decision.** The strings the tests pin:

- A library answer (`tier == "library"`):
  `<path>: resolved <identifier> via <source> (from the library)`.
  `<source>` is the event's `source`, as today. For a provenance-less
  item that is `library`, giving `… via library (from the library)`.
  The stream is what it is, and the line renders it.
- A resolution event whose `library.kind` is a problem: the line it
  would have had, followed by `; the library could not answer: <what>`.
  That covers `resolved` (with ` (cached)` where it applies) and
  `skipped`.
- `tracked` without `tier: "library"` (a supplied re-identification),
  `untracked`, and `null`: the line is unchanged.

`<what>`, where `<artifacts>` is `artifact <id>` for one and
`artifacts <id>, <id>` for several:

| kind | `<what>` |
|---|---|
| `unrecognised-content` | `the library records <artifacts> at this path, but not these bytes` |
| `ambiguous` | `<n> artifact records claim this file: <id>, <id>` |
| `no-item` | `artifact <id> is linked to no item` |
| `dangling-item` | `artifact <id> links to item <item>, which the library does not hold` |
| `unreadable-item` | `artifact <id> links to item <item>, which could not be read from <path>: <message>` |
| `ambiguous-item` | `artifact <id> links to item <item>, which <n> item files claim: <path>, <path>` |
| `unhashable` | `the file could not be hashed, so <artifacts> recorded at this path cannot be confirmed` |
| `unreadable-records`, `listed: false` | `the library's artifact records could not be listed, so it cannot say whether it tracks this file` |
| `unreadable-records`, `listed: true` | `<n> artifact record files could not be read, so the library cannot say whether it tracks this file` (`1 artifact record file` for one) |

Examples:

```text
paper.pdf: resolved doi:10.1039/c5ay00042d via crossref (from the library)
paper.pdf: resolved doi:10.1039/c5ay00042d via crossref (cached); the library could not answer: artifact 0192…-… links to item 0192…-…, which the library does not hold
guide.pdf: skipped, no identifier found; the library could not answer: the library records artifact 0192…-… at this path, but not these bytes
```

No `<what>` names a remedy. `borax reconcile` would be the wrong advice
for `unrecognised-content` today: step 3 accepts whatever bytes sit at
the path, and that acceptance is change 15's to fix.

The unreadable-record warning (`Level::Warning`, stderr, at most once
per run, only for a run that consulted a library whose artifact store
has faults). Exactly one of:

- unlistable: `the library's artifact records could not be listed, so
  no file is resolved from the library; borax validate reports why`
- one file: `1 artifact record could not be read, so a file it records
  is resolved as if the library did not track it; borax validate names
  it`
- several: `<n> artifact records could not be read, so a file one of
  them records is resolved as if the library did not track it; borax
  validate names them`

The interactive description (`describe.rs`):

- `identifier` for a library answer names `found` with no clause.
  `whence("library")` returns `None`: nothing was looked up, and the
  living `rename` rule is to name an origin "only where the run found
  it". Today `whence` would say `from the file`, which is false.
- `record` reads `<services>, from the library`, or `the library` when
  `source` is `library` alone.
- A problem kind adds a line labelled `library` whose value is `<what>`.
  It follows `record` in a resolved file's description, and follows the
  reason's lines in a failed file's (`failure`).
- `file says` reads `nothing read`, as for a content-index answer.

**Rejected: a second human line per problem.** A file's verdict is one
line in batch output. A problem belongs to the resolution it qualifies,
and the rename requirement "A run reports one file at a time" is easier
to keep with one line.

**Rejected: showing the item's file path on a library answer.** It is
not on the event. Adding it would add a field the machine contract does
not need, and the description may show only what the event carries.

## D6. The content index is left alone by a library answer

**Decision.** A library answer neither reads nor writes the content
index. Whatever entry the index holds for the file's hash is left
exactly as it is, stale or not.

This is the least committal choice. Change 13 turns the index into
references to libraries, and change 12 gives a library the identity a
reference needs. Until then the index has one entry per hash. The review
raised the design catch that identical content can belong to several
libraries with intentionally different records. Writing the current
library's record into that one shared entry would decide, by whichever
library ran last, what every other library and every run outside a
library gets. That is Phase 4's policy question. Leaving the entry alone
decides nothing.

A fallback resolution after a library problem writes the index as any
fresh resolution does today. It is an ordinary resolution of those
bytes.

**Rejected: refresh the index entry from the item.** It would repair
the stale copy the review found, which is the repair the review's
"Agreed direction" describes for a run inside a relocated library. But
that paragraph is about index references, which do not exist yet, and
refreshing would make the index a second authoritative copy of library
state, the arrangement change 13 exists to remove.

**Rejected: read the index to compare with the item and report
staleness.** "Comparing records can reveal staleness; it does not
establish authority" (review). Reporting it is evidence work for
Phase 3's sections, and it would cost an index read for every tracked
file to produce a field nothing acts on.

## D7. Commands and flags

**Decision.**

- **`resolve`** consults the run's library.
- **`rename`** consults it in batch and interactive runs, previewing
  and applying, whatever the `record` setting. `--no-record` still
  withholds the account, so neither duplicate check runs, and still
  suppresses every store write. Consulting the library is neither of
  those. The `ledger` requirement frames the gate as governing
  "bookkeeping, not safety". A read of authority is not bookkeeping.
  `resolve` and `bib`, which have no record gate at all, consult the
  library, and a rename under `--no-record` that resolved the same file
  differently from `resolve` would be two authorities.
- **`bib`** consults it. The roadmap names `resolve` and `rename`, but
  `bib` resolves through the same passes (`resolved_record`), and its
  purpose is citing the record. A `bib` that cited the stale content
  index copy while `resolve` showed the corrected item would be the
  same defect in the command whose output is the record. The deferred
  bibliography item in the review (merge and sidecar behaviour) is not
  touched.
- **`--no-cache`** bypasses the response cache and content index as
  today, and does not bypass the library (roadmap: settled).
- **Interactive `rename`.** A library answer is a resolved record like
  any other. The file is asked the question its record calls for: the
  move, or, with `rename.skip-named` off, keep-or-supply for an
  already-named file. With skip-named on (the default), an already-named
  tracked file is passed over. Supplying an identifier re-identifies
  the file and re-links its record, which is the explicit accepted
  operation the library requirement already provides for changing what
  a recorded file is. Accepting a library answer's move writes nothing
  to the content index (`Offer::kept` holds, `remember` stays false).
  No new question kind is added, and a library problem adds no
  question.
- **`adopt`, `status --identify`, `validate`, `reconcile`** are
  unchanged. Adoption works on orphans, which are untracked by
  definition.

**Rejected: `--no-record` also turns off the consultation.** The file's
record would then depend on a setting about writing. The same file would
name itself differently under `rename --no-record` than under `resolve`,
and a person turning off bookkeeping would silently lose the library's
corrections.

**Rejected: leave `bib` for later.** That would be cheaper for this
change. But a later change would have to reach the same resolution call
with the same library, and in the meantime the command that writes
citations would ignore the library's corrections. Including it costs
one argument at one call site.

**Rejected: no question for a library-answered file in an interactive
run.** The question an interactive run asks is about the move, and the
library item says what the file is, not whether it should be moved now.
Skipping the question would move files nobody authorised.

## D8. Which library, read how often

**Decision.** The run's library is `Adapters::collection_root`, as
discovered today. A file is consulted when, after both are normalised
(D1a), `library::library_relative(root, path)` is `Some` and
`library::excludes(root, path)` is false. Any other file gets
`library: null`. The stores are read once per run, before the first
file is resolved. A read never refuses a run: an absent store is empty,
and a fault is D2's. Neither `preflight` nor anything else gains a way
to fail on it.

`resolve_batch` receives the stores by shared reference and hands them
to every worker in `map_bounded`. Resolve never admits anything, so
nothing is learned and nothing needs synchronising. The requirement
"Concurrency does not reorder the stream" holds unchanged.

In `rename`, consultation reads an **immutable snapshot**: the `Stores`
as read before the first file, never the copy the run learns into. The
learning copy is not equivalent. `Stores::take_in` inserts an in-memory
item carrying the admitting record whenever the store lacks the linked
item (Context). Take two artifacts linked to one missing item in one
applying run:

1. The first gets `dangling-item`, falls back, and is admitted. `admit`
   keeps its link and writes no item file, but `learn` puts an item
   carrying the *fallback* record under that identity.
2. Consulted against the learning copy, the second would read
   `tracked`, and the first file's fallback information would be
   promoted to library authority.

Against the snapshot, the second also gets `dangling-item`, as disk
says (tasks 5.3). The account and all its `learn`/`foresee` stay on the
learning copy exactly as today, and only with the `record` setting on.

**Rejected: consult the learning copy.** See the sequence above. It
would also make an answer depend on the order of the inputs.

**Rejected: discover a library per input.** `borax resolve a/x.pdf
b/y.pdf` across two libraries would then consult both. But the run's
library also anchors its run log, its account and its admissions (`cli`
"Configuration resolution order", `library` "A library is one directory
tree"). A second, per-input discovery would be a new notion of "the
library" that nothing else uses. Selecting among several libraries is
the Phase 4 design catch about several references. The roadmap's gate
for this change requires only "the current library found by its
marker".

**Rejected: read the stores per file.** A walk of `items/` and
`.borax/artifacts/` per file is the cost `Stores` exists to avoid.

## D9. No conflict check and no file opened on a library answer

**Decision.** A library answer runs no title comparison and does not
open the file. `claims` is empty.

The conflict check exists to refuse a service's record for the wrong
work, before anyone has accepted it. A library item is past that point:
it was admitted by a run that checked it, adopted from a record some run
accepted, or corrected by a person. The correction case is the one this
change exists for, and there the file's embedded title is what is being
corrected against. Re-judging the item against the file would let a
heuristic veto the library. That breaks the rule the living
`resolution` requirement already states for an operator's accepted
conflict: it is not judged again.

**Rejected: check and skip on conflict.** For the reasons above.

**Rejected: open the file and report its claims without enforcing a
check.** That is evidence for Phase 3's `extraction` and `match_check`
sections, which will say "not attempted" with a reason. Adding it now
would open every tracked file on every run, which costs what the content
index was saving. It would also fill `claims` for a record nothing was
checked against, when the living requirement ties claims to "the
evidence it was checked against".

## D10. The interface the tests are written against

```rust
// crates/borax/src/event.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LibraryAnswer {
    Tracked { artifact: String, item: String },
    Untracked,
    UnrecognisedContent { artifacts: Vec<String> },
    Ambiguous { artifacts: Vec<String> },
    NoItem { artifact: String },
    DanglingItem { artifact: String, item: String },
    UnreadableItem { artifact: String, item: String, path: PathBuf, message: String },
    AmbiguousItem { artifact: String, item: String, files: Vec<PathBuf> },
    Unhashable { artifacts: Vec<String> },
    UnreadableRecords { listed: bool, unreadable: usize },
}
// Event::Resolved { .., library: Option<LibraryAnswer> }
// Event::Skipped  { path, reason, library: Option<LibraryAnswer> }
// Both `#[serde(default)]`: a line written before this change reads
// back with `library: None`. Both always serialized, `null` included.

// crates/borax/src/library.rs
pub struct Consulted {
    pub answer: LibraryAnswer,
    /// The item answered with: `Some` exactly when `answer` is `Tracked`.
    pub item: Option<Item>,
}
/// What the artifact store could not read.
pub struct RecordFaults { pub listed: bool, pub unreadable: usize }
impl Stores {
    /// `None` for a path outside the library or in a subtree it
    /// excludes. Root and path are normalised first (D1a).
    pub fn consult(&self, path: &Path, hash: Option<&ContentHash>) -> Option<Consulted>;
    /// `None` when the artifact store was read whole.
    pub fn record_faults(&self) -> Option<RecordFaults>;
}

// crates/borax/src/pipeline.rs
pub enum Provenance { Extracted(Tier), Supplied, Library } // as_str: "library"
pub struct FileRecord { .., pub library: Option<LibraryAnswer> }
pub struct Standing { .., pub library: Option<LibraryAnswer> }
pub fn standing<C: Cache>(path, documents, sources, index, config,
    account: Option<&Account<'_>>, library: Option<&Stores>) -> Standing;
pub fn resolve_batch<C: Cache>(paths, documents, sources, index,
    library: Option<&Stores>, config, concurrency) -> Run;
/// The event reporting `standing`'s verdict for `path`, carrying its
/// library answer on a `skipped` event as on a `resolved` one.
pub fn verdict_event(path: &Path, standing: &Standing) -> Event;
```

`resolve_file` keeps its signature and means "no library". `event_for`
keeps its signature and writes `library: None` on a skip.
`resolve_supplied` carries the standing's `library` into the
`FileRecord` it builds (the caller passes it), which is how a supplied
re-identification reports `tracked` beside `tier: "supplied"`.

`ArtifactId` and `ItemId` render through `Display`, so `LibraryAnswer`
carries `to_string()` of each.

## D11. The consultation travels with the file through a rename

**Decision.** Whatever path a file takes through `rename_events`, the
`library` answer consulted for it reaches every event that reports its
resolution verdict:

- `alone`: a verdict skip carries `standing.library`. `Settled::Skip`
  gains the answer beside its reason.
- `asked`:
  - the held event it describes is built with `verdict_event`, not
    `event_for`;
  - `skipped()` keeps the held skip's answer;
  - a retry after a service outage carries the standing's answer into
    the retried `FileRecord` and into any skip it produces (a conflict
    on retry);
  - a supplied candidate carries it, as D10 says;
  - an operator's Skip of an unidentified file reports the verdict skip
    with its answer.
- `rename_events` writes the `Skipped` event for a settled skip with
  that answer. Non-verdict skips (`declined`, `target-taken`, …) stay
  `null` (D3).
- The interactive description of a failed file shows the `library` line
  (D5), because it renders the held event, which now carries it.

Task 2.3's rule of initialising existing `Skipped` constructions to
`None` applies only to non-verdict skips. Tasks 5.3 and 5.5 cover the
verdict paths.

## Risks / Trade-offs

- **Unrelated bytes, once written into a history, become the item's to
  answer for.** Reconciliation step 3 ("Edited in place") appends a
  recorded path's unknown bytes to its record. The review watched it do
  this for an unrelated PDF. An applying rename's record update (library
  "An applying run records what it admits") does the same for a file
  whose consultation said `unrecognised-content`: it keeps the item
  link and appends the hash. Before this change, the wrong association
  was latent. After it, the next run serves the old item for the new
  bytes, and a rename would propose the old work's name. This change
  cannot close that without deciding acceptance policy, which the
  roadmap assigns to the Phase 4 discussion and change 15. What it does
  instead is report `unrecognised-content` on the resolution event and
  in the interactive description *before* either writer runs, so the
  moment is visible. Change 15's scope should name both writers.
- **A rollback after a re-link answers with the new item.** D1's
  interim policy counts every history entry, and a hash entry carries
  no item. An artifact re-identified from work A to work B keeps A's
  hash, so A's bytes restored at that path resolve as B. This is
  distinct from the risk above. That one is bytes accepted
  automatically. This one is correctly accepted bytes answered for
  under a link that has since moved. Change 15 and the Phase 4
  discussion own the fix: bind entries to items, or discount entries
  older than a re-link. Tasks 1.1 pins today's answer so the fix
  changes a test deliberately.
- **A moved artifact falls back until reconciled.** Between an
  out-of-band move and `borax reconcile`, a tracked file is untracked
  and can get the stale content-index record. The library capability
  already accepts that interval for recorded paths. A rename run in
  that interval still warns that the library holds paths to reconcile.
- **`--no-cache` no longer refreshes a tracked file.** That is the
  settled direction. The way to ask the services about a tracked file is
  change 14's to design.
- **`resolve` now reads the stores.** One read of `items/` and
  `.borax/artifacts/` per run inside a library. The library capability
  accepts that cost ("the direct implementation is deliberate") and
  `rename` already pays it.
- **Existing tests assume the content index answers inside a library.**
  Task 5.4 audits them. Any assertion changed because the library now
  answers is stated in the commit that changes it.

## Candidate tests carried from the roadmap

The roadmap lists behaviour the review did not exercise. Where it
touches this change it becomes a test in `tasks.md`:

| Roadmap candidate | Test here |
|---|---|
| Unavailable file hash | `unhashable` with a record at the path; `untracked` without (1.1, 3.1) |
| Corrupt library records | unreadable item file and unlistable `items/` (`unreadable-item`); duplicate item identity (`ambiguous-item`); unreadable or unlistable artifact records (`unreadable-records`, warning, others unaffected) (1.1, 4.1) |
| Multi-entry historical rollback | bytes matching an older history entry resolve from the item; after a re-link, from the new item (interim, 1.1, 3.1) |
| Cache-write failure | a library answer writes nothing to the content index (3.1) |
| Operator acceptance of a candidate or conflict | supplying an identifier for a tracked file reports `supplied` and `tracked` (5.2) |
| Service failure followed by success | a retry after an outage keeps the library answer (5.3) |

Service unavailability as such, encrypted PDFs, interrupted writes and
BibTeX export with a destination are not touched by this change. They stay with Phase 3 and Phase 4.
