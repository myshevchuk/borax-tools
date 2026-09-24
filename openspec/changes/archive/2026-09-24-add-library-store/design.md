# Design: add-library-store

The architecture settles the model: the boundary, both identities, the
edge's direction, the flat artifact record, the fast path, and
validation as the home of the invariants. It leaves open the decisions
below, and a store cannot be built without them. Each is settled here
with the alternative it beat.

One question is deliberately *not* settled: D7 records what the search
for a same-rank conflict about symlinks found and what remains open.

## D1. The library is the collection, grown up

**Decision.** One boundary under two names. The nearest `.borax.toml`
above an input establishes the library root, exactly as it establishes
the collection root today; `.borax/` under that root holds the library's
state beside the run logs already there; library paths are
root-relative and `/`-separated, as ledger entries already are. There
is no second marker, no second root and no initialization step. The
requirements adopt the word "library" because the boundary now governs
items and artifacts and not only rename accounting, and the
`collection-root` configuration key becomes `library-root`.

**Rejected: a new concept beside the collection** — a
`.borax-library.toml` marker, or a `library-root` key discovered
independently of the collection root. It fails twice over. A directory
could then have two roots that disagree, so the ledger's accounting and
the artifact store could be anchored at different levels of one tree and
every library-relative path would have to say which root it was relative
to. And discovering a library separately from a collection means a
directory that is one but not the other, which is an initialization step
wearing a different hat — the Hyperbole test rules it out.

**Rejected: a pure rename, with the concept unchanged.** The collection
today is a scope for rename accounting: a root, a ledger, run logs.
The library is a scope for the library's contents, and the difference is
not cosmetic — `.borax/` gains its first authoritative content, the root
becomes the thing item and artifact paths are relative to, and an
artifact walk of the tree becomes a defined operation. Calling that a
rename would understate what a reader has to learn.

**Consequence worth stating.** A run with no marker above its input has
no library root, and writes nothing: no artifact record, no item. That
is exactly what the ledger does today ("No collection root" — nothing
read or written), and it is what keeps borax useful on an unmarked
directory. Reading needs no root either, because `borax status` takes
the directory it was given as the root of what it reports. So the
marker buys writing, not seeing, which is the division the Hyperbole
test asks for.

## D2. The artifact record subsumes the ledger, in this change

**Decision.** `.borax/ledger.jsonl` is retired. borax neither reads,
writes nor rebuilds it, and the artifact store answers the two questions
it answered:

- a **content** duplicate is an incoming hash that appears anywhere in
  some artifact record's hash history;
- a **work** duplicate is a resolved identifier already carried by an
  item the library holds, reached through the records linked to it —
  with D16 deciding what each kind of run does about one.

`borax ledger rebuild` is withdrawn; `borax reconcile` brings records
back into agreement with the tree and `borax adopt` records offline
what the content index can answer for. The capability keeps the name
`ledger`, which is now wrong in the letter and right in the question it
asks — D14 records why the rename is not expressible.

**Rejected: the artifact record beside the ledger.** Two per-file stores
that can disagree, which is the worst available outcome and was named as
such before this proposal was written. They would disagree on the first
out-of-band move: the ledger's recorded path goes stale with no
mechanism to repair it, while reconcile repairs the artifact record. A
duplicate check reading both would then have to decide which to believe,
and there is no principled answer — one is authoritative and one is
derived from an admission that has since moved.

**Rejected: the ledger as a derived projection of the artifact store.**
It keeps the JSONL shape for whoever might be reading it, and it is
still a second file to keep in step, still a thing a reader can find
disagreeing with the store, and still a rebuild to implement. Nothing
consumes it: the JSONL *event* stream is the declared public contract,
and the ledger is not part of it. `CLAUDE.md` prefers replacing an
obsolete format cleanly over a dual reader, and `0.y.z` promises no
compatibility.

**Rejected: retiring the ledger in a change after this one.** It is
smaller to review and it is the interval in which both stores exist and
both are written, which is the thing being avoided. A proposal cannot
recommend the worst outcome as a transitional state and then argue that
the transition makes it acceptable.

**No migration is owed.** borax is before `1.0.0` and promises no
compatibility, and `CLAUDE.md` prefers replacing an obsolete format
cleanly over a shim, a dual reader or a legacy branch. A library that
predates the artifact store therefore starts with an empty one. Nothing
reads `.borax/ledger.jsonl`, nothing writes it, and nothing deletes it:
it is borax's own accounting rather than a user's document, so leaving
it is courtesy and removing it by hand is the operator's to do. The
freedom stops exactly where `CLAUDE.md` stops it — a user's PDFs are
not disposable, and nothing here touches the contract that borax moves
nothing without an explicit decision, never overwrites and never
deletes a file a user owns.

**Cost, stated plainly.** Until a library's store is populated it has no
account, so both checks answer nothing: a re-downloaded identical file
is admitted a second time. Two commands populate it — `borax rename
--apply` over the library, which records every file it settles, and
`borax adopt`, which records offline whatever the content index still
answers for. The window is real for anyone who runs neither, and that
is the whole of the cost.

## D3. An item file is `items/<key>.<uuid>.toml`

**Decision.** Item files live under `items/` at the library root, named
`<citation-key>.<uuid>.toml`: the citation key the `bib-output`
capability already renders through the `citation-keys` templates in
force **when the file is created**, folded by the public `slug` fold,
then a full UUID, then `.toml`. The `id` field inside is authoritative;
the name is legible; the key is a label, not a maintained field. An item
whose key renders empty at that moment is named by its UUID alone.

**Why the citation key.** It is a name the operator already sees in
their `.bib`, produced by machinery that already exists and is already
specified, and it sorts the way a bibliography sorts. Inventing a second
human-facing name fold for item files would be a second thing to
specify and keep in step with the first.

**Rejected: a bare UUID filename.** The architecture rejects it in
terms: if the filesystem is a first-class interface, a file name has to
carry enough for a human to act on, and `018f2b36-….toml` does not.

**Rejected: a truncated UUID, as in the discussion's
`milner1978-type-polymorphism.018f2b36.toml`.** Shorter and prettier,
and it needs a collision rule that the full form does not: two items
sharing eight hex digits are two files competing for one name, and the
rule for breaking the tie would have to be specified, implemented and
tested. Worse, a truncated identity in the name is ambiguous about which
item the file is, which invites exactly the mistake of treating the name
as the reference. The full UUID is unique by construction and visibly
not an abbreviation.

**Rejected: the key alone, with the UUID only inside.** Two items can
render one key, so the name is not unique, and a name that is only the
key would have to change whenever the metadata did.

**The key is a creation-time label.** It is rendered once, when the file
is written, and never maintained afterwards: borax does not rename an
item file because the templates changed, because the record was
corrected, or because the key now renders differently. An earlier draft
of this entry claimed the UUID suffix makes the whole pathname stable,
which it does not — a suffix makes a name *unique*, and nothing about
one stops a rule from rewriting the part before it. Stability comes from
the rule, and the rule is that borax does not rename. Enforcing the name
against current configuration would be the opposite: it would rename
files whose bibliographic content never changed, turning one
configuration edit into a diff across the whole item store.

So a stale key is not a finding. It was a legible handle when it was
written and that is the whole of its job; the reference is the UUID, and
finding an item is a read of the files.

**Consequence.** borax finds an item by reading item files and trusting
the `id` field, never by parsing the name. So an operator may rename an
item file to anything and borax still finds the item; a name whose
*UUID* disagrees with the `id` inside is a validation finding, because
the two disagreeing is a mistake worth reporting even though the field
wins.

## D4. An applying run records what it settles, already-named files
included

**Decision.** An applying `rename` run writes an artifact record for
every file whose fate it settled with a record in hand — one it moved,
and one it found already named — minting the artifact UUID, and linking
it to the item the library already holds for one of that record's
identifiers or to an item it mints. A preview writes no library state;
its run log is another matter, and `run-logs` requires one by default.
The only other command that writes the store is `borax adopt` (D13),
which
records offline and moves nothing; `status`, `validate` and `reconcile`
write no record that did not already exist.

**An artifact that already has a record keeps its item link, unless the
operator re-identified the file.** Item selection happens when a record
is *minted* and not when one is updated by an ordinary re-run. Without
that much, selecting the item through resolved identifiers alone has a
failure that compounds: a recorded file carrying no identifier matches
no item, so a re-run mints a second item and moves the artifact's link
to it, and the run after that mints a third — detached items
accumulating behind one file nobody touched.

**But an operator's own answer is not an ordinary re-run, and an earlier
draft of this entry made the exception unreachable.** It said deliberate
re-identification was unavailable in this change, while `rename`'s
living "Re-identifying a named file" scenario keeps exactly that
workflow: `--no-skip-named`, the operator supplies the right identifier,
the file is renamed from the right record. Run against a file that
already has an artifact record, the unconditional rule corrected the
name and the cached record and left views and work-duplicate detection
using the old item — a file whose name says one work and whose library
link says another, with nothing reporting the disagreement.

So a supplied identifier, or a record accepted over a conflict,
re-links that file's artifact record to the item for the record so
identified. The signal is provenance, which `resolution` already keeps:
an operator's answer is distinguishable from an extraction's find, and
correcting a file's name from a supplied identifier is correcting what
the file *is*. The name and the link move together or neither does. The
anti-accumulation property survives untouched, because an ordinary
re-run supplies no identifier and therefore still re-links nothing.

**Cost.** The item the artifact left may end up with nothing linking to
it. That is an ordinary library state — indistinguishable from an item
written for a work with no file — so `borax validate` counts it and
reports no finding, and sweeping such items stays deferred.

**Why already-named files too.** A tidy library re-run reports every
file already named and moves nothing, so a rule that recorded only moves
would never record the libraries that are in the best order.
"Admitted" means borax has a record of this artifact and agrees with its
name, which is what an already-named outcome is. This is a *route* into
the store and not *the* adoption route: D13's `borax adopt` is that, and
an earlier draft of this entry leaned on `rename --apply` for a job it
cannot do — it may query services, it cannot admit a file it fails to
resolve, and it renames under whatever templates are in force.

**Rejected: a preview writes records too.** It would populate the store
without `--apply`, and it contradicts the preview contract. It would
also walk into a contradiction that is already in the tree and is not
this change's to settle: the living `rename` requirement "Preview is
the default; apply is explicit" says a batch run by default "print[s]
the planned old→new mapping and mutate[s] nothing on disk", while
`crates/borax/src/run.rs` cites whenever a master `.bib` path or
sidecar output is configured, apply or not — so a preview already
writes bibliography output. Two more living requirements pull the same
way: `run-logs` requires a preview to write a run log, and `rename`
itself pairs a *planned* rename with a written sidecar. Restoring the
literal reading means settling what "mutate nothing" excludes, which
nobody has. This change archives neither half: its rule is that a
preview writes no artifact record and no item, which is true under the
present behaviour and under any restoration of it.

**Rejected: minting an item only when an identifier is present.** It
would leave a resolved file with no work identity precisely in the case
`supply-identifiers-interactively` exists for — the patent, the author
manuscript, the scan. An item's identity is its minted UUID and
identifiers are optional, so a record with none gets an item like any
other. The cost is that two copies of one identifier-less work become
two items; merging them is deferred, and inventing a title match to
avoid it would be the guess this tool does not make.

## D5. `borax status` opens no document unless asked

**Decision.** `borax status` reports artifacts, items, artifact records
and orphans from the tree and the store, opening no document.
`--identify` adds the identifiable count by running the extraction
passes over each artifact, and queries no service.

**Rejected: status always reports the identifiable count.** It is the
literal reading of the Hyperbole test's "200 artifacts, 0 items, 173
identifiable", and it makes the most basic question about a library cost
an extraction pass over every file in it. A status command that reads
two hundred PDFs is not one an operator runs to see where they are. The
test's substance is that no *ceremony* stands between pointing borax at
a directory and being told what is there, and `--identify` is not a
ceremony: it is the same command, in the same run, asked for more.

**Rejected: caching the identifiable count.** It is an index, and this
change has none by design.

## D6. Reconcile repairs and appends; it neither creates nor deletes

**Decision.** `borax reconcile` compares `(size, mtime)` against each
artifact record and hashes only what does not match. A record whose
last-known path no longer holds its artifact is repaired by a bounded
walk of the library, matching any hash the record ever recorded. A
record whose artifact is nowhere in the library keeps its last-known
path and is reported. `--rehash` skips the fast path. A reconcile over a
library nothing has touched writes no library state — its run log is
written as any run's is.

**The four steps, and why they are ordered.** Matching "any historical
hash" is not a policy on its own: it says nothing about two records
matching one file, one record matching two files, or two files that
swapped paths. The requirement therefore fixes an order, each step run
to completion across every record before the next begins:

1. settle the records whose own path still holds their artifact, and
   *claim* those files;
2. match the remaining records against the unclaimed files by any hash,
   current hash preferred, and repair a record with exactly one
   candidate that no other record also wants;
3. treat a still-unresolved record whose path holds an unclaimed file
   that matched nothing as its artifact edited in place, and append;
4. leave everything still unresolved exactly as it is.

Step 1 before step 2 is what stops a byte-identical copy elsewhere in
the library from competing for a record whose own file is fine. Step 2
before step 3 is what makes a swap work: when two files trade paths,
each matches the *other* record's history, so both repair in step 2 —
whereas step 3 first would read each as the other's artifact edited in
place, append a hash that belongs to a different artifact, and destroy
both identities in a way nothing afterwards could detect.

**Confirming a record refreshes its `(size, mtime)`, and an earlier
draft left that out.** Step 1 originally said a confirmed record was
"unchanged". That is wrong in the one case the fast path exists for.
`touch paper.pdf` changes the modification time and not the bytes, so
the fast path misses, the hash confirms the record, and — under the
earlier wording — the recorded fields stayed as they were. They then
disagreed with the file permanently: no later reconcile would repair
them, because every later reconcile would reach the same hash match and
do the same nothing.

That is invisible inside this change and not invisible above it. The
hierarchical-views change derives an item's position in the filesystem
view only for an artifact whose recorded `(size, mtime)` match the
file, so a touched artifact's item would be unconfirmed for the life of
the library. The fix belongs here rather than there: weakening that
derivation rule to tolerate stale fields would make it tolerate genuinely
unverified artifacts too, whereas a record that describes the file as
borax last saw it is what the fields were always supposed to mean.

So: whenever a reconcile confirms or repairs a record, it writes that
file's current size and modification time. A record already agreeing
with its file is confirmed by the fast path and needs no write, which is
what keeps a routine pass over an untouched library diff-free — and
"untouched" now means what it says, a file whose modification time
changed having been touched.

**Ambiguity preserves rather than guesses.** A record with more than one
candidate, and a record whose only candidate another record also wants,
keeps its path, its history and its link untouched and is reported
naming the candidates. The asymmetry that decides this: a stale path is
repaired by the next pass, and a wrong item link is not detectable at
all — nothing downstream can tell a correct link from one reconcile
invented, so the cost of guessing wrong is unbounded and the cost of
declining is one more report.

**Rejected: resolve ambiguity by taking the nearest path, the newest
mtime, or the first in walk order.** Each is a rule that always produces
an answer, which is the property that makes it dangerous here: it turns
"borax cannot tell" into a silent assignment of one artifact's identity
and item link to another file.

**Rejected: reconcile adopts orphans.** It is the obvious extension and
it destroys the concept: if reconcile wrote a record for every artifact
without one, a library would have no orphans, and the worklist that
makes the library self-describing about its own incompleteness would be
empty by construction. An orphan persists until something decides what
it is.

**Rejected: reconcile deletes a record whose artifact is gone.** The
flat layout exists partly so that a record can outlive its artifact and
say "the library held this and it is gone" — which is the thing a record
beside the file loses to `rm`. Deleting it would throw away the dividend.

**Rejected: hashing everything, with no fast path.** Correct and
unusable: a ten-thousand-file library would rehash every byte on every
pass. The fast path's weakness is real and stated rather than hidden — a
change within the granularity the recorded mtime keeps is missed, which
is what `--rehash` is for.

## D7. The symlink question: no same-rank conflict, and the policy is
carried rather than changed

**What was looked for.** The architecture records the boundary and the
path policy as pulling against each other: the single-directory rule
wants an escape hatch for material that cannot live in the tree, and
`crates/borax/src/paths.rs` deliberately does not resolve symlinks, with
the `ledger` capability depending on that. The instruction was to check
whether two living requirements of the same rank actually disagree.

**What is there.** One living requirement mentions symlinks at all:
`ledger`, "Duplicate detection operates at two levels with distinct
reasons" — *"Paths SHALL be compared as the collection builds them …
normalised lexically and matched the way the platform matches file
names. Symlinks SHALL NOT be resolved."* Nothing in force requires a
symlink to reach outside the tree, or requires material outside the tree
to participate in a library at all. The code agrees: `paths.rs` states
the
no-resolution policy in its module docs, and `documents` in
`crates/borax/src/run.rs` reads directory entries without following
links, so a symlink in a walked directory is today neither a file nor a
directory and contributes nothing.

**Conclusion: they do not disagree.** There is one authority and it says
symlinks are not resolved. The tension the architecture records is
between that policy and an *unbuilt* escape hatch — a wish, not a
requirement — so there is nothing of the same rank to defer to and
nothing to resolve on my own authority.

**So the boundary is specified as the standing policy already implies.**
A path is inside the library when it lies under the root by lexical
containment with symlinks unresolved; a symlink inside the tree does not
bring its target in; a symlink is neither an artifact nor an orphan,
which is what `documents` already does. That is the architecture's own
statement of the consequence, carried forward, and it needs no new
decision.

**What stays open, and is not settled here.** Whether borax should
change the no-resolution policy to make symlinks the escape hatch. That
would amend both the boundary requirement added here and the `ledger`
requirement that depends on links not being resolved — with duplicate
detection in the blast radius, since a link into a library would then
bring its target's admission with it — and it is the architecture's
open question rather than a gap in this change. The proposal defers it,
and the requirement is written so that the deferral is visible in it
rather than only here.

## D8. borax brings no file into the library

**Decision.** Neither copy nor move. A file named as input from outside
the library tree is resolved and renamed where it sits, and gets no
artifact record and no item. A run that renames a file outside the
library reports that it did, so nothing is silently unrecorded.
Relocating a file into a library is the operator's, in whatever tool
they like, and the file's appearance in the tree is the trigger borax
then recognises.

**Rejected: copy the file in.** It leaves two copies of everything and
makes borax the owner of where files live, which is the managed
attachment the whole design refuses. It also doubles a library's size
for no information gained.

**Rejected: move the file in.** It is one file operation rather than
two, and it is a relocation the operator did not ask for. `--apply`
authorises a *rename*, and an answer to a question authorises the move
it names; a third kind of decision — relocating a file into a library —
is exactly the third explicit decision `openspec/project.md` says there
is not.

**Precedent, not invention.** This is what the code already does.
`Planning::base_for` in `crates/borax/src/renaming.rs` files from the
file's own directory when the file is not under the root, and `admit` in
`crates/borax/src/run.rs` records nothing when `collection_relative`
cannot express the path below the root. The requirement states a
behaviour that exists; what is new is that it is now a rule with a
reason rather than a consequence of two helpers.

## D9. Thirteen requirements in one change, and the splits that are
worse

The `library` capability arrives whole: the boundary, both records, both
identities, the edge, the artifact and orphan definitions, status,
validate, reconcile, adopt, the no-index rule, the no-import rule, and
the one write path. Splitting it was considered along three lines and
all are worse.

**Split at the operations** — the store in one change, `validate` and
`reconcile` in the next. It leaves a change that adds authoritative
state with no way to check it and no way to repair a stale path, which
is a store whose correctness nothing states. The architecture puts the
invariants in the validator; a store shipped before its validator puts
them nowhere.

**Split at the ledger** — the library store first, the ledger's
retirement after. That is D2's rejected interval: two per-file stores,
both written, able to disagree.

**Split at adoption** — retire the ledger here and provide `borax adopt`
in a change of its own. It leaves a release in which the only way to
record a library is to run a renaming pass over it, since `rename
--apply` would be the sole writer. Recording and renaming are different
operations with different risk, and separating them is the point of the
command.

What is kept out instead is everything the model can be stated without:
views and every part of the organizational half, any command that
creates an item by hand, adopting an orphan borax knows nothing about,
and the index. Each is named in the proposal's Deferred section with
what it waits for.

## D10. Identities are UUIDv7 from the `uuid` crate

**Decision.** Both identities are UUIDv7, minted through `uuid` with the
`v7` and `serde` features, which brings `getrandom` — borax's first
entropy source.

**Why v7.** It is time-ordered, so a directory listing of
`.borax/artifacts/` and a sorted item store are roughly chronological,
and two identities minted in one run sort in the order they were minted.

**Rejected: minting from a pid and a counter**, the way
`temporary_name` in `crates/borax-sources/src/store.rs` deliberately
does for temporary files. A temporary name only has to be unique among
the processes writing one directory now. An identity has to stay unique
across two machines whose libraries are merged through Git, and a pid
and a counter collide there.

**Rejected: deriving the identity from the content hash.** It is the one
thing the architecture rules out by name: annotating a file changes its
bytes, and an identity that changes with the content is not an identity.

**Rejected: hand-rolling UUIDv7.** The generation is a few lines over an
entropy source, and the parsing, formatting and validation are not. The
precedent for hand-rolling in this project is the template engine, which
bought a specified, versioned, total grammar that no crate offered.
There is nothing to specify here: UUIDv7 is published, and a bespoke
implementation of it would be a liability rather than a surface.

## D11. An item file is TOML, and verbatim source fields are JSON text
inside it

**Decision.** An item file is TOML: `id` at the top, the canonical
record under `[record]` as TOML tables and values. The one exception is
`borax.source-fields`, the record's map of provider fields with no
CSL-JSON equivalent, whose values are written as their JSON text — one
TOML string per key, parsed back as JSON on read.

**Why an exception is needed.** `source_fields` is a
`BTreeMap<String, serde_json::Value>` in
`crates/borax-core/src/record.rs`, so its values are arbitrary JSON, and
one JSON value has no TOML spelling at all: **null**. TOML 1.0 does have
mixed-type arrays and nested tables — an earlier draft of this entry
said otherwise and was wrong, and the overbroad reason is worth
correcting because it would have justified far more than the encoding
needs. The narrow reason is enough on its own: a provider that answers
with a null makes a naive `toml::to_string` of a record fail at
runtime, on a record borax accepted and cached without complaint, and a
store whose write can fail on data the pipeline considers ordinary is
not a store.

**Why the encoding is uniform rather than applied to nulls alone.**
Encoding every value as JSON text keeps the whole `serde_json::Value`
tree distinguishable on the way back: `"42"` and `42` are different JSON
texts, so a provider string that merely looks like a number or an object
survives as the string it was, and a null survives as a null. A rule
that encoded only the values TOML cannot hold would make the decoder
guess which TOML strings were meant as JSON, which is the ambiguity this
avoids.

**What it does not preserve.** The provider's own whitespace, and its
key ordering inside an object — both are already gone before the store
sees them, since the value is a `serde_json::Value` parsed from the
response, whose object is a map. The claim is that the *value tree*
round-trips, not that the provider's bytes do, and task 1.3 tests the
distinction that carries the weight: string against number, string
against null, and the numeric boundaries where a JSON number and a TOML
integer disagree.

**Rejected: dropping `source_fields` from the item file.** The record
would no longer round-trip through the item store, and the provenance
`add-core-pipeline` went out of its way to retain would be lost exactly
where the library's authoritative statement about a work is kept.

**Rejected: the whole record as one JSON string, behind a marker, as the
citation sidecar does it.** It is the existing precedent and it defeats
the point of the text store: an item file would not be diffable or
editable as TOML, and a small correction to a title would be a change to
one long line.

**Cost.** One field in the store has an encoding of its own, which is
one thing to document and one thing a hand-editor can get wrong. A
value that does not parse as JSON is a validation finding about that
item, which is what makes it recoverable.

## D12. Durability: whole-file atomic replacement, and an order for the
two writes

**Decision.** Every write borax makes to the store replaces a whole file
atomically — temporary in the same directory, renamed over the
destination, which `write_atomically` in
`crates/borax-sources/src/store.rs` already implements and the cache
already writes every entry through. borax never appends to, truncates
or rewrites a record in place. When an admission writes both an item
and an artifact record, the item is written first.

**What atomic replacement buys, beyond the reader isolation an earlier
draft stopped at.** A write that fails or is interrupted leaves the
record *exactly as it was* — its artifact identity, its whole hash
history, its item link. That is preservation and not merely isolation,
and it matters here more than it does for a cache entry: a hash history
is evidence of what the file used to be, and nothing can derive it
again. Re-resolving the file recovers its record; it does not recover
the hash it had two annotations ago, or the run that admitted it.

**Field-level editing and whole-file replacement are not alternatives.**
The architecture asks for field- or section-level writes through
`toml_edit` where that avoids rewriting an otherwise unchanged
document. That is about the *document* — comments, key order and
formatting a person put there survive an edit to one field — and it is
composed with atomic replacement rather than opposed to it: edit the
parsed document, render it, replace the file in one step.

**The two-file write has no atomicity, and the order is the answer.**
Nothing can make an item write and an artifact write one transaction
where the writers are peers and no lock may be a precondition for
correctness. So the order is chosen for what an interruption between
them leaves:

- **item first** leaves an item nothing links to, which is an ordinary
  library state — items exist without artifacts by design — and which
  `borax validate` counts rather than reports as a finding;
- **artifact first** leaves an artifact record naming an item that does
  not exist, which is a finding and a dangling link.

**Retry semantics.** borax retries nothing within a run. A store write
that fails costs the file its record, not its rename, and is reported;
the next applying run over that file records it again. The residue of an
interrupted admission is at worst an item nothing links to. For a record
carrying an identifier the next run finds that item again and links to
it; for one carrying none it mints another, and the stranded item stays
as a counted, harmless artifact of the interruption. Sweeping such items
is not proposed here: an item with no artifact is indistinguishable from
one the operator wrote for a work they have no file for, and deleting
the second in order to tidy the first is not a trade this tool makes.

## D13. `borax adopt`: offline recording, and why it survives without
the ledger

**Decision.** `borax adopt` hashes each orphan and writes an artifact
record and an item wherever the content index already holds the record
borax resolved for those bytes. It queries no service, extracts from no
document, moves, renames and deletes nothing, and leaves an artifact
the index cannot answer for as an orphan. It reads neither a citation
sidecar nor the retired ledger.

**It was judged on its merits without the ledger half, and it stands.**
An earlier draft had it read `.borax/ledger.jsonl` too, as the migration
off the retired file; that half is gone, and the question is whether
what remains is a command or a leftover. What remains is the only way to
populate the store that cannot move a file. `borax rename --apply` also
records what it settles, but it is a renaming run: it may query services
for anything the index misses, it may move files under whatever
templates are now in force, and it cannot record a file it fails to
resolve. An operator who wants records for a library they are happy with
— names as they are, no network — has `adopt` and nothing else.

It also pays into the Hyperbole test from the other side. Pointing borax
at a directory it has never seen reports orphans; pointing it at one it
has *seen before* should not require re-resolving every file to turn
that report into a library, and with a warm content index it does not.

**The weakness, stated rather than hidden.** Its only source is a cache.
`borax cache --clear` empties the content index, and adoption after that
records nothing at all — the requirement says so and has a scenario for
it. That makes `adopt` a best-effort convenience rather than a recovery
guarantee, which is the honest description of any command whose input is
disposable state.

**Rejected: dropping the command with the ledger half.** It would leave
`rename --apply` as the only writer, so recording a library would be
inseparable from renaming it. Those are different operations with
different risk: one moves files, the other cannot.

**Rejected: having adopt extract identifiers from the artifacts.** That
is a resolution pass wearing another name — it would want the network
the moment extraction produced an identifier, and offline is the
property that makes this command distinct. A file borax has never
resolved stays an orphan, which is the truthful report.

## D14. Renaming: the flag moves, the capability cannot

**Decision.** `--ledger`/`--no-ledger` and the configuration key
`ledger` become `--record`/`--no-record` and `record`. The `ledger`
*capability* keeps its name, because the rename is not expressible in
the tooling. No deprecation window either way: before `1.0.0` there is
no compatibility to keep, and `CLAUDE.md` prefers a clean replacement
over a legacy branch.

**Why `record`.** The setting governs whether a run keeps a record of
what it admitted — the artifact records and the items — and whether it
checks incoming files against what is already recorded. "Record" is the
noun the user already sees: `borax status` counts artifact records, and
`.borax/artifacts/` holds them.

**Rejected: `--account`/`--no-account`.** It avoids colliding with
`Record`, the canonical bibliographic type, and "the library's account
of what it has admitted" is the phrase this proposal keeps reaching
for. But the collision is in *our* prose rather than in the user's:
nothing on the command line or in `borax status` output calls a
bibliographic record an account, and `--no-account` reads like
something about credentials.

**The capability rename is not expressible, and this was tested rather
than assumed.** A capability rename has to be a `## REMOVED` block
emptying the old capability and an `## ADDED` block filling a new one.
Run against a copy of this repository's `openspec/` tree, that change
passes `openspec validate --strict` and then fails to archive:

```
Specs to update:
  admission: create
  ledger: update
Applying changes to openspec/specs/admission/spec.md:
  + 1 added

Validation errors in rebuilt spec for ledger (will not write changes):
  ✗ Spec must have at least one requirement
Aborted. No files were changed.
```

Two things are wrong there and the second is worse than the first. A
capability may not be emptied, so the old directory cannot be vacated;
and "No files were changed" is untrue — `openspec/specs/admission/`
had already been created and written before the abort, leaving a new
capability on disk, the old one untouched, and the change unarchived.
Shipping a delta that does that to the living specs is worse than a
stale name.

**So the name stays, and the proposal says so rather than leaving it to
look like an oversight.** Within the capability, the one requirement
whose *title* named the ledger is renamed by the means that does work: a
`## REMOVED` block for the old title with its reason, and an `## ADDED`
block for "The record of admissions is optional and degrades loudly,
never blocking". A `## RENAMED` block followed by a MODIFIED under the
new title would archive, but `scripts/check-spec-deltas.py` rejects a
MODIFIED title that matches nothing living, so removal-and-addition is
the form that satisfies both tools.

**What the gate governs: bookkeeping, not safety.** `--no-record`
suppresses every write to the store and both duplicate checks. It does
not suppress the one read an applying run makes against the record of
the file it is about to move. That question came from the change above
this one, which requires any applying run that moves an artifact to
identify its record by reconciliation's settled test before the move and
rewrite it once afterwards — a contract a gate forbidding *both* reading
and writing cannot satisfy.

The two reads are different things and only one of them is the account.
The duplicate checks ask a policy question about admissions — have these
bytes, or this work, been here before — and an operator turning the
account off is entitled to turn that off with it. The pre-move check
asks whether borax is about to damage a record it already holds. It
reads one record, the one belonging to the file in hand, and does
nothing with it but decide whether the move is safe.

So the gate writes no library state, checks no duplicates, and still
refuses one move: the move of an artifact whose record names its current
path and whose history does not hold that file's hash. That is the case
where skipping the post-move write is destructive rather than merely
untidy. Where the history does hold it, the artifact can be moved
freely under the flag, because the hash survives the move and
reconciliation's bounded walk finds the file by it. Where it does not,
the file's bytes have changed since the record last recorded them, so
after an unwritten move that record names a path holding nothing and
carries no hash matching anything — reconciliation's walk matches by
hash and its edited-in-place step needs a file at the recorded path,
and neither has anything left to work with.

The next entry explains why the test is the hash rather than the
settled test this paragraph originally named.

**"Settled" is not evidence about content, and the first draft of this
decision assumed it was.** That draft let a move proceed under the flag
whenever the pre-move check *settled* the record. The settled test has
two clauses — the fast path, or a recorded hash matching — and the fast
path compares size and modification time only. A same-length in-place
edit preserves both, so a record whose every recorded hash is stale
passes it. Move that file without writing and the record names a path
holding nothing while holding no hash matching the file's bytes: the
exact strand the refusal exists to prevent, admitted through the
permission beside it.

So where no rewrite follows, the fast-path clause is not sufficient and
the computed hash must be in the record's history. This costs nothing:
`rename`'s "Applied renames are recorded in the run log" requires every
applied rename to carry the file's content hash *and* requires the event
to be flushed before the move it records, so the hash is in hand
strictly before the decision. That was confirmed against the living
requirement rather than taken on trust, because the whole defect came
from assuming what an unverified clause guaranteed.

**The hole was wider than the flag, and the general half is worse.**
Nothing in this change said what *updating* a record writes. An update
that wrote only the new path would leave a history whose newest hash is
not the file's — and unlike the flag case nothing would ever report it,
because the fast path would settle that record on every later reconcile
while the file's actual bytes appeared in no record at all. The flag
case fails loudly at the next out-of-band move; the general case fails
silently and forever. So the requirement now says what an update writes:
the file's hash appended when it is not already the current one, the new
path, and the size and modification time as they are after the run's
work. A record an applying run touched describes the file it names.

The root of both is one mistake worth naming, since it will recur: the
fast path is an optimisation about *whether a record still describes a
file*, and it is not evidence about *what the file now contains*. Any
operation that writes, or declines to write, on the strength of it
inherits that gap. Reconciliation may rely on it because nothing moves
there — a record it wrongly settles still points at its file, and
`--rehash` catches it later. A move may not, because the path is
precisely what a move takes away.

**Rejected: `--no-record` refuses to move a recorded artifact at all.**
Honest, and far too blunt. Every library in use has records, so the flag
would stop working on exactly the libraries it is used on, and it would
refuse a great many moves reconciliation repairs without anyone
noticing.

**Rejected: the flag suppresses writes only, and every read still
happens.** It is the smallest edit to the text and it takes away what
the flag is for. Turning off duplicate detection is a legitimate thing
to want — these are duplicates and I know it, rename them anyway — and
it is what `--no-ledger` has always done.

**Cost.** A run under the flag can leave stale paths behind, which is
the state reconciliation exists for, and can refuse a move it would
otherwise make, which costs one report and a re-run without the flag.
The refusal is a new skip reason, which `cli` counts as an addition.

**Cost of the name.** A flag named for a file that no longer exists,
gating state that is not accounting but authority. The name is
inherited, the capability's name is inherited with it, and the
alternative was worse.

## D15. A library's tree stops at a nested marker

**Decision.** A directory holding its own `.borax.toml` is a library of
its own, and an enclosing library excludes that directory and everything
under it from its artifacts, its orphans, its admissions and its
reconciliation. One rule, obeyed by all four.

**Why not overlap.** Nearest-marker discovery already decides which
configuration and which accounting root a file gets, so a nested marker
already means the inner files are the inner library's. An unrestricted
walk in the parent contradicts that in a way the operator sees as
incoherence rather than as policy: the parent counts PDFs whose records
it does not read, an applying run over the parent records those files
into the *child's* store because that is where discovery sends them, and
the parent then lists orphans that its own `borax adopt` will not adopt.

**Rejected: deliberate overlap, with the parent reporting a child's
files as belonging to both.** It is defensible for reporting and
indefensible for writing: two libraries would each believe they own an
artifact's record, and reconciliation in one could repair a path the
other had just changed.

**Cost.** A marker dropped into a subdirectory silently removes that
subtree from the enclosing library's counts. It is discoverable —
`borax status` in the parent reports the nested root — and it is the
same surprise `.borax.toml` already carries for configuration.

## D16. A work duplicate is asked about, and only where there is
somebody to ask

**Decision.** The two-level check keeps both levels and both reasons.

- **Batch run:** a work duplicate diverts to the skip queue with the
  reason and the existing path it has today. Unchanged behaviour.
- **Interactive run:** the operator is asked — file it as another
  artifact of that item, skip, or quit — and accepting returns the file
  to planning, where the ordinary move question is put. Only an operator
  can admit a second artifact of a work the library holds.
- **A record the operator reached:** told, not asked. An identifier
  they supplied, a lookup they retried and a record they accepted over
  a conflict are all checked, and a collision is stated once — the same
  item and path the question would have named — with the move question
  put again afterwards.

**Why a supplied identifier is told rather than asked.** The question
exists to obtain one statement from the operator: that this file is
that work. Supplying the identifier *is* that statement, so asking for
it again is asking somebody to confirm what they have just said, and
the second question could only ever be answered the same way. What they
may genuinely not know is that the library already holds a file of it,
which is a fact rather than a decision — so the run states it and puts
the question the operator was already answering. Stating it once is
what keeps the move reachable: a collision restated on every pass would
be a file that can never be moved.

**Re-justified on the merits alone.** An earlier draft of this entry
argued partly from compatibility — that reinstating a behaviour a
review gate had once caught would be that defect with a rationale
attached. That argument is withdrawn: before `1.0.0` this project does
not preserve a behaviour because it is the behaviour, and a newer
decision beats an older one unless it is illogical. The conclusion does
not move, because nothing in it rested on that argument: three faults
in the draft it replaced, and two standing merits of this one, carry it
without help. That draft removed the work-level skip outright and had a
file whose identifier an item already carried "planned, questioned and
admitted like any other resolved file". The three faults, in increasing
order of seriousness:

*It contradicted a living requirement.* `rename`, "An interactive run
asks about files it could not settle", says in terms: **"A file reported
as a duplicate, or whose content hash is unknown, SHALL NOT be asked
about."** A work duplicate is a file reported as a duplicate, so the
earlier draft's interactive half was not merely unspecified but
forbidden. This change now amends that sentence deliberately — narrowing
it to a *content* duplicate, with the reason — rather than leaving the
archive holding a requirement and a practice that disagree.

*`--apply` is not the decision the earlier draft took it for.*
`openspec/project.md` describes it as a decision "given to a batch run
over a plan it would otherwise preview", and the living `rename`
requirement has apply carry out the plan without showing it again. The
tempting reading is that a preview therefore always precedes an apply,
so a batch operator has seen the additional artifact before authorising
it. That reading is false on this project's own facts:
`add-interactive-rename` made `--apply` select a batch run *by itself*,
so `borax rename --apply papers/` is one invocation that previews
nothing. Under the earlier draft it would silently file a second copy of
a work the library already held.

*And it guesses at an intention.* `openspec/project.md` puts "never
guesses" beside "never overwrites" in the contract borax is built on.
Whether a work you already hold should gain a second artifact is not a
fact about the file, so a batch run that decides it has guessed — and
the guess it would make is the one that puts a file somewhere rather
than the one that leaves it alone.

**The precedent is a design pattern, not an inherited behaviour.**
`resolution`,
"Ambiguity is skipped, never guessed", was amended by
`supply-identifiers-interactively` so that an operator — and only an
operator, in an interactive run — may accept a record whose title
conflicts with the file's, while batch runs still skip, always. The
shape is identical: borax cannot tell, a person can, so the run with a
person asks and the run without one takes the conservative branch. A
title conflict is "borax cannot tell whether this record is for this
file"; a work duplicate is "borax cannot tell whether you want a second
copy of this work". Neither is answerable from the bytes.

**The asymmetry of being wrong.** Skipping a genuine second
manifestation costs a report naming the existing path and a re-run with
a terminal attached; the file sits untouched where it was. Admitting an
unwanted re-download puts a second file into the library — very possibly
moved into a rendered subdirectory beside the copy it duplicates — plus
a record and an item link to unpick. Nothing is destroyed either way,
which is why this is a judgement about which mistake is cheaper to
recover from rather than about safety, and the cheaper one is the skip.

**Rejected: the work check becomes advisory everywhere, with the preview
as where a batch operator sees it.** Simpler, and coherent exactly if a
preview precedes every apply — which is the condition that fails above.
It also puts the weight on a batch operator reading a report they did
not ask for, in a run whose whole point is not stopping.

**Rejected: an override flag so a batch run can take them.** Two
authorities forbid the obvious forms. `cli`, "The apply gate is never
configurable", says no configuration may authorise a move, so it could
not be a setting; and `openspec/project.md` says there are two explicit
decisions "and no third", so a command-line selector that authorises
what `--apply` does not would be that third. The remedy for a batch
operator is the remedy the conflict case already has: run it again with
a terminal.

**Cost.** Filing a second artifact needs an interactive run. A library
curated entirely through `--apply` — scripted, or over a pipe — cannot
acquire a second artifact of a work it holds, and will report one every
time until somebody answers for it. That is a real limit on the
architecture's headline case, bounded by the fact that interactive is
the default whenever there is a terminal, and it is the limit this
project already accepts for a title conflict.

**A sibling must not meet its own item.** Once an item has several
artifacts, a plain "does an item carry this identifier?" check reports
every one of them as a work duplicate of the item it already belongs to,
on every run — and under this decision a *batch* run would then skip
each of them. So the check passes over the item that the incoming file's
own artifact record already links to, exactly as the content check
passes over the file's own record. That rule is what makes a
multi-artifact library usable at all, not a nicety.
