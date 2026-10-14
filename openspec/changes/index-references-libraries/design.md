# Design: index-references-libraries

## Context

Everything below was read from source at `da6706d`, the merge of
`library-identity`. This is change 13 of the roadmap's Phase 4, the
third of its four changes. Change 15 (`separate-change-from-acceptance`)
and change 12 (`library-identity`) are done; change 14
(`refresh-as-candidate`) follows.

**Settled by the maintainer** (the Phase 4 library-authority design
record, maintainer's notes, outside the repository). These are
decisions, not open questions:

1. No operator decision lives only in a disposable cache. Inside a
   library, each artifact history entry records the identification
   evidence of its accepted bytes, the identifier and its origin, beside
   the item it was accepted under.
2. The reduced content index moves out of the cache root into the XDG
   data directory. Afterwards `borax cache --clear` removes service
   responses only.
3. Several reachable references answer only when their items carry the
   same preferred identifier. Otherwise none is chosen, the entry's own
   identification evidence is looked up, and the event names the
   disagreeing references. Two locations of one library are two
   references. A run inside a library is not ambiguous for its own
   tracked files: the current library answers.
4. No migration of `v1` entries, and no reader or importer for them.
   The reduced format may be called `v2` on disk. No format-version
   handling beyond naming the directory.
5. Refresh is change 14's.

**Defaults from the same record, kept here, refined only in spelling:**
an entry is keyed by content hash and holds identification evidence and
zero or more references, each naming a library identity, a last-known
root, an artifact and an item; outside a library an entry's answer
begins with its identifier, then the service cache or the network; an
unavailable reference is preserved, never pruned automatically, its
cause reported, and its stale root repaired by a run inside the library
it names; the write order is item, then artifact record with its
evidence, then the index reference by atomic replacement, every step
idempotent; the origins are an extraction pass, an operator-supplied
identifier, an operator acceptance over a conflict, and adoption.

**Decided by the orchestrator for this proposal, and recorded below:**
the paths (D1); the storage seam and its in-memory implementation (D2,
D12); when references are written and only under a present identity
(D5); where missing references are restored and stale roots repaired
(D6); offline adoption's order (D9); what an index answer means (D7);
the schema rule applied (D10).

**Owned by this proposal:** the entry's and the history entry's on-disk
spelling (D2, D3); which identifier each write records (D3); the
operator-evidence rule (D4); the reference upsert (D6); reading a
referenced library (D8); the event vocabulary and human strings (D10,
D11); the interactive driver's `accept`/`remember` path (D4, D12); the
requirement deltas (D14).

What the engine does today, at the points this change touches:

- **Where the index lives.** `borax_sources::store::cache_root`
  (`crates/borax-sources/src/store.rs`, line 43) tries `XDG_CACHE_HOME`
  then `HOME/.cache` on Unix, `LOCALAPPDATA` then `XDG_CACHE_HOME` on
  Windows (`CANDIDATES`, lines 64 and 69), skips a variable that is
  unset, empty or relative, and appends `borax/<FORMAT_VERSION>`
  (`FORMAT_VERSION = "v1"`, line 27). One `FileCache` per source over
  that root is the response cache (`run::polite`, `run.rs` line
  4132), and `run::execute` opens the content index over the same kind
  of store: `ContentIndex::new(response_cache())` (`run.rs`, line 4035).
  `response_cache()` (near line 4146) falls back to a `MemoryCache` when
  no cache root qualifies, so a system naming none gets an in-memory
  index that forgets at exit. Nothing is reported for it.
- **What the index holds.** `ContentIndex<C: Cache>` (`store.rs`, line
  362) wraps a `Cache` of `Record`s and stores the whole record under
  `content_key(hash)` = `content/<hash>` (line 120), so an entry is
  `<root>/content/sha256-<hex>.json`. `Cache` (`crates/borax-sources/src/cache.rs`,
  line 81) is typed to `Record`, and `MemoryCache` (line 95) is its
  in-memory implementation.
- **`cache --clear`.** `run::preflight`'s `Command::Cache` arm
  (`run.rs`, line 749) calls `cache::clear(root)`
  (`crates/borax/src/cache.rs`), which is `FileCache::clear`
  (`store.rs`, line 157): `remove_dir_all` of the whole cache root, the
  content index included.
- **Reading the index.** `pipeline::index_read` (`crates/borax/src/pipeline.rs`,
  line 178) returns the record on a hit (line 197). `beyond_library`
  (line 645) turns a hit into `indexed_record` (line 208): every later
  step not attempted for `content-index-hit`, `record_retrieval`
  `content-index` (`evidence.rs`, `Evidence::retrieval`, line 173).
- **Writing the index.** `beyond_library` writes the resolved record
  under the file's hash unless the title check refused it (line 716),
  whatever `ResolveConfig::cache` says. `pipeline::accept` (line 943)
  turns a refused write into `awaiting-acceptance`, and
  `pipeline::remember` (line 968) writes the operator's record at the
  move, from the rename driver (`run.rs`, lines 1686–1699), reported as
  `Event::ContentIndexWrite` (`event.rs`, line 250).
- **Adoption.** `library::adopt` (`crates/borax/src/library.rs`, line
  3136) takes `lookup: &dyn Fn(&ContentHash) -> Option<Record>`, which
  `run::adopt_events` (`run.rs`, line 3963) fills with
  `adapters.index.get(hash)`. An orphan the lookup does not answer is
  `Adoption::Unindexed` (`event.rs`, line 796).
- **History entries.** `HashEntry` (`crates/borax-core/src/library.rs`,
  line 440) holds `hash`, `run`, `timestamp`, `tool_version` and the
  optional `item` change 15 added, written by `history_table`
  (`library.rs`, line 2042) only when present. Entries are made by
  `admit` (line 2880, through `settle_history`, line 1439), and by
  `reconcile`'s step 4 for an accepted change (near line 2531).
- **Library identity.** `library::identity(root)` (line 380) reads
  `Absent`, `Present(LibraryId)` or `Faulty`; every store write goes
  through a `StoreWriter` (line 471). Change 12's design warns that a
  successful admission does not imply a `Present` identity.
- **The schema.** `event::SCHEMA` is 4 (`event.rs`, line 29), first
  released in 0.9.0. The `cli` requirement "JSON Lines output is
  first-class" fixes a schema version at its first release.
- **Retry.** The interactive driver's `Answer::Retry` (`run.rs`, near
  line 2247) asks again only for `lookup.extracted()`, an identifier of
  `Origin::Extracted`.
- **A defect found on the way, not fixed here.** On Windows the run-log
  state root (`runlog::state_root`, `crates/borax/src/runlog.rs`, line
  79: `LOCALAPPDATA`, then `XDG_STATE_HOME`) and the cache root are the
  same directory, `LOCALAPPDATA\borax\v1`, so `borax cache --clear`
  there removes the run logs of runs outside a library. CI does not run
  on Windows. It bears on D1 and is recorded in the proposal's
  Deferred.

## D1. Where the index lives

**Decision.**

- **The data root.** `borax_sources::store::data_root(lookup)`,
  mirroring `cache_root`: the first candidate whose value is a
  non-empty absolute path is taken, a variable unset, empty or relative
  being skipped.
  - Unix: `XDG_DATA_HOME`, then `HOME` with `.local/share` appended.
  - Windows: `APPDATA`, then `XDG_DATA_HOME`.

  `borax` and `INDEX_FORMAT_VERSION = "v2"` are appended, so the root is
  `$XDG_DATA_HOME/borax/v2/` or `$HOME/.local/share/borax/v2/`. It is
  neither created nor checked. `default_data_root()` applies it to the
  process environment.
- **Entries** are at `<data root>/content/<hash>.json`, which is
  `entry_path(root, &content_key(hash))` as today. JSON, as the
  response cache's entries are.
- **No data root.** `run::execute` opens the index over a `MemoryCache`,
  as it opens it today when no cache root qualifies. The run reads and
  writes it, and it is forgotten at exit. Nothing is reported, as
  nothing is today.
- **The response cache stays where it is**: `cache_root`, `v1`,
  `FileCache`, unchanged. Old `v1` entries under its `content/` are
  neither read nor written by anything, are counted by `borax cache` as
  the files they are, and `borax cache --clear` removes them with the
  rest of that root. `FileCache::clear` and `cache::clear` are not
  changed: the index is simply no longer under the root they remove.
- **Why `APPDATA` on Windows.**
  1. It keeps the data root on a different base from the cache root,
     `LOCALAPPDATA\borax\v1`, so no `remove_dir_all` of the cache root
     can reach the index whatever version segments either root carries
     later. Under `LOCALAPPDATA` the two would be separated only by
     `v1` against `v2`; the state root's collision with the cache root
     (Context, last bullet) is what that kind of separation costs.
  2. It is the Windows counterpart of `XDG_DATA_HOME` in the usual
     mapping, and borax's own configuration directory already uses it
     (`config.rs`, line 1088: `APPDATA`, then `XDG_CONFIG_HOME`).
  3. What the index holds is the user's: operator-supplied
     identifiers and the references that find their libraries. Roaming
     it is the right default. Its roots are machine-specific, and a
     root that does not exist on another machine is a reference
     reported unavailable (D8) and repaired by the next run inside the
     library there (D6), which the design already handles.

**Rejected: `LOCALAPPDATA` (`borax\v2` beside the cache's `borax\v1`).**
Reason 1 above. It would also make the index non-roaming, losing the
operator's identifiers on a second machine.

**Rejected: keeping the index under the cache root and teaching
`cache --clear` to spare `content/`.** Settled otherwise, and an index
under a directory the platform treats as purgeable cache is still a
cache.

**Rejected: the XDG state directory.** State is "data that should
persist between restarts but is not important or portable enough for
the data directory". Operator decisions are the opposite.

**Rejected: reporting a run with no data root.** Today's run with no
cache root is not reported. Reporting the index alone would be new
noise for a case CI's own environment (`env_clear`) hits on every
binary test.

## D2. What an entry holds, on disk

**Decision.** An entry is one JSON object:

```json
{"evidence":{"identifier":"doi:10.1039/c5ay00042d","origin":"extracted"},
 "references":[{"library":"0190c3a2-7f21-7abc-8def-0123456789ab",
                "root":"/home/u/papers",
                "artifact":"0190c3a2-7f21-7abc-8def-0123456789ac",
                "item":"0190c3a2-7f21-7abc-8def-0123456789ad"}]}
```

- `evidence` is absent when the entry holds none. `identifier` is the
  text `Identifier`'s `Display` writes (`doi:…`, `arXiv:…`, `pmid:…`,
  `isbn:…`) and is read back with `identifier::supplied`, which accepts
  each of those forms. `origin` is one of D3's four words.
- `references` is absent when empty. Each names a library identity
  (`LibraryId` text), the library's root as last written (absolute,
  normalised lexically, symlinks not resolved, as `Stores::read`
  normalises it), an artifact identity and an item identity.
- **A reference's key is `(library, root, artifact)`.** Two references
  with the same key are one reference. Two roots of one library are two
  references (settled 3); two artifacts of one library at one root,
  which a library holding the same bytes in two records would give,
  are two references too.
- **A reader ignores keys it does not know.** An entry that does not
  parse as a whole, including an `identifier` that does not parse or an
  `origin` outside D3's words, reads as no entry, as a record that does
  not parse reads as a miss today. A write to such an entry replaces it.
- **No record.** Nothing in an entry is bibliographic. The index stops
  being a copy of anything a library or a service holds.

**The storage seam.** `ContentIndex<S>` stays generic, now over a new
trait `IndexStore` (D12) with `entry(key)` and `put_entry(key, entry)`.
`FileCache` implements it over its root, and `MemoryCache` implements it
with a second map beside its records. So `ContentIndex::new(MemoryCache::new())`,
`ContentIndex<MemoryCache>` and `Adapters { index: &index, .. }` compile
unchanged at every test site that does not read or seed an entry: the
type that held records in memory holds entries in memory too, as the one
`FileCache` directory held both before this change.

**Rejected: a new `MemoryIndex` type.** It would be cleaner by a little
and would change 399 `ContentIndex::new(MemoryCache::new())` sites and
their imports for nothing a test asserts.

**Rejected: making `Cache` and `MemoryCache` generic over the value
(`MemoryCache<V = Record>`).** `ContentIndex::new(MemoryCache::new())`
would then compile only by inference from a single impl, the three
`ContentIndex<MemoryCache>` annotations would break anyway, and every
response-cache signature would carry a parameter it does not use.

**Rejected: TOML for entries.** The store the index lives in has always
been JSON, the event stream is JSON, and nobody edits an entry by hand.

**Rejected: an index of entry by item or by library.** Nothing asks
those questions; the hash is the only key a file arrives with.

## D3. Identification evidence and its origins

**Decision.** `IdentificationEvidence { identifier, origin }`, with
`EvidenceOrigin`:

| Word | Meaning |
|---|---|
| `extracted` | an extraction pass read the identifier from these bytes |
| `supplied` | an operator supplied it for a file with these bytes |
| `overridden` | an operator accepted the record it reaches over the title check's conflict |
| `adopted` | `borax adopt` recorded these bytes on it |

`supplied` and `overridden` are the **operator origins**.

`overridden` rather than `accepted`, because `acceptance` `accepted`
already means a supplied record taken with no conflict, which is
`supplied` here; `overridden` is that section's word for the conflict
case, and the two vocabularies then agree. An operator who supplied an
identifier and renamed over its conflict gives `overridden`: the
stronger fact, and the one D7 relies on.

**On a history entry.** `HashEntry` gains `evidence:
Option<IdentificationEvidence>`, written by `history_table` after
`item` as two keys, `identifier` and `origin`, only when present, as
change 15 wrote `item`:

```toml
[[history]]
hash = "sha256-…"
run = "20261014T120000Z"
timestamp = "20261014T120000Z"
tool_version = "0.10.0"
item = "0190c3a2-…"
identifier = "doi:10.1039/c5ay00042d"
origin = "extracted"
```

- Both keys or neither. An entry holding one without the other, an
  `identifier` that does not parse, or an `origin` outside the four
  words makes the record a file that does not parse, as an `item` that
  is not a canonical UUID does. `validate` reports it as such a file.
- An entry written before this change holds neither, reads unchanged,
  is never rewritten, and is not a finding.
- `ArtifactRecord::evidence_for(hash)` is the evidence of the newest
  entry holding `hash` that carries any.

**Which evidence each write records.** `FileRecord::identification()`
(D12) computes it for an admission from the file's evidence:

| The record was reached by | `identifier` | `origin` |
|---|---|---|
| the file's own lookup, origin extracted | the lookup's identifier | `extracted` |
| an operator's lookup, acceptance `accepted` | the supplied identifier | `supplied` |
| any lookup, acceptance `overridden` | the lookup's identifier | `overridden` |
| a lookup of the content index's identifier (D7) | that identifier | the index evidence's origin, carried |
| a library reference (D7) | the index entry's evidence, when it holds one | carried; none when it holds none |
| the run's library (tracked) | none | none |

Other writes:

| Write | Evidence |
|---|---|
| `reconcile` step 3, accepted on the extracted identifier | that identifier, `extracted` |
| `reconcile --accept` | none: the operator accepted bytes as the artifact, not an identifier |
| a restatement (change 15's Rule 2) | none: the earlier entry for those bytes keeps its own |
| `adopt` | the identifier its record was found by (D9), `adopted`; none when the record carries none |

An admission of the run's library's own tracked answer appends no entry
in the ordinary case (change 15's Rule 1), so the "none" there costs
nothing.

**Rejected: one `evidence` table on the entry
(`evidence = { identifier = …, origin = … }`).** A history array of
tables would gain a nested inline table per entry, and a person reading
the record would find it harder to scan than two plain keys beside
`item`.

**Rejected: the tier as part of `extracted`.** The pass that read an
identifier is reported at the time, by the event's `extraction`
section. Nothing decided later depends on it.

**Rejected: an origin for `reconcile --accept`.** The decision it
records is "these bytes are this artifact", which the entry itself is,
with its run. An origin with no identifier would be evidence of
nothing.

## D4. When evidence is written

**Decision.**

- **At resolution, where today's record is written** (`pipeline.rs`,
  line 716): a record reached by extraction and a lookup, which the
  title check did not refuse, writes `{lookup identifier, extracted}`.
  The same not-attempted reasons apply. This keeps a batch `resolve`
  outside any library writing evidence for every file it resolves,
  which is what lets a later run skip extraction (decision 3 of the
  brief): yes.
- **At the move, by `remember`**: an operator's record, supplied or
  accepted over a conflict, writes `{identifier, supplied|overridden}`
  under the file's hash, on the same terms as today: only once the file
  moved, never for an abandoned candidate, inside or outside a library,
  under `--no-record` too.
- **Beside a reference** (D5): a reference write also sets the entry's
  evidence to the history entry's, when the entry holds none.
- **The operator-evidence rule.** Evidence of an operator origin is
  replaced only by evidence of an operator origin. A resolution write
  of `extracted` evidence over `supplied` or `overridden` evidence is
  not made, and `content_index.write` reports it not attempted for the
  new reason `operator-evidence`. That write is reached only under
  `--no-cache`, since any other run reading such an entry answers from
  it (D7) and writes nothing. Without the rule, re-extracting a file
  under `--no-cache` would put back the very identifier the operator
  corrected.
- **References are kept** by every evidence write, and evidence by
  every reference write.

The interactive driver keeps its shape. `pipeline::accept` is
unchanged. `pipeline::remember` takes the file record and writes
`file.identification()` instead of the record. The rename driver's
`Settled::CarryOut::remember` keeps meaning "this record was the
operator's".

**Rejected: letting `--no-cache` replace operator evidence, as it
replaces a record today.** Today that loses an operator decision held
only in a cache. The design record forbids exactly that.

**Rejected: writing evidence for an unresolvable identifier.** Today
nothing is written when no service holds the identifier, and an
identifier nobody holds answers nothing later.

## D5. When references are written, and in what order

**Decision.** A reference names a decision the library already holds.
It is written:

- **after an applying rename's admission** that returned `Ok`, written
  or unchanged: minted, updated, changed, separated, relinked. The
  reference is for the admitted bytes' hash and names the admitted
  artifact and `item_for(hash)` of the record afterwards. An admission
  that wrote nothing (its record already described the file) restores a
  missing reference in passing. `pending`, `outside` and `unwritten`
  admissions write none, and the old record of a separation keeps
  whatever references it had;
- **after `adopt`** recorded an orphan, for the orphan's hash;
- **by `reconcile`**, for every record's current hash (D6).

And never otherwise: not by a preview, `--no-record`, `status`,
`validate`, `resolve` or `bib`.

**Only under a present identity.** Immediately before writing any
reference, borax reads `library::identity(root)`, as change 12 asks:

- `Present(id)`: the reference names `id`.
- `Absent`: no reference is written. The run warns once
  (`references::unreferenced_warning`, D11). This is reached when an
  admission made no store write in a library no write has reached since
  0.10, or a concurrent minter failed. Reference writes do not mint: the
  `library` requirement "A library's first store write mints its
  identity" keeps minting to item and record writes.
- `Faulty`: no reference is written, and the run warns once with
  change 12's `identity_fault_warning`, whose text already says the
  library "has none another location can name". The warning is
  deduplicated with the `StoreWriter`'s.

**Write order.** Item, then artifact record with its evidence, both
through the `StoreWriter`, then the reference, by reading the entry,
merging (D6) and replacing the entry atomically
(`write_atomically` through `FileCache::put_entry`). An interruption
leaves one of:

- nothing new (killed before the item);
- an item nothing links to (as today);
- the decision in the library and no reference: the next reconcile, or
  the next applying run or adoption that reaches the file, restores it;
- everything.

It never leaves a reference as the only record of a decision, because
no reference is written before the record it names is on disk.

**Idempotent.** Re-running any step writes nothing new: an item is
selected by identifier rather than minted again, a record that already
describes the file renders unchanged and is not written, and a reference
already in the entry with the same item is not written (D6).

**Reported** as a `content-index-write` event after the file's
`renamed` and `library-admission` events, or after the run's adoption
or repair events (D10). A failed reference write never fails the
rename, the adoption or the reconcile, and never changes an exit status.

**Rejected: writing the reference first, or with the record.** That is
the order the design record forbids: a reference before its record is a
reference to nothing.

**Rejected: minting the identity in order to write a reference.**
Change 12 settled minting on item and record writes and named "nothing
else creates one" in a living requirement. Changing that is change 12's
decision to reopen, not this change's. The cost, a library written by
0.9.0 getting no references until its next store write, is warned
about and listed in Risks.

**Rejected: a lock around read-merge-write.** No lock may be a
precondition (`library`, "Validation defines a well-formed library").
Two concurrent runs merging into one entry can lose one reference. The
library still holds the decision, and the next reconcile restores it.

## D6. Restoring missing references and repairing stale roots

**Decision.**

- **`borax reconcile` restores.** After its pass, it reads the identity
  once and, when `Present(id)`, writes for every artifact record it
  read, missing or pending records included, the reference
  `(id, root, record.id, record.item_for(current hash))` under the
  record's current hash, with `record.evidence_for(current hash)` as
  the evidence to fill. Most of these find the reference there and
  write nothing.
- **Other commands restore only what they touch**: an applying rename,
  the references of the files it admits; `adopt`, those of the orphans
  it records.
- **`status` and `validate` write nothing**, as their requirements say.
  `resolve` and `bib` write evidence and nothing else.

**The upsert** (`IndexEntry::with_reference`, D12), for a reference
`r = (L, R, A, I)`:

1. The entry holds a reference keyed `(L, R, A)`: its `item` becomes
   `I`. That is a write only when `I` differs, which is how a link
   corrected by hand in a library reaches the index at the next
   reconcile.
2. Otherwise the entry holds a reference `(L, R′, A)` whose root `R′`
   is **stale**, meaning that `R′` does not exist, or that it does not
   hold library `L`: no identity file, or another identity. The first
   such reference in the entry's order gets root `R` and item `I`. That
   is the repair. A root that cannot be read is not stale: an unmounted
   or unreadable copy is left alone.
3. Otherwise `r` is appended.

In every case the entry's evidence is set to the history entry's
evidence when the entry holds none. No reference is ever removed,
including one left stale because the entry already holds `(L, R, A)`:
that is a second location that no longer answers, and pruning it is an
explicit operation this change does not provide.

**Cost.** A reconcile reads one entry per record, about 200 bytes of
JSON each, plus a stat and an identity read for a stale-root candidate.
A reconcile over 2,000 records reads 2,000 small files and writes none
once the references are in place. Today an untouched reconcile reads
only the records and stats the artifacts, so this is the new floor. It
is paid by a command that exists to bring the library's derived state
back into agreement.

**Rejected: restoring from every command, `status` and `validate`
included.** Both are read-only by requirement.

**Rejected: restoring a reference for every hash in every history.**
That multiplies the first reconcile by the history's length, for bytes
nobody holds any more. A hash that was accepted after this change got
its reference when it was accepted, and that reference is never pruned.

**Rejected: pruning a stale reference when a fresh one for the same
artifact exists.** That is automatic pruning under another name, and an
unmounted copy would be forgotten.

**Rejected: repairing stale roots on read (`resolve`, `bib`).** Reads
would start writing to answer a question, and the run would have to
know the library is the same one, which only a run inside it does.

## D7. What an index answer means

**Decision.** For a file the run's library does not answer for, with
the cache on and the file hashed (otherwise as today), the entry for
its hash is read:

1. **No entry:** `read` `miss`. Extraction, the lookup, the title
   check, and the evidence write follow, as today.
2. **An entry. Its references first** (D8 gives each its outcome):
   - exactly one reachable reference, or several whose items agree
     (below): its item's record is the file's record. The first
     reachable reference in the entry's order is `answered`, and the
     others `agreed`. Extraction, its titles, the lookup, the title
     check and the evidence write are not attempted for
     `content-index-hit`. `record_retrieval` is `library-reference`
     naming the answering reference. `acceptance` is `automatic`;
   - several reachable references that do not agree: each is reported
     `disagreeing` with the preferred identifier its item carries, and
     none answers.
3. **Then its evidence**, when no reference answered: the evidence's
   identifier is looked up through the run's sources, the response
   cache first where one stands in front, as any lookup is. The lookup's
   origin is `content-index`. Extraction and its titles are not
   attempted for `content-index-hit`, the file is not opened, and **the
   title check is not made** (`content-index-hit`), whatever the
   evidence's origin:
   - for `overridden` evidence, the operator's acceptance stands; that
     is the requirement "Ambiguity is skipped, never guessed" ("SHALL
     NOT be judged again");
   - for the other origins, the check passed when the evidence was
     written, since D4 writes none for a refused record, and the bytes
     are the same.

   Found: the file is resolved, `record_retrieval` is the service's
   response cache or the network, and the evidence write is not
   attempted for `content-index-hit`. Not found: the file is skipped
   `unresolvable`, with the lookup's attempts. Retry is offered as for
   any inconclusive lookup and repeats the same identifier with the same
   origin.
4. **Neither**, an entry whose references all failed to answer and
   which holds no evidence: extraction and the rest follow as on a miss.
   The evidence write then adds evidence to the entry and keeps its
   references.

**Agreement.** Reachable references agree when every one's item carries
the same preferred identifier: the first the record holds in the order
DOI, arXiv, PMID, ISBN, as `pipeline::identifier_of` (`pipeline.rs`,
line 1191) picks the event's `identifier`. Items that carry none agree
only when they are one item: the same item identity in the same library
identity. This narrows the settled rule only where it had nothing to
compare. Two locations of one library are two references, and they
agree when their copies of the item do.

**A run inside a library.** Its own tracked files are answered by the
library before the index is read, as today, so they are never ambiguous.
An untracked file inside it, such as a byte-identical copy of a tracked
artifact, reads the index like any file. A reference to its own library
answers from the item, and its `library` section still says `untracked`.

**What the read reports.** `content_index.read` `hit` carries the
entry's `evidence` and every reference's outcome, in the entry's order,
so a fallback through the evidence is always distinguishable from an
answer by a library, and an unavailable library from a disagreeing one
(D10).

**Rejected: evidence before references.** The design record makes a
reachable library authoritative. The evidence is what the entry falls
back on.

**Rejected: re-running the title check on an evidence answer.** It
would open every file the index knows to read its titles, which is half
of what the index exists to save, and for `overridden` evidence it would
judge again what only the operator could judge.

**Rejected: falling back to extraction when the evidence's lookup finds
nothing.** The same bytes would extract the same identifier, and an
outage would cost a needless open. The interactive run offers Retry and
supply as for any unresolvable file.

**Rejected: choosing among disagreeing references by recency, or by
the run's own library.** Settled 3.

## D8. Reading a referenced library

**Decision.** A reference `(L, R, A, I)` is:

| Condition, in order | Outcome |
|---|---|
| `fs::symlink_metadata(R)` is `NotFound` | `unavailable`, cause `missing` |
| any other metadata error | `unavailable`, cause `unreadable`, with `R` and the message |
| `R` is not a directory | `unavailable`, cause `other-library`, no `found` |
| `identity(R)` is `Absent` | `unavailable`, cause `other-library`, no `found` |
| `identity(R)` is `Faulty(fault)` | `unavailable`, cause `unreadable`, with the identity file and its message |
| `identity(R)` is `Present(other)`, `other ≠ L` | `unavailable`, cause `other-library`, `found` = `other` |
| `I` is carried by no item file of `R`, and the item store or a file whose name claims `I` could not be read | `item-unavailable`, cause `unreadable`, with that path and message |
| `I` is carried by no item file | `item-unavailable`, cause `dangling` |
| `I` is carried by several item files | `item-unavailable`, cause `ambiguous`, with the files |
| otherwise | reachable: the item `I`'s record |

The three `item-unavailable` causes are `AcceptedItemCause`'s
(`event.rs`), with its attribution rule (`Stores::item_fault`,
`library.rs`, line 1262).

- **The reference's own item answers.** The referenced library's
  artifact record is not read: the reference names the item the record
  answered with when the reference was written, and D6 keeps it current
  for every record a reconcile or an admission reaches. A link corrected
  by hand inside a library reaches outside runs at that library's next
  reconcile.
- **Read once per run.** A run reads each referenced root at most once,
  when it first needs it, and answers every later reference to that
  root from that reading. `pipeline::resolve_batch` and the rename
  driver hold one `references::Libraries` for the run, and `adopt` holds
  one for its pass. `pipeline::standing` and `resolve_file` keep their
  signatures and use a fresh one per call, so tests that call them
  directly change nothing.
- **Nothing is written** to a referenced library or by reading one.

**Rejected: reading the referenced artifact record and answering with
`item_for(hash)`.** It is more current, but it reads a second store of
another library for every file, and the answer the design record
settles is "its item". The stored item and D6's refresh give the same
answer once the library has been reconciled.

**Rejected: finding the item by file name only.** Item files carry the
identity in their names, but borax "SHALL find an item by reading item
files and trusting that field, never by parsing a file name" (`library`,
"An item file's name carries a legible key and its identity").

## D9. Adoption

**Decision.** For each orphan that is readable and not `held`, as today,
`adopt` reads the entry for its hash, through
`references::adoptable` (D12). It queries no service, extracts nothing
and opens nothing beyond hashing.

1. **A reference that answers**, under D7's rule (one reachable, or
   several agreeing): the item's record. `retrieval` is
   `library-reference`. Its history entry's evidence is the record's
   preferred identifier, origin `adopted`, or none when it carries none.
2. **Otherwise the evidence**, read from the response cache under
   `cache_root` by `cache::key(service, identifier)` for each service of
   `dispatch::priority(identifier)` (`crates/borax-sources/src/dispatch.rs`,
   line 62), in that order, whatever the `sources` setting: `adopt`
   consumes no setting (`cli`, "A subcommand accepts only the settings
   it consumes"), and a stored answer is what that service gave. The
   first entry found is the record. `retrieval` is `service-cache`
   naming the service, and the evidence is `{identifier, adopted}`.
3. **Otherwise** it stays an orphan:
   - `unindexed` when the index holds no entry for its bytes. The kind
     is kept, with its meaning: nothing in the index;
   - `unanswered`, new, when an entry exists and nothing above supplied
     a record. It carries the entry's `identifier` when it held one, and
     every reference's outcome.

After the pass, `adopt_events` writes each recorded orphan's reference
(D5) and reports it. `library::adopt` keeps the `lookup` seam, now
returning `Option<Adopted>` (D12) rather than `Option<Record>`, so the
history entry can carry the evidence and the event its retrieval.

**Rejected: the network as a last resort.** `adopt` is "the offline way
to record a library" by requirement. An orphan whose identifier no
cache holds is one `borax resolve` will fetch, after which `adopt`
records it.

**Rejected: extraction for an orphan with no entry.** Same reason, and
`adopt` "extracts from no document".

**Rejected: renaming `unindexed`.** Its released meaning, "nothing for
these bytes", survives. The new case gets a new kind instead of
widening an old one.

## D10. The schema: 5

The `cli` requirement "JSON Lines output is first-class" changes the
version when "a consumer that reads the stream correctly today could
read a later stream wrongly: an event or a reason that is removed or
renamed, or a field whose meaning changes", and fixes a version at its
first release. Schema 4 was released in 0.9.0. These released keys and
values change meaning or go:

| Key or value | Schema 4 meaning | Now |
|---|---|---|
| `content_index.read` `hit` | the index held a **record**, and no step after it ran | the index held an **entry**. A lookup, or extraction, may follow it |
| `content_index.write` | the **record** written under the hash | the identification **evidence** written |
| `record_retrieval` `content-index` | the record was the index's copy | **removed**: no record is kept there to retrieve |
| `content-index-write` | an interactive run remembered an operator's record at the move | any write of a file's entry after its verdict: evidence at the move, or a reference after an admission, an adoption or a reconcile |
| `library-adoption` `unindexed` | the index held no record for the bytes | the index held no **entry**. An entry that could not supply a record is the new `unanswered` |
| `extraction`, `lookup`, `match_check` not attempted for `content-index-hit` | always together, with no service asked | `lookup` is now attempted after an evidence answer |

So the version is **5**, bumped once, here, before any release carries
5. Later changes in this phase may change schema 5 freely until a
release emits it. The interim value is reset at 1.0.0 (settled 4's
companion, the design record).

What schema 5 adds, all listed for the field table:

- `content_index.read` `hit`: `evidence` (`identifier`, `origin`),
  absent when none, and `references`, always present, possibly empty.
  Each reference outcome is `library`, `root`, `artifact`, `item` and a
  `status`:
  - `answered`, `agreed`;
  - `disagreeing`, with `identifier`, the item's preferred identifier,
    absent when it carries none;
  - `unavailable`, with `cause` `missing`, `other-library` (with
    `found` when another identity is there) or `unreadable` (with
    `path` and `message`);
  - `item-unavailable`, with `cause` `dangling`, `unreadable` (with
    `path` and `message`) or `ambiguous` (with `files`).
- `content_index.write` not attempted: the new reason
  `operator-evidence`.
- `lookup.origin`: the new value `content-index`.
- `record_retrieval`: the new kind `library-reference`, with `library`,
  `root`, `artifact` and `item`.
- `content-index-write`: `hash`, and `entry`, tagged `kind`:
  - `evidence`, with `identifier` and `origin`;
  - `reference`, with `library`, `root`, `artifact`, `item`,
    `repaired_from` (the stale root replaced, only for a repair) and
    `evidence` (only when the write also filled it).

  `write` is `written` or `failed`. The event is emitted for every entry
  write made after a verdict, and for none that was not made.
- `library-adoption` `recorded`: `retrieval`, `library-reference` or
  `service-cache`, as `record_retrieval` spells them. The new kind
  `unanswered` carries `identifier`, absent when none, and
  `references`, absent when empty, as above.

Event order:

- `content-index-write` follows a moved or already-named file's
  `renamed` and `library-admission` events: the reference first, then
  the evidence `remember` wrote.
- In `adopt` and `reconcile`, the `content-index-write` events follow
  the per-object events and any `skipped`, and precede the missed
  lookups and the totals.

**Rejected: keeping 4 by making every change additive.** Keeping
`record_retrieval` `content-index` for reference answers would change
its meaning, and keeping `hit` would change its implication. There is
no additive spelling of "the index no longer holds records".

**Rejected: a new section (`references`) beside `content_index`.** The
references are what the read found, and eight sections in pipeline order
is a requirement a ninth would amend for no gain.

## D11. Human lines and the description

Every value from an entry, a library or a file is escaped, as the text
after `<path>: ` already is in `human_line`.

- **`resolved` retrieval clause:** `from the library at <root>` for
  `library-reference`. `from the content index` goes with its kind.
- **Reference outcomes**, on a `resolved` line or a resolution skip
  line, after any `; the library could not answer: …` clause and before
  the rejected-candidate clause:
  - one clause per unavailable reference: `; a library reference could
    not answer: ` and one of:
    - `<root> is missing`;
    - `<root> holds another library (<found>)`, or `<root> holds no
      library` without `found`;
    - `<path> could not be read (<message>)`;
    - `the library at <root> holds no item <item>`;
    - `the library at <root> holds item <item> in several files`;
  - one clause for disagreement: `; library references disagree: ` then,
    `, `-separated, `the library at <root> names <identifier>` or
    `… names no identifier`.
- **`content-index-write` failed:**
  - evidence: `<path>: the content index could not keep this answer
    (<message>)`, unchanged;
  - reference: `<path>: the content index could not keep its reference
    to the library at <root> (<message>)`.

  A written one renders nothing, as now.
- **`library-adoption` `unanswered`:**
  - with an identifier: `the content index knows its bytes as <id>, but
    no library reference answered and no response cache holds that
    record, so it is still an orphan; borax resolve fetches it`;
  - without one: `the content index holds no identifier for its bytes
    and no library reference answered, so it is still an orphan`.

  `recorded`, `unindexed` and the others are unchanged.
- **Warning** (`references::unreferenced_warning(root, count)`):
  `warning: <root> has no library identity yet, so no content-index
  reference to it was written for <count> records; its next store write
  gives it one, and borax reconcile then writes them`. Once per run,
  after `Aftermath::identity` and before `Aftermath::diagnostic`.
- **The interactive description** (`describe.rs`):
  - identifier line on an evidence answer: `<id>, found by an earlier
    run`, `<id>, supplied in an earlier run`, `<id>, accepted over a
    title conflict in an earlier run` or `<id>, adopted in an earlier
    run`, as the read's evidence `origin` says;
  - on a reference answer, the record's own identifier with no origin,
    as for a library answer, and a `record` line `<services>, from the
    library at <root>`, or `the library at <root>` with none;
  - `file says` keeps `not read; an earlier run answered` for
    `content-index-hit`;
  - the `from an earlier run` record line goes with its kind.

## D12. The interface the tests are written against

`borax-core` (`crates/borax-core/src/library.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceOrigin { Extracted, Supplied, Overridden, Adopted }

impl EvidenceOrigin {
    /// `extracted`, `supplied`, `overridden`, `adopted`.
    pub fn as_str(self) -> &'static str;
    /// The inverse of `as_str`; `None` for any other text.
    pub fn parse(text: &str) -> Option<EvidenceOrigin>;
    /// `Supplied` and `Overridden`.
    pub fn is_operator(self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentificationEvidence {
    pub identifier: Identifier,
    pub origin: EvidenceOrigin,
}

// HashEntry gains, after `item`:
//     pub evidence: Option<IdentificationEvidence>,
// written as `identifier` and `origin` (D3), both or neither.

impl ArtifactRecord {
    /// The evidence of the newest entry holding `hash` that carries
    /// any.
    pub fn evidence_for(&self, hash: &ContentHash) -> Option<&IdentificationEvidence>;
}
```

`borax-core`, a new module `crates/borax-core/src/index.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryReference {
    pub library: LibraryId,
    pub root: PathBuf,
    pub artifact: ArtifactId,
    pub item: ItemId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexEntry {
    pub evidence: Option<IdentificationEvidence>,
    pub references: Vec<LibraryReference>,
}

/// What `IndexEntry::with_reference` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referred {
    pub entry: IndexEntry,
    /// Whether `entry` differs from the one it was made from.
    pub changed: bool,
    /// The stale root a repair replaced (D6, step 2).
    pub repaired_from: Option<PathBuf>,
    /// Whether the evidence was filled.
    pub filled: bool,
}

impl IndexEntry {
    /// D2's JSON; keys absent when empty.
    pub fn to_json(&self) -> String;
    /// Unknown keys ignored; anything else D2 refuses is `Malformed`.
    pub fn from_json(text: &str) -> Result<IndexEntry, Malformed>;
    /// D4: this entry with `evidence`, references kept; `None` when the
    /// operator-evidence rule withholds it.
    pub fn with_evidence(&self, evidence: &IdentificationEvidence) -> Option<IndexEntry>;
    /// D6's upsert. `stale` says whether a reference's root no longer
    /// holds its library; it is asked only about step-2 candidates.
    pub fn with_reference(
        &self,
        reference: &LibraryReference,
        evidence: Option<&IdentificationEvidence>,
        stale: &dyn Fn(&LibraryReference) -> bool,
    ) -> Referred;
}
```

`borax-sources` (`crates/borax-sources/src/store.rs`):

```rust
pub const INDEX_FORMAT_VERSION: &str = "v2";
/// D1.
pub fn data_root(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf>;
pub fn default_data_root() -> Option<PathBuf>;

/// Where content-index entries are kept. `Sync` as `Cache` is.
pub trait IndexStore: Sync {
    /// The entry under `key`; `None` when absent, unreadable or
    /// unparseable (D2).
    fn entry(&self, key: &str) -> Option<IndexEntry>;
    fn put_entry(&self, key: &str, entry: &IndexEntry) -> CacheWrite;
}
impl IndexStore for FileCache { /* entry_path(root, key), JSON */ }
impl IndexStore for MemoryCache { /* a second map */ }

pub struct ContentIndex<S> { /* store */ }
impl<S: IndexStore> ContentIndex<S> {
    pub fn new(store: S) -> ContentIndex<S>;
    pub fn get(&self, hash: &ContentHash) -> Option<IndexEntry>;
    pub fn put(&self, hash: &ContentHash, entry: &IndexEntry) -> CacheWrite;
}
```

`ContentIndex::put(hash, &Record)` is gone. `content_key` and
`entry_path` are unchanged.

`borax` (`crates/borax/src/`):

```rust
// evidence.rs
pub enum IndexRead { Hit(IndexHit), Miss, Bypassed, Unavailable { message: String }, NotAttempted(Unattempted) }
pub struct IndexHit {
    pub evidence: Option<IdentificationEvidence>,
    pub references: Vec<crate::event::ReferenceOutcome>,
}
pub enum Origin { Extracted(Tier), Operator, Indexed(EvidenceOrigin) }
pub enum RecordRetrieval { Library { artifact, item }, LibraryReference(LibraryReference), ServiceCache { service }, Network { service } }
// Unattempted gains OperatorEvidence ("operator-evidence").

// pipeline.rs
impl FileRecord {
    /// D3's first table.
    pub fn identification(&self) -> Option<IdentificationEvidence>;
}
/// D4: writes `file.identification()`; `NotAttempted(Unhashable)` with
/// no hash, as now. Never withheld: its evidence is an operator's.
pub fn remember<S: IndexStore>(index: &ContentIndex<S>, file: &FileRecord) -> IndexWrite;
// index_read, standing, resolve_file, resolve_batch: signatures unchanged
// but for `C: Cache` becoming `S: IndexStore`.

// references.rs (new)
pub struct Libraries { /* the per-run reading, D8 */ }
impl Libraries { pub fn new() -> Libraries; }
/// Each reference's outcome (D8), in order, and the answering item (D7).
pub fn answer(entry: &IndexEntry, libraries: &Libraries) -> Answered;
pub struct Answered { pub outcomes: Vec<ReferenceOutcome>, pub item: Option<(LibraryReference, Item)> }
/// D5/D6: write `reference` (and `evidence` to fill) under `hash`;
/// `None` when the entry already held it.
pub fn refer<S: IndexStore>(
    index: &ContentIndex<S>,
    hash: &ContentHash,
    reference: &LibraryReference,
    evidence: Option<&IdentificationEvidence>,
) -> Option<EntryWrite>;
/// D6: `reconcile`'s restoration over every record of `root`.
pub fn restore<S: IndexStore>(root: &Path, index: &ContentIndex<S>) -> Restored;
pub struct Restored { pub writes: Vec<EntryWrite>, pub withheld: Withheld }
pub enum Withheld { Nothing, NoIdentity(usize), Faulty(StoreFault) }
pub struct EntryWrite { pub path: PathBuf, pub hash: ContentHash, pub entry: WrittenEntry, pub write: CacheWrite }
/// D9's lookup for `adopt`. `responses` is the response cache under
/// `Adapters::cache_root`, `None` when there is none.
pub fn adoptable<S: IndexStore>(index: &ContentIndex<S>, responses: Option<&FileCache>, libraries: &Libraries, hash: &ContentHash) -> Result<Adopted, Unadopted>;
pub fn unreferenced_warning(root: &Path, count: usize) -> Diagnostic;

// library.rs
// Admitting gains `pub evidence: Option<&'a IdentificationEvidence>`,
// written on the entry an admission appends.
pub struct Adopted { pub record: Record, pub evidence: Option<IdentificationEvidence>, pub retrieval: RetrievedFrom }
pub fn adopt(root: &Path, lookup: &dyn Fn(&ContentHash) -> Result<Adopted, Unadopted>, key: …, run: RunId, timestamp: &str, tool_version: &str) -> Adoptions;
pub enum Unadopted { Unindexed, Unanswered { identifier: Option<String>, references: Vec<ReferenceOutcome> } }

// event.rs
pub const SCHEMA: u32 = 5;
pub struct ReferenceOutcome { pub library: String, pub root: PathBuf, pub artifact: String, pub item: String, #[serde(flatten)] pub status: ReferenceStatus }
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ReferenceStatus { Answered, Agreed, Disagreeing { identifier: Option<String> }, Unavailable { #[serde(flatten)] cause: UnavailableCause }, ItemUnavailable { #[serde(flatten)] cause: AcceptedItemCause } }
#[serde(tag = "cause", rename_all = "kebab-case")]
pub enum UnavailableCause { Missing, OtherLibrary { found: Option<String> }, Unreadable { path: PathBuf, message: String } }
// EvidenceStep { identifier: String, origin: String }, as the stream carries evidence
// IndexReadStep::Hit { evidence: Option<EvidenceStep>, references: Vec<ReferenceOutcome> }
// IdentifierOrigin gains ContentIndex ("content-index")
// RetrievedFrom gains LibraryReference { library, root, artifact, item }; loses ContentIndex
// Event::ContentIndexWrite { path, hash, entry: WrittenEntry, write: WriteStep }
// WrittenEntry: #[serde(tag = "kind")] Evidence { identifier, origin } | Reference { library, root, artifact, item, repaired_from: Option<PathBuf>, evidence: Option<EvidenceStep> }
// Adoption::Recorded { id, item, retrieval: RetrievedFrom }; Adoption::Unanswered { identifier: Option<String>, references: Vec<ReferenceOutcome> }
// optional keys are #[serde(default, skip_serializing_if = …)]

// run.rs
// Adapters<'a, S: IndexStore> (was C: Cache); Aftermath gains `unreferenced: Option<Diagnostic>`.
```

Keeping their signatures: `admit`, `Admitted`, `reconcile`,
`Reconciling`, `Stores::consult`, `standing`, `resolve_file`,
`resolve_batch`, `resolve_supplied`, `accept`, `identity`,
`ensure_identity`, `StoreWriter`.

The private parts are the implementer's: how the rename driver passes
the index and a `Libraries` to `recorded`, how the retry finds the
file's own lookup (`LookupEvidence::extracted` widened, or a sibling),
and the enum `execute` opens the index over.

## D13. Sequences traced

`H` is a file's hash, `L` a library identity, `R1` and `R2` roots.
"Outside" is a run over a copy of the file with no library above it,
online unless stated.

**(a) An admission interrupted before its reference.**
1. `borax rename --apply` in `L` at `R1` resolves `paper.pdf` by
   extraction. The entry `H` gets `{doi, extracted}`.
2. The run moves the file, writes the item and the record (entry
   naming `doi`, `extracted`), and is killed.

State: the library holds the decision. Entry `H` holds evidence and no
reference.

- Outside: `hit`, `references: []`. The DOI is looked up (origin
  `content-index`), the response cache answers, and `record_retrieval`
  is `service-cache`.
- Inside, the next `borax reconcile`: one `content-index-write`
  (`reference`). The next applying rename over the file is an unchanged
  admission and writes the same reference.

Either way the next outside run is answered by `library-reference`.

**(b) A library copied, then a file resolved outside both.** `L` at
`R1` is `rsync`ed to `R2`. Then `reconcile` runs at `R2`.

State: entry `H` holds `(L,R1,A,I)` and `(L,R2,A,I)`. `R1` holds `L`,
so it is not stale, and the second is appended.

- Outside: both reachable, the same DOI. `(L,R1)` is `answered` and
  `(L,R2)` `agreed`, and `record_retrieval` names `R1`.

**(c) One copy deleted.** `R1` is removed.

- Outside: `(L,R1)` is `unavailable` `missing`, and `(L,R2)` answers
  alone. The human line ends `from the library at R2; a library
  reference could not answer: R1 is missing`.
- `reconcile` at `R2`: step 1 finds `(L,R2,A)`, so nothing is written,
  and `(L,R1,A)` stays, unpruned.

**(d) The copies' items disagree.** In (b), the item at `R2` is
corrected by hand to another DOI.

- Outside: both reachable, with different preferred identifiers. Both
  are `disagreeing` with their DOIs. The entry's evidence DOI is looked
  up, and `lookup.origin` is `content-index`. The human line ends `;
  library references disagree: the library at R1 names doi:X, the
  library at R2 names doi:Y`.
- With no evidence in the entry, extraction runs and the evidence write
  adds it.

**(e) A library moved.** `L` moves from `R1` to `R3`, and entry `H`
holds `(L,R1,A,I)`.

- Outside: `unavailable` `missing`, then the evidence fallback.
- Inside `R3`, `resolve` of the tracked file: the library answers, and
  the index is not read.
- `reconcile` at `R3`: step 2 finds `(L,R1,A)` stale (missing) and
  rewrites it to `R3`. The event is `content-index-write` with
  `repaired_from: R1`.
- Outside afterwards: `answered` from `R3`.

If `R1` had been an unmounted copy rather than a move, the same repair
happens: the reference now names a location that answers. The design
record accepts this as the repair it asks for.

**(f) `cache --clear`, then `adopt` offline into `L2`.** The response
cache root is removed and the data root is untouched.

- An orphan whose entry references reachable `L`: recorded from `L`'s
  item. `retrieval` is `library-reference`, and the history entry has
  `origin` `adopted`. Then a `content-index-write` for `(L2,R,…)`.
- An orphan with only `{doi, extracted}`: `unanswered` with `doi`, and
  the human line says `borax resolve` fetches it.
- An orphan with no entry: `unindexed`. Exit 0 in every case.

**(g) An operator's answer outside any library.** Interactive rename
of `scan.pdf`, the operator supplies `doi:X`, and the file moves.

- `content-index-write` `{evidence, doi:X, supplied}`.
- `borax cache --clear`, then a batch run online: `hit`, lookup `doi:X`
  from the network, no question, no extraction.
- The same offline: `unresolvable` with every attempt `unavailable`.
  The identifier is not lost, and the next online run resolves it.

**(h) A library written by 0.9.0.** Records exist, there is no identity
and no reference.

- `borax rename --apply` over an already-recorded file: an unchanged
  admission, so no store write and no mint. The identity is `Absent`,
  no reference is written, and the run warns once.
- After any store write (a new file renamed in, an adoption, a repair),
  the identity is `Present`. The next reconcile restores every record's
  reference.

**(i) `--no-cache` over operator evidence.** Entry `H` holds `{doi:X,
supplied}`.

- `borax resolve --no-cache`: `read` `bypassed`, extraction finds
  `doi:Y` and resolves it, and `content_index.write` is not attempted
  for `operator-evidence`. The entry still holds `doi:X`.

**(j) An untracked copy inside the library.** `copy.pdf` holds a
tracked artifact's bytes, and entry `H` holds `(L,R1,A,I)`.

- `resolve copy.pdf`: `library` is `untracked`, `hit` `(L,R1)`
  `answered`, and `record_retrieval` `library-reference`.
- A batch rename's content check skips it as a content duplicate before
  any of this, as today.

**(k) A referenced library whose identity file is damaged.**

- Outside: `unavailable` `unreadable` with `.borax/library.toml` and
  the parser's message, then the evidence fallback.
- `reconcile` inside it writes no reference, and its run warns once
  with `identity_fault_warning`.

**(l) An admission whose reference write fails.** The data directory is
read-only.

- `renamed`, then `content-index-write` `reference` `failed` with the
  store's message, and the rename and the record stand. Exit as
  without it.
- The next reconcile with a writable directory restores the reference.

## D14. How the specifications change

ADDED, in `resolution`:

- "The content index keeps identification evidence and library
  references", covering D1, D2, D3's index half and D4. It also defines
  "a record written to, held by or served from the content index" for
  the requirements that keep that phrase.
- "A content-index entry answers through its references, then its
  identifier", covering D7 and D8.

ADDED, in `library`:

- "A library decision is referenced from the content index after it is
  written", covering D5 and D6.

MODIFIED, each restated whole with every living paragraph and scenario
name kept, so no `<!-- drops: -->` marker is used. In `resolution`:

- "Responses are cached locally": the index is not a cache; `--no-cache`
  bypasses reading it; `cache --clear` leaves it. The renamed-file
  scenario goes through the response cache, and a scenario is added.
- "A resolution reports the evidence it was checked against": the two
  content-index scenarios.
- "An operator's answer about a file is remembered": the library is the
  durable home; evidence rather than records; two scenarios added.
- "A file its library tracks resolves from its library item": the
  copy's paragraph and scenario, now answered by a reference.
- "A resolution says whether its library answered": schema 5 in its
  stream scenario; its human-line scenario.
- "A resolution keeps every service it asked, in order": the content
  index as an origin, and retry keeping it.
- "A resolution says where its record came from": a library reference
  as a place, replacing the content index.
- "A resolution says what the content index did": a held entry, and the
  `operator-evidence` reason.
- "A resolution says why a step was not taken": the content-index
  scenario split.
- "Resolution events carry their evidence in sections": schema 5, the
  reason list, and the in-sections scenario.
- "A resolution reports what the content index did and every cache
  write": `hit`'s `evidence` and `references`.
- "Each service asked is reported with its outcome": `origin`
  `content-index`.
- "A resolution reports where its record was retrieved":
  `library-reference`.
- "What became of remembering an operator's answer is reported": the
  event covers reference writes.
- "Human resolution lines name the work, where it came from, and why a
  file was skipped": the new clauses.

In `library`:

- "An artifact record is authoritative state keyed by an artifact
  identity": entries carry evidence.
- "The library's text is authoritative and is read directly": the
  content index is not a cache.
- "Validation defines a well-formed library": evidence is never a
  finding.
- "borax adopt records what the library already holds": D9.

In `rename`:

- "An interactive question shows what the answer rests on": the
  identifier's origin on an evidence answer, and the library's root on
  a reference answer.
- "An interactive question says whether the library answered": its
  scenario for a library that could not answer.

Checked and left unchanged (the proposal lists them with reasons):

- `resolution`: "Ambiguity is skipped, never guessed", "A library that
  cannot answer for a file says so", "A failed cache write is evidence,
  not a failure", "A refused record is kept as evidence and never
  remembered" and "A resolution says what the title check concluded";
- `library`: "An applying run records what it admits", "Reconciliation
  repairs a stale path by a bounded walk", "A library carries an
  identity that does not depend on where it is" and "A library's first
  store write mints its identity";
- `ledger`, `extraction`, `bib-output`, `cli`, `run-logs`: unchanged.

The search behind this list covered `openspec/specs/` for `content
index`, `content-index`, `content_index`, `index`, `cache --clear`,
`earlier run`, `schema version`, `schema 4`, `version 4`, `origin`,
`unindexed` and `record_retrieval`.

## D15. Existing tests: what changes and how

The rules of the earlier changes apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed.

The searches were `rg` over `crates/*/tests` for:

- `index.put(`, `index.get(`, `ContentIndex::new`, `ContentIndex<`,
  `HashEntry {`, `Admitting {`;
- `RetrievedFrom::ContentIndex`, `RecordRetrieval::ContentIndex`,
  `IndexReadStep::Hit`, `IndexRead::Hit`, `content-index-hit`,
  `ContentIndexHit`, `from the content index`, `"content-index"`;
- `ContentIndexWrite`, `content-index-write`, `remember(`,
  `Adoption::Unindexed`, `unindexed`;
- `SCHEMA, 4`, `"schema":4`, `cache_root`, `FORMAT_VERSION`,
  `content_key`, `earlier run`.

**Unchanged.** Every `ContentIndex::new(MemoryCache::new())` (399
sites) and every `index: &index` in an `Adapters` literal (295) whose
test neither seeds nor reads an entry. That holds because `MemoryCache`
implements `IndexStore` (D2).

**Shape-only (a):**

| File | Site | Edit |
|---|---|---|
| `crates/borax-core/tests/library.rs` | `hash_entry` (line 39) | `evidence: None` |
| `crates/borax/tests/library.rs` | `hash_entry` (101), `entry_naming` (112), `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded` (2419, the literal it builds) | `evidence: None`. The last one's reconcile now appends evidence; see (b) |
| | the 37 `Admitting { … }` literals, `admitting` (9348) among them | `evidence: None` |
| `crates/borax/tests/dispatch.rs` | `lib_hash_entry` (7180), `hash_entry_for` (10602), `seed_tracked_file` (17167), `seed_artifact_record` (17808), `resolve_answers_restored_bytes_from_the_item_they_were_accepted_under` (17707, 17714) | `evidence: None` |
| `crates/borax/tests/pipeline.rs` | `record_at` (3052) | `evidence: None` |
| `crates/borax/tests/binary.rs` | `seed_tracked_binary_fixture` (401) | `evidence: None` |
| `crates/borax/tests/dispatch.rs` | `adopt_over` (14931) | none: its `ContentIndex<MemoryCache>` still names the type |

**Deliberate rewrites (b).** The general translation for a test that
seeds a record:

- the seeded `index.put(&hash, &record)` becomes
  `index.put(&hash, &IndexEntry { evidence: Some({record's DOI,
  extracted}), references: vec![] })`;
- a `fake_source` (or `FakeSource`) holding `record` under that DOI is
  added where the test had none;
- the fact the test pins (not opened, gated, provenance named, no
  service asked beyond the evidence's lookup) is restated for an
  evidence answer.

A test whose fact is "the index answers with no service at all" seeds a
reference to a `real_library` holding the record's item instead.

| File | Test | New form |
|---|---|---|
| `crates/borax-sources/tests/store.rs` | `content_index_round_trips_a_record`, `content_index_put_returns_written_over_memory_cache`, `content_index_put_returns_what_its_cache_returned`, `content_index_misses_an_unknown_hash`, `renamed_file_with_identical_content_is_served_from_the_content_index`, `entries_survive_a_fresh_file_cache_opened_on_the_same_root` | the same facts over `IndexEntry`; each test's name keeps its subject with `record` read as `entry` where it says so: `content_index_round_trips_an_entry` and `renamed_file_with_identical_content_is_answered_by_the_content_index` |
| `crates/borax/tests/pipeline.rs` | `content_index_hit_is_returned_without_opening_the_file`, `a_content_index_hit_marks_every_later_section_not_attempted`, `a_content_match_at_the_incoming_path_is_not_a_duplicate_and_resolves_from_the_index`, `standing_defaults_identifier_input_to_not_asked_on_every_verdict_but_a_content_duplicate` | seeded with evidence and a source. The lookup is now attempted with origin `content-index`, and extraction, titles and the title check are not attempted for `content-index-hit` |
| | `a_content_index_hit_reports_its_record_retrieval_as_content_index` | renamed `a_content_index_evidence_answer_reports_the_service_as_its_record_retrieval` |
| | `cache_false_bypasses_the_content_index`, `bypassed_with_cache_false_still_writes_the_index` | the seeded entry is evidence, and what is written is evidence |
| | `a_successful_resolution_is_written_to_the_content_index`, `a_successful_resolution_is_written_to_the_index_even_with_cache_bypassed`, `a_dangling_item_problem_falls_back_and_writes_the_content_index` | `index.get` asserts `{doi, extracted}` evidence |
| | `a_conflict_candidate_keeps_its_whole_evidence`, `a_refused_record_is_never_remembered_and_a_later_run_still_conflicts` | `index.get(&hash)` is `None` |
| | `an_earlier_index_entry_survives_a_no_cache_conflict` | the earlier entry is evidence and survives |
| | `remember_writes_a_record_a_later_resolve_then_answers_with` | renamed `remember_writes_evidence_a_later_resolve_then_looks_up` |
| | `remember_over_a_write_failing_cache_reports_the_failure`, `remember_with_no_hash_is_not_attempted_as_unhashable` | `remember(index, &file)` |
| | `a_second_identical_batch_is_served_from_the_index_and_never_touches_a_source` | renamed `a_second_identical_batch_is_answered_through_the_index_and_never_extracts`. The source's response cache answers. The fact kept is no extraction and no network |
| | `a_renamed_file_with_identical_content_is_served_from_the_index_without_opening_or_resolving` | the same, through a reference to a `real_library` |
| | `resolve_supplied_after_a_content_index_hit_re_reads_titles_and_keeps_the_read`, `prior_content_index_hit`, `evidence_via_content_index_hit`, `sections_projects_every_index_read_and_write`, `sections_projects_record_retrieval_for_every_place_and_none` | `IndexRead::Hit(IndexHit { … })`. `RecordRetrieval::ContentIndex` becomes `LibraryReference` |
| | `default_content_index`, `evidence_via_lookup`, `every_reason_in_a_projected_sections_is_a_known_unattempted_reason`, `unattempted_as_str_is_kebab_case_for_every_reason` | kept, and extended with `operator-evidence` where they enumerate reasons |
| `crates/borax/tests/dispatch.rs` | `a_batch_cached_resolution_names_its_provenance_not_the_cache`, `an_untracked_file_answered_by_the_content_index_is_not_opened_for_the_gate`, `a_content_index_answer_at_a_recorded_path_with_changed_bytes_is_gated_on_extraction`, `resolve_reports_untracked_for_an_unrecorded_file_inside_a_library`, `resolve_reports_the_librarys_corrected_item_over_a_stale_index_entry` | the general translation |
| | `a_rename_from_a_supplied_identifier_is_written_to_the_content_index`, `skipping_after_a_supplied_identifier_leaves_the_content_index_as_it_was`, `quitting_after_a_supplied_identifier_leaves_the_content_index_as_it_was`, `skipping_a_conflict_that_could_have_been_overridden_writes_nothing`, `a_supplied_identifier_resolving_into_a_taken_target_reports_and_reasks`, `a_content_index_write_that_fails_leaves_the_rename_standing_and_asks_again_next_time`, `a_resolved_candidate_that_led_nowhere_leaves_the_files_own_record_to_cite`, `a_rejected_candidate_is_never_written_to_the_content_index` | `index.get` asserts evidence (`supplied`), or `None`, or the earlier evidence unchanged |
| | `resolve_no_cache_still_reads_the_library_and_writes_nothing_to_the_index`, `interactive_tracked_file_offers_the_move_question_and_writes_nothing_to_the_index`, `tracked_file_already_named_reports_already_named_and_writes_nothing` | `index.get` is `None` as an `IndexEntry`. For a run that records, a reference may now exist; these runs record nothing or only resolve, so `None` holds |
| | `a_supplied_identifier_renamed_is_followed_by_content_index_write`, `a_rename_time_content_index_write_failure_is_reported_without_failing_the_rename`, `sections_for`, `accepting_a_supplied_candidate_with_agreeing_titles_reports_accepted`, `the_round_two_session_reports_every_submission_on_the_final_skip`, `a_batch_run_over_three_artifacts_of_one_item_skips_none_of_them` | `ContentIndexWrite` gains `hash` and `entry`. A run that records in a library now also emits the reference's `content-index-write` before the evidence's. Each assertion on the event sequence states both |
| | `adopt_records_each_orphan_the_content_index_answers_for`, `adopt_leaves_the_unknown_and_the_recorded_alone_and_is_idempotent`, `adopt_holds_an_orphan_whose_bytes_a_record_already_holds`, `adopt_every_orphan_is_accounted_for`, `adopt_outside_any_library_is_refused_and_writes_nothing`, `an_adoption_into_a_library_with_a_faulty_identity_file_warns_once` | seeded with evidence and a response-cache entry under `cache_root` (a `tempdir`) by `FileCache::put(&cache::key(SourceName::Crossref, &doi), &record)`. `Recorded` gains `retrieval` `service-cache` |
| | `adopt_after_the_cache_is_cleared_adopts_nothing_and_succeeds`, `adopt_after_the_cache_is_cleared_still_tells_unindexed_held_and_unreadable_apart` | the entries survive the clear: an evidence-only orphan is now `unanswered` with its identifier, a no-entry orphan still `unindexed`. The names keep their subject; the first is renamed `adopt_after_the_cache_is_cleared_records_nothing_from_evidence_alone_and_succeeds` |
| | `adopt_human_mode_lists_an_unindexed_orphan_before_the_totals_line`, `adopt_human_mode_ends_on_its_own_totals_line_with_no_summary` | kept: their orphans have no entry |
| `crates/borax/tests/end_to_end.rs` | `a_rename_from_a_supplied_identifier_is_recognised_offline_by_a_later_batch_run` | the later run is answered by evidence through the response cache. "Offline" means no network, which a seeded response cache gives |
| | the two `from the content index` assertions | `from the response cache` |
| `crates/borax/tests/binary.rs` | `seed_stale_content_index` and its caller `resolve_reports_a_tracked_file_the_same_way_for_every_relative_spelling` | the stale entry is seeded as v2 evidence for another DOI under `XDG_DATA_HOME/borax/v2/content/`, and the run sets `XDG_DATA_HOME` |
| `crates/borax/tests/event.rs` | `schema_is_four` | renamed `schema_is_five`, asserting 5 |
| | every `"schema":4` JSON literal (`json_line_of_library_extraction_matches_design_d4_for_every_kind`, `json_line_of_library_condition_matches_design_d3_for_every_kind`, `json_line_of_library_adoption_unindexed_matches_design_d6`, `library_status_without_an_identity_is_written_without_an_id_key`, `library_status_carries_the_id_directly_after_the_root`, `library_validated_without_an_identity_is_written_without_an_id_key`, `library_validated_carries_the_id_directly_after_the_root`) | `"schema":5` |
| | `all_events`, `content_index_write`, `sections_via_content_index`, `evidence_via_content_index_hit`, `a_content_index_answer_whose_provenance_names_crossref_reports_crossref`, `human_line_appends_the_library_problem_clause_to_a_content_index_resolved_line`, `human_line_of_resolved_shows_the_work_and_the_from_clause` | the content-index retrieval becomes `library-reference` and its clause `from the library at <root>`. `ContentIndexWrite` gains `hash` and `entry` |
| `crates/borax/tests/describe.rs` | `sections` and the tests built on it naming `content-index-hit` or `RetrievedFrom::ContentIndex` | the record line `…, from an earlier run` becomes `…, from the library at <root>` for a reference answer. An evidence answer's identifier line names its origin (D11) |
| `crates/borax/tests/renaming.rs` | `sections_via_content_index` | `RetrievedFrom::LibraryReference` |
| `crates/borax/tests/runlog.rs` | `run_log_contains_resolved_renamed_and_content_index_write_in_order`, `run_log_reports_a_failed_content_index_write_without_failing_the_rename`, `run_log_contains_exactly_the_json_stdout_stream_including_framing_events` | the event's new fields |
| `crates/borax/tests/library.rs` | `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded` | its appended entry carries the extracted evidence |
| | the four `library::adopt(` calls, `adopt_that_records_nothing_mints_nothing` among them | the `lookup` closure returns `Result<Adopted, Unadopted>` |

**Deletions (c):** none. No test's fact disappears. The ones whose
subject was "a record in the index" are renamed above, and the rename is
the rewrite.

A test the searches did not find, which runs twice over one index and
expects the second run to be answered with no service asked, will fail
on its source count. The test stage reports each such test with the
translation it applied, rather than guessing silently. Line numbers
drift as earlier groups edit a file.

## Not changed, deliberately

- What any library answers for its own files, and that the library is
  asked first.
- The duplicate checks, the acceptance gate (change 15), and minting
  (change 12). The gate extracts from a file whose extraction did not
  run, as today, even where the index's evidence for those bytes is
  `extracted`.
- `FileCache::clear`, `cache::inspect`, `borax cache`'s events, and the
  response cache's location and format.
- `status`, `validate` and `--no-record`: none of them writes to the
  index more than today.
- The order of an admission's library writes.
- `run-started`. Change 12 deferred naming the library's identity there
  to this change. A reference reads the identity when it is written, so
  nothing needs it.

## Risks / Trade-offs

- **Offline resolution outside a library now needs the response
  cache.** Settled. `cache --clear` then going offline turns a 0.9.0
  index hit into an `unresolvable` skip. The identifier is kept, and the
  next online run resolves it.
- **A library written by 0.9.0 gets no references** until something
  writes an item or a record into it. Each applying rename or reconcile
  over it warns. Reopening change 12's minting rule would close this.
- **Concurrent runs can lose a reference** in a read-merge-write race.
  The library keeps the decision, and the next reconcile restores it.
- **A stale reference stays forever** once the entry holds a fresh one
  for the same artifact. It is reported `unavailable` on every outside
  run over those bytes, and pruning is deferred.
- **A hand-edited link reaches outside runs at the next reconcile**, not
  at once (D8).
- **Reconcile reads one entry per record** (D6).
- **The test sweep is large** (D15). It is mechanical where the type
  changes and deliberate where the index used to answer by itself.
- **An entry that does not parse is replaced by the next write** to it.
  Nobody writes entries but borax, and a write is atomic, so such an
  entry is one a person or another program damaged.
- **Two locations whose item carries no identifier** agree only as one
  item identity (D7). Two items of different libraries with no
  identifier never agree.
- **Windows roams the index** (D1). Its roots are local paths, which
  another machine reports unavailable until a run inside the library
  there repairs them.

## Illustrative schema-5 lines

An evidence answer outside any library:

```json
{"schema":5,"event":"resolved","path":"/home/u/Downloads/paper.pdf","identifier":"doi:10.1039/c5ay00042d","record":{…},"library":{"status":"not-attempted","reason":"no-library"},"content_index":{"read":{"status":"hit","evidence":{"identifier":"doi:10.1039/c5ay00042d","origin":"extracted"},"references":[]},"write":{"status":"not-attempted","reason":"content-index-hit"}},"extraction":{"result":{"status":"not-attempted","reason":"content-index-hit"},"titles":{"status":"not-attempted","reason":"content-index-hit"}},"identifier_input":{"status":"not-attempted","reason":"not-asked"},"lookup":{"status":"attempted","identifier":"doi:10.1039/c5ay00042d","origin":"content-index","attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"service-cache"}}]},"record_retrieval":{"kind":"service-cache","service":"crossref"},"match_check":{"status":"not-attempted","reason":"content-index-hit"},"acceptance":"automatic"}
```

A reference answer, one other location unavailable:

```json
{"schema":5,"event":"resolved","path":"/tmp/copy.pdf","identifier":"doi:10.1039/c5ay00042d","record":{…},"library":{"status":"not-attempted","reason":"no-library"},"content_index":{"read":{"status":"hit","references":[{"library":"0190c3a2-7f21-7abc-8def-0123456789ab","root":"/mnt/old","artifact":"0190c3a2-…","item":"0190c3a2-…","status":"unavailable","cause":"missing"},{"library":"0190c3a2-7f21-7abc-8def-0123456789ab","root":"/home/u/papers","artifact":"0190c3a2-…","item":"0190c3a2-…","status":"answered"}]},"write":{"status":"not-attempted","reason":"content-index-hit"}},"extraction":{…},"identifier_input":{…},"lookup":{"status":"not-attempted","reason":"content-index-hit"},"record_retrieval":{"kind":"library-reference","library":"0190c3a2-7f21-7abc-8def-0123456789ab","root":"/home/u/papers","artifact":"0190c3a2-…","item":"0190c3a2-…"},"match_check":{"status":"not-attempted","reason":"content-index-hit"},"acceptance":"automatic"}
```

Disagreement, then the evidence:

```json
"content_index":{"read":{"status":"hit","evidence":{"identifier":"doi:10.1000/x","origin":"supplied"},"references":[{"library":"…","root":"/a","artifact":"…","item":"…","status":"disagreeing","identifier":"doi:10.1000/x"},{"library":"…","root":"/b","artifact":"…","item":"…","status":"disagreeing","identifier":"doi:10.1000/y"}]},"write":{"status":"not-attempted","reason":"content-index-hit"}}
```

Writes:

```json
{"schema":5,"event":"content-index-write","path":"/home/u/papers/Smith2015.pdf","hash":"sha256-…","entry":{"kind":"reference","library":"0190c3a2-…","root":"/home/u/papers","artifact":"0190c3a2-…","item":"0190c3a2-…"},"write":{"status":"written"}}
{"schema":5,"event":"content-index-write","path":"/home/u/papers/Smith2015.pdf","hash":"sha256-…","entry":{"kind":"evidence","identifier":"doi:10.1000/x","origin":"supplied"},"write":{"status":"written"}}
{"schema":5,"event":"content-index-write","path":"/home/u/papers/Smith2015.pdf","hash":"sha256-…","entry":{"kind":"reference","library":"0190c3a2-…","root":"/home/u/papers","artifact":"0190c3a2-…","item":"0190c3a2-…","repaired_from":"/mnt/old"},"write":{"status":"written"}}
```

Adoption:

```json
{"schema":5,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"recorded","id":"0190…","item":"0190…","retrieval":{"kind":"service-cache","service":"crossref"}}}
{"schema":5,"event":"library-adoption","path":"scan.pdf","adoption":{"kind":"unanswered","identifier":"doi:10.1000/z"}}
{"schema":5,"event":"library-adoption","path":"other.pdf","adoption":{"kind":"unindexed"}}
```

Human lines:

```text
/tmp/copy.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the library at /home/u/papers; a library reference could not answer: /mnt/old is missing
/tmp/x.pdf: resolved doi:10.1000/x to "X" (Ng, 2020) via crossref, from the response cache; library references disagree: the library at /a names doi:10.1000/x, the library at /b names doi:10.1000/y
/home/u/papers/Smith2015.pdf: the content index could not keep its reference to the library at /home/u/papers (cannot write …: Read-only file system (os error 30))
scan.pdf: the content index knows its bytes as doi:10.1000/z, but no library reference answered and no response cache holds that record, so it is still an orphan; borax resolve fetches it
warning: /home/u/old-lib has no library identity yet, so no content-index reference to it was written for 12 records; its next store write gives it one, and borax reconcile then writes them
```

A history entry:

```toml
[[history]]
hash = "sha256-…"
run = "20261014T120000Z"
timestamp = "20261014T120000Z"
tool_version = "0.10.0"
item = "0190c3a2-7f21-7abc-8def-0123456789ad"
identifier = "doi:10.1039/c5ay00042d"
origin = "supplied"
```
