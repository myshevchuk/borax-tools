## Why

A library is found by its marker: the directory holding the nearest
`.borax.toml`, or the configured `library-root` (`library::library_root`,
re-exported from `crates/borax/src/config.rs`). Everything it records is
relative to that root, so a library can be moved, copied or synchronised
as a unit. Nothing in it names the library itself. Items and artifact
records each carry a minted v7 UUID (`ItemId` and `ArtifactId`,
`crates/borax-core/src/library.rs`, near lines 45 and 101). The library
that holds them has only its current path.

This is change 12 of the roadmap's Phase 4 ("library authority"), the
second of its four. Change 15 (`separate-change-from-acceptance`) is done.
Change 13 (`index-references-libraries`) comes next. It will store
library references in a reduced content index, and each reference names
a library identity, the library's last-known root, an artifact and an
item. A path cannot be that identity: the same library is at a different
path on every machine that holds a copy, and two libraries can stand at
one path in turn. The roadmap's done-when for this change is: "A library
reference can identify the library independently of its current
location." This change makes that identity exist and be readable. It
stores no reference.

The maintainer has settled the following, and this change preserves it:

1. A library receives a v7 UUID in a new state file,
   `.borax/library.toml`, under the root's `STATE_DIR`
   (`crates/borax/src/library.rs`, line 49). It does not go in
   `.borax.toml`, which is user-edited, layered configuration.
2. The first store write the library receives mints the identity:
   - an admission by an applying rename;
   - an adoption;
   - any other store write, including reconcile's record rewrites.

   A library that has received no write may have no identity.
3. A synchronised copy (rsync, git clone) carries the UUID, so it is the
   same library at another location. The current library still answers
   for its own files, as today.
4. A library without an identity still answers for its own files, and
   discovery still uses its marker. It needs an identity only when
   another location stores a reference to it.
5. There is no migration and no handling of version transitions before
   1.0.0. Nothing adds a format-version field, and nothing reads an
   older shape.

## What Changes

- **The identity file.** `.borax/library.toml` holds a single key, `id`.
  Its value is the canonical lowercase hyphenated UUID text that item and
  artifact records use. A reader ignores keys it does not know, as for
  item files and artifact records. An `id` that is absent, is not a
  string, or is not canonical UUID text does not parse. borax mints a v7
  UUID, but when it reads one it does not check the version, as for
  items and artifacts. Design D1.
- **What the identity means.** A copy that includes `.borax/` is the
  same library. Absence is healthy: it is neither a finding nor a
  condition. Deleting the file loses the identity, and the next store
  write mints another. Inside the library, the identity decides nothing.
  Design D2.
- **When it is minted.** Every item or artifact-record write goes
  through a `StoreWriter`. Immediately before each actual write, it
  ensures that the identity file exists. This covers `admit` (rename
  and adoption) and reconcile's `rewrite`. A run that writes no item and
  no artifact record mints nothing. That includes:
  - `status`, `validate`, `resolve` and `bib`;
  - a preview, and a run under `--no-record`;
  - a reconcile of an untouched library;
  - an adoption that records nothing;
  - an admission whose rewrite leaves the record as it was.

  Design D3.
- **How it is minted.** The file is created with no-replace semantics.
  borax writes a temporary file, then links it into place with a hard
  link, which fails if anything is already there. Where hard links are
  not supported, borax falls back to an exclusive create. A run that
  loses a race reads and uses the winner's identity. A reader never sees
  a partial file where hard links work. This is a new function,
  `borax_sources::store::create_atomically`, beside `write_atomically`.
  Design D4.
- **Failure.** If the identity file is absent and minting it fails, the
  guarded store write is not made. It is reported as that write already
  reports a failure: an `unwritten` admission, adoption or repair, with a
  message naming the identity file. If the file is present but cannot
  be read or does not parse, borax neither mints over it nor blocks the
  write. The write proceeds, `validate` reports the file, and a run that
  wrote into such a library prints one warning on stderr. Design D5.
- **Validation.** `borax validate` reports an identity file that cannot
  be read or does not parse as `Finding::Unreadable`, against that file,
  before the findings about the item and artifact stores. Design D6.
- **Reporting.** `library-status` and `library-validated` gain an `id`.
  It is written after `root`, and only when the library has a readable
  identity. Each human report line gains a closing `; library <uuid>`
  clause under the same condition. This is an addition, so the schema
  stays 4. No event reports a mint. Design D7.
- **Interface.** In `borax_core::library`, this change adds `LibraryId`
  and `LibraryIdentity`. In `borax::library`, it adds:
  - `Identity`, `identity` and `ensure_identity`;
  - `StoreWriter`, `store_create` and `identity_fault_warning`.

  Design D8.

Explicitly out of scope:

- library references and the reduced content index, which are
  change 13;
- refresh, which is change 14;
- any command that shows, sets or regenerates an identity;
- naming the library's identity in a run log or on `run-started`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library`:
  - ADDED:
    - "A library carries an identity that does not depend on where it
      is": where the identity lives, what it holds, what a copy is,
      and the healthy absence
    - "A library's first store write mints its identity": when, how
      without replacing, what a failed mint costs, and an unreadable
      identity file
  - MODIFIED:
    - "A library is one directory tree": a copy carries the identity,
      and a run with no library root mints none
    - "borax reports a library it has never seen": `status` reports
      the identity and mints none
    - "The library's text is authoritative and is read directly": the
      identity file is authoritative state, not derived state
    - "Validation defines a well-formed library": the identity-file
      finding, absence is not a finding, and `library-validated`
      carries the identity
    - "borax adopt records what the library already holds": adoption
      writes the identity too, before its first record
    - "Library conditions are named where they are counted": the
      sentence "The totals lines keep their wording" makes an exception
      for validate's identity clause

Requirements checked and left unchanged:

- `library`:
  - "An applying run records what it admits". Its rule that a failed
    store write costs the file its record and not its rename already
    covers a failed mint, because the mint is part of the guarded write.
    The ADDED requirement states that a preview and `--no-record` mint
    nothing. Item-before-record order is kept, with the identity before
    both.
  - "Reconciliation repairs a stale path by a bounded walk". "A
    reconcile over a library nothing has touched SHALL write no library
    state" still holds, because such a reconcile makes no store write
    and so mints nothing. A reconcile that finds only pending changes
    writes nothing either.
  - "An artifact record is authoritative state keyed by an artifact
    identity". Its sentence "This is the first authoritative content
    `.borax/` holds" stays true: artifact records came first.
- `cli`:
  - "JSON Lines output is first-class". Schema 4 was first released in
    0.9.0, and `id` is an addition, which the requirement says needs no
    bump.
  - "A run's human summary fits its command". `status` and `validate`
    still end on their own report lines. The new warning goes to stderr.
  - "A subcommand accepts only the settings it consumes". This change
    adds no flag and no setting.
- `run-logs`: unchanged. A run log is evidence, not store state, so
  writing one mints nothing.

## Impact

- `crates/borax-core/src/library.rs`: `LibraryId` and `LibraryIdentity`.
- `crates/borax-sources/src/store.rs`: `create_atomically` and
  `create_atomically_with`.
- `crates/borax/src/library.rs`:
  - `IDENTITY_FILE` and `identity_file`;
  - `Identity`, `identity` and `ensure_identity`;
  - `store_create`, `StoreWriter` and `identity_fault_warning`;
  - `Survey::identity`, `Validation::identity`,
    `Reconciliation::identity_fault` and `Adoptions::identity_fault`;
  - `reconcile` and `adopt` write through a `StoreWriter`;
  - `validate` reports the identity file;
  - `status_event` and `validation_events` carry the identity.
- `crates/borax/src/event.rs`:
  - `id` on `Event::LibraryStatus` and `Event::LibraryValidated`;
  - their human lines;
  - the `Finding::Unreadable` docstring, which also covers the identity
    file.
- `crates/borax/src/run.rs`:
  - `Aftermath::identity`;
  - the rename driver records through one `StoreWriter` per run;
  - `reconcile_events` and `adopt_events` return the identity warning;
  - `dispatch` writes the warning to stderr.
- Tests: design D10 lists every existing test that changes, and how.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`:
    - the library-root passage under "Where a setting can be set";
    - the library-model passage;
    - `borax status`, `borax validate` and `borax adopt`;
    - the Run logs field sentence;
  - `CHANGELOG.md`: an Unreleased entry.
- `openspec/STATE.md` (implementer): a paragraph for this change.
- No new dependency, configuration key, flag or event type.

## Deferred

- **Naming the identity in a run log.** `run-started` does not say
  which library a run wrote into. Change 13 decides whether a reference
  needs that.
- **A command to show, set or regenerate an identity.** `borax status`
  shows the identity. To make a forked copy a different library, delete
  its `.borax/library.toml`, and its next store write mints a new
  identity.
- **Warning about an unreadable identity on a run that writes
  nothing.** Only a run that made a store write warns. `validate`
  reports the file in every case.
