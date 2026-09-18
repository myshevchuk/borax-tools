# Tasks: add-library-store

Work on branch `change/add-library-store`, which sits directly on
`main`. Nothing below waits on another change.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it. Two tasks are marked *arrives green* and say why — neither
is a test that could be made to fail on an assertion before the thing it
asserts about exists.

One thing to hold in view while implementing group 7: the work-duplicate
question is permitted only because this change's `rename` delta narrows
the living sentence "A file reported as a duplicate … SHALL NOT be asked
about" to a *content* duplicate. A run that asks without that amendment
is a run contradicting a living requirement.

## 1. The two identities and the two records, as values

- [ ] 1.1 Red: `crates/borax-core/tests/library.rs` — `ItemId` and
      `ArtifactId` parse a UUID, render it back byte-identically, refuse
      a string that is not one, and compare and sort by their text.
- [ ] 1.2 Green: the two identity types in
      `crates/borax-core/src/library.rs`, over the `uuid` crate's v7
      type (design D10). `uuid` and `toml` join the workspace and
      `borax-core`'s dependencies; `borax-core` performs no I/O still,
      since parsing text is not I/O and minting is the adapter's.
- [ ] 1.3 Red: the `Item` value round-trips through TOML losslessly for
      every entry type, including a record whose `borax.source-fields`
      holds values that must stay distinguishable through the JSON-text
      encoding of design D11 — `null` (the one value TOML cannot hold at
      all), the *string* `"42"` against the *number* `42`, the string
      `"null"` against a null, the string `"{\"a\":1}"` against a nested
      object, a heterogeneous array, and the numeric boundaries where a
      JSON number and a TOML integer part company (`i64::MIN`,
      `u64::MAX`, a float that is integral). Assert what is *not*
      preserved as well: provider whitespace and key order inside an
      object, both already gone before the store sees the value. A
      field whose JSON text does not parse reads as a finding rather
      than an error that loses the file.
- [ ] 1.4 Green: `Item`, its TOML serialization, and the source-fields
      encoding.
- [ ] 1.5 Red: the `ArtifactRecord` value round-trips through TOML: the
      artifact identity, an optional item identity, a hash history of
      one and of several in order with each entry carrying the run, the
      timestamp and the tool version that recorded it, a
      library-relative `/`-separated path, and the size and
      modification time. A record with no item link is representable;
      one with an empty history, and one whose history entry names no
      run, each parse and are findings.
- [ ] 1.6 Green: `ArtifactRecord` and its TOML serialization.
- [ ] 1.7 Red: the pure predicates validation is made of — an identity
      that disagrees with a file name, a path that is not
      library-relative, a malformed hash, an empty history — each as a
      value-in/value-out function with no filesystem anywhere near it.
- [ ] 1.8 Green: the predicates.

## 2. The library boundary

- [ ] 2.1 Red: `crates/borax/tests/library.rs` — the library root is the
      directory holding the nearest `.borax.toml`, `library-root`
      replaces the search outright, and a tree with no marker above it
      has no root. This is `collection_root`'s behaviour under its new
      name, so the existing discovery tests are renamed rather than
      rewritten and one new case covers the key's new name.
- [ ] 2.2 Red: containment is lexical and symlinks are not resolved: a
      path under the root is inside, a path outside is outside, a
      symlink inside the tree pointing out does not bring its target
      in, and a symlink is neither an artifact nor an orphan. Asserted
      against the real filesystem, which a fake cannot establish, and
      skipped on a platform that cannot create a link.
- [ ] 2.3 Green: `library_root` and the containment predicate in
      `crates/borax/src/library.rs`; `collection_root` and the
      `collection-root` key are renamed in
      `crates/borax/src/config.rs`, with `borax config` reporting
      `library-root`.
- [ ] 2.4 Red: the artifact walk — every `.pdf` under the root at any
      depth, case-insensitively, in sorted order; nothing under
      `.borax/`, nothing at all under `items/` whatever its extension
      (a PDF placed there is not an artifact, which an earlier draft of
      this task contradicted), no sidecar, no symlink, no file of
      another extension.
- [ ] 2.5 Red: the walk stops at a nested `.borax.toml`, and the same
      exclusion is obeyed by the orphan count, by an applying run's
      admissions and by reconcile — one test per operation, because the
      defect this rules out is the four disagreeing rather than the
      walk alone being wrong.
- [ ] 2.6 Green: the walk, built from `documents` in
      `crates/borax/src/run.rs` rather than beside it, the nested-root
      stop shared by all four operations, plus the library-relative
      rendering of a path.

## 3. Reading the store

- [ ] 3.1 Red: the item store reads every `items/*.toml` that parses as
      an item and finds an item by the `id` inside it, not by its file
      name; an item file renamed by hand is still found; a `.toml` that
      is not an item record is reported rather than skipped silently.
- [ ] 3.2 Red: the artifact-record store reads every
      `.borax/artifacts/*.toml`, answers by artifact identity, by any
      hash in a record's history, and by item identity; one file that
      does not parse costs its own record and no other.
- [ ] 3.3 Green: both stores, reading files directly with no index.
- [ ] 3.4 Red: an orphan is an artifact with no record naming it, and an
      artifact record whose last-known path holds no file is *not* an
      orphan but a record whose artifact cannot be found.
- [ ] 3.5 Green: the orphan and missing-artifact computations.
- [ ] 3.6 Red: `borax bib` over a library holding an item with no
      artifact emits no entry for it. The command takes files, and this
      pins the gap the `library` requirement states rather than letting
      a later reader assume the store is wired into bibliography output.
      Nothing about `bib` changes in this group; the test exists so the
      limitation is asserted rather than implied.

## 4. `borax status`

- [ ] 4.1 Red: `crates/borax/tests/dispatch.rs` — the Hyperbole test:
      over a marked directory of PDFs with no records and no items,
      `borax status` reports the artifact count, zero items, zero
      records and every artifact an orphan, with no preceding command,
      and opens no document while doing it.
- [ ] 4.2 Red: `--identify` reports how many artifacts yield an
      identifier, runs the extraction passes and queries no service; a
      directory with no marker is reported on as given and nothing is
      written under it.
- [ ] 4.3 Green: the `status` subcommand, its events and both
      renderings.

## 5. `borax validate`

- [ ] 5.1 Red: one test per finding in the requirement's list — a
      dangling item link, two records under one identity, an identity
      disagreeing with a file name, a path that is not
      library-relative, an empty and a malformed hash history, and a
      `.toml` in the item store that is not an item record.
- [ ] 5.2 Red: what is not a finding — a library of nothing but orphans
      validates clean and exits 0, a record whose artifact cannot be
      found is a count rather than a finding, and a half-written record
      is a finding about that file alone while the rest of the library
      is reported as it is.
- [ ] 5.3 Red: validation repairs nothing and refuses nothing: the store
      is byte-identical after a `validate` that reported findings, and
      an applying `rename` over a library with a finding proceeds and
      records what it admits.
- [ ] 5.4 Green: the `validate` subcommand, its findings, and the
      partial-success exit code when it reports any.

## 6. `borax reconcile`

- [ ] 6.1 Red: a record whose artifact was moved out of band is repaired
      by the bounded walk, matching any hash in its history, keeping its
      artifact identity and its item link.
- [ ] 6.2 Red: the fast path — an artifact whose recorded size and
      modification time match is not hashed, and a file whose bytes,
      size or modification time changed at its recorded path has its new
      hash appended after the ones already there and is reported as
      changed. One test must be the exact counterexample: **bytes
      changed while size and mtime did not**, where a plain reconcile
      reports nothing and writes no library state, and `--rehash`
      reports the change and appends. That is what the fast path
      deliberately misses, so it is what pins the flag's reason for
      existing.
- [ ] 6.2a Red: **the touch counterexample** — `touch` an artifact so
      that its modification time changes and its bytes do not, then
      reconcile. The fast path misses, the hash confirms the record, and
      the recorded size and modification time are refreshed to the
      file's current ones; a second reconcile then settles that record
      on the fast path and hashes nothing. Assert the refreshed fields
      directly, not just that the run reported something: the defect
      this rules out is a record that is confirmed forever and never
      corrected, which would leave the artifact permanently unconfirmed
      for anything deriving from `(size, mtime)` — including the
      filesystem view the change above this one builds on the store.
- [ ] 6.2b Red: a repair writes the file's current size and modification
      time along with its new path, so the reconcile after a repair
      settles that record on the fast path.
- [ ] 6.3 Red: the precedence of design D6, one test per step and one
      per way it can be got wrong — two records whose files swapped
      paths both repair and neither gains a history entry; a settled
      record's file is not a candidate for another record; a current-hash
      match wins over a historical one.
- [ ] 6.4 Red: ambiguity preserves — a record with two candidates, and
      two records contending for one file, each leave path, history and
      item link byte-identical and are reported naming the candidates.
      Assert the item link specifically: the failure this rules out is
      silent reassignment, which nothing downstream can detect.
- [ ] 6.5 Red: a reconcile over a library nothing has touched writes no
      library state — every file of the item store and of
      `.borax/artifacts/` byte-identical, mtimes included, with the run
      log the only thing the run leaves — and a
      reconcile creates no record for an orphan and deletes no record
      whose artifact is gone.
- [ ] 6.6 Green: the `reconcile` subcommand, the fast path, the four
      ordered steps, the ambiguity rule, and field-level editing through
      `toml_edit` rendered into an atomic whole-file replacement so that
      an unchanged document is not rewritten.

## 7. The write path, and the ledger's retirement

- [ ] 7.1 Red: an applying run over a library writes an artifact record
      for a file it moved — the new library-relative path, the file's
      hash as the history's first entry, the size and modification
      time, and an item link — and mints the item when the library
      holds none for the record's identifiers.
- [ ] 7.2 Red: an applying run reuses an item the library already holds
      for one of the record's identifiers, so a second PDF of one work
      is a second record naming one item; a record with no identifier
      at all still gets an item, and two different such files get two.
- [ ] 7.2a Red: an artifact that already has a record keeps its item
      link. Re-run an applying run three times over one recorded file
      whose record resolves to no identifier: the link is the same
      after each, the item count never rises, and no item is left with
      nothing pointing at it. This is the accumulation design D4 rules
      out, and one repetition is not enough to see it.
- [ ] 7.2b Red: an operator's re-identification moves the item link, and
      only an operator's. With a scripted asker and `--no-skip-named`,
      supplying the right identifier for a recorded file named from the
      wrong record renames it *and* re-links its artifact record to the
      item for the supplied record, keeping the artifact's own identity;
      the run reports both items; the item left behind is counted by
      `borax validate` and is not a finding. The same file re-run
      without a supplied identifier re-links nothing, which is task
      7.2a's property and must still hold.
- [ ] 7.2c Red: an item with no artifact takes the incoming file as its
      first artifact — not a duplicate at either level, no question put,
      a minted record naming that same item, and no second item. Repeat
      for an item whose every artifact record names a path holding no
      file: admitted the same way, the absent artifact's record left
      exactly as it is, and the run reporting that there are paths to
      reconcile.
- [ ] 7.3 Red: an already-named file inside the library is recorded by an
      applying run, a preview writes no artifact record and no item,
      `--no-record` writes neither either, and a file renamed outside
      the library gets no record and is reported as outside it.
- [ ] 7.3a Red: what `--no-record` does and does not suppress. Under the
      flag: no artifact record and no item is written and neither
      duplicate check is made; an artifact no record names the path of
      is moved; an artifact whose record's history holds that file's
      hash is moved, its record left with a stale path, and a
      `borax reconcile` afterwards repairs that path by the hash
      already recorded, with identity and item link unchanged
      throughout; and an artifact whose record's history does *not*
      hold that file's hash is **not moved** and is reported with a
      reason naming the record.

      The refusal case MUST be exercised with the file edited in place
      to the same length with its modification time preserved, so the
      fast path still matches the record and only the hash comparison
      refuses the move. A test that arranges a size or mtime change
      instead passes while the defect is present, because the fast path
      alone would refuse it. Assert the counterpart by running the same
      inputs without the flag: the move happens and the file's hash is
      written into the history, so the refusal is the flag's and not the
      file's.
- [ ] 7.3b Red: an update describes the file it names. After an applying
      run moves an artifact edited in place to the same length with its
      modification time preserved, its record's newest hash is the
      file's, the earlier hashes are still there, and the path, size and
      modification time are the file's — so the next reconcile settles
      it on the fast path and hashes nothing. This is the silent half of
      the same defect: nothing reports a record whose newest hash is not
      its file's, so only an assertion on the history catches it.
- [ ] 7.4 Red: durability — a store write that fails leaves the rename
      standing and reported, and the next run over the file records it
      again; a write interrupted part-way leaves the previous record
      byte-identical with its identity, its whole hash history and its
      item link intact, and leaves no temporary a reader would take for
      a record; an admission interrupted between its two writes leaves
      an item nothing links to and never an artifact record naming an
      item that does not exist, whichever write is made to fail.
- [ ] 7.5 Green: `admit` writes an artifact record and mints or reuses
      an item, item first; every store write is `write_atomically` over
      a document edited through `toml_edit`; the outside-the-library
      report.
- [ ] 7.6 Red: duplicate detection against the artifact store — content
      by any hash in any history, a file's own record passed over rather
      than ending the search, a stale last-known path reported as
      something to reconcile and never vetoing an admission, and
      `--no-record` turning the checks off silently.
- [ ] 7.6a Red: the work check, and where the two runs part company. A
      batch run over a file whose identifier an item already carries
      skips it with the work-duplicate reason naming the recorded path,
      writes no record and moves nothing — including under `--apply`,
      which is the guarantee design D16 keeps. With a scripted asker, an
      interactive run over the same file puts the question, its choices
      are file-as-another-artifact, skip and quit, filing is not the
      default, accepting is followed by the ordinary move question, and
      answering rename writes a second artifact record against that same
      item with no second item minted. Declining reports the
      work-duplicate reason and not `declined`; a content duplicate is
      still never asked about; and no question is put when the matching
      item has no artifact whose recorded path still holds a file, since
      the question has no sibling path to name.
- [ ] 7.6b Red: the case that makes a multi-artifact library usable —
      each of the three recorded artifacts of one item, reached in turn
      by a batch run, is reported neither a duplicate nor skipped,
      because the check passes over the item its own record links to.
      Without this the same run skips all three.
- [ ] 7.7 Green: the checks over the store; the additional-artifact
      verdict and what it carries; `crates/borax/src/ledger.rs` and
      `crates/borax-core/src/ledger.rs` are both deleted, nothing having
      a reason to parse a ledger line any more; the `ledger` subcommand
      and the `ledger-rebuilt` event are withdrawn, and the event
      schema version becomes 3. `Collection`'s path comparison,
      `relative_to` and `collection_relative` move to the library
      module.
- [ ] 7.8 Red: no command reads, writes or deletes
      `.borax/ledger.jsonl`. A library holding one is reported,
      validated, reconciled and adopted with the file byte-identical
      afterwards, and an applying run appends nothing to it. The file is
      borax's own retired accounting, so it is left alone rather than
      cleaned up — deleting it is the operator's to do.

## 8. `borax adopt`

- [ ] 8.1 Red: adoption from the content index — each orphan whose hash
      the index answers for gains an artifact record carrying that hash,
      its library-relative path and its size and modification time,
      linked to an item holding the cached record; an item the library
      already holds for one of that record's identifiers is reused
      rather than minted again. No service is queried, nothing is
      extracted from the artifact, and no file is renamed, moved or
      deleted.
- [ ] 8.2 Red: what adoption leaves alone — an artifact the index cannot
      answer for is still an orphan and counted as one; an artifact that
      already has a record is byte-identical afterwards, item link
      included; a second run writes no library state, its run log
      excepted.
- [ ] 8.3 Red: adoption reads neither a citation sidecar nor
      `.borax/ledger.jsonl`. Over a library whose files have sidecars
      and which holds a ledger from an earlier version, with the content
      index empty, nothing is adopted, every artifact stays an orphan,
      and both the sidecars and the ledger file are byte-identical
      afterwards. This is the test that keeps authoritative state from
      being derived from optional output, and it is also what pins the
      ledger as retired rather than quietly consulted.
- [ ] 8.4 Red: adoption after `borax cache --clear` adopts nothing and
      reports every artifact an orphan, rather than reporting a failure.
      The command's source is a cache and the test says so.
- [ ] 8.5 Green: the `adopt` subcommand, its events and both renderings.

## 9. The suite and the surface

- [ ] 9.1 Rewrite `crates/borax/tests/ledger.rs` as `library.rs`: every
      test asserting a ledger append asserts an artifact record. *This
      arrives green* — it is a translation of assertions whose subjects
      group 7 has already changed, and writing it red would mean
      asserting the old store's behaviour in order to watch it fail.
- [ ] 9.2 Red: the flag surface — `borax status --identify`,
      `borax reconcile --rehash`, `borax validate` and `borax adopt`
      with no setting of their own, `identify` refused as a
      configuration key, `--record`/`--no-record` accepted as a pair
      wherever the setting is, `--ledger` refused as an unknown
      argument, and `borax ledger` refused as an unknown subcommand.
- [ ] 9.3 Green: the `cli.rs` declarations.
- [ ] 9.4 Verify by hand on a copy of the real-PDF corpus: point
      `borax status` at it unmarked and marked, run `--identify`, run
      `borax adopt` with a warm content index and again after
      `borax cache --clear`, record the rest with `rename --apply`,
      move a file in a file manager, swap two files' paths, reconcile,
      and validate. *This
      arrives green by construction* — it is the Hyperbole test
      performed rather than asserted, and it is the one part of the
      change the suite cannot establish, since what is being checked is
      that a corpus borax has never seen needs no ceremony.

## 10. Documents

- [ ] 10.1 Run `codex-docs` update jobs on `docs/manual.org` (the
      library and its boundary, `borax status`, `borax validate`,
      `borax reconcile`, `borax adopt` and when to run it, the item and
      artifact record formats and how neither is a `.bib` sidecar, what
      `--no-record` suppresses, the `library-root` and `--record`
      renames, and the withdrawal of `borax ledger`),
      `README.md` and `CHANGELOG.md`, with this proposal and its design
      as sources; check the diff against both.
- [ ] 10.2 Record the built state in `openspec/STATE.md`: the library
      store as built, the ledger retired with no migration owed, the
      `ledger` capability's name left stale because a rename does not
      archive, and what the next change in the stack adds on top of it.
      Leave the orphaned-sidecar defect recorded — this change settles
      what a sidecar is and fixes nothing about the defect — and update
      its closing paragraph, which says the fix is blocked on deciding a
      sidecar's identity and is no longer true.
- [ ] 10.3 `openspec validate add-library-store --strict` and
      `scripts/check-spec-deltas.py` pass; full suite green.
