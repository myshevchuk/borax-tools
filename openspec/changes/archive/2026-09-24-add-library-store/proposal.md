## Why

borax knows what a file is and what to call it. It does not know what
the file *is of*. A collection today is a directory with a marker, a
ledger of what it has admitted, and run logs: accounting about renames,
keyed on content hashes, with no logical work anywhere in it. So a
paper with two PDFs is two unrelated rows, a work the operator wants to
cite but has no file for cannot be named at all, and the only thing
borax can say about a directory it has never seen is how many of its
files it could rename.

The settled architecture for borax manager makes the physical artifact
and the logical item co-equal parts of a library, and it is built on one
constraint: **a library is one directory tree**, with every library file
inside it, artifacts placed freely in its subdirectories. That
constraint dissolves the managed/linked attachment distinction every
other reference manager has to make, and it buys four properties borax
can actually rest on — every path is library-relative, reconciliation is
a bounded walk, a file appearing in the tree is a trigger borax
recognises without being told, and an orphan is well defined.

The acceptance criterion for the whole design is the **Hyperbole test**:
point borax at a directory of two hundred PDFs it has never seen and it
reports what it sees — artifacts, items, what is identifiable — with no
initialization, no import and no ingestion ceremony. Hyperbole makes
existing text addressable without the text having been authored in
Hyperbole's format; borax must make an existing pile of files visible as
a library without the files having entered through borax. Any component
that can see only what borax created fails that test, and this proposal
treats a requirement that would break it as wrong rather than as a
trade.

This change is the foundation the rest of the library work stacks on. It
builds the store and nothing over it: the boundary, the two record kinds
and their two identities, the edge between them, and the three routine
operations that make a library legible — report, validate, reconcile.
There is no index, deliberately, so that the first implementation reads
the files directly and proves that the files rather than a database are
the library.

It also draws a line the shipped tool has never drawn. borax writes a
`.bib` sidecar beside a file when asked to, and `borax ledger rebuild`
reads those sidecars back to reconstruct the collection's accounting —
authoritative state derived from optional output. The artifact record
ends that: a sidecar is citation output for other tools and governs no
decision borax makes, the artifact record is the library's own binding
state, and the two are different files with different readers,
different lifetimes and different authority. The rebuild goes rather
than being repaired.

## What Changes

- **The library is the collection, grown up.** One boundary, one marker,
  one accounting directory: the nearest `.borax.toml` establishes the
  library root exactly as it establishes the collection root today, and
  `.borax/` under it holds the library's state beside the run logs
  already there. There is no second root, no second marker and no
  initialization step — the two names describe one directory, and the
  requirements adopt "library" for it because the boundary now governs
  more than rename accounting. A run with no marker above its input has
  no library root and writes no library state, which is what keeps
  borax useful on a directory nobody has marked. Design D1.
- **Every library path is library-relative, and the boundary is
  lexical.** A path is inside the library when it lies under the root by
  lexical containment, with symlinks not resolved — the policy
  `crates/borax/src/paths.rs` already states and the `ledger` capability
  already depends on. So a symlink inside the tree does not bring its
  target into the library, and a symlink is neither an artifact nor an
  orphan. Whether borax should *change* that policy to make symlinks the
  escape hatch for material that cannot live in the tree is the
  architecture's own open question and is deferred, not settled here.
  Design D7 reports what the search for a same-rank conflict found.
- **A library's tree stops at a nested marker.** A subdirectory holding
  its own `.borax.toml` is a library of its own, and the enclosing
  library excludes it from its artifacts, its orphans, its admissions
  and its reconciliation alike — all four, by one rule, so that a
  library never counts files another library records or lists orphans
  its own commands will not adopt. Design D15.
- **An item is a logical, independently citable work.** It lives at
  `items/<key>.<uuid>.toml` under the library root, carries a minted
  UUID as a field, and may exist with no artifact at all — which is what
  lets the library name a work it holds no file for. References name the
  UUID and never a path, a citation key, a DOI or a node name. The file
  name carries the citation key the `bib-output` capability already
  renders, so the path is legible at the filesystem interface while the
  UUID inside stays authoritative; deleting an item is `rm` and reading
  one is `cat`. The key is a **creation-time label**: borax never
  renames an item file because the templates changed or the record was
  corrected, so a configuration edit is not a diff across the item
  store, and a stale key is not a finding. Design D3.
- **An artifact record is authoritative state, flat under
  `.borax/artifacts/<uuid>.toml`.** It is keyed by a minted artifact
  UUID rather than by a content hash, because annotating a PDF changes
  its bytes and a hash-keyed record would have to be renamed — an
  identity that moves when the content moves is not one. The record
  holds the item link, the hash history oldest-first, the last-known
  library-relative path, and the `(size, mtime)` the fast path compares.
  Each history entry carries the run, the timestamp and the borax
  version that recorded it, which is the admission provenance the
  retired ledger kept a row per admission for — folded into the record,
  so one file answers which run admitted this artifact, when, and under
  which version. A record can outlive its artifact and keep the fact
  that the library once held it, which a record beside the file would
  lose to an ordinary deletion. This is the first authoritative content
  `.borax/` has ever held, and the requirement says so.
- **A record an applying run touched describes the file it names.**
  Updating a record writes the file's content hash into its history when
  that hash is not already the newest, along with the new path and the
  file's size and modification time. Without that rule an update could
  write the path alone and leave a history whose newest hash is not the
  file's — which nothing would ever report, since the fast path would
  settle that record on every later reconcile while the file's actual
  bytes appeared in no record at all. The rule exists because the
  evidence identifying a record is not evidence about its content: the
  fast path compares size and modification time, and a same-length
  in-place edit passes it.
- **Every write to the store replaces a whole file atomically, and a
  failed write preserves what was there.** The bytes go to a temporary
  beside the destination and are renamed over it — what
  `write_atomically` already does — so an interrupted write leaves the
  record's identity, its whole hash history and its item link exactly as
  they were, and a reader never sees a partial file. That matters more
  here than for a cache: a hash history is evidence, and re-resolving a
  file cannot reconstruct the hash it had two annotations ago. Where an
  admission writes two files, the item is written first, because an
  interruption then leaves an item nothing links to — an ordinary
  library state — rather than an artifact record naming an item that
  does not exist. borax retries nothing within a run. Design D12.
- **The edge points from artifact to item, and the two records are
  separate files.** The artifact record names its item; an item names no
  paths, so a moving physical thing cannot make the logical record
  stale. All three of the architecture's independent reasons for the
  separation are carried into the requirement: the edge belongs with
  artifact-specific state, items must exist when there is no file, and
  no one of several artifacts may become the privileged item record
  merely by being a file.
- **`borax status` reports a library with no ceremony.** Artifacts,
  items, artifact records and orphans, counted from the tree and the
  store without opening a document. `--identify` adds the identifiable
  count by extracting from each artifact, which costs a pass over the
  files and so is asked for rather than assumed; it queries no service.
  A directory of two hundred unseen PDFs therefore reports two hundred
  artifacts, no items and two hundred orphans immediately, and
  `--identify` is what turns that into "173 identifiable". Design D5.
- **An orphan is a worklist item, not an invalid state.** An artifact in
  the tree with no artifact record is counted and listed, and it is not a
  validation finding. A freshly pointed-at library of two hundred
  orphans is well formed and validates clean, which is the Hyperbole
  test stated as an invariant rather than as an aspiration.
- **Validation is where the invariants live.** `borax validate` reads the
  store and reports findings — an artifact record naming an item that
  does not exist, two records carrying one identity, a record whose
  identity disagrees with its file name, a path that is not
  library-relative, an empty or malformed hash history, a `.toml` in the
  item store that is not an item record. It repairs nothing and refuses
  nothing: every writer, borax's own CLI included, is judged by it
  rather than gated on it. No invariant depends on holding continuously,
  and no advisory lock is a precondition for correctness, because a file
  manager will not take one. Findings exit with the existing
  partial-success code.
- **Reconciliation repairs a stale path by a bounded walk, with a stated
  precedence and an explicit refusal to guess.** `borax reconcile`
  compares `(size, mtime)` against each record first and hashes only
  what does not match, so an unchanged library is not rehashed. Then
  four ordered steps, each completed across every record before the
  next: settle the records whose own path still holds their artifact and
  claim those files; match the rest against the unclaimed files by any
  recorded hash, current hash preferred, repairing a record with exactly
  one uncontested candidate; treat a still-unresolved record whose path
  holds an unclaimed file matching nothing as its artifact edited in
  place and append the new hash; leave the rest alone. The order is what
  makes two files that swapped paths both repair rather than each being
  mistaken for the other edited in place. **An ambiguous match preserves
  the record** — more than one candidate, or a candidate another record
  also wants, and the path, the history and the item link are left
  exactly as they were and the ambiguity is reported naming the
  candidates. Reconcile never gives one artifact's identity or item link
  to another file: a stale path is repaired by the next pass and a wrong
  link is not detectable at all. **Confirming or repairing a record
  refreshes its recorded `(size, mtime)`**, so those fields describe the
  file as borax last saw it — including after a `touch`, where the fast
  path misses, the hash confirms the record and the fields are the only
  thing that was wrong. Without that, a touched artifact's record would
  be confirmed forever and corrected never, and anything deriving from
  those fields — the filesystem view the next change in the stack builds
  on this store — would treat that artifact as unconfirmed for the life
  of the library. Reconcile creates no record and deletes none —
  adopting an orphan is not its job, or there would be no orphans — and
  a reconcile over a library nothing has touched writes no library
  state. Design D6.
- **A second artifact of a work you hold is asked about, and only where
  there is somebody to ask.** Both duplicate levels and both reasons
  survive. A batch run skips a work duplicate exactly as it does today.
  An interactive run puts it to the operator — file it as another
  artifact of that item, skip, or quit — naming the item and a path
  already recorded against it; accepting returns the file to planning,
  where the ordinary move question follows. borax cannot tell a scan
  beside its published PDF from an unwanted re-download, because the two
  differ in intention and not in bytes, so the run with an operator asks
  and the run without one leaves the file where it is. This is the split
  `resolution`'s "Ambiguity is skipped, never guessed" already makes for
  a title conflict, which an operator and only an operator may accept in
  an interactive run. The check also passes over the item the incoming
  file's *own* record links to — without that, every artifact of a
  multi-artifact item would meet its siblings as a work duplicate on
  every run, and a batch run would skip each of them for belonging to
  the item it belongs to. Design D16.
- **The ledger is retired outright, and no migration is owed.**
  `.borax/ledger.jsonl` is neither read, written, rebuilt nor deleted.
  Duplicate detection asks the artifact store, and gets better answers,
  because hash history recognises a file that was annotated since it was
  admitted and the item link is a work identity where the ledger had
  only the identifiers of one record. `borax ledger rebuild` goes with
  the file. A library that predates the store starts with an empty one:
  borax is before `1.0.0` and promises no compatibility, and `CLAUDE.md`
  prefers replacing an obsolete format cleanly over a shim or a dual
  reader. The old file is left where it is — borax's own accounting, not
  a user's document — and may be deleted by hand. What that freedom does
  *not* reach is user data: the contract that borax moves nothing
  without an explicit decision, never overwrites and never deletes a
  file a user owns is untouched here. Design D2.
- **`borax adopt` records what the library already holds, offline.** For
  each orphan it hashes the file and asks the content index; where the
  index holds the record borax resolved for those bytes, it writes an
  artifact record and links it to an item it finds or mints. It queries
  no service, extracts from no document, moves, renames and deletes
  nothing, invents nothing, is idempotent, and leaves an artifact that
  already has a record untouched — including its item link. The library
  state it writes is those records and items and nothing else; its run
  log is written as every subcommand's is, which the requirement says
  rather than leaving "writes nothing" to be read against a capability
  that makes the run-log pair operative everywhere. An artifact
  the index cannot answer for stays an orphan. It reads neither a
  citation sidecar nor the retired ledger. This is the only way to
  populate the store that cannot move a file, which is what distinguishes
  it from `rename --apply`; its source is a cache, which the requirement
  says out loud rather than promising a recovery it cannot give.
  Design D13.
- **An applying run records what it admits.** An applying `rename` run
  writes an artifact record for each file whose fate it settled with a
  record in hand — moved or already named — minting the artifact UUID
  and linking it to an item the library already holds for that record's
  identifiers or to one it mints. A preview writes none, so the preview
  contract is untouched, and `--no-record` writes none either. **An
  artifact that already has a record keeps its item link on an ordinary
  re-run**: selection happens when a record is minted, so re-running
  over a recorded file that resolves to no identifier cannot mint a
  second item and move the link to it, run after run. **An operator's
  own answer is the exception**: an identifier they supply, or a record
  they accept over a conflict, re-links that artifact to the item for
  the record so identified, because correcting a file's name from a
  supplied identifier is correcting what the file is, and a name and a
  library link that disagree is the state worth ruling out. The run
  reports both items. Design D4.
- **An item with no artifact takes the next matching file as its
  first.** A work the library cited before it had the PDF is the case an
  artifactless item exists for, so the file that finally supplies it is
  admitted and linked to that item rather than diverted. A work
  duplicate accordingly requires an item with an artifact whose recorded
  path still holds a file — there has to be a file for the incoming one
  to be a duplicate *of*, and the report and the interactive question
  both have to name it. An item whose artifacts are all absent is the
  same case, and the run says there are paths to reconcile.
- **`--no-ledger` becomes `--no-record`, and the gate governs
  bookkeeping rather than safety.** A run under it writes no artifact
  record and no item and makes no duplicate check. It still asks one
  question of the store: whether a record names the path the file now
  occupies, and whether that record's history holds the file's hash. If
  it does, the artifact moves freely — the path goes stale and reconcile
  repairs it by that hash. If it does not, the artifact is **not
  moved**, because a move without the post-move write would leave a
  record naming a path that holds nothing and carrying no hash matching
  anything, the one state reconcile cannot repair. The fast path is not
  accepted as evidence here, though reconcile accepts it: it compares
  size and modification time, which a same-length in-place edit
  preserves, and what it cannot see is exactly what would be lost. The
  hash costs no extra read — `rename` already requires an applied move's
  run-log event to carry the file's content hash and to be flushed
  before the move. Turning the account off asks borax to keep no record of this
  run, not to damage the records it already has. The flag also loses a
  name pointing at a file this change deletes. The configuration key
  `ledger` becomes `record`, and before `1.0.0` there is no deprecation
  window to need. The `ledger` *capability* keeps its name, not by
  choice: a capability rename is a
  `REMOVED` block emptying the old one and an `ADDED` block filling a
  new one, and `openspec archive` refuses to leave a capability with no
  requirements — it aborts, having already created the new capability
  directory, and reports "No files were changed" while a new spec sits
  on disk. That was tested against a copy of this tree rather than
  assumed. Inside the capability the one requirement whose title named
  the ledger is renamed by removal and addition, which both tools
  accept. Design D14.
- **borax does not bring a file into the library.** Neither copy nor
  move: a file named as input from outside the tree is resolved and
  renamed where it sits, exactly as today, and gets no artifact record
  and no item. Relocating a file into a library is the operator's `mv`
  in whatever tool they like, and the file appearing in the tree is the
  trigger borax then recognises. A run that renames a file outside the
  library says so rather than silently recording nothing. Design D8.
- **The text is authoritative and there is no index.** Every answer in
  this change is read from the files: no SQLite, no FTS, no cached
  listing. Deleting the response cache or the content index changes no
  answer about the library's contents, and the cost of reading the store
  directly is accepted deliberately as the proof that the files are the
  library.

The event schema version rises from 2 to 3. Almost everything here is an
addition, which the `cli` capability states a consumer reads unchanged —
but `ledger rebuild` and its `ledger-rebuilt` event are withdrawn, and a
removal is exactly what the version exists to announce.

## Capabilities

### New Capabilities

- `library`: the boundary and its root, items and their identity and file
  names, artifact records and theirs, the artifact-to-item edge and why
  the records are two files, what an artifact and an orphan are, `borax
  status`, `borax validate`, `borax reconcile`, `borax adopt`, the rule
  that borax brings no file into the library, the rule that an applying
  run records what it admits, and the rule that the text is read
  directly with no index. Thirteen requirements, which is the size of
  the foundation rather than a change that should have been two —
  design D9 records the splits considered and why each is worse.

### Modified Capabilities

- `ledger`: the capability keeps its name and its question — what has
  this library already admitted — and changes where the answer is read
  from. Duplicate detection at two levels is amended to ask the artifact
  store and to stop treating a work match as a duplicate; "duplicates
  are skipped, never destroyed" is amended to scope the skip to content
  duplicates while keeping its guarantee whole; staleness becomes a
  last-known path reconcile repairs rather than an entry to distrust;
  and the "optional and degrades loudly" requirement is amended because
  authoritative state does not degrade the way derived accounting did
  and because the setting gating it is now `--no-record`. Two
  requirements are removed with reasons and migrations: the append-only
  JSONL file, and the rebuild — the second naming `borax adopt` and
  `borax reconcile` as the two halves that replace it.
- `cli`: the subcommand list loses `ledger` and gains `status`,
  `validate`, `reconcile` and `adopt`; configuration resolution says the
  nearest `.borax.toml` establishes the library root; the per-subcommand
  setting surface names what the four new subcommands accept.
- `rename`: three requirements. The interactive-move one stops saying
  the collection's ledger admits the file and says the artifact record
  does. "An interactive run asks about files it could not settle" gains
  a fifth situation — a file resolving to an identifier an item already
  carries — with the choices it is offered, and its sentence "A file
  reported as a duplicate … SHALL NOT be asked about" is narrowed to a
  *content* duplicate, which is the living requirement this change has
  to amend deliberately rather than contradict in practice. The
  already-named requirement — whose whole point is that a record, not an
  account of past runs, decides the name — says the same about the
  artifact store and gains the record write an applying run now makes
  for such a file.
- `bib-output`: one added requirement, stating what a sidecar is and
  what it is not. It is derived citation output, borax never reads one
  to decide what a file's record is or what name it is given, and it is
  not an artifact record — with a table of the differences, because a
  per-file `.bib` beside a PDF and a per-artifact TOML under `.borax/`
  are the two things a reader of this proposal is most likely to
  conflate. Nothing about where a sidecar goes, what it contains or
  that it is off by default changes.

`run-logs` takes no delta: run logs keep their place under `.borax/runs/`
and their pre-flush rule, and nothing here changes what a run log
records or when. `resolution` takes none: what a file resolves to is
unchanged, and an item is minted from a record rather than being a new
way to get one. `record-model` takes none: an item carries the canonical
record as it stands, and `collection-title` is somebody else's change.

## Impact

- `crates/borax-core/src/library.rs` (new): the values — `ItemId`,
  `ArtifactId`, `Item`, `ArtifactRecord`, their TOML serialization, and
  the pure predicates validation is made of. Values in, values out, the
  way `borax_core::ledger` is; minting a UUID needs entropy and a clock
  and therefore belongs to the adapter.
- `crates/borax-core/Cargo.toml` gains `toml`, which the workspace
  already pins at 1.1 and only `crates/borax` uses today. Parsing and
  rendering TOML is pure text, so it does not breach the crate's no-I/O
  rule.
- `uuid` is a new workspace dependency (`v7`, `serde`), and with it
  `getrandom` — borax's first entropy source: `temporary_name` in
  `crates/borax-sources/src/store.rs` deliberately uses a pid and a
  counter instead. A minted identity has to stay unique across two
  machines whose libraries are merged through Git, which a pid and a
  counter do not. Design D10.
- `crates/borax/src/library.rs` (new): the store on disk — reading and
  writing item files and artifact records, the artifact walk with its
  nested-marker stop, the `(size, mtime)` fast path, the bounded
  reconciliation walk with its four steps and its ambiguity rule, the
  adoption sources, and the minting of both identities. Field-level
  editing through `toml_edit`, which is what lets a reconcile that
  changes nothing leave every file byte-identical, composed with
  `write_atomically` from `crates/borax-sources/src/store.rs`, which is
  what makes every write a whole-file replacement.
- `toml_edit` is a new workspace dependency, and the only one this
  change adds beyond `uuid`. It is `toml` 1.x's sibling over the same
  parser, so nothing new is compiled beyond the document model itself.
- `crates/borax/src/ledger.rs` is deleted along with
  `crates/borax-core/src/ledger.rs`. The duplicate checks move onto the
  artifact store; `Collection`'s path comparison, `relative_to` and
  `collection_relative` move to the library module, which needs all
  three for the same reasons.
- `crates/borax/src/run.rs`: `admit` writes an artifact record instead of
  appending a ledger entry, and is reached by an already-named outcome as
  well as by a move. The `Ledger` command arm goes; `Status`, `Validate`
  and `Reconcile` arms arrive. `inputs` and `documents` are the artifact
  walk's basis and gain a library-relative form.
- `crates/borax/src/cli.rs`: four subcommands, `status --identify`,
  `reconcile --rehash`, and the flag surface each one declares.
- `crates/borax/src/event.rs`: `library-status`, `finding`, `validated`,
  `reconciled`, `adopted` and `record-written` events with their human
  and JSON renderings, the last of them naming both items when an
  operator's re-identification moved an artifact's link; the
  work-duplicate reason carries the item as well as the existing path,
  so the question's description can name it without the description
  knowing more than the stream does;
  `ledger-rebuilt` is withdrawn and the schema version becomes 3. The
  work-duplicate reason itself stays, which is what keeps a batch run's
  report identical to today's.
- `crates/borax/src/config.rs`: `collection_root` is renamed to
  `library_root` and the `collection-root` configuration key to
  `library-root`; the `ledger` key and its `Config` field become
  `record`, with the `OPTIONS` row and the `--record`/`--no-record` pair
  following. Discovery itself is unchanged, and `borax config` reports
  both new keys. Neither old spelling is accepted: before `1.0.0` a
  configuration file naming one fails at load as any unknown key does,
  which is the existing behaviour rather than a new refusal.
- `crates/borax/tests/ledger.rs` is rewritten as `library.rs`, and every
  test asserting a ledger append asserts an artifact record instead.
- Documents a person reads: `docs/manual.org` (a section on the library
  and its boundary, `borax status`, `borax validate`, `borax reconcile`,
  `borax adopt` and when to run it, the item and artifact record
  formats and how neither is a `.bib` sidecar, what `--no-record`
  suppresses, and the removal of `borax ledger`), `README.md`, and
  `CHANGELOG.md` — where the ledger's retirement with no migration, the
  `library-root` and `--record` renames, and an interactive run now
  asking about a work duplicate are the breaking notes — and
  `openspec/STATE.md`.

## Deferred

- **Views, nodes, memberships and cardinality.** The whole organizational
  half of the architecture: the `constraints` array, a node's optional
  `item` field, versions and containers as views, the filesystem-backed
  view, and the derivation of an item's position from its artifacts'.
  This change gives them the store to sit on and says nothing about
  them; one item, one artifact record and one edge is the whole model
  here. The next change in the stack is where they arrive.
- **Template filing as view assignment.** The renamer already renders
  subdirectories from the library root, and under the architecture that
  operation writes the filesystem view. Unifying the two needs views to
  exist first, so the filing requirement in `rename` is left exactly as
  it reads and this change takes no delta against it.
- **SQLite and FTS5.** Named as later work and gated on measurement. This
  change's requirement that the text be read directly is what the
  measurement would be taken against, and deleting an index that does
  not exist yet is trivially a non-event.
- **Title search, smart views, `collection-title` on `Record`.** Each is
  its own change, and the first two are ahead of this one in the
  architecture's build order rather than part of it.
- **Whether symlinks become the escape hatch.** The architecture's open
  question, untouched here. This change specifies the boundary the
  standing path policy already implies and no more; changing the policy
  would amend both the boundary requirement and the `ledger`
  requirement that depends on symlinks not being resolved, which is a
  change of its own with the `ledger` capability's duplicate detection
  in its blast radius. Design D7.
- **How a read-only external corpus participates.** The same open
  question from the other side: material on a network share that cannot
  live in the tree is a counterexample to the model, and the model does
  not yet answer it. Naming an outside file as input still works; what
  it does not do is make that file part of a library.
- **A command that creates an item, or links an artifact to one by
  hand.** Two commands write the store — an applying `rename` run and
  `borax adopt` — and both write only what borax already knows about a
  file it can see. Neither can make an item for a work with no file at
  all. An operator who wants one writes the TOML, which the text store
  is designed to make possible, and a subcommand for it is worth having
  once views exist to file it into.
- **Merging two items for one work.** A record with no identifier mints
  an item every time it is admitted, so two copies of an
  identifier-less manuscript become two items. That is honest about
  what borax knows rather than a match it guessed, and merging them is
  a change with its own surface.
- **Adopting an orphan borax knows nothing about.** `borax adopt`
  records what the content index can answer for and leaves the rest as
  orphans; `reconcile` adopts nothing at all, since a library with no
  orphans has no worklist. Resolving and recording a file borax has
  never seen is `rename --apply`'s job, and doing it in bulk without
  renaming — identify a shelf of files, record them where they sit —
  wants the title search that is ahead of this change in the build
  order.
- **Emitting an item with no artifact into a bibliography.** An item
  holds the whole canonical record, and `borax bib` takes files: it
  resolves each input and emits from that, so a work the library has no
  file for has no way out to a `.bib`. The representation is there and
  the operation is not. What it needs is `bib` able to take the item
  store as its input — a selection syntax, and a decision about whether
  an item and an artifact of it may both contribute an entry — which is
  its own change. The `library` requirement says this in terms rather
  than promising a citability nothing delivers.
- **Renaming the `ledger` capability.** Done for the flag and the
  configuration key, and not done for the capability, because
  `openspec archive` cannot empty a capability and aborts half-applied
  when a delta asks it to (design D14 has the transcript). So one
  directory under `openspec/specs/` keeps a name that describes a file
  borax no longer writes. The honest names are `admission` or
  `accounting`. Lifting it wants either tooling that performs the
  directory move or a hand-edit of the living specs outside an archive,
  and it is recorded here so the stale name reads as known rather than
  as an oversight.
- **Filing a second artifact from a batch run.** Design D16 leaves this
  to an interactive run, and a library curated entirely through
  `--apply` therefore cannot acquire a second artifact of a work it
  already holds. Neither of the obvious overrides is available: a
  configuration key is forbidden by `cli`'s "The apply gate is never
  configurable", and a command-line selector would be the third explicit
  decision `openspec/project.md` says there is not. If this limit bites
  in practice, the change that lifts it is a change about what `--apply`
  authorises, which is a larger subject than this one.
- **A coincidental fast-path match deflecting a content search.**
  Reconciliation settles a record in step 1 when the file at its
  recorded path matches its recorded size and modification time, and a
  settled record claims that artifact, taking it out of the pool step 2
  searches by hash. So a record whose recorded metadata happens to match
  some other file can claim a path once that file arrives there — after
  a move, say — and a third record that would have been repaired to that
  file by content finds nothing to match. The move rule is not where
  this lives and constraining it would not close it; the cost is in
  settling a record on metadata alone, which is exactly what buys not
  rehashing a library on every pass. It is bounded and visible rather
  than silent: no hash evidence is lost, so a later pass can still match
  by content once the coincidence has gone, and meanwhile the
  unrepaired record is reported as an artifact borax has a record of and
  cannot find. Closing it means changing what step 1 accepts as
  confirmation, which is a change about reconciliation's matching.
- **Sweeping an item nothing links to.** An interrupted admission of a
  file carrying no identifier can leave one, and so can deleting an
  artifact record by hand. It is indistinguishable from an item the
  operator wrote for a work they have no file for, so deleting the
  second in order to tidy the first is not a trade this change makes:
  `borax validate` counts them and reports neither as a finding.
- **Artifact types beyond PDF.** An artifact is a file whose extension
  borax recognises as a document, and that set holds one entry today,
  exactly as the input walk does. Widening it is not the one-line edit
  an earlier draft of this proposal called it: the extension decides the
  artifact count, the orphan worklist, what gets hashed, what
  reconciliation covers and what `status --identify` tries to extract
  from, and the last of those needs an extractor that can read the
  format at all. It is a change with its own surface, and naming it
  cheap would have understated it.
- **What triggers a reconcile.** The architecture leaves this open and so
  does this change beyond making `reconcile` a command: whether a
  `rename` run should reconcile first is a question about ordering and
  cost that wants measurement, and nothing here depends on the answer.
- **Repairing `same_name` for case-insensitive volumes.**
  `crates/borax/src/paths.rs` compares names case-sensitively on every
  Unix target, macOS included, so it answers wrongly on a
  case-insensitive APFS volume. The library boundary compares paths with
  that same function and inherits the gap. Fixing it reaches collision
  detection and already-named reporting too, which is a change with its
  own surface and its own tests.
- **The orphaned-sidecar defect.** `openspec/STATE.md` records it: a
  rename writes the new sidecar and leaves the old one beside the
  vacated name. This change settles what a sidecar *is* — derived
  output, load-bearing for nothing — which is what the defect was
  blocked on, and fixes nothing about it. The orphan is borax's own
  output, still correct about the record and wrong only about a path
  nothing occupies. It stays recorded as a known defect.
