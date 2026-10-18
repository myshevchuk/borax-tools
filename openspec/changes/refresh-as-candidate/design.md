# Design: refresh-as-candidate

## Context

Everything below was read from source at `786628f`, the merge of
`index-references-libraries`. This is change 14 of the roadmap's Phase
4, after 15, 12, 13a and 13b, all archived under
`openspec/changes/archive/` (2026-10-08 to 2026-10-18). Release 0.10.0
follows it.

**Settled by the maintainer** (the Phase 4 library-authority design
record, maintainer's notes, outside the repository):

1. A fresh lookup produces a candidate that requires acceptance. It
   never silently overwrites a library item.
2. The change preserves the distinction between obtaining a fresh
   candidate and accepting it as a replacement.
3. This change owns the flag spelling and the prompt.

**Decided by the maintainer for this change**, and recorded below with
the alternatives rejected:

- the flag, `--refresh`, apart from `--no-cache` (D1);
- what a refresh produces: the item still answers, and a fresh lookup
  of its preferred identifier is a candidate reported with what differs
  (D2);
- acceptance only in an interactive `rename`, as a document edit through
  the `StoreWriter` seam, and declining touches nothing (D3);
- another library's item is reported and never written, and a file
  answered by evidence or extraction just has its lookup made fresh
  (D4);
- additions to schema 5 only (D5).

**Settled by the maintainer after the critic gate**, and recorded
below:

- replacing field by field, as the designer proposed (D9);
- the re-read narrows and does not close the race with a concurrent
  write, which is stated, not locked against (D9, Risks);
- a replacement whose fields carry, at any depth, a key the model does
  not read or a comment is refused as a whole (D9, step 2);
- a reference answer is the run's own library's only when its identity
  and its lexically normalised root both match the run's library (D3),
  and the run's view of its root is refreshed after a replacement
  (D10);
- a refresh run mints an identity exactly when it makes a store write
  (D13 j).

**Decided by the designer and recorded below:**

- every lookup of the run is fresh, through a refreshing response cache
  that writes and never reads (D1, D6);
- `bib` takes the flag, and `resolve` never asks (D1, D3);
- the thirteen compared fields, their order and their equality (D7);
- the `refresh` key, its statuses, its acceptance and the four new
  reasons (D8);
- `run-started.refresh` (D8);
- what a replacement writes, field by field rather than the record
  whole, and the check that the item file has not changed since the run
  read it (D9);
- the `library-replacement` event, written when the write is made
  (D9);
- the refresh question: first, before any other; its choices and
  default; once per item per run; shown though passed over (D10);
- what Retry, Supply and Quit do around it (D10);
- the human strings and the description lines (D11).

What the engine does today, at the points this change touches:

- **The flags.** `ResolutionOptions` (`crates/borax/src/cli.rs`, line
  44) holds `--cache`/`--no-cache` (lines 61 to 67) and is flattened
  into `resolve` (line 171), `rename` (line 192), `bib` (line 212) and
  `config` (line 227). `config` takes it because an override is the
  question that command answers. `--apply` (line 189) is a field of
  `Command::Rename` and no setting, and the config loader refuses an
  `apply` key by name (`COMMAND_LINE_ONLY`,
  `crates/borax/src/config.rs`, line 782). `Settings` (cli.rs, line
  333) has no `apply`.
- **The response cache.** `run::execute` (`crates/borax/src/run.rs`,
  line 4255) builds the run's sources once, each through `polite` (line
  4394):
  - `Paced`, then `Cached` over it when `network.cache` is on (line
    4397);
  - `Cached::fetch` (`crates/borax-sources/src/cache.rs`, line 239)
    serves a held answer as `Retrieval::ServiceCache` and otherwise asks
    and stores (`Retrieval::Network { stored: Some(…) }`).

  `dispatch::resolve` (`crates/borax-sources/src/dispatch.rs`, line 90)
  asks the sources in priority order.
- **Resolution.** `pipeline::ResolveConfig` (`crates/borax/src/pipeline.rs`,
  line 188) holds `extraction` and `cache`. `run::resolving` (run.rs,
  line 1234) builds it from a directory's `Config`. `standing_in` (line
  733) asks the run's library through `Stores::consult`
  (`crates/borax/src/library.rs`, line 1178):
  - a tracked file is `library_record` (pipeline.rs, line 346), with
    every later step not attempted for `library-answered`, "whatever
    `ResolveConfig::cache` says, since the library is not a cache"
    (line 480);
  - otherwise `beyond_library` (line 802) reads references only with
    the cache on (line 814), then the evidence (`index_read`, whose
    `Bypassed` is the `--no-cache` read, line 229), then extraction.

  A reference answer is `referenced_standing` (line 311). Both answers
  then go through the work check (`Standing::of`, line 608).
  `Consulted::item` (library.rs, line 1495) is the item answered with:
  the link, or the item named as `accepted_under`.
- **The verdict.** `FileRecord` (pipeline.rs, line 67) carries `record`,
  `hash`, `evidence`, `overridden` and `accepted`. `resolved_event`
  (line 1440) builds `Event::Resolved` (`crates/borax/src/event.rs`,
  line 63) from it. `Sections` (event.rs, line 935) is flattened into
  `resolved` and resolution `skipped` events alike.
- **Items.** `Item` (`crates/borax-core/src/library.rs`, line 293) is
  `id` and `record`. Its file is `id`, then `[record]` (`to_toml`, line
  392). `from_toml` (line 415) ignores keys the record model does not
  know. An item is written only when minted:
  - `select_item` (borax library.rs, line 3160) writes a minted item
    whole (line 3180);
  - `admit`'s contract says "an item that was already there is not
    rewritten" (line 2997).

  Every store write goes through a `StoreWriter` (line 472), which
  ensures the identity first. `rewrite` (line 2162) is the precedent for
  a `toml_edit` edit that keeps formatting and comments, for artifact
  records. `ItemStore::file_of` (line 680) names an item's file.
  `Stores` keeps its `ItemStore` private.
- **The rename driver.** `rename_events` (run.rs, line 1317):
  - consults the start-of-run snapshot whatever `record` says (line
    1465);
  - keeps its account in a filtered copy (line 1381);
  - holds a file's rendering while `rename.skip-named` may pass it over
    (lines 1444 to 1459);
  - writes through one `StoreWriter` (line 1338).

  `asked` (line 1963) puts the questions, and `situation` (line 2388)
  chooses them. An already-named file under `skip-named` is `Settle`
  (lines 2473 to 2478). Only `Command::Rename` can be interactive
  (`session_for`, line 4364).
- **Questions.** `Answer` (`crates/borax/src/session.rs`, line 138) has
  eight variants, labelled in `Choice`'s `Display` (line 291). `describe`
  (`crates/borax/src/describe.rs`, line 115) renders a description from
  the `resolved` event, and `file_says` (line 370) is its titles line.
- **The stream.** `Event::RunStarted` (event.rs, line 41) carries
  `command`, `version`, `applying`, `interactive` and `tables`, and is
  built in `dispatch` (run.rs, line 3833). `human_line` (event.rs, line
  1678) renders `resolved` with the reference clauses, then the
  rejected-candidate clause last. `Unattempted` is at
  `crates/borax/src/evidence.rs`, line 796. `pipeline::preferred_identifier`
  (line 1455) is DOI, then arXiv, PMID, ISBN.

## D1. The flag: `--refresh`, a selector on three commands

**Decision.** `resolve`, `rename` and `bib` each take `--refresh`, a
field of the subcommand as `--apply` is.

- **What it means.** Every lookup the run makes asks the service: the
  response cache is not read, and each answer is stored in it (D6).
  Where a library item answers for a file that resolves, the item's
  preferred identifier is looked up too, and the record found is a
  candidate (D2).
- **Which files.** Every file of the run.
  - For a file an item answers for, the refresh is the candidate.
  - For every other file it is its own ordinary resolution, with the
    lookup made fresh: the content index is read as usual, and its
    evidence or the file's extraction supplies the identifier.
- **Not a setting.** It is not in `ResolutionOptions`, so `config`
  refuses it. No configuration key sets it, and it has no `--no-`
  form, like `--identify`, `--rehash` and `--accept`. `refresh` in a
  configuration file is an unknown key, as `identify` is.
  `COMMAND_LINE_ONLY` stays `apply` alone. That list names a key a
  reader expects to be settable and must be told is not, and `refresh`
  authorises no write.
- **`--no-cache` keeps its meaning.** It still bypasses the response
  cache and the content index's read, references included, and still
  does not bypass the library. The two compose: `--refresh --no-cache`
  extracts every untracked file afresh, asks every service with no
  cache in front, and makes a candidate for every tracked file.
- **`bib` takes it.** `bib` cites what files resolve to. For an
  untracked file, a fresh lookup changes what is cited, so the flag
  changes what `bib` writes, which is the `cli` criterion. For a
  tracked file `bib` cites the item, as ever, and reports the
  candidate.

**Why every file.** The flag says "ask the services again, now". That
means the same thing for a file the content index answers for as for
one the library answers for. Restricting it to item-answered files
would make `--refresh` a silent no-op on every other file. Getting a
fresh record for those would then need `--no-cache`, which also throws
away the identification evidence, a blunter tool than the request. A
file outside every library has no authoritative item to protect, so
asking afresh is all a refresh can mean for it.

**Rejected:**

- **Giving `--no-cache` the meaning.** It would change a released
  flag's meaning for tracked files, from "the library still answers,
  nothing is asked" to "a lookup is made". Folding the two together is
  also what the settled distinction rules out: `--no-cache` is about
  caches, and the library is not one.
- **Refreshing only the files an item answers for.** Argued above.
- **`network.refresh` as a setting, with `--no-refresh`.** A file that
  turned it on would make every run in its tree ask every service for
  every file. The point of the response cache, a re-run at disk speed,
  would be gone without anybody having asked at the command line.
- **`--refresh` in `ResolutionOptions`.** `config` would then accept
  a flag it has no setting to report.
- **A `borax refresh` subcommand.** It would duplicate `resolve`'s
  discovery, configuration and reporting, and could not offer
  acceptance without becoming a second interactive driver beside
  `rename`'s.

## D2. What a refresh produces: a candidate beside the item

**Decision.** In a run given `--refresh`, a file whose verdict is
`resolved` from an item gets a refresh. The item is the run's own,
`record_retrieval` `library`, or another library's through a reference,
`library-reference`.

- **What is looked up.** The item's preferred identifier
  (`preferred_identifier`, D12), over the run's sources, in
  `dispatch::resolve` order. The item answered with is the one
  `Consulted::item` and the reference name: for bytes accepted under an
  earlier item, that item.
- **The verdict.** It stays the item's. The file resolves to the item's
  record, its sections are what they were, and it is renamed and cited
  from it. The only exception is an interactive replacement (D10).
- **The candidate.** The record the lookup found. It is compared with
  the item's record (D7). It is `current` when equal in every compared
  field and needs nothing. Otherwise it `differs`, and is offered or
  reported (D3, D4).
- **Steps not taken.** No title check: the file's titles were not read,
  and the candidate is judged against the item, not the file. No
  content-index read or write, no extraction. The lookup's response is
  stored in the response cache like any network answer (D6).
- **When there is none.** A verdict that is not `resolved` gets no
  refresh. That covers the work-duplicate skip a library or reference
  answer can still reach in a rename's account, and it holds even when
  an interactive operator then files that file as another artifact: its
  later `resolved` event carries no `refresh`. A file answered by the
  content index's evidence or by extraction gets no refresh either: its
  own lookup is the fresh one.

**Rejected:**

- **The candidate as the verdict, the item as fallback.** That is the
  silent overwrite the settled decision forbids, applied to renaming
  and citing rather than to the item file.
- **Looking up every identifier the item carries.** Two identifiers can
  name two records, a preprint's and the published one. Which is "the
  fresh record" would then be a choice. The preferred identifier is
  the one every other comparison uses (13b's agreement rule).
- **Checking the candidate's title against the file.** The question is
  whether the service and the library disagree about the work, not
  whether the file agrees with the service. Opening every tracked file
  would also make a refresh of a library cost a full extraction pass.

## D3. Acceptance: only an interactive rename, into its own library

**Decision.** A differing candidate is offered only where all of these
hold:

- the run is an interactive `rename`;
- the run records (`record` on);
- the item is the run's own library's (below);
- the operator has not already answered for that item in this run.

**Whose item it is.** An item is the run's own library's (`Whose::Own`)
when:

- the run's library answered for the file (`record_retrieval`
  `library`); or
- a content-index reference answered for it, the reference's `library`
  equals the run's library's identity, and the reference's `root`,
  normalised lexically, equals the run's library root, normalised
  lexically (`Stores::root`). This is an untracked copy inside the run's
  library, answered through a reference to that same library at that
  same root.

The run's library's identity is the one `library::identity` gives for
its root when its stores are read (`Stores::identity`), `Present` only.
A library with an absent or faulty identity can be named by no
reference (13b writes references only for a `Present` one), so every
reference answer in such a run is another library's. Every other
reference answer is another library's (`Whose::Referenced`): another
identity, or the same identity at another root, which is a copy of the
library elsewhere. A self-reference is treated exactly as a library
answer from here on: the same offer, the same item file
(`Stores::item_file`), the same write path and the same once-per-item
memory, keyed by the item's identity.

Accepting replaces the item's record through the run's `StoreWriter`
(D9). Declining leaves the item exactly as it was. Everywhere else the
candidate is reported, with an acceptance `not-attempted` and the first
reason that applies:

| Order | Reason | Where |
|---|---|---|
| 1 | `another-library` | the item is another library's: a reference to another identity, or to the run's library's identity at another root (D4) |
| 2 | `not-asked` | there is nobody to ask: `resolve`, `bib`, a batch `rename`, previewing or applying |
| 3 | `not-recording` | an interactive `rename` with `record` off (`--no-record` or configuration) |
| 4 | `already-asked` | an interactive `rename` whose operator already answered for the item at an earlier file of this run (D10) |

**`resolve` does not offer acceptance.** It is never interactive: only
`Command::Rename` can be (`session_for`, run.rs, line 4364). `resolve`
is the scripting entry point whose stdout is records, and it runs files
on a pool where a question would block the others. `rename --refresh`
offers acceptance for every file, an already-named one included (D10),
so a person who wants to accept runs that. A dedicated acceptance
command is Deferred.

**Rejected:**

- **Writing in an applying batch run.** `--apply` authorises moves, and
  a batch run has nobody to show the differences to. A replacement it
  made would be exactly a fresh lookup silently overwriting a
  correction.
- **Accepting automatically where the item was never corrected.**
  borax keeps no copy of the record it minted an item from: the content
  index holds identifiers since 13a. It therefore cannot tell a
  correction from staleness.
- **Offering acceptance in `resolve` on a terminal.** That would be a
  second interactive mode with its own rules for `--json`, pipes and
  concurrency, for a command whose output is the record stream.

## D4. Outside the run's library

**Decision.**

- **Through a reference.** A file answered by a reference (13b) gets a
  refresh of the referenced item. Where the reference names the run's
  own library at its own root, the item is the run's own (D3). Otherwise
  its candidate is reported with `another-library` and never written: no
  run writes into a library other than its own. The same identity at
  another root is a copy of the library, which the run holds no
  `StoreWriter` and no account for, and which may not even be reachable
  for writing. The human line and the description name the library that
  can take it, by its root.
- **Evidence or extraction.** A file answered by the content index's
  evidence or by extraction gets no refresh. `--refresh` makes its
  lookup fresh, and the result is its ordinary resolution: the record
  the services return now, its title check, and its extracted evidence
  written as ever. There is no authoritative item to protect.

**Rejected:**

- **Writing the candidate into the referenced library.** The run has
  no `StoreWriter` there and no account of it. The referenced library's
  own runs are where its items change.
- **Judging a reference by its identity alone.** Two copies of one
  library share an identity (change 12). A run in one copy writing into
  the other would be a write into a library other than its own.
- **Judging it by its root alone.** A root can hold another library
  than the one the reference was written for. 13b reports that as
  `other-library`, and it is not this run's item.
- **Re-extracting evidence-answered files under `--refresh`.** That is
  `--no-cache`'s job. A refresh is about the record, not about which
  identifier names the file.

## D5. The schema: 5, added to

**Decision.** No release carries schema 5, so the `cli` requirement
"JSON Lines output is first-class" lets it change without a bump. This
change only adds:

| Addition | Present |
|---|---|
| `run-started.refresh`, `true` | only on a run given `--refresh` |
| `resolved.refresh` (D8) | only on a run given `--refresh`, for a verdict whose own record an item answered with |
| event `library-replacement` (D9) | only when an interactive rename writes a replacement |
| reasons `no-identifier`, `another-library`, `not-recording`, `already-asked` | inside `refresh` only |

**Every line a run not given `--refresh` writes is byte-identical** to
13b's, since both keys are omitted when absent.

**Re-checked and unchanged in meaning:**

- `lookup` is still the lookup the verdict's record came from. A
  refresh's attempts are in `refresh`, never in `lookup`. A tracked
  file's `lookup` stays not attempted for `library-answered`, and a
  reference answer's for `content-index-hit`.
- `record_retrieval` `library` still names the item whose record the
  verdict carries. After an interactive replacement that record is the
  item's as replaced. Before this change an item never changed during
  a run, so "the item's record" and "the item's record as read" were
  the same thing. Now the event says which, through
  `refresh.acceptance` `accepted`.
- The verdict's own `acceptance` stays `automatic` for an item's record.
  What the operator decided about the candidate is
  `refresh.acceptance`, as a submission's is its own (D8).
- `service-cache` never appears in a run given `--refresh`, because the
  cache is never read. That is a value not emitted, not a value whose
  meaning changed. `network` with `stored` `written` keeps its meaning,
  now possibly over an entry the cache held.
- `already-named`, the passed-over count and `content-index-write` are
  unchanged.

**Rejected:**

- **A bump to 6.** Nothing changes meaning, and 5 is unreleased either
  way.
- **`refresh` on every resolution event**, not attempted when no
  refresh ran. It would change every schema-5 line and every test
  pinning one, for a key that says nothing on a run not given the flag.

## D6. A refreshing response cache, run-wide

**Decision.** `Cached::refreshing(source, cache)` is the same decorator
with reading turned off:

- `fetch` never calls `cache.get`;
- it asks the wrapped source;
- it stores a successful answer with `cache.put`, returned as
  `Retrieval::Network { stored: Some(write) }`;
- it stores nothing for a failure.

`run::polite` takes the run's selector. With the cache on, it wraps the
paced client in `Cached::refreshing` under `--refresh` and in
`Cached::new` otherwise. With the cache off it adds no cache either way.
`execute` reads the selector from `Command::refreshes`.

**Why run-wide.** Every lookup of a refresh run is fresh (D1): the
candidate's, an evidence-answered file's, an extracted file's, and a
Retry or a Supply in its interactive questions. One source set suffices,
and each service keeps one `Paced` clock, so the pacing the `resolution`
requirement "Network use is polite and bounded" demands holds across
all of them.

**Why the answer is stored.** The response cache holds what the
services said, and a fresh answer is what they say now. Leaving the old
one would make the next run without `--refresh`, and `borax adopt`,
which reads that cache (`run::held_answer`), serve a staler record than
the one this run just fetched. Storing it is no acceptance: the
response cache is a cache, and the library is not.

**Rejected:**

- **`Source::fetch_fresh` and a `dispatch::resolve_fresh`.** Needed
  only if some lookups of a run were fresh and others not. They add a
  trait method every decorator must forward correctly.
- **A second source set in `Adapters`.** Two `Paced` clocks per
  service, or an `Arc` to share one. A field added to `Adapters`
  changes 282 literals in `crates/borax/tests/dispatch.rs` alone.
- **Not storing the fresh answer.** Argued above.

## D7. What is compared, and how a difference is stated

**Decision.** Thirteen fields, in this order, named by the key the
record serializes them under:

| # | Field | Record key |
|---|---|---|
| 1 | entry type | `type` |
| 2 | title | `title` |
| 3 | authors | `author` |
| 4 | date of issue | `issued` |
| 5 | container | `container-title` |
| 6 | volume | `volume` |
| 7 | issue | `issue` |
| 8 | pages | `page` |
| 9 | publisher | `publisher` |
| 10 | DOI | `DOI` |
| 11 | PMID | `PMID` |
| 12 | ISBN | `ISBN` |
| 13 | arXiv identifier | `arxiv` (under `borax`) |

- **Equality** is equality of the field's JSON value as `Record`
  serializes it, an absent field included. Nothing is normalised:
  - a title differing only in whitespace or case differs, since the
    record keeps values as the source supplied them;
  - authors are compared in order, a reordered list differing;
  - `issued` is compared to its precision, so `2015` and `2015-03`
    differ.
- **Not compared:**
  - `borax.provenance`, which says where a value came from, not what
    the work is. A hand correction leaves it naming the service;
  - `borax.confidence`, which no lookup by identifier sets;
  - `borax.source_fields`, the service's own bookkeeping (Crossref's
    `subject`, arXiv's categories, the OpenAlex id), which no person
    corrects and which would make a refresh differ every time a service
    reclassifies a work.
- **A difference** is the field, the item's value and the candidate's
  value. Either value is absent where that record has none. Differences
  come in the table's order.

**Rejected:**

- **Whole-record equality.** Provenance and source fields would make
  nearly every refresh `differs`, with nothing a person could judge.
- **Comparing the names the records render.** A name depends on the
  configuration in force, and a difference in pages never shows in one.
- **Normalising before comparing.** The item is text a person edits.
  Whether a whitespace or case correction matters is theirs to decide,
  and the description shows them both.

## D8. How a refresh is reported

**Decision.** `Event::Resolved` gains `refresh`, written after the eight
sections, so after `acceptance`. It is present exactly when the file got
a refresh (D2): its own resolution reached an item's record, so the
event's `record_retrieval` is `library` or `library-reference`. Where an
operator's submission later displaced the file's own steps,
`identifier_input.displaced.record_retrieval` names the item instead,
and the verdict keeps the refresh. It is tagged by `status`:

| `status` | Keys beside it | When |
|---|---|---|
| `not-attempted` | `reason`: `no-identifier` | the item's record carries no DOI, arXiv id, PMID or ISBN |
| `no-eligible-service` | `identifier` | no configured service supports its kind |
| `unresolved` | `identifier`, `attempts` | every service asked failed, an outage or `not-found` alike |
| `current` | `identifier`, `attempts` | the candidate equals the item in every compared field |
| `differs` | `identifier`, `attempts`, `differences`, `candidate`, `acceptance` | otherwise |

- `identifier` is the identifier looked up, as the stream writes one
  (`doi:…`).
- `attempts` are the services asked, in order, each as the `lookup`
  section reports one: `service`, and an `outcome` tagged `found` (with
  `retrieval` and `stored`), `not-found`, `unavailable`,
  `rate-limited` or `malformed`. They are never empty, and a found
  attempt is last.
- `differences` is the D7 list, each
  `{"field": <record key>, "item": <value>, "candidate": <value>}`, with
  `item` or `candidate` left out where that record has no value.
- `candidate` is the whole fresh record.
- `acceptance` is tagged by `status`:
  - `pending`, carried only by the event a refresh question's
    description is rendered from, never by a reported verdict;
  - `accepted`, the operator replaced the item's record and the write
    landed;
  - `unwritten` with `message`, the operator chose to replace it and
    the write failed or was refused (D9), so the item stands;
  - `declined`, the operator kept the item's record;
  - `not-attempted` with `reason`, D3's table.

`run-started` gains `refresh: true` on a run given `--refresh`, left out
otherwise. A run log then says why no answer came from the response
cache.

**Why on `Event::Resolved`, not in `Sections`.** `Sections` is also a
resolution skip's, and a skip never carries a refresh (D2). On
`Resolved` the field can only be where it means something. The JSON is
the same either way: `refresh` is one more top-level key.

**Rejected:**

- **A `refresh-candidate` event after `resolved`.** The candidate is
  evidence about the verdict's record and belongs on the verdict, as a
  rejected submission's record does. A separate event would also have
  to be held, released and ordered with the verdict in an interactive
  run.
- **The lookup as a `LookupStep` with a new `origin` `item`.** That adds
  a value to `lookup.origin`, which every consumer reads as "where the
  verdict's identifier came from". It would also force an
  `Origin::Item` through `FileRecord::identification` and `remember`,
  which must never treat a candidate as identification evidence.
- **`differences` as a map keyed by field.** JSON maps carry no order,
  and the order is part of the contract (D7).

## D9. The replacement: what is written, and how

**Decision.** `library::replace_record(file, expected, record, write)`:

1. **Re-read.** It reads `file` and parses it as an item. It refuses,
   writing nothing, when:
   - the file cannot be read: `<file> could not be read (<error>)`;
   - it does not parse, or holds another identity: `<file> no longer
     holds item <id>`;
   - its record is not `expected.record`, the record the candidate was
     compared with: `<file> changed after this run read it`.

   That last refusal catches an edit completed before the re-read: one
   a person saved while the question was on screen, or another run's
   replacement written before this one's check. It cannot catch one
   that lands after it (below).
2. **Refuse to discard annotations.** The keys to be replaced are those
   whose value differs between `expected.record` and `record`: compared
   fields in `[record]` (and `arxiv` in `[record.borax]`), and their
   entries in `[record.borax.provenance]`. For each, the value the
   re-read document holds is inspected. The replacement is refused as a
   whole, writing nothing, when any of them is annotated:
   `<file> holds notes in <fields> that replacing them would discard`,
   naming every annotated key in D7's order, provenance entries as
   `provenance.<field>`. A value is annotated when either of these
   holds:
   - **It holds a key the model does not read.** The keys the item's
     serde shape reads inside a compared field are exactly: in an
     `author` entry (an array of tables or of inline tables), `family`
     and `given` (`Name`); in `issued` (a table or inline table),
     `date-parts` (`DateParts`'s `Csl`). Every other field, and every
     provenance entry, is a string and holds no keys. Any other key at
     any depth inside the value is one the model does not read: `orcid`
     in an author table, `circa` in `issued`, a stray key in a nested
     inline table. Deserializing ignores such a key, so it would never
     reach the description, and replacing the value would delete it.
   - **It holds a comment.** That is any comment on a line the value
     occupies: from its `key =` line, a comment trailing the value
     included, to the value's last line. For a value written as a table
     or an array of tables (`[record.issued]`, `[[record.author]]`), it
     runs from the first header to the line before the next header or
     key outside it, and takes in the comment lines directly above each
     of its headers. A comment on the lines above a plain `key =` line
     is the key's, not the value's: the key is kept where it is and the
     comment with it.

   A replaced value carrying neither is replaced as step 3 says. The
   refusal is all-or-nothing, so that no item is left half replaced
   with the operator told only about the half.
3. **Edit as a document.** It parses the text with `toml_edit` and sets
   every key step 2 named, or removes it where `record` lacks it. Keys
   holding the same value are not touched. These survive untouched,
   byte for byte:
   - `id`;
   - every key the record model does not know, in `[record]` or
     elsewhere, since step 2 refused any inside a replaced value;
   - every comment, by the same rule, and the key order and formatting
     of every key left alone.

   The file name never changes: it is a creation-time label (`library`
   requirement "An item file's name carries a legible key and its
   identity").
4. **Write.** It writes the whole file through `write`, which is the
   run's `StoreWriter::write`. That ensures the library's identity
   first, minting an absent one, and replaces the file atomically
   (`write_atomically`, `crates/borax-sources/src/store.rs`, line 441:
   a temporary renamed over the file). A failed mint, or a failed
   write, is the error the writer returns, naming the identity file for
   a mint.

**The window that remains.** The rename in step 4 is an unconditional
replacement: no portable filesystem operation replaces a file only if
it still holds what was read. So between step 1's re-read and step 4's
rename there is a window. An edit saved there, by a text editor or by
another run's replacement, is overwritten by this write, and nothing
reports the edit that was lost. borax takes no lock, since the library's
writers are peers and a text editor takes none either.

- **Protected:** every edit completed before the re-read, which refuses
  the write and is reported as `failed`.
- **Not protected:** an edit landing between the re-read and the rename.
- **Beyond that window**, nothing is lost silently. Every write borax
  makes is reported in a `library-replacement` event naming the file and
  the fields, in the stream and the run log, and the file's previous
  record is in that run's `resolved` event, `refresh.differences`
  giving each replaced field's old value.

The window is the time it takes to parse, edit and render one small
file. An editor holding the file open and saving later overwrites
borax's write in turn; most editors warn that the file changed on disk.

**What `record` is.** `refresh::replaced(item, candidate)`, the item's
record with:

- every differing compared field (D7) taken from the candidate, set or
  removed;
- the provenance entry under each such field's name taken from the
  candidate's, set or removed.

Everything else is the item's own: its other provenance entries, its
source fields, its confidence. So `replaced(item, candidate)` has no
difference from the candidate, a refresh after it is `current`, and the
write changes nothing the operator was not shown, apart from the
provenance of the fields they were.

**The event.** `library-replacement` reports every write attempted:

```json
{"event":"library-replacement","path":"…","item":"<uuid>","file":"<item file>","fields":["title"],"write":{"status":"written"}}
```

- `path` is the file the question was about, where it is now.
- `file` is the item file, as a full path.
- `fields` are the differing fields' names, in D7 order.
- `write` is `written`, or `failed` with the `message` from step 1,
  step 2 or step 4.

It is emitted when the write is made, at the answer (D10). It counts
toward nothing, and a failure changes no exit status, as a failed
admission's does not.

**What else a replacement changes:**

- no artifact record, no history entry, no link;
- no content-index fact and no reference, since the item's identity is
  unchanged and outside runs read the item;
- the run's own view (D10).

**Rejected:**

- **Writing the candidate whole**, provenance, source fields and
  confidence included. It would overwrite, unshown, anything a person
  edited in those parts. That is a silent overwrite of a correction,
  however unusual the correction.
- **Writing at the file's settlement, after its move question.** A
  write that then fails would leave a file already moved, or about to
  be, to a name its library's record does not render. And a Quit would
  have to discard an answer the operator gave. Writing at the answer
  tells the operator about a failure before the move question, and the
  file goes on with the record that actually stands.
- **Writing without re-reading.** The question can sit on screen while
  the operator edits the very item in another window.
- **A lock file.** No other writer of the library, a text editor least
  of all, would honour it, and a stale lock left by a killed run would
  need its own recovery.
- **Rewriting through `Item::to_toml`.** It would drop every unknown
  key and comment.
- **Replacing the annotated fields' values and keeping their notes
  somewhere.** There is nowhere in the candidate's value to keep an
  `orcid` the record model does not hold, and a comment has no position
  in a value rewritten whole.
- **Replacing the unannotated fields and skipping the annotated ones.**
  The item would then equal neither its old record nor the candidate,
  and a refresh after it would differ again with no question settling
  it. The operator answered about the whole candidate.

## D10. The interactive question, and the run around it

**Decision.**

- **When.** In an interactive `rename`, a file's refresh that `differs`
  and that D3 offers puts the **refresh question**. It comes first,
  before the situation's own question (move, keep name, filing), and
  whatever `rename.skip-named` says. Its target is the file itself,
  since no move is in play.
- **Choices**, in order: `Answer::KeepRecord`, "Keep the library's
  record" (the default); `Answer::Replace`, "Replace it with the fresh
  record"; and `Answer::Quit`. Keep leads because Enter must never be
  the answer that overwrites a library correction. Skip is not offered:
  keeping the record goes on to the file's own question, where skipping
  is offered whenever there is a move to decline.
- **The description** is the file's `resolved` event with
  `refresh.acceptance` `pending`, with no `new name` line, since the
  name depends on the answer. It shows the `refresh`, `differs` and
  `decision` lines (D11).
- **Keep.** The acceptance becomes `declined`, nothing is written, and
  the file goes on to its own situation with the item's record.
- **Replace.**
  1. `replace_record` is called at once, with the item file
     (`Stores::item_file`), the item as the run consulted it, and
     `refresh::replaced`.
  2. `library-replacement` is emitted at once.
  3. Written: the acceptance is `accepted`, and the file's record
     becomes the replaced one. The stores the run consults and its
     account both learn the item as replaced (`Stores::learn_item`),
     because a later file of the same item must be answered with what
     the item now holds, and the `ledger` requirement makes the work
     check answer from "the library as the run has left it so far".
     The run's `references::Libraries` forgets the run's own root
     (`Libraries::forget`), so a later file answered through a
     self-reference (D3) reads that root's items afresh and meets the
     replaced record. Forgetting rather than updating, because what
     `Libraries` caches for a root is its whole item store as read, and
     the file just written is what a fresh read finds. Another root,
     including a copy of the run's library elsewhere, is not forgotten:
     its item was not written.
  4. Failed or refused: the acceptance is `unwritten` with the message,
     and the file goes on with the item's record as the run read it.
  5. The file goes on to its own situation, which may now be a move to
     the name the replaced record renders, or already named.
- **Quit** at the refresh question ends the run there and writes
  nothing. Quit at a later question about the same file, after Replace,
  ends the run as any quit does: the file is counted unreached and has
  no verdict. The replacement stands, being work already done, and its
  `library-replacement` event is in the stream and the run log, written
  when the write was made.
- **Once per item.** The driver keeps, for the run, every item whose
  refresh question it put. A later file answered by such an item is not
  asked again:
  - after Replace it is consulted against the item as replaced, by the
    run's library or through a self-reference, so its candidate is
    normally `current`;
  - after Keep, or after Replace that did not write, a candidate that
    still differs is reported with `already-asked`.
- **Shown, not passed over.** Under `rename.skip-named`, an
  already-named file is passed over only when it has nothing to report:
  no refresh, or a refresh that is `current`. A file whose refresh is
  anything else is shown, and counts as shown in the summary: its
  question was put, or it carries a fact the operator asked for with
  `--refresh` (an outage, no identifier, a candidate not offered). It is
  still not asked the already-named question `skip-named` suppresses.
- **Retry** is never offered for a refresh. An item-answered file made
  no lookup of its own, so its own question offers no Retry, as today.
  A refresh that found nothing costs the file nothing, since the item
  stands, and `rename --refresh` again is the retry.
- **Supply** is unchanged and is not offered at the refresh question.
  At the file's own question it re-identifies the file as ever. The
  verdict reached carries the file's refresh as it stood, as it carries
  the file's own lookup in `displaced`. A replacement already made
  stands: refreshing an item and re-identifying a file are separate
  acts.
- **The hold.** Putting the refresh question ends the file's hold, so
  the `library-replacement` line and the file's lines are shown.

**Rejected:**

- **Folding the choice into the move question** (rename and replace,
  rename and keep, …). The move question's choices multiply, and a file
  with no move, the common case for a library already in order, would
  need a third question shape anyway.
- **Asking after the move.** The file would be named from a record the
  operator then replaces, or is moved before the question that decides
  its name.
- **Asking again for a second file of the same item.** The answer is
  about the item, which is one thing however many files it has.
- **Keeping `skip-named`'s silence for a differing candidate.** A
  person running `rename --refresh` over a library in order would then
  never be asked, which is the one thing the run was for.

## D11. Human strings

Every value from a record, an item file, a library store or a service
is escaped, as the text after `<path>: ` already is.

**The `resolved` line.** One clause, after the reference clauses and
before the rejected-candidate clause, which stays last:

| `refresh` | Clause |
|---|---|
| `not-attempted` `no-identifier` | `; no fresh record: the library's record carries no identifier to ask about` |
| `no-eligible-service` | `; no fresh record: no configured service could be asked about <identifier>` |
| `unresolved` | `; no fresh record: no source had a record for <identifier> (<answers>)`, each service's answer as the `unresolvable` skip line words it, joined by `; ` |
| `current` | `; a fresh record from <service> does not differ` |
| `differs` | `; a fresh record from <service> differs in <fields>; <acceptance>` |

`<service>` is the found attempt's service, lowercase as after `via`.
`<fields>` are the differences' field names joined by `, `.
`<acceptance>` is one of:

| `acceptance` | Text |
|---|---|
| `not-attempted` `not-asked` | `the library's record is unchanged` |
| `not-attempted` `another-library` | `only a rename in the library at <root> can take it`, with the reference's root from `record_retrieval` |
| `not-attempted` `not-recording` | `this run records nothing, so the library's record is unchanged` |
| `not-attempted` `already-asked` | `answered at an earlier file of the same item` |
| `declined` | `the library's record was kept` |
| `accepted` | `the library's record was replaced` |
| `unwritten` | `the library's record could not be replaced (<message>)` |
| `pending` | `awaiting an answer` (never on a reported line) |

**The `library-replacement` line:**

- written: `<path>: replaced <fields> in <file>`;
- failed: `<path>: could not replace <fields> in <file> (<message>)`.

**The description.** After `file says` and before `conflict`:

- a `refresh` line:
  - for `no-identifier`, the text `the library's record carries no
    identifier to ask about`;
  - otherwise the identifier, written whole, then one continuation line
    (empty label):
    - `current`: `<Service> does not differ from the library's record`;
    - `differs`: `<Service> differs from the library's record`;
    - `no-eligible-service`: `no configured service can be asked about
      it`;
    - `unresolved`: one line per attempt, as the `no record` lines are
      written.

  `<Service>` is the description's own service name (`Crossref`).
- for `differs`, one `differs` line per difference: `<field>: <item
  value> in the library, <candidate value> from <Service>`. A value is
  written as follows:
  - text fields in double quotes;
  - `type` as the `type` line names it (`journal article`);
  - `author` as every author, given name first, joined by `, `, with
    none left out, since a difference past the third author must be
    visible;
  - `issued` as the `issued` line writes it;
  - an absent value as `nothing`.
- for `differs`, a `decision` line:

| `acceptance` | `decision` line |
|---|---|
| `pending` | `pending; keeping it leaves the library's record as it is` |
| `accepted` | `the library's record was replaced` |
| `unwritten` | `the library's record could not be replaced (<message>)` |
| `declined` | `the library's record was kept` |
| `another-library` | `only a rename in the library at <root> can take it` |
| `not-recording` | `this run records nothing, so the library's record stays as it is` |
| `already-asked` | `answered at an earlier file of the same item` |

`not-asked` never reaches a description: a run that asks puts the
question instead.

**The menu labels:** `Keep the library's record`, `Replace it with the
fresh record`.

## D12. The interface the tests are written against

`borax-sources`, `crates/borax-sources/src/cache.rs`:

```rust
impl<S, C> Cached<S, C> {
    /// Wrap `source` so every answer is asked of it and stored in
    /// `cache`, and none is served from it.
    pub fn refreshing(source: S, cache: C) -> Cached<S, C>;
}
// Cached::new and the Source impl keep their signatures.
```

`borax`, a new `crates/borax/src/refresh.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field { Type, Title, Author, Issued, ContainerTitle, Volume, Issue, Page,
                 Publisher, Doi, Pmid, Isbn, Arxiv }
impl Field {
    /// D7's order.
    pub const ALL: [Field; 13];
    /// The record key: `type` … `ISBN`, and `arxiv`.
    pub fn as_str(self) -> &'static str;
    /// The field's value as `record` serializes it; `None` where absent.
    pub fn value(self, record: &Record) -> Option<serde_json::Value>;
}
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub field: Field,
    pub item: Option<serde_json::Value>,
    pub candidate: Option<serde_json::Value>,
}
/// Pure: every compared field whose value differs, in `Field::ALL` order.
pub fn differences(item: &Record, candidate: &Record) -> Vec<Difference>;
/// Pure: D9's replacement record.
pub fn replaced(item: &Record, candidate: &Record) -> Record;

#[derive(Debug, Clone, PartialEq)]
pub enum Fresh {
    NoIdentifier,
    NoEligibleService { identifier: Identifier },
    Unresolved { identifier: Identifier, attempts: Vec<ServiceAttempt> },
    Current { identifier: Identifier, attempts: Vec<ServiceAttempt> },
    Differs { identifier: Identifier, attempts: Vec<ServiceAttempt>,
              candidate: Record, differences: Vec<Difference> },
}
#[derive(Debug, Clone, PartialEq)]
pub enum Decision { NotAttempted(Unattempted), Pending, Accepted,
                    Unwritten { message: String }, Declined }
/// Whose item a refresh was of (D3). `Own`: the run's library answered,
/// or a reference whose `library` is the run's library's identity and
/// whose root, normalised lexically, is the run's library root.
/// `Referenced`: every other reference answer, another identity or the
/// same identity at another root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose { Own, Referenced }
/// Pure: D3's classification of a reference answer for a run whose
/// library is `library` (`None` for a run in no library).
pub fn whose(reference: &LibraryReference, library: Option<&Stores>) -> Whose;
#[derive(Debug, Clone, PartialEq)]
pub struct Refresh { pub item: ItemId, pub whose: Whose, pub fresh: Fresh, pub decision: Decision }

/// Look `record`'s preferred identifier up over `sources`, in
/// `dispatch::resolve` order, and compare what is found with `record`.
/// Asks nothing for a record with no identifier.
pub fn refresh(record: &Record, sources: &[&dyn Source]) -> Fresh;
impl Refresh {
    /// The `refresh` key (D8). `decision` is reported only for `Differs`.
    pub fn step(&self) -> RefreshStep;
}
```

`borax::pipeline`:

```rust
pub struct ResolveConfig { pub extraction: ExtractionConfig, pub cache: bool, pub refresh: bool }
// FileRecord gains `pub refresh: Option<Refresh>`: Some exactly where D8
// says the event carries `refresh`. standing_in attaches it, with
// Decision::NotAttempted(NotAsked) for Whose::Own and
// NotAttempted(AnotherLibrary) for Whose::Referenced; a library answer
// is Whose::Own, and a reference answer is `refresh::whose` of it.
// resolved_event projects it. Every other signature is unchanged.
```

`borax::cli`:

```rust
// Command::Resolve, Command::Rename and Command::Bib gain
// `#[arg(long)] refresh: bool`. Command::resolve, rename and bib set it
// false.
impl Command {
    /// This command given `--refresh`; a command that does not take it,
    /// unchanged.
    pub fn refreshing(self) -> Command;
    /// Whether this command was given `--refresh`.
    pub fn refreshes(&self) -> bool;
}
// Settings is unchanged: `--refresh` sets no setting.
```

`borax::evidence`: `Unattempted` gains `NoIdentifier` (`no-identifier`),
`AnotherLibrary` (`another-library`), `NotRecording` (`not-recording`)
and `AlreadyAsked` (`already-asked`).

`borax::event`:

```rust
// Event::RunStarted gains
//   #[serde(default, skip_serializing_if = "std::ops::Not::not")] refresh: bool
// Event::Resolved gains, after `sections`,
//   #[serde(default, skip_serializing_if = "Option::is_none")] refresh: Option<Box<RefreshStep>>
// Event::LibraryReplacement { path: PathBuf, item: String, file: PathBuf,
//                             fields: Vec<String>, write: WriteStep }
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum RefreshStep {
    NotAttempted { reason: String },
    NoEligibleService { identifier: String },
    Unresolved { identifier: String, attempts: Vec<ServiceAnswer> },
    Current { identifier: String, attempts: Vec<ServiceAnswer> },
    Differs { identifier: String, attempts: Vec<ServiceAnswer>,
              differences: Vec<DifferenceStep>, candidate: Box<Record>,
              acceptance: RefreshAcceptance },
}
pub struct DifferenceStep {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub item: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub candidate: Option<serde_json::Value>,
}
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum RefreshAcceptance { Pending, Accepted, Unwritten { message: String }, Declined,
                             NotAttempted { reason: String } }
```

`borax::library`:

```rust
/// D9: replace the record of the item `expected` held in `file` with
/// `record`, through `write`. `Err` names why nothing was written:
/// unreadable, another item, changed since read, or annotated keys
/// (step 2), in that order of checking.
pub fn replace_record(file: &Path, expected: &Item, record: &Record,
                      write: &dyn Fn(&Path, &[u8]) -> io::Result<()>) -> Result<(), Unrecorded>;
impl Stores {
    /// The library root, normalised lexically, as `Stores::read` holds it.
    pub fn root(&self) -> &Path;
    /// The identity `library::identity` gave for the root when these
    /// stores were read, where it was `Present`; `None` otherwise.
    pub fn identity(&self) -> Option<&LibraryId>;
    /// The file the item of identity `id` was read from.
    pub fn item_file(&self, id: &ItemId) -> Option<&Path>;
    /// Hold `item` in place of the held item of its identity, keeping
    /// that item's file; nothing when none is held.
    pub fn learn_item(&mut self, item: Item);
}
```

`Stores::read` reads the identity file once, as `library::identity`
does, and writes nothing: no mint, no warning. It is a private field,
so no literal changes.

`borax::references`:

```rust
impl Libraries {
    /// Drop what this run read of `root`, so the next reference to it
    /// reads it afresh. Nothing for a root not read.
    pub fn forget(&self, root: &Path);
}
```

`borax::session`: `Answer` gains `KeepRecord` and `Replace`, labelled as
D11 says.

The private parts are the implementer's:

- how `rename_events` keeps the consulted copy and the per-run item
  memory;
- how `asked` reaches the writer and the sink;
- how `describe` lays out its new lines within D11.

## D13. Sequences traced

`L` is a library at root `R` with an identity, holding the artifact
record `A` for `R/Klykov2015.pdf` (bytes `H`), linked to item `I`, whose
DOI is `doi:10.1039/c5ay00042d`. `I`'s file is
`R/items/klykov2015.<I>.toml`. The filename template is
`[auth][year]`, which renders the first author's family name and the
year: `Klykov2015`, so `Klykov2015.pdf` is already named under any
title (the `templates` capability's bracket syntax).
Crossref holds the DOI with title `Determination of things`.

**(a) An item corrected by hand, then `rename --refresh --apply R`.**

1. `I`'s `title` is edited to `REVIEW CORRECTION: Determination of
   things`.
2. The run: `run-started` `refresh: true`, applying, not interactive.
   The library answers for `H`. The refresh looks the DOI up: Crossref,
   network, stored.
3. `resolved`:
   - `record` is `I`'s, the corrected title;
   - `record_retrieval` `library`;
   - `refresh` `differs` in `title`, `acceptance` `not-attempted`
     `not-asked`.
4. Then `already-named`. The admission appends nothing, and the
   reference is kept as 13b keeps it.
5. State: `I`'s file is byte-identical. The response cache holds
   Crossref's answer.
6. Human:
   `R/Klykov2015.pdf: resolved doi:10.1039/c5ay00042d to "REVIEW CORRECTION: Determination of things" (Klykov, 2015) via crossref, from the library; a fresh record from crossref differs in title; the library's record is unchanged`,
   then `R/Klykov2015.pdf: already named`.

**(b) Then interactively, declined: `borax rename --refresh R`**, from
a terminal with `skip-named` on.

1. The hold starts. The refresh `differs` and D3 offers it, so the hold
   ends and the refresh question is put: Keep, Replace, Quit. Its
   description:
   - `refresh` with the DOI;
   - `Crossref differs from the library's record`;
   - `differs  title: "REVIEW CORRECTION: Determination of things" in
     the library, "Determination of things" from Crossref`;
   - `decision  pending; …`.
2. Keep: `declined`. The file's situation is already-named under
   `skip-named`, so it is settled with no further question.
3. Events: `resolved` (`refresh.acceptance` `declined`), then
   `already-named`, both shown. The summary counts it already named and
   not among those not shown.
4. State: unchanged.

**(c) Then accepted.** The same run, answered Replace.

1. `replace_record` re-reads the file. Its record is the one compared,
   so it sets `[record].title` and nothing else: the provenance entry
   for `title` is `crossref` in both, so unchanged. It writes through
   the `StoreWriter`.
2. `library-replacement` `{fields: ["title"], write: written}` is
   emitted, with the human line `R/Klykov2015.pdf: replaced title in
   R/items/klykov2015.<I>.toml`.
3. The consulted stores and the account learn `I` as replaced.
4. The file goes on with the replaced record. `[auth][year]` still
   renders `Klykov2015`: already named, settled with no further
   question.
5. Events: `library-replacement`, `resolved` (with `record` the replaced
   one and `refresh.acceptance` `accepted`), `already-named`.
6. State: `I`'s file differs from (b)'s by the `title` line alone. Its
   name, `id`, every other key and every comment are as they were.
7. A later `rename --refresh R`: `current`, no question, passed over.

Branch: with template `[title:slug]`, an earlier run named the file
from the corrected record, `review-correction-determination-of-things.pdf`.
Before step 1 the file is already named under that record, and the
refresh question is put as in (b). After Replace, step 4 renders
`determination-of-things` from the replaced record, so the plan is a
move to `determination-of-things.pdf`, and the move question follows:
Rename, Supply, Skip, Quit, with Rename the default. Its description
carries `decision  the library's record was replaced` and `new name
determination-of-things.pdf`.

- **Rename:** `library-replacement`, `resolved` (`accepted`),
  `renamed`; the summary is `1 resolved, 1 renamed, 0 skipped`.
- **Skip:** `library-replacement`, `resolved` (`accepted`), `skipped`
  `declined`; the file keeps the corrected record's name, the item holds
  the replaced record, and the summary is `1 resolved, 0 renamed, 1
  skipped`, exit 2. The next `rename` proposes the move again.
- **Quit:** `library-replacement` only, already emitted at the answer.
  The file keeps its name, `I` stays replaced, and `run-finished` counts
  `unreached` 1, exit 2.

**(d) A refresh whose fresh lookup fails.**

- **Outage.** Crossref and OpenAlex both answer `unavailable`:
  - `refresh` `unresolved`, `identifier` the DOI, both attempts in
    order;
  - the item stands, nothing is written, no question;
  - interactive: the file is shown, not passed over, with no Retry
    offered and no question of its own unless it has a move;
  - human: `…; no fresh record: no source had a record for doi:10.1039/c5ay00042d (crossref: unavailable: timed out; openalex: unavailable: timed out)`.
- **Not found.** Both answer `not-found`: the same, with `not found` in
  the clause.
- **No eligible service** (`--sources arxiv`): `no-eligible-service`,
  no request sent.

**(e) A file answered through a reference to another library.** `F` is
a copy of `H` at `/home/u/Downloads/paper.pdf`, outside any library, and
the content index holds a reference `(L, R, A, I)` (13b).

1. `borax resolve --refresh F`:
   - `record_retrieval` `library-reference` at `R`;
   - `refresh` `differs` in `title`, `acceptance` `not-attempted`
     `another-library`;
   - human: `…, from the library at R; a fresh record from crossref
     differs in title; only a rename in the library at R can take it`.
2. Nothing is written in `L` or anywhere but the response cache.
3. In an interactive `rename` of a copy inside another library `L2`:
   - no refresh question is put, and the description carries the
     `refresh`, `differs` and `decision` lines;
   - the move question is put as ever;
   - an admission into `L2` mints its item from `I`'s record, the
     verdict's, not from the candidate.

**(f) A refresh with `--no-record`.**

- **Interactive** `rename --refresh --no-record R`:
  - `refresh` `differs`, `acceptance` `not-attempted` `not-recording`;
  - no refresh question, and the file is shown;
  - no admission, no item write, no mint.
- **Batch** `rename --refresh --no-record --apply R`: `not-asked`, which
  D3's order puts first.

**(g) A refresh in a preview.** `borax rename --refresh --batch R`:

- as (a), with `planned` or `already-named`;
- `acceptance` `not-attempted` `not-asked`;
- nothing is written to the library;
- the response cache is written, as a preview's lookups always write
  it.

**(h) Two artifacts of one item, interactive.** `I` also links `A2`,
`R/Klykov2015-si.pdf`.

1. At the first file, Replace writes `I`. The consulted stores learn
   it.
2. At `A2`, the library answers with the replaced record, and its
   refresh is `current`: no question, passed over if already named.
3. Had the first answer been Keep, `A2`'s refresh would still be
   `differs`, reported with `already-asked`, shown, and not asked.

**(i) The item edited while the question is on screen.** At (c)'s step
1, before answering, the operator edits `I`'s `volume` in an editor,
then answers Replace.

1. The re-read record is not the one compared. Nothing is written.
2. `library-replacement` `write` `failed`, message `<file> changed after
   this run read it`. `acceptance` `unwritten` with that message.
3. The file goes on with the record as the run read it. The next run
   reads the operator's edit.

**(j) A library with no identity.** As (c), in a library an earlier
release wrote:

- the `StoreWriter` mints `.borax/library.toml` before the item write;
- where the mint fails, nothing is written, and the event's message
  names the identity file;
- a refresh run mints exactly when it makes a store write, as any run
  does: a replacement, or an admission that writes an item or an
  artifact record. A refresh run whose only store writes would be none
  (`resolve`, `bib`, a preview, `--no-record`, an interactive run that
  keeps every item's record and whose admissions all leave their
  records exactly as they were) mints nothing;
- references are written only for a `Present` identity (13b), so in
  this library no reference can name it, and no reference answer in
  the run is a self-reference (D3).

**(k) `--refresh --no-cache` on an untracked file.**

- The index read is `bypassed` and the file is extracted. Its lookup
  goes to the network with no cache in front: `stored` not attempted,
  `cache-bypassed`.
- No `refresh` key, since no item answered.
- Its extracted evidence is written, as `--no-cache` always writes it.

**(l) A self-reference, and a copy of the library.** `L` at `R` holds
`A` (`R/Klykov2015.pdf`) and the content index holds the reference
`(L, R, A, I)`. `C` is an untracked byte-identical copy at
`R/inbox/Klykov2015.pdf`, so the library does not answer for it
(untracked) and the reference does.

1. Interactive `borax rename --refresh R`, reaching `A` first:
   - `A` is a library answer, `Whose::Own`. Its refresh differs and the
     question is put. Replace writes `I`.
   - The consulted stores and the account learn `I`, and `Libraries`
     forgets `R`.
2. At `C`:
   - `C`'s answer is the reference. Its `library` is `L`, which is
     `Stores::identity`, and its root is `R`, which is `Stores::root`:
     `Whose::Own`;
   - `Libraries` reads `R` afresh and finds `I` as replaced. The refresh
     is `current`, and nothing is asked, since `I` was already answered
     for.
3. Had `C` been reached first:
   - its question would have been put as `A`'s, its Replace writing the
     same item file through the same `StoreWriter`;
   - `A` would then be consulted against `I` as replaced, through the
     consulted stores.
4. A copy of the library: `rsync -a R/ R2/`, and the content index also
   holds `(L, R2, A, I)`.
   - A run in `R` reaching an untracked file answered by the `R2`
     reference: same identity, another root, so `Whose::Referenced`,
     `another-library` naming `R2`, nothing written, and `Libraries`
     does not forget `R2`.
   - A run in a library `L2` reaching any reference to `L`: another
     identity, so `another-library`.

## D14. Group boundaries and green points

Each type change lands with every consumer of that type, in one
red/green pair. Every group ends with the workspace compiling and the
whole suite green.

| Group | Lands | Why it is green at its end |
|---|---|---|
| 1 | `Cached::refreshing` | Additive |
| 2 | `--refresh` on three commands; `refreshing`, `refreshes`; `ResolveConfig::refresh`, read by nothing yet; `run-started.refresh`; `polite` and `resolving` taking the selector | `ResolveConfig`'s one test literal and `RunStarted`'s two test literals change in the pair (D15). No resolution reads the field yet |
| 3 | `borax::refresh`, `refresh::whose` included; `Stores::root` and `Stores::identity`, with `Stores::read` reading the identity; the four `Unattempted` values; `RefreshStep`, `DifferenceStep`, `RefreshAcceptance` | Additive: nothing calls them, and `Event` is not touched. `Stores` has private fields, so reading one more file changes no literal |
| 4 | `FileRecord::refresh`, `Event::Resolved::refresh`, `standing_in` attaching it, `resolved_event`, the human clause | Every `FileRecord` and `Event::Resolved` literal and full pattern changes in the pair (D15). The behaviour is behind `ResolveConfig::refresh`, false in every existing test |
| 5 | `replace_record` with its annotation check, `Stores::item_file`, `Stores::learn_item`, `Libraries::forget`, `Event::LibraryReplacement` and its line | Additive: no driver calls them yet. `human_line`'s match gains the arm |
| 6 | `Answer::KeepRecord`, `Answer::Replace`; the driver's question, write, memory and passing over; the description lines | `Choice`'s `Display` gains the arms. No existing test answers a refresh question, because none passes `--refresh` |

There is no interim state worth stating. After group 4, a refresh is
reported everywhere and nothing asks. After group 5, nothing asks yet.
After group 6, everything is in place.

## D15. Existing tests: what changes and how

The rules of the earlier changes apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. None is.

**How the sweep was made.** A script found every `ResolveConfig {`,
`FileRecord {`, `Event::Resolved {` and `RunStarted {` in
`crates/*/tests/`, followed it to its closing brace, and kept those
with no `..` at their top level: a literal, or a pattern naming every
field. Function signatures returning `FileRecord` were dropped by
reading them. Then:

- every test body that runs `resolve`, `rename` or `bib` through
  `events_for`, `dispatch` or the binary was searched for `--refresh`,
  `refresh` and `Command::refreshing`. None exists, so no behaviour any
  test reaches changes;
- every exact-key-set test (`…_has_exactly_the_documented_field_set` in
  `crates/borax/tests/event.rs`) was read. `resolved()`'s and
  `run_started()`'s key sets are unchanged, because both new keys are
  left out when absent;
- every `impl Asker` in the tests (`dispatch.rs` 3004 and 4081,
  `runlog.rs` 223, `end_to_end.rs` 1056) was read. None matches `Answer`
  exhaustively.

Line numbers drift as earlier groups edit a file. A site the test stage
cannot find, or a break it finds that is not here, is reported back.

**Shape-only (a):**

| Group | File | Sites | Edit |
|---|---|---|---|
| 2 | `crates/borax/tests/pipeline.rs` | the `config` helper's `ResolveConfig` literal (near 423) | `refresh: false` |
| 2 | `crates/borax/tests/event.rs` | `run_started` (near 320) and `run_started_with_a_table` (near 330) | `refresh: false` |
| 4 | `crates/borax/tests/bib.rs` | `FileRecord` literal near 175 | `refresh: None` |
| 4 | `crates/borax/tests/event.rs` | `FileRecord` literals near 1107, 1132, 1171, 3276, 3330 | `refresh: None` |
| 4 | `crates/borax/tests/pipeline.rs` | `FileRecord` literals near 1004, 1050, 3091, 3121, 6724, 6755, 6790, 6897, 6944 | `refresh: None` |
| 4 | `crates/borax/tests/renaming.rs` | `FileRecord` literal near 236 | `refresh: None` |
| 4 | `crates/borax/tests/describe.rs` | `Event::Resolved` near 235, 1474, 1490, 1612, 1627, 1700, 1717, 1851, 1865, 1977, 2132, 2147, 2469, 2624, 2660, 2704, 2739 | a literal gains `refresh: None`; a pattern gains `..` |
| 4 | `crates/borax/tests/dispatch.rs` | `Event::Resolved` near 555 | the same |
| 4 | `crates/borax/tests/event.rs` | `Event::Resolved` near 346, 1996, 2088, 2523, 2548, 2577, 2591, 2610, 3125, 3131, 3247, 3385, 3414, 3445, 3654, 3729, 3784, 3802, 3817, 3835, 3846, 3857, 3874, 3893, 3915, 5303, 5689 | the same |
| 4 | `crates/borax/tests/per_file.rs` | `Event::Resolved` near 268 | the same |
| 4 | `crates/borax/tests/pipeline.rs` | `Event::Resolved` near 1019 (a pattern) | `..` |
| 4 | `crates/borax/tests/renaming.rs` | `Event::Resolved` near 1266, 1272, 1319 | the same |

`pipeline.rs` near 9550 is `FileRecord { …, ..accepted }` and needs no
edit.

**Deliberate rewrites (b):** none.

**Re-read and unchanged:**

- `Cached::new` and every `impl Source` in the tests: the trait is
  unchanged.
- The CLI tests comparing a parsed command with `Command::rename(…)` and
  the like: both sides have `refresh` false.
- `resolve_accepts_every_setting_it_consumes` and its siblings in
  `crates/borax/tests/cli.rs`: `Settings` is unchanged.
- `all_events` in `event.rs`: it lists events and matches none
  exhaustively. Group 5 may add `library-replacement` to it.

## D16. How the specifications change

ADDED:

- `resolution`:
  - "A refresh looks a file's library item up again and never replaces
    it": D1, D2, D4, D6, D7;
  - "A refresh is reported beside the verdict": D8.
- `library`: "An item's record is replaced only when an operator
  accepts a refresh candidate": D9.
- `rename`: "An interactive run asks whether a differing refresh
  candidate replaces the library's record": D10, D11.

MODIFIED, each restated whole with every living paragraph and scenario
name kept, so no `<!-- drops: -->` marker is used:

- `resolution`:
  - "Responses are cached locally";
  - "A file its library tracks resolves from its library item";
  - "A resolution keeps every service it asked, in order";
  - "A resolution says why a step was not taken";
  - "Resolution events carry their evidence in sections";
  - "Human resolution lines name the work, where it came from, and why
    a file was skipped".
- `library`: "A library's first store write mints its identity".
- `rename`:
  - "Quitting an interactive run leaves the rest untouched";
  - "An interactive run passes over already-named files";
  - "An interactive question shows what the answer rests on";
  - "An interactive run asks about files it could not settle";
  - "A candidate the operator abandoned leaves nothing behind";
  - "An interactive question says whether the library answered".
- `cli`: "A subcommand accepts only the settings it consumes".

The proposal lists what was checked and left unchanged.

## Not changed, deliberately

- What a library or a reference answers for a file, and the order the
  answers are asked in.
- `--no-cache`'s meaning, and that it does not bypass the library.
- The content index: no refresh reads or writes it, and an
  evidence-answered file's own resolution writes what it always wrote.
- Admission, the acceptance gate for changed bytes, references and
  `reconcile`.
- `status`, `validate`, `adopt`, `cache` and `config` beyond refusing
  the flag.

## Risks / Trade-offs

- **A refresh of a library costs one request per tracked file**, paced
  per service. `resolve --refresh` over two hundred tracked files is two
  hundred requests to Crossref. That is the request the operator made.
- **A refresh of a reference answer reads the other library** as 13b
  already does, and adds a request. Its candidate can only be reported.
- **`already-asked` takes the first file's answer for every file of the
  item**, even where a second file's candidate differs from the first's
  because a service changed between the two lookups. The window is one
  run.
- **A replacement takes every differing field.** An operator who wants
  some fields and not others must edit the item by hand (Deferred).
- **The stale check compares records, not bytes.** An edit that changes
  only a comment or an unknown key while the question is up is not a
  change of record. The write keeps it where it lies outside the
  replaced values, since it edits the document as re-read, and refuses
  the whole replacement where it lies inside one (D9, step 2).
- **A window remains between the re-read and the rename** (D9). An edit
  saved in it, by an editor or by another run's replacement, is
  overwritten, and nothing reports the edit lost. An edit completed
  before the re-read is protected and refuses the write. Outside that
  window nothing is lost silently: every write is a
  `library-replacement` event in the stream and the run log, and the
  verdict's `refresh.differences` keep each replaced field's old value.
  borax takes no lock, as change 13b took none for references, because
  no filesystem offers a portable compare-and-replace and the library's
  other writers, a text editor among them, honour none.
- **Annotated fields refuse the whole replacement.** An item whose
  authors carry `orcid` keys, or whose date carries a comment, cannot
  be replaced in those fields by a refresh at all. The operator edits
  the item by hand, or removes the notes and refreshes again. That is
  the price of never discarding what was not shown.
- **Unpaced refreshes across runs.** Two concurrent `--refresh` runs
  pace independently, as any two runs do today.

## Illustrative schema-5 lines

A batch refresh of a hand-corrected item:

```json
{"schema":5,"event":"run-started","command":"rename","version":"0.10.0","applying":true,"interactive":false,"tables":[],"refresh":true}
{"schema":5,"event":"resolved","path":"/home/u/papers/Klykov2015.pdf","identifier":"doi:10.1039/c5ay00042d","record":{"type":"article-journal","title":"REVIEW CORRECTION: Determination of things","author":[{"family":"Klykov","given":"S."}],"issued":{"date-parts":[[2015]]},"DOI":"10.1039/c5ay00042d","borax":{"provenance":{"DOI":"crossref","author":"crossref","issued":"crossref","title":"crossref","type":"crossref"}}},"library":{"status":"consulted","answer":{"kind":"tracked","artifact":"0190c3a2-7f21-7abc-8def-0123456789ab","item":"0190c3a2-7f21-7abc-8def-0123456789ac"}},"content_index":{"read":{"status":"not-attempted","reason":"library-answered"},"write":{"status":"not-attempted","reason":"library-answered"}},"extraction":{"result":{"status":"not-attempted","reason":"library-answered"},"titles":{"status":"not-attempted","reason":"library-answered"}},"identifier_input":{"status":"not-attempted","reason":"not-asked"},"lookup":{"status":"not-attempted","reason":"library-answered"},"record_retrieval":{"kind":"library","artifact":"0190c3a2-7f21-7abc-8def-0123456789ab","item":"0190c3a2-7f21-7abc-8def-0123456789ac"},"match_check":{"status":"not-attempted","reason":"library-answered"},"acceptance":{"status":"automatic"},"refresh":{"status":"differs","identifier":"doi:10.1039/c5ay00042d","attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}],"differences":[{"field":"title","item":"REVIEW CORRECTION: Determination of things","candidate":"Determination of things"}],"candidate":{"type":"article-journal","title":"Determination of things","…":"…"},"acceptance":{"status":"not-attempted","reason":"not-asked"}}}
```

The other statuses, as the `refresh` value alone:

```json
{"status":"current","identifier":"doi:10.1039/c5ay00042d","attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}]}
{"status":"unresolved","identifier":"doi:10.1039/c5ay00042d","attempts":[{"service":"crossref","outcome":{"status":"unavailable","message":"timed out"}},{"service":"openalex","outcome":{"status":"not-found"}}]}
{"status":"no-eligible-service","identifier":"doi:10.1039/c5ay00042d"}
{"status":"not-attempted","reason":"no-identifier"}
```

A difference with one side absent, and the acceptances:

```json
{"field":"volume","candidate":"7"}
{"status":"accepted"}
{"status":"unwritten","message":"/home/u/papers/items/klykov2015.0190c3a2-7f21-7abc-8def-0123456789ac.toml changed after this run read it"}
{"status":"unwritten","message":"/home/u/papers/items/klykov2015.0190c3a2-7f21-7abc-8def-0123456789ac.toml holds notes in author, issued that replacing them would discard"}
{"status":"declined"}
{"status":"not-attempted","reason":"another-library"}
```

The replacement:

```json
{"schema":5,"event":"library-replacement","path":"/home/u/papers/Klykov2015.pdf","item":"0190c3a2-7f21-7abc-8def-0123456789ac","file":"/home/u/papers/items/klykov2015.0190c3a2-7f21-7abc-8def-0123456789ac.toml","fields":["title"],"write":{"status":"written"}}
```

Human lines:

```text
/home/u/papers/Klykov2015.pdf: resolved doi:10.1039/c5ay00042d to "REVIEW CORRECTION: Determination of things" (Klykov, 2015) via crossref, from the library; a fresh record from crossref differs in title; the library's record is unchanged
/home/u/papers/Klykov2015.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Klykov, 2015) via crossref, from the library; a fresh record from crossref does not differ
/home/u/papers/Klykov2015.pdf: resolved doi:10.1039/c5ay00042d to "REVIEW CORRECTION: Determination of things" (Klykov, 2015) via crossref, from the library; no fresh record: no source had a record for doi:10.1039/c5ay00042d (crossref: unavailable: timed out; openalex: not found)
/home/u/Downloads/paper.pdf: resolved doi:10.1039/c5ay00042d to "REVIEW CORRECTION: Determination of things" (Klykov, 2015) via crossref, from the library at /home/u/papers; a fresh record from crossref differs in title; only a rename in the library at /home/u/papers can take it
/home/u/papers/Klykov2015.pdf: replaced title in /home/u/papers/items/klykov2015.0190c3a2-7f21-7abc-8def-0123456789ac.toml
/home/u/papers/Klykov2015.pdf: could not replace title in /home/u/papers/items/klykov2015.0190c3a2-7f21-7abc-8def-0123456789ac.toml (/home/u/papers/items/klykov2015.0190c3a2-7f21-7abc-8def-0123456789ac.toml changed after this run read it)
```

The refresh question's description:

```text
── 1 of 1 ──────────────────────────────────────────────────────────────────────
file        Klykov2015.pdf
identifier  doi:10.1039/c5ay00042d
record      Crossref, from the library
type        journal article
title       REVIEW CORRECTION: Determination of things
authors     S. Klykov
issued      2015
file says   not read; the library answered
refresh     doi:10.1039/c5ay00042d
            Crossref differs from the library's record
differs     title: "REVIEW CORRECTION: Determination of things" in the
            library, "Determination of things" from Crossref
decision    pending; keeping it leaves the library's record as it is
```

The item file before and after the replacement. Only the `title` line
differs:

```toml
id = "0190c3a2-7f21-7abc-8def-0123456789ac"

[record]
type = "article-journal"
# corrected by hand, 2026-10-12
title = "REVIEW CORRECTION: Determination of things"
DOI = "10.1039/c5ay00042d"
shelf = "B3"
```

```toml
id = "0190c3a2-7f21-7abc-8def-0123456789ac"

[record]
type = "article-journal"
# corrected by hand, 2026-10-12
title = "Determination of things"
DOI = "10.1039/c5ay00042d"
shelf = "B3"
```

`shelf` is a key the record model does not know, and the comment is a
person's. Both survive.

The comment sits on the line above the plain `title =` key, so it is
the key's and survives (D9, step 2). `shelf` is a key of `[record]`
itself, not inside a replaced value, so it survives too.

An item a replacement refuses. Its candidate differs in `author` and
`issued`:

```toml
[[record.author]]
family = "Klykov"
given = "S."
orcid = "0000-0002-1825-0097"

[record.issued]
date-parts = [[2015]]  # online first; print 2016
```

`orcid` is a key `Name` does not read, and the comment trails a value
inside `issued`. Nothing is written, and the event's message is `<file>
holds notes in author, issued that replacing them would discard`.
Without `orcid` and the comment, `author` and `issued` are replaced as
any other field.
