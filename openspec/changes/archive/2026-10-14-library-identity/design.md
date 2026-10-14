# Design: library-identity

## Context

Everything below was read from source at `c0ccf17`, the merge of
`separate-change-from-acceptance`. This is change 12 of the roadmap's
Phase 4, and the second of its four changes.

**Settled by the maintainer.** These are decisions, not open questions:

1. A v7 UUID lives in `.borax/library.toml`, under the root's
   `STATE_DIR`. It is not stored in `.borax.toml`.
2. The first store write a library receives mints the identity. That
   write can be an admission by an applying rename, an adoption, or any
   other store write, reconcile's included. A library that has received
   no write may have no identity.
3. A synchronised copy carries the UUID and is the same library at
   another location. The current library still answers for its own
   files.
4. A library without an identity still answers for its own files, and
   is still found by its marker. It needs an identity only when another
   location refers to it.
5. Nothing migrates, and no format version is added, before 1.0.0.

**Decided by the orchestrator for this proposal, and recorded below:**

- the fields and the parse rules (D1);
- minting without replacing, and what concurrent minting guarantees (D4);
- the order of the identity and the store writes (D3);
- failure (D5);
- the validation finding (D6);
- the reporting (D7);
- the interface (D8).

**Owned by this proposal:**

- the mechanism of the no-replace create, and how it behaves on a
  filesystem without hard links (D4);
- where the identity is ensured (D3);
- whether a run warns about an unreadable identity file (D5);
- the JSON placement and the human strings (D7);
- the requirement wording (D9).

What the engine does today, at the points this change touches:

- **The identifiers.** `ItemId` and `ArtifactId`
  (`crates/borax-core/src/library.rs`, lines 45 and 101) wrap a `Uuid`.
  They parse through `canonical_uuid` (line 30), which accepts only the
  lowercase hyphenated spelling. It does **not** check the UUID's
  version. No code in the workspace calls `get_version`. Minting is
  always `Uuid::now_v7()`, at `crates/borax/src/library.rs` lines 1155,
  2682 and 2766. The workspace manifest's comment on `uuid` explains
  that v7 was chosen for time-ordered listings.
- **Unknown keys.** `ArtifactRecord` (line 379) and the private
  `ItemFile` (line 218) derive `Deserialize` without
  `deny_unknown_fields`, so a reader ignores keys it does not know. Only
  the configuration layers in `crates/borax/src/config.rs` deny unknown
  fields. A scratch check against `toml` 1 confirmed three things:
  - a struct `{ id: String }` serialises as `id = "<text>"\n`;
  - unknown keys and tables are ignored when it is read;
  - empty text fails with "missing field `id`".
- **Replacing writes.** `borax_sources::store::write_atomically`
  (`crates/borax-sources/src/store.rs`, line 225):
  - creates the parent directories;
  - writes a temporary named by the private `temporary_name`
    (`.tmp-<pid>-<n>`, line 244) in the destination's directory;
  - renames the temporary over the destination, which replaces it;
  - removes the temporary on failure.

  Nothing in the workspace creates a file without replacing one.
- **The store writes.** Every write to the item store or the artifact
  store goes through a `write: &dyn Fn(&Path, &[u8]) -> io::Result<()>`
  seam, reached in three places:
  - `library::admit` (line 2641). Its `select_item` (line 2752) writes a
    minted item through `write_whole` (line 2781). `admit` then writes a
    minted record through `write_whole`, or rewrites a held one through
    `rewrite` (line 1898). `rewrite` calls `write` only when the
    rendered document differs from the text it read (line 1918).
    `admit` is reached from:
    - `run::recorded` (`crates/borax/src/run.rs`, line 3088), passing
      `&crate::library::store_write`;
    - `library::adopt` (line 2930), passing `&store_write`.
  - `library::reconcile` (line 2126), whose step 4 calls
    `rewrite(file, &record, &store_write)` at line 2334.
  - `store_write` (line 2802), which is `write_atomically`.

  `bib.rs` writes bibliographies, and `runlog.rs` writes run logs.
  Neither writes to the store.
- **The rename run's library.** `rename_events` (`run.rs`, line 1291)
  writes into one library, `adapters.collection_root` (line 1308), and
  records only when `recording` (line 1307) is set.
- **Status and validate.** `status_event` (`library.rs`, line 1634) and
  `validation_events` (line 1758) build `Event::LibraryStatus` and
  `Event::LibraryValidated` (`crates/borax/src/event.rs`, lines 184 and
  254). The human lines are at `event.rs` lines 1552 and 1580.
  - `LibraryStatus`'s `identifiable` serialises as `null` when absent.
  - Change 15's new optional keys use
    `skip_serializing_if = "Option::is_none"`.
  - `validate` (line 1709) collects `item_findings` and then
    `record_findings`, each in path order.
  - `Finding::Unreadable { message }` (`event.rs`, line 505) is
    documented as "a file of one of the stores that cannot be read or
    does not parse as the record it claims to be". Its human clause is
    `unreadable (<message>)` (line 2017).
- **Diagnostics.** Run-level diagnostics travel on `run::Aftermath`
  (line 1114), which has `diagnostic` and `library`. `dispatch` writes
  them to stderr (line 3739) as `warning: <message>` (`event.rs`,
  line 2198). `record_faults_warning` (`library.rs`, line 1307) is the
  model: one warning per run, naming `borax validate`.
  - `emit_events` (`run.rs`, line 1026) returns `Aftermath::default()`
    for `reconcile` and `adopt`.
  - `events_for` discards the `Aftermath`.

## D1. Where the identity lives, and what the file holds

**Decision.**

- **The file.** `.borax/library.toml` under the library root:
  `root.join(STATE_DIR).join(IDENTITY_FILE)`, where `IDENTITY_FILE` is
  `"library.toml"`.
- **One key.** `id` holds the canonical lowercase hyphenated UUID text.
  Written whole, the file is exactly `id = "<uuid>"\n`. There is no
  creation time, borax version or format version.
- **Unknown keys are ignored**, matching `ArtifactRecord` and
  `ItemFile`. A later field, or one a person adds, does not make the
  file unreadable to this release.
- **A malformed file does not parse.** That covers:
  - text that is not TOML;
  - a missing `id`, or an `id` that is not a string;
  - an `id` that is not canonical UUID text, which `LibraryId`'s
    `TryFrom<String>` refuses with `not a canonical library uuid: "<text>"`.

  Each reads as `Malformed`.
- **The version is minted, not checked.** borax mints a v7 UUID
  (`Uuid::now_v7()`), as it does for items and artifacts. When it reads
  an identity it requires canonical text and does not check the
  version, as `canonical_uuid` does for items and artifacts.

The brief said that a non-v7 `id` is malformed "as for items and
artifacts". The house rule does not say that: `canonical_uuid` checks
spelling only, and no reader checks a version. This design follows the
house rule. Checking the version for one of the three identities would
reject a UUID in `library.toml` that the same parser accepts in an item
file. The version carries no meaning borax relies on. A test pins that a
canonical non-v7 UUID parses.

**Rejected: `.borax.toml`.** Settled by the maintainer. That file is
configuration, layered and merged and edited by hand, and the
configuration readers deny unknown fields.

**Rejected: a creation time, a borax version, or a format version.** The
UUID's own timestamp records when it was minted. A tool version would be
stale after the first upgrade and would mean nothing. A format version is
ruled out before 1.0.0.

**Rejected: denying unknown keys.** No library file borax reads does so.
A denial would make a key added by a later release, or by a person,
cost the library its identity under this one.

**Rejected: checking the version.** See above.

**Rejected: the identity in each artifact record.** A library with no
records would have nowhere to keep it, and records would have to agree
with one another.

## D2. What an identity means

**Decision.**

- **A copy is the same library.** A tree copied, synchronised or cloned
  with its `.borax/` directory carries the file and answers with the
  same identity at its new location.
- **It decides nothing inside the library.** Discovery uses the marker.
  Which files a library answers for, and every answer it gives, are
  what they were. The identity is how another location names the
  library.
- **Absence is healthy.** An absent file is neither a finding nor a
  condition. It is the state of every library written by 0.9.0 and of
  every library borax has never written.
- **The file is authoritative state.** Deleting it loses the identity.
  Nothing reconstructs it, and the next store write mints another.
  Neither the response cache, the content index nor a sidecar holds an
  identity, so clearing them changes nothing.

**Rejected: minting on `status` or `validate`.** Both are read-only by
requirement. Minting there would also make an unwritten library, or a
directory nobody marked, acquire state by being looked at.

**Rejected: a command that mints (an `init`).** The `library` capability
requires that no operation needs a library to be initialised before it
can be reported on. The first write is where a library starts to hold
state.

**Rejected: deriving the identity from the root path, the marker's
content, or the store's contents.** A path changes with every copy. The
marker is configuration and changes when edited. The store's contents
change with every write.

## D3. Where the identity is ensured

**Decision.** Ensure it **immediately before every actual store write**,
by routing each store write through a `StoreWriter` (D8). A
`StoreWriter` holds:

- the root;
- a `create` seam, which writes without replacing (D4);
- the replacing `write` seam that the call site passes today.

`StoreWriter::write(path, bytes)` first calls
`ensure_identity(root, create)`, then:

- on `Err`, returns that error without calling `write`;
- on `Ok(Identity::Faulty(fault))`, keeps the first such fault and calls
  `write`;
- on `Ok(Identity::Present(_))`, calls `write`.

The call sites:

| Site | Today | After |
|---|---|---|
| `run::recorded` (`run.rs` 3088) | `admit(library, &admitting, &store_write)` | `admit(library, &admitting, &|p, b| writer.write(p, b))`. The rename driver builds one `StoreWriter` per run over its one library, `StoreWriter::new(library, &store_create, &store_write)` |
| `library::adopt` (line 2930) | `admit(root, …, &store_write)` | the same, through a `StoreWriter` built in `adopt` |
| `library::reconcile` step 4 (line 2334) | `rewrite(file, &record, &store_write)` | `rewrite(file, &record, &|p, b| writer.write(p, b))`, through a `StoreWriter` built in `reconcile` |

The signatures of `admit`, `rewrite`, `reconcile` and `adopt` do not
change. `Reconciling` does not change. `recorded` is private, and how
the writer reaches it is the implementer's call.

Because the guard sits at the actual write:

- **The identity comes before the first item or record.** `admit`
  writes the item first (`select_item`), then the record, and each goes
  through the writer. So the order on disk is identity, item, record.
  An interruption leaves one of three states:
  - an identity alone, which is healthy;
  - an identity and an item nothing links to, as today;
  - all three.
- **A write that does not happen mints nothing.** These make no write,
  so they reach no writer:
  - an admission whose `rewrite` renders its document unchanged
    (line 1918);
  - an `outside` or `pending` admission, which returns before `admit`;
  - a reconcile whose confirmations write nothing, or that finds only
    pending changes or missing records. Every confirmation on the fast
    path writes nothing. A confirmation by hash whose record already
    describes the file, as under `--rehash`, writes nothing too. A
    touched artifact confirmed by hash is different: its recorded size
    and modification time are rewritten (`Decided::Confirmed {
    refreshed: true }`; the test near `crates/borax/tests/library.rs:2522`).
    That is a store write, and it mints;
  - an adoption whose orphans are all held, unreadable or unindexed.
- **Runs that never write mint nothing.** A preview and a run under
  `--no-record` never call `recorded`. `status`, `validate`, `resolve`
  and `bib` have no store write.

**Cost.** Each store write is preceded by one `symlink_metadata` and one
read of a file of about 45 bytes. The store write it guards already
reads, parses, renders and atomically writes a whole record, so the
extra cost is negligible. Per write is also the simplest rule that
always holds: if someone deletes the file in the middle of a run, the
next write mints again.

**Rejected: ensuring once at the start of a run.** That would mint on
runs that end up writing nothing: an untouched reconcile, an adoption
that records nothing, a rename whose files are all skipped or already
recorded. That is decision 3's "nothing mints without a write" broken
for the commonest runs. It also could not report a failed mint as the
failure of the write it guards.

**Rejected: ensuring inside `admit`, with a new parameter.** `admit` has
32 call sites in `crates/borax/tests/library.rs`, and `reconcile` does
not call it. The writer covers both without touching either signature.

**Rejected: memoising a `Present` result for the run.** It saves reads
that cost nothing, and a deleted file would then go unnoticed for the
rest of the run.

## D4. Minting without replacing

**Decision.** There are two layers: a filesystem primitive in
`borax-sources`, and the library's algorithm on top of it.

**`create_atomically(path, content)`** (`store.rs`, beside
`write_atomically`):

1. Create `path`'s parent directories.
2. **Create a temporary that this call owns.** Take a name from
   `temporary_name` (`.tmp-<pid>-<n>`) in `path`'s directory and open
   it with `OpenOptions::new().write(true).create_new(true)`.
   - `AlreadyExists`: that name is taken, for example by a temporary a
     crashed process left whose pid has been reused. Leave the file
     alone and try a fresh name, up to 16 names in all. If every name
     is taken, return `io::Error::other("no free temporary name in
     <dir> after 16 attempts")`. That error is deliberately not
     `AlreadyExists`, which would read as a lost race.
   - Any other error: return it. Nothing was created.

   Write `content` with `write_all`. If that fails, remove this
   temporary and return the error. From here on, the call removes only
   the temporary it created itself, and no other file.
3. Hard-link the temporary to `path` (`std::fs::hard_link`). `link(2)`
   and `CreateHardLinkW` both fail when `path` exists, so the link is a
   no-replace operation. Then:
   - `Ok`: remove the temporary, ignoring a failure to remove it, and
     return `Ok(())`.
   - `ErrorKind::AlreadyExists`: remove the temporary and return that
     error. Something is already there, and it is not touched.
   - **any other error**: remove the temporary and fall back (below).
4. **Fallback, for a filesystem that will not link.** FAT and exFAT are
   the usual cases; on Linux vfat, `link` fails with `EPERM`. Open
   `path` with `OpenOptions::new().write(true).create_new(true)`, which
   is an exclusive create (`O_CREAT|O_EXCL`, `CREATE_NEW`) and never
   replaces. Write `content` with `write_all`.
   - If the open fails, return its error. That includes `AlreadyExists`
     for a file already there.
   - If the write fails, remove the file this call created, then return
     the error.

**Why the temporary is not written with `fs::write`.**
`write_atomically` writes its temporary with `fs::write`, which
truncates and reuses a file already at that name, and this design
deliberately does not do the same. Suppose a crash comes after step 3's
link and before the temporary is removed. The temporary is then a
second name for `library.toml` itself. A later process that drew the
same name (pids are reused, and the counter restarts at 0) would truncate
the identity file through that other name, and so rewrite an identity
borax promises never to rewrite. `write_atomically`'s temporaries carry
no such risk, because its rename moves the temporary's name away rather
than adding a second one.

`create_atomically_with(path, content, link, name)` is the same function
with two seams, so that tests can force the fallback and a taken name:

- `link` replaces the link step;
- `name` gives each temporary name in turn.

`create_atomically` passes `std::fs::hard_link` and `temporary_name`.

**What a reader can see.**

- **Hard links supported**, which covers ext4, XFS, Btrfs, APFS, HFS+
  and NTFS. The file appears whole, in one step, under its final name.
  Nothing can replace it. A crash leaves at most a temporary `.tmp-*`
  in `.borax/`, the same litter `write_atomically` can leave. If the
  crash came after the link, that temporary is a second name for the
  identity file. Step 2 never opens an existing temporary, so nothing
  writes through that name.
- **Hard links not supported.** The file still never replaces another,
  because only one exclusive create succeeds. For the duration of one
  `write_all` of about 45 bytes, a concurrent reader can see the file
  empty or short. That reader finds it `Faulty` (D5): it does not mint,
  its write proceeds without an identity, and its run warns. A crash
  inside that window leaves a short file, which `validate` and the
  warning report and a person deletes. This is the one way decision 2's
  "a reader never sees a partial file" is weakened, and it applies only
  where the filesystem cannot do better.

**`ensure_identity(root, create)`**:

1. Read `identity(root)`. If it is `Present` or `Faulty`, return it.
   Nothing is written.
2. Mint `LibraryId::from_uuid(Uuid::now_v7())` and call
   `create(identity_file(root), LibraryIdentity { id }.to_toml())`.
   - `Ok`: return `Present(id)`.
   - `AlreadyExists`: another writer won the race. Read
     `identity(root)` once more:
     - `Present(winner)` or `Faulty`: return it;
     - `Absent`, meaning the file vanished between the two: return the
       failure below.
   - any other error: return the failure below.
3. The failure is `io::Error::new(kind, format!("cannot create the
   library's identity at {}: {error}", path.display()))`, where `kind`
   and `error` are the create's. Its `Display` is that message, and it
   becomes the `unwritten` message (D5).

**`identity(root)`** decides as follows:

| Condition | Result |
|---|---|
| `fs::symlink_metadata` of the file is `NotFound` | `Absent` |
| any other metadata error (for example `.borax` is a plain file) | `Faulty` |
| `fs::read_to_string` fails (a directory, a dangling symlink, an unreadable file, text that is not UTF-8) | `Faulty` |
| `LibraryIdentity::from_toml` fails | `Faulty`, with the parser's message (`Malformed`'s `Display`) |
| otherwise | `Present(id)` |

`Faulty` carries a `StoreFault` (line 272) whose `path` is the identity
file and whose `message` is the reader's or the parser's. A dangling
symlink is `Faulty` rather than `Absent`. If it were `Absent`, every
mint would meet `AlreadyExists` and then read `Absent` again, and every
write into the library would fail.

**What concurrent minting guarantees.** These two hold on every
filesystem:

- **(a)** borax never replaces or rewrites an identity file that
  exists.
- **(b)** A run that goes on with an identity goes on with the one on
  disk.

Beyond that, the outcome depends on the filesystem:

- **Hard links supported.** Two runs reach step 2 of `ensure_identity`
  together. One create wins. The other gets `AlreadyExists`, reads the
  winner's whole file, and returns `Present` with the winner's id. Every
  concurrent minter ends up `Present` with the same identity.
- **Under the fallback, a run can meet the file mid-write.** Take run A
  that has exclusively created the file and not yet finished its
  `write_all`. Run B's `identity` reads the short file as `Faulty`. B
  does not mint, writes its item or record without an identity,
  remembers the fault and warns, and does not read the file again
  within that write. When A finishes, the library holds A's identity. If
  A's write fails instead, A removes the file, and the library holds no
  identity. That is the healthy state of a 0.9.0 library with records
  and no identity, and the next store write mints one.

Either way, the library ends with at most one identity. This design adds
no retry and no delay to close the fallback window. A retry would
wait on a peer that may have crashed, and the window costs a warning,
not a wrong identity.

**Rejected: `write_atomically`.** Its rename replaces. Two runs would
each install their own UUID, the last rename would win, and the first
run would have used an identity the library no longer has.

**Rejected: checking for the file, then renaming.** The gap between the
check and the rename is exactly the race this needs to close.

**Rejected: `renameat2(RENAME_NOREPLACE)`, `renamex_np(RENAME_EXCL)`, or
`MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`.** Stable `std`
exposes none of them. Each would need a platform crate or `unsafe`
code, three code paths, and a fallback anyway where the call is
unsupported. The workspace warns on `unsafe_code`.

**Rejected: an exclusive create alone.** It never replaces, but it lets
a reader see a partial file on every filesystem, not just the ones that
cannot link. It would therefore expose every filesystem to the
mid-write case described above.

**Rejected: writing the temporary with `fs::write`, as
`write_atomically` does.** See "Why the temporary is not written with
`fs::write`" above.

**Rejected: a lock file.** No lock may be a precondition for
correctness, and a peer writer would not take one.

**Rejected: failing the mint where hard links are unsupported.** Every
store write into a library on such a filesystem would fail until a
person created the file by hand.

## D5. Failure

**Decision.**

- **Absent, and the mint fails.** The guarded write is not made, and it
  fails the way that write's failure is already reported. The message is
  `cannot create the library's identity at <root>/.borax/library.toml:
  <error>`:

  | Where | Reported as | Human line |
  |---|---|---|
  | rename | `Admission::Unwritten { message }` (through `admit`'s `Unrecorded`) | `<path>: renamed but not recorded (cannot create the library's identity at …: …)` |
  | adopt | `Adoption::Unwritten { message }` | `<path>: could not be recorded (cannot create the library's identity at …: …), so it is still an orphan` |
  | reconcile | `Repair::Unwritten { message }` | `<path>: could not be put right (cannot create the library's identity at …: …)` |

  The rename is not undone, as for any failed store write ("An applying
  run records what it admits"). Nothing is retried within the guarded
  write. The next store write in the same run tries to mint again,
  because each write ensures the identity for itself. That is not a
  retry of the failed write, and the next run tries again too.
- **Present, but unreadable or malformed.** borax does **not** mint over
  the file: doing so would silently rename the library for every
  location that refers to it. It does **not** block the write either: an
  identity nothing in this change consumes is no reason to refuse to
  record a file. The write proceeds, and `validate` reports the file
  (D6).
- **A warning, once per run.** A run whose `StoreWriter` saw a `Faulty`
  identity writes one diagnostic to stderr through `Aftermath`. That
  channel is the one `record_faults_warning` uses. The run log does not
  carry it, because diagnostics are not events.
  - The warning is a `Diagnostic` of level `Warning`.
  - The function is `library::identity_fault_warning(&fault)`.
  - The message is `<path> could not be read as the library's
    identity, so it was left as it is and the library has none another
    location can name; borax validate reports why`, where `<path>` is
    `fault.path.display()`.
  - On the terminal it reads `warning: ` followed by that message.

  Reaching `dispatch`:

  | Command | Route |
  |---|---|
  | rename | the run's `StoreWriter::identity_fault()` |
  | reconcile | `Reconciliation::identity_fault` |
  | adopt | `Adoptions::identity_fault` |

  `dispatch` writes the warning after `Aftermath::library` and before
  `Aftermath::diagnostic`. A run that made no store write does not warn.
  A preview, `status`, `validate`, `resolve` and `bib` therefore never
  warn.

**Why warn.** It is cheap: one new field each on `Aftermath`,
`Reconciliation` and `Adoptions`, none of which a test builds as a
literal, and a fault the writer already holds. Without a warning, a
damaged identity file would surface only when someone ran `validate`,
or when change 13 first needed the identity.

**Rejected: a new event or skip.** The file is about the run's library,
not about a file the run settled. A skip would turn a healthy run into a
partial success. An event would add a type, which the brief excludes.

**Rejected: blocking the write.** That would make an identity nothing
yet reads into a reason to lose records.

**Rejected: minting over the file.** See above.

## D6. Validation

**Decision.**

- **The finding.** `validate` reads `identity(root)`. `Faulty(fault)`
  becomes one finding, `(fault.path, Finding::Unreadable { message:
  fault.message })`. It is counted in `findings`, and it makes the run's
  exit the partial-success code, as any finding does. The human line is
  the existing `<path>: unreadable (<message>)`.
- **Order.** The identity finding comes first, then `item_findings`,
  then `record_findings`. `Validation::findings`' docstring changes to
  say so.
- **Absent is not reported.** An absent file is neither a finding nor a
  condition.
- **Nothing is written.** `validate` writes nothing and mints nothing,
  as now.
- **The docstring.** The `Finding::Unreadable` docstring gains the
  library's identity file beside the store files. Its serialised form
  does not change.

**Why `Unreadable`.** It already means "a file that cannot be read or
does not parse as the record it claims to be". It carries the path on
the event and the message in the finding. A consumer already handles
it.

**Rejected: a new finding kind (`identity-unreadable`).** It would say
nothing a consumer cannot read from the path, and it would add
vocabulary.

**Rejected: a condition for an absent identity.** A condition is a
counted object named in the library. An absent file is not an object,
and every 0.9.0 library would gain one.

## D7. Reporting

**Decision.**

- **The key.** `Event::LibraryStatus` and `Event::LibraryValidated` gain
  `id: Option<String>`, declared directly after `root`, with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. It
  carries the canonical text when `identity(root)` is `Present`, and
  nothing when it is `Absent` or `Faulty`.
- **Where the value comes from.** `Survey` and `Validation` gain
  `identity: Option<LibraryId>`. `contents` reads `identity(root)` once,
  and both `survey` and `validate` read from it. `status_event` and
  `validation_events` copy that value into `id`.
- **Human lines.** When the identity is present, each report line ends
  with `; library <uuid>`, after every other clause. When it is absent
  or faulty, the line is byte-identical to 0.9.0's:
  - `<root>: N artifacts, N items, N records, N orphans[, N
    identifiable][ (excluding nested libraries: a, b)]; library <uuid>`
  - `<root>: N findings, N orphans, N missing, N unlinked; library
    <uuid>`

  The UUID is canonical hex, so it is not escaped.
- **No event for minting.**
- **The schema.** It stays 4. Schema 4 was first released in 0.9.0.
  `id` is a new key that a consumer ignoring unknown keys reads
  unchanged. The `cli` requirement "JSON Lines output is first-class"
  asks no bump for an addition. A line for a library with no identity is
  byte-identical to 0.9.0's.

**Why `id`.** The event is about the library. Every event that reports
on one record names its identity `id` (`library-repair`,
`library-admission`, `library-condition`). The `root` key beside it
already says which library.

**Why omit rather than `null`.** Change 15 omitted every new optional
key, so that a 0.9.0-shaped line stays byte-identical, and this follows
it. `identifiable`'s `null` predates that, and it distinguishes "not
asked" from zero, which has no counterpart here. In `jq`, `.id` reads
`null` either way.

**Why the clause at the end.** Existing human-line assertions, and a
reader's eye, find the counts where they always were. The identity is
a property of the library, not a count.

**Rejected: an `identity` object such as `{"kind":"present","id":…}`
with `absent` and `faulty`.** Validation already reports the faulty
case, and status reports no findings by requirement.

**Rejected: `status` reporting a faulty identity.** `status` reports
counts and names orphans. Findings are `validate`'s.

**Rejected: the identity on `run-started`.** That is deferred to
change 13.

## D8. The interface the tests are written against

`borax-core` (`crates/borax-core/src/library.rs`, beside `ArtifactId`):

```rust
/// The identity of a library: the tree under one marker, wherever that
/// tree is. Minted once by a caller, and rendered, compared, sorted and
/// serialized as `ItemId` is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LibraryId(Uuid);

impl LibraryId {
    /// `None` unless `text` is the canonical spelling of a UUID, of
    /// any version.
    pub fn parse(text: &str) -> Option<LibraryId>;
    pub fn from_uuid(uuid: Uuid) -> LibraryId;
    pub fn uuid(&self) -> Uuid;
}
impl fmt::Display for LibraryId { /* hyphenated */ }
impl TryFrom<String> for LibraryId {
    type Error = Malformed; // Malformed::Toml("not a canonical library uuid: \"<text>\"")
}
impl From<LibraryId> for String {}

/// The library's identity file, `.borax/library.toml`, as a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryIdentity {
    pub id: LibraryId,
}

impl LibraryIdentity {
    /// Exactly `id = "<uuid>"\n`.
    pub fn to_toml(&self) -> String;
    /// Fails with `Malformed` for text that is not TOML, an absent or
    /// non-string `id`, or one that is not canonical UUID text.
    /// Unknown keys and tables are ignored.
    pub fn from_toml(text: &str) -> Result<LibraryIdentity, Malformed>;
}
```

`borax-sources` (`crates/borax-sources/src/store.rs`):

```rust
/// D4. Fails with `ErrorKind::AlreadyExists` when `path` exists, leaving
/// it untouched. Leaves behind no temporary it created, and touches no
/// file it did not create.
pub fn create_atomically(path: &Path, content: &[u8]) -> io::Result<()>;

/// `create_atomically`, with `link` in place of `std::fs::hard_link`
/// and `name` in place of `temporary_name`. `name` is called once per
/// temporary name tried, at most 16 times. An error from `link` other
/// than `AlreadyExists` takes the exclusive-create fallback.
pub fn create_atomically_with(
    path: &Path,
    content: &[u8],
    link: &dyn Fn(&Path, &Path) -> io::Result<()>,
    name: &dyn Fn() -> String,
) -> io::Result<()>;
```

`borax::library` (`crates/borax/src/library.rs`):

```rust
/// The library's identity file, under `STATE_DIR`.
pub const IDENTITY_FILE: &str = "library.toml";

/// `root.join(STATE_DIR).join(IDENTITY_FILE)`.
pub fn identity_file(root: &Path) -> PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// No file at `identity_file(root)`.
    Absent,
    Present(LibraryId),
    /// A file is there and cannot be read or does not parse. `path` is
    /// the identity file.
    Faulty(StoreFault),
}

impl Identity {
    /// The id when `Present`.
    pub fn id(&self) -> Option<&LibraryId>;
}

/// D4's table. Reads; never writes.
pub fn identity(root: &Path) -> Identity;

/// D4. `Ok` is `Present` or `Faulty`, never `Absent`. `create` must
/// write without replacing and fail with `AlreadyExists` when the path
/// exists. It is `store_create` in a run, and a stand-in in a test.
pub fn ensure_identity(
    root: &Path,
    create: &dyn Fn(&Path, &[u8]) -> io::Result<()>,
) -> io::Result<Identity>;

/// `create_atomically`: the `create` seam a run passes.
pub fn store_create(path: &Path, bytes: &[u8]) -> io::Result<()>;

/// D3. The store writes a run makes into one library.
pub struct StoreWriter<'a> { /* root, create, write, first fault */ }

impl<'a> StoreWriter<'a> {
    pub fn new(
        root: &'a Path,
        create: &'a dyn Fn(&Path, &[u8]) -> io::Result<()>,
        write: &'a dyn Fn(&Path, &[u8]) -> io::Result<()>,
    ) -> StoreWriter<'a>;

    /// `ensure_identity(root, create)`, then `write(path, bytes)` unless
    /// that failed, in which case its error is returned and `write` is
    /// not called.
    pub fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// The first `Faulty` identity `write` met, if any.
    pub fn identity_fault(&self) -> Option<StoreFault>;
}

/// D5's warning.
pub fn identity_fault_warning(fault: &StoreFault) -> Diagnostic;

// Survey gains        `pub identity: Option<LibraryId>`
// Validation gains    `pub identity: Option<LibraryId>`
// Reconciliation gains `pub identity_fault: Option<StoreFault>`
// Adoptions gains     `pub identity_fault: Option<StoreFault>`
```

`borax::event`:

```rust
// Event::LibraryStatus    { root, id: Option<String>, artifacts, items,
//                           records, orphans, nested, identifiable }
// Event::LibraryValidated { root, id: Option<String>, findings, orphans,
//                           missing, unlinked }
// `id` is #[serde(default, skip_serializing_if = "Option::is_none")].
```

`borax::run`: `Aftermath` gains `pub identity: Option<Diagnostic>`.

These keep their signatures:

- `admit`, `Admitting`, `Admitted`;
- `reconcile`, `Reconciling`;
- `adopt`, `survey`, `validate`;
- `status_event`, `validation_events`;
- `store_write`.

Tests reach the run-level behaviour through `run::events_for`, and the
warning through `run::dispatch` with `Streams`.

## D9. How the specifications change

ADDED requirements in `library`:

- "A library carries an identity that does not depend on where it is",
  covering D1 and D2;
- "A library's first store write mints its identity", covering D3, D4
  and D5.

MODIFIED requirements in `library`, each restated whole, keeping every
living paragraph and scenario name:

- "A library is one directory tree":
  - the "moved, copied, synchronised or versioned as a unit" sentence
    gains the identity;
  - the no-root paragraph adds "no library identity";
  - one scenario is added.
- "borax reports a library it has never seen":
  - the first paragraph says the identity is not a count;
  - a paragraph gives the `id` key and the human clause, and says that
    `status` mints nothing;
  - two scenarios are added.
- "The library's text is authoritative and is read directly":
  - the identity file joins the authoritative state;
  - a sentence says it is not derived state;
  - "Derived state is disposable" gains "and so is the library's
    identity".
- "Validation defines a well-formed library":
  - a finding bullet for the identity file;
  - a paragraph covering absence, ordering and `library-validated`'s
    `id`;
  - three scenarios are added.
- "borax adopt records what the library already holds":
  - adoption also writes the identity, before its first record;
  - two scenarios are added.
- "Library conditions are named where they are counted". Its sentence
  "The totals lines keep their wording" gains an exception for
  validate's closing identity clause. Left as it was, it would
  contradict D7. An absent identity is not a condition, as the ADDED
  requirement states, so nothing else in it changes.

Each requirement's first line carries its SHALL. No MODIFIED requirement
drops a scenario or a paragraph, so no `<!-- drops: -->` marker is used.

The search behind this list covered `openspec/specs/` for these terms:

- `.borax/`, `STATE_DIR`, `write no library state`, `writes nothing`;
- `authoritative`, `moved, copied`, `copied`, `synchronis`;
- `initial`, `library-status`, `library-validated`, `schema version`;
- `keeps its wording`, `keep their wording`. These found the two
  sentences about report lines that D7's clause qualifies, in "borax
  reports a library it has never seen" and in "Library conditions are
  named where they are counted".

These hits are left alone:

- `library`, "Reconciliation repairs a stale path by a bounded walk"
  ("SHALL write no library state" over an untouched library). It holds,
  because no store write means no mint.
- `library`, "An applying run records what it admits": the preview,
  `--no-record` and failed-write paragraphs. Each holds, and the ADDED
  requirement states the identity half.
- `library`, "An artifact record is authoritative state keyed by an
  artifact identity" ("the first authoritative content `.borax/`
  holds"). This stays true in time.
- `cli`, "JSON Lines output is first-class". An addition needs no bump.
- `cli`, "A run's human summary fits its command". The report lines stay
  the last lines.
- `run-logs`. A run log is evidence, not store state.

## D10. Existing tests: what changes and how

The rules of the earlier changes apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. None is.

The search was `rg` over `crates/*/tests` and `crates/*/src` for the
following:

- `LibraryStatus {`, `LibraryValidated {`, `Survey {`, `Validation {`;
- `Reconciliation {`, `Adoptions {`, `Aftermath {`, `admit(`;
- `snapshot(` and `read_dir(` in every test that writes, and the
  `outside_the_stores` helper;
- the human report-line strings (`items, N records`,
  `findings, N orphans`).

`Survey`, `Validation`, `Reconciliation`, `Adoptions` and `Aftermath`
have no literal in any test. `admit`'s signature is unchanged.

| File | Test or site | Edit |
|---|---|---|
| `crates/borax/tests/dispatch.rs` | `Event::LibraryStatus { … }` constructors in nine tests: `status_over_a_marked_directory_of_new_pdfs_reports_every_count_unopened` (7365), `status_identify_counts_artifacts_yielding_an_identifier_and_queries_no_source` (7496), `status_identify_reports_each_artifact_in_survey_order_before_the_totals` (7586), `status_identify_tells_the_two_controlled_cases_apart` (7660), `status_identify_counts_a_title_holding_a_doi_as_identifiable` (7737), `status_identify_tells_encrypted_and_unreadable_apart` (7820), `status_over_an_unmarked_directory_is_reported_as_given_and_writes_nothing` (8455), `status_names_only_the_orphan_and_opens_nothing` (8535), `status_over_a_library_with_a_pending_change_opens_and_names_nothing` (10355) | (a) `id: None`. Each test runs `status` alone over a fixture seeded by writing files directly (`write_lib_artifact_record`, `write_lib_item`), so none holds an identity file |
| | `Event::LibraryValidated { … }` constructors in four tests: `validate_emits_the_finding_then_the_orphan_and_missing_conditions_then_the_totals` (9077), `validate_names_all_three_conditions_in_kind_order_and_succeeds` (9264), `validate_names_conditions_beside_a_finding_and_is_partial` (9429), `validate_names_two_missing_conditions_for_two_records_of_one_identity` (9611) | (a) `id: None`, for the same reason |
| | the `LibraryStatus { .. }` patterns at lines 7890, 8714 and 8796 | none: they already use `..` |
| | `outside_the_stores` (line 14845). Its only caller is `adopt_records_each_orphan_the_content_index_answers_for` (line 14957) | (b) Adoption now writes `.borax/library.toml`, so the helper also leaves that one file out. The test additionally asserts that `library::identity(&root)` is `Present`. The fact it pins, that adoption writes nothing outside the library state it owns, is unchanged |
| | the human report-line assertions near lines 8214, 8861, 9514, 16362, 16414 and 16469 | kept: their libraries hold no identity file, and the clause is written only when one does |
| | `a_failed_write_after_an_accepted_change_reports_unwritten_alone` (line 13236) | kept: it makes `.borax/artifacts/` read-only, not `.borax/`, so the mint succeeds and the record write fails as before. It asserts nothing about `.borax/library.toml` |
| | `adopt_leaves_the_unknown_and_the_recorded_alone_and_is_idempotent` (line 15052) | kept: its snapshot is taken after the first adoption, which mints |
| `crates/borax/tests/library.rs` | the snapshot tests around a writing reconcile, near lines 2522 and 2603: `a_touch_is_confirmed_by_hash_and_refreshes_the_recorded_fields` and `a_repair_settles_the_record_on_the_fast_path_for_the_next_reconcile` | kept: each takes its snapshot after the reconcile that wrote and minted, then compares a second run that writes nothing |
| | the snapshot tests over runs that write nothing, near lines 1517, 2318, 2431, 2945, 3013, 3053, 3134, 3285 and 4051 | kept: no store write means no mint |
| | every `admit(…)` call (32) | kept: `admit` is unchanged, and `&store_write` does no minting |
| | every `reconcile(…)` call | kept: `Reconciling` is unchanged |
| `crates/borax/tests/event.rs` | `all_events` | extended with `LibraryStatus` and `LibraryValidated`, each with and without `id`, by task 5.1. This is an addition, not an edit of an expectation |
| `crates/borax/tests/end_to_end.rs` | `library_names` (line 241) lists the root's entries. `records_of` (line 804) lists `.borax/artifacts/`. The store-alone test near line 948 counts `.borax/artifacts/` and `items/` | kept |
| `crates/borax/tests/runlog.rs` | the plain-file `.borax` tests (near line 1026) | kept: they abort before any rename, so no store write is reached |

Line numbers drift as earlier groups edit a file. The test stage reports
a site it cannot find, or a count that differs, rather than guessing.

## Not changed, deliberately

- What any library answers for its files. Resolution, duplicate checks,
  the gate, and reconciliation's matching do not read the identity.
- Discovery of a library root, and `.borax.toml`.
- The order of an admission's writes: the item before the record. The
  identity comes before both.
- `admit`'s, `reconcile`'s and `adopt`'s signatures, and their behaviour
  when no mint fails.
- Run logs, and every event other than `library-status` and
  `library-validated`.

## Risks / Trade-offs

- **A copy made before the first write.** Each copy mints its own
  identity at its first write, so the two become two libraries. This
  follows from decision 2. Copying a library after it has been written
  avoids it.
- **A fork carries its source's identity.** Two copies that someone
  means to be different libraries still answer as one. The remedy is to
  delete `.borax/library.toml` in one of them (Deferred).
- **Deleting the file renames the library.** A later reference
  (change 13) to the old identity would no longer find it by identity.
  `validate` does not report an absent file, by decision.
- **An unreadable file persists** until a person fixes or deletes it.
  `validate` reports it, and every run that writes warns about it.
- **Filesystems without hard links.** During a mint there is a window of
  a few microseconds in which a reader can see a short file. A crash in
  that window leaves one. A concurrent run that meets the file in that
  window writes without an identity and warns. If the minting run then
  fails and removes the file, the library is left with records and no
  identity until its next store write (D4).
- **A successful admission does not imply an identity.** Change 13 must
  not assume that every admission, adoption or repair that succeeded
  wrote into a library whose identity is `Present`. There are three
  exceptions:
  - the identity file was `Faulty` when the write was made, whether
    damaged or met mid-write under the fallback;
  - the identity was `Absent` afterwards, because a concurrent minter
    failed and removed its file;
  - the admission made no write at all, because `rewrite` rendered it
    unchanged, so nothing was ensured.

  Change 13 has to read the identity at the moment it stores a
  reference. Where it finds none, it should leave that reference
  unwritten, for a later run to restore, rather than invent or assume
  an identity.
- **A peer writer can replace the file.** No-replace protects only
  borax's own mint. A sync tool or a person can still overwrite the
  file, which then holds whatever they wrote.
- **One small read per store write.** This is negligible next to the
  write it guards (D3).

## Illustrative schema-4 lines

```json
{"schema":4,"event":"library-status","root":"/lib","id":"0190c3a2-7f21-7abc-8def-0123456789ab","artifacts":3,"items":2,"records":2,"orphans":1,"nested":[],"identifiable":null}
{"schema":4,"event":"library-status","root":"/lib","artifacts":3,"items":2,"records":2,"orphans":1,"nested":[],"identifiable":null}
{"schema":4,"event":"library-finding","path":"/lib/.borax/library.toml","finding":{"kind":"unreadable","message":"malformed toml: not a canonical library uuid: \"not-a-uuid\""}}
{"schema":4,"event":"library-validated","root":"/lib","findings":1,"orphans":0,"missing":0,"unlinked":0}
{"schema":4,"event":"library-validated","root":"/lib","id":"0190c3a2-7f21-7abc-8def-0123456789ab","findings":0,"orphans":0,"missing":0,"unlinked":0}
{"schema":4,"event":"library-admission","path":"/lib/Smith2024.pdf","admission":{"kind":"unwritten","message":"cannot create the library's identity at /lib/.borax/library.toml: Permission denied (os error 13)"}}
```

The `library-finding` line's `message` is illustrative. The parser's
exact wording, including any position it adds, is whatever `Malformed`'s
`Display` gives, and tests compare it with `contains`.

Human lines:

```text
/lib: 3 artifacts, 2 items, 2 records, 1 orphans; library 0190c3a2-7f21-7abc-8def-0123456789ab
/lib: 0 findings, 0 orphans, 0 missing, 0 unlinked; library 0190c3a2-7f21-7abc-8def-0123456789ab
/lib/.borax/library.toml: unreadable (malformed toml: …)
warning: /lib/.borax/library.toml could not be read as the library's identity, so it was left as it is and the library has none another location can name; borax validate reports why
```

The identity file:

```toml
id = "0190c3a2-7f21-7abc-8def-0123456789ab"
```
