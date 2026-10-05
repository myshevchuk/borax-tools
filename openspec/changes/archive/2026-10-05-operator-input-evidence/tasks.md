# Tasks: operator-input-evidence

Work on branch `change/operator-input-evidence`, cut from `main` at
`a62839b`. This is change 10 of the roadmap's Phase 3. It builds on
change 8's evidence (`crates/borax/src/evidence.rs`) and change 9's
sections (`crates/borax/src/event.rs`).

Groups 1 to 8 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D9 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.

These parts of the design fix the contract:

- the JSON shape: design D8 and the illustrative lines at the end of
  design.md;
- the Rust interface: D9;
- the parser's reasons: D3;
- the acceptance values: D4;
- the retry rounds: D12;
- the human strings: D6 (lines) and D7 (description).

**Existing tests change.** Design D11 lists each file and the kind of
edit. The rules are change 9's:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. None is listed.

Task 9.1 audits every edit to an existing test against these rules.

**Fixtures.** Reuse these:

- in `crates/borax/tests/pipeline.rs`: `FakeDocuments`, `FakePdf`,
  `fake_source`, `ContentIndex` over `MemoryCache`, and
  `WriteFailingCache` where a write must fail;
- in `crates/borax/tests/dispatch.rs`: the interactive harness, with
  `ScriptedAsker::with_texts` for what the operator types and
  `KeyedSource` for services that answer per identifier.

A `ScriptedAsker` text of `None` is an abandoned prompt. A `Some` text
is returned exactly as given.

**Design tests.** Each case below is a test, in the group named:

| Case | Group |
|---|---|
| the round-two session: refused text, truncated DOI not found, candidate, Skip | 5, 6 |
| a truncated DOI parses and is looked up as typed | 1, 5 |
| operator accepts a supplied candidate (`accepted`) | 4, 5 |
| operator accepts a supplied candidate over its conflict (`overridden`) | 4, 5 |
| operator overrides the file's own conflict (`not-supplied`) | 5 |
| a correction keeps the file's own failed lookup (`displaced`) | 3, 5 |
| a candidate is pending even when its titles agree | 4, 5, 7 |
| a rejected candidate is never written to the content index | 5 |
| batch and passed-over files report `not-asked` | 4, 5, 8 |
| outage, Retry, found: both rounds kept | 4, 5 |
| outage, Retry, all not-found: both rounds kept, no further Retry | 4, 5, 6 |
| the human line and the description show the current round only | 6, 7 |
| restored: candidate A found, supply B unheld, A still on offer and pending; Rename accepts A | 5 |
| candidate A found, supply B found, A rejected and B pending | 5, 7 |
| a collision notice keeps a candidate pending; then Rename, Skip, Supply, Quit | 5 |
| Retry finds a conflict, Override gives `overridden` | 4, 5 |

## 1. The parser's reasons

- [x] 1.1 Red, in `crates/borax-core/tests/identifier.rs`:
      - Every row of design D3's example table: each text gives exactly
        the `SuppliedError` or `Identifier` the table names.
        `SuppliedError::reason()` and `expected()` give the D8 strings.
        `IdentifierKind::as_str()` gives `doi`, `arxiv`, `pmid` and
        `isbn`.
      - **Truncation pinned**: `supplied("10.1039/c9cc02492")` is
        `Ok(Identifier::Doi(_))` whose string is `10.1039/c9cc02492`,
        unchanged and not extended.
      - **The accepted set is unchanged**: every input the existing
        `supplied_parses_…` tests use gives the same `Identifier` as
        before.
      - Prefixes are matched without regard to case: `DOI:abc` and
        `ArXiv:12345` are `Invalid` with `Doi` and `Arxiv`.
      - Kind (b): the four `supplied(...).is_none()` tests assert
        `Err(SuppliedError::Unrecognised)`.
- [x] 1.2 Green, in `crates/borax-core/src/identifier.rs`:
      - add `IdentifierKind` and `SuppliedError`, with `Display` and
        `Error`;
      - make `supplied` return `Result<Identifier, SuppliedError>`,
        naming the form by D3's first-match order;
      - leave `IdentifierError` and the four `parse` functions unchanged.

      Update `supplied`'s docstring for the new contract. Update the one
      caller, `run::supplied_identifier`, only as far as it has to
      compile. Group 5 changes what it does.

## 2. The event vocabulary

- [x] 2.1 Red, in `crates/borax/tests/event.rs`:
      - **Key set and order**: a `resolved` event and every resolution
        `skipped` event carry `identifier_input`, after `extraction`
        and before `lookup`.
      - **The not-attempted section**: it serializes as exactly
        `{"status":"not-attempted","reason":R}`.
      - **The `supplied` section**: it serializes with `status`,
        `submissions`, `used` and `displaced` in that order. `used`
        and `displaced` are `null` when `None`.
      - **A submission with an outcome**: it serializes `submission`,
        `raw`, `syntax`, `lookup`, `record_retrieval`, `match_check`,
        `acceptance` and, when present, `record`. The used submission
        (`outcome: None`) serializes `submission`, `raw` and `syntax`
        only.
      - **`syntax`**: it is `parsed` with `identifier`, or `rejected`
        with `reason`. `expected` is present only when `Some`.
      - **`acceptance`**: `Acceptance::Pending` and
        `Acceptance::Accepted` serialize as `{"status":"pending"}` and
        `{"status":"accepted"}`. `SubmissionAcceptance::Rejected`
        serializes as `{"status":"rejected"}`.
      - **The illustrative lines**: the round-two final skip, the
        pending event and the `.author` correction are built from the
        D9 types and compared with the JSON text written in the test.
      - **The round trip**: it covers a `resolved` event whose
        `identifier_input` is `supplied`, with a used entry, an entry
        carrying a record and a conflict `match_check` (an `f64`
        similarity), and `displaced` holding a record. It also covers a
        `skipped` event with `used: null`. Both go through `json_line`
        and back to an equal `Event`.
      - **`lookup.earlier`** (D12):
        - an `attempted` lookup with one earlier `attempted` round and
          one earlier `no-eligible-service` round serializes `earlier`
          after `attempts` as
          `[{"status":"attempted","attempts":[…]},{"status":"no-eligible-service"}]`;
        - a `no-eligible-service` lookup with a round serializes
          `earlier` after `origin`;
        - empty `earlier` is omitted, and a line without it
          deserializes to `earlier: vec![]`;
        - the round trip covers both.
      - Kind (b): the key-set and key-order tests near lines 619–720,
        and every exact schema-4 JSON text, per D11. Kind (a): the
        `LookupStep` literals take `earlier: vec![]`.
- [x] 2.2 Green, in `crates/borax/src/event.rs`:
      - add `Sections::identifier_input`, the D9 types, and the two
        `Acceptance` variants;
      - add `earlier` to `LookupStep::Attempted` and `NoEligibleService`,
        and add `LookupRound`;
      - write docstrings that state each type's contract (D2, D4, D8).

      The crate must keep compiling. Until group 3, callers may build
      `identifier_input` as not attempted for `not-asked`.

## 3. Evidence and its projection

- [x] 3.1 Red, in `crates/borax/tests/pipeline.rs` (the `sections`
      block). Build `Evidence` values directly and assert
      `Evidence::sections`:
      - **The not-attempted input**:
        `IdentifierInput::NotAttempted(r)` for each of `NotAsked`,
        `NotSupplied` and `ContentDuplicate` gives `not-attempted` with
        `r.as_str()`.
      - **Rejected syntax**: a submission with `syntax:
        Err(SuppliedError::Invalid { kind: Pmid })` gives `rejected`,
        `invalid`, `expected` `pmid`. `Unrecognised` gives no
        `expected`.
      - **Parsed syntax**: an `Ok(identifier)` gives `parsed` with the
        identifier's `Display`.
      - **Outcomes**: one test per outcome — `unparsed`, `no-record`
        (attempts kept), `rejected` with a record (`record_retrieval`
        `network` or `service-cache` naming the found attempt's
        service), and `no-move`.
      - **The used entry**: a `Displacement` gives `used` as its
        number and `displaced` with the file's own `lookup`,
        `record_retrieval` (from the `RecordRetrieval` it holds, any
        kind) and `match_check`, plus `record` when `Some`.
      - **The reason vocabulary**: `Unattempted::{NotAsked,
        NotSupplied, Unparsed, NoMove}.as_str()` are `not-asked`,
        `not-supplied`, `unparsed` and `no-move`. Every reason string
        in any projected section equals some `Unattempted::as_str()`.
      - **Rounds**: a `LookupEvidence::Attempted` with `earlier` holding
        a round with attempts and a round with none projects to D12's
        two round shapes, in order. The current round projects as
        before.
      - Kind (a): `Evidence` and `LookupEvidence::Attempted` literals
        take `identifier_input` and `earlier`.
- [x] 3.2 Green, in `crates/borax/src/evidence.rs`:
      - add `Evidence::identifier_input` and the D9 engine types;
      - add `earlier` to `LookupEvidence::Attempted`, and add
        `LookupRound`;
      - add the four `Unattempted` variants;
      - extend `Evidence::sections`, with no wildcard arms.

      `resolve_supplied` and `unheld_evidence` copy
      `prior.identifier_input`.

## 4. Defaults and acceptance in the engine

- [x] 4.1 Red, in `crates/borax/tests/pipeline.rs`:
      - **Defaults**: through `standing`, every verdict's evidence has
        `identifier_input` `NotAttempted(NotAsked)`. Check a network
        resolution, a content-index hit, a library answer, each
        extraction failure, unresolvable, a conflict skip and a work
        duplicate. A content duplicate has
        `NotAttempted(ContentDuplicate)`, and its projected sections are
        all not attempted for `content-duplicate`. `resolve_file` gives
        the same.
      - **`accept`**: it sets `accepted` on every record. It sets
        `overridden` exactly on a conflict, and leaves the evidence
        unchanged otherwise. The existing
        `accept_over_a_resolved_record_leaves_its_evidence_unchanged`
        keeps passing.
      - **`FileRecord::acceptance`**: every row of D5's table, using
        records from `resolve_supplied` with `Origin::Operator` and
        with `Origin::Extracted(tier)`, before and after `accept`. A
        supplied record whose titles agree is `Pending` before `accept`
        and `Accepted` after. One whose titles conflict is `Pending`
        before and `Overridden` after. A retried record whose titles
        agree is `Automatic` before and after `accept`. A retried
        record whose titles conflict is `Automatic` before `accept` and
        `Overridden` after, since `accept` sets `overridden`.
      - **`resolved_event` passes the acceptance through**: for a
        pending supplied record it carries `acceptance` `pending`.
      - **Rounds are carried forward** (D12):
        - `unheld_evidence(prior, id, Origin::Extracted(tier), unheld)`,
          over a `prior` whose lookup of `id` met an outage, gives a
          lookup whose `attempts` are `unheld`'s and whose `earlier` is
          `prior`'s round;
        - a second call over that result gives two rounds, oldest
          first;
        - `resolve_supplied` with the same `prior` and the extraction
          origin gives a found lookup with the same `earlier`;
        - with `Origin::Operator` both give an empty `earlier`;
        - a `prior` lookup of a different identifier, or one not
          attempted, gives an empty `earlier`;
        - `standing` and `resolve_file` never give an `earlier`.
      - **`is_conclusive` reads the current round**: a lookup whose
        current attempts are all `NotFound` and whose `earlier` round is
        `Unavailable` is conclusive. One whose current round is
        `Unavailable` and whose earlier round is all `NotFound` is not.
      - Kind (a): `FileRecord` literals take `accepted: false`.
- [x] 4.2 Green, in `crates/borax/src/pipeline.rs`:
      - set the defaults (D5);
      - carry the rounds forward in `resolve_supplied` and
        `unheld_evidence`, through `found_lookup` and `unheld_lookup`
        (D12);
      - add `FileRecord::accepted`;
      - extend `accept` and `FileRecord::acceptance`.

      Update `FileRecord`'s and `acceptance`'s docstrings.

## 5. The interactive driver

- [x] 5.1 Red, in `crates/borax/tests/dispatch.rs`, on the JSON stream
      of interactive runs through the harness:
      - **The round-two session.** A file with text and no identifier.
        Crossref and OpenAlex both say they do not hold
        `doi:10.1039/c9cc02492`. Crossref holds
        `doi:10.1039/c9cc02492a`. The answers are `[Supply, Supply,
        Skip]` and the texts are `not-an-identifier`,
        `10.1039/c9cc02492` and `10.1039/c9cc02492a`. The file's
        `skipped` event:
        - has reason `text-without-identifier` and the file's own
          sections;
        - has `identifier_input` equal to the first illustrative line's,
          submission for submission;
        - has `used` and `displaced` `null`.

        The sources record the identifiers they are asked about.
        `KeyedSource` keeps no log at `a62839b`, so add one in the test
        file or wrap it. The log shows exactly one query for each DOI as
        typed, and no other DOI.
      - **Accepting a supplied candidate.** Its titles agree, and the
        answers are `[Supply, Rename]`. The `resolved` event has
        `acceptance` `accepted`, `used` 1, a bare submission 1, and
        `lookup.origin` `operator`. It is followed by `renamed` and then
        `content-index-write`.
      - **Over its conflict.** Its titles conflict, and the operator
        answers `Override`. The event has `acceptance` `overridden`,
        `match_check` `conflict` and `used` 1.
      - **The file's own conflict overridden** with nothing supplied.
        The event has `acceptance` `overridden` and `identifier_input`
        not attempted for `not-supplied`.
      - **A correction keeps the file's own failed lookup.** The file's
        text-layer DOI is held by no service, and the operator supplies
        the published DOI and renames. `displaced.lookup` is the file's
        own, with origin `extracted` and its `not-found` attempts.
        `displaced.record_retrieval` is `null`.
      - **A reference's DOI caught.** The file's own record resolved,
        and the operator supplies another DOI and renames.
        `displaced.record` is the file's own record, and
        `displaced.record_retrieval` names its service.
      - **A candidate replaced by another.** The answers are `[Supply,
        Supply, Rename]`. Submission 1 is `rejected` with its record,
        and submission 2 is `used`.
      - **Restored: an unheld supply keeps the candidate** (D4, D5).
        This test fails against today's driver, which reverts to the
        file's own record. The setup: a file with no identifier; DOI A
        resolves to a record that renders a move; DOI B is held by no
        service. The answers are `[Supply, Supply, Rename]` and the
        texts are A then B.
        - The third question offers the move to A's record: its
          `target` is A's rendered name, and its choices are Rename,
          Supply, Skip, Quit.
        - Its description carries the `candidate` line, read from
          `ScriptedAsker::questions_asked`, so A is still pending.
        - After Rename, the `resolved` event reports A's record with
          `acceptance` `accepted` and `used` 1. Submission 2 has
          `acceptance` not attempted for `no-record`.
        - `renamed` and `content-index-write` follow.
      - **A found, B found** (D4). The answers are `[Supply, Supply,
        Skip]`, and both DOIs resolve to records that render moves.
        - The third question describes B's record as pending, and
          carries a `rejected` line naming A.
        - The final `skipped` event has submission 1 `rejected` with
          A's record, and submission 2 `rejected` with B's.
      - **No-move B keeps A** (D4's "decided here"). A resolves to a
        move. B resolves to a record whose name is taken. The answers
        are `[Supply, Supply, Rename]`.
        - The `name taken` report is shown, and the third question
          still offers A's move.
        - The `resolved` event has `used` 1, and submission 2 not
          attempted for `no-move`.
      - **A collision notice keeps the candidate pending** (D4). A
        library holds a file of the work that supplied DOI A resolves
        to. Each case starts with the answers `[Supply, Rename]`. The
        second question is the notice: it carries the `same work` block
        and a description with the `candidate` line. Then:
        - `[…, Rename]`: the file is moved and filed against the same
          item. The `resolved` event has `acceptance` `accepted` and
          `used` 1.
        - `[…, Skip]`: the file is reported with its own verdict, and
          submission 1 is `rejected`.
        - `[…, Supply, Rename]` with a DOI B whose record renders a
          move: submission 1 is `rejected`, and submission 2 is `used`
          and `accepted`.
        - `[…, Quit]`: no event names the file, and the content index
          holds nothing new.
      - **Retry finds a conflict, Override** (D4). Crossref fails
        `Unavailable` once, then answers with a record whose title
        conflicts with the file's (`FlakySource`). The answers are
        `[Retry, Override]`. The `resolved` event has `match_check`
        `conflict`, `acceptance` `overridden`, `lookup.origin`
        `extracted`, one `earlier` round, and `identifier_input` not
        attempted for `not-supplied`.
      - **A refused text after a candidate.** The answers are `[Supply,
        Supply, Rename]`, with texts `DOI-A`, `garbage`, then `None`.
        Submission 1 is `used`, and submission 2 follows it with
        `unparsed`.
      - **No move.** A supplied record renders the file's current name.
        Its submission is not attempted for `no-move`, and the file's
        own situation is asked again.
      - **Skip after a candidate, own record resolved.** The answers are
        `[Supply, Skip]`. The file's own `resolved` event carries the
        candidate as `rejected`, and a `skipped` event with reason
        `declined` follows it, carrying no sections.
      - **Not supplied.** A question was put and nothing was typed: the
        answers are `[Supply, Skip]` with a `None` text. The event
        carries `not-supplied`.
      - **Not asked.** A file passed over under `rename.skip-named`, and
        a batch `rename --apply`, both carry `not-asked`. A content
        duplicate carries `content-duplicate`.
      - **A rejected candidate is never written to the content
        index.** After the round-two session and after "A candidate
        replaced by another", the index holds nothing for the file in
        the first case, and only the accepted record in the second.
      - **Quitting at a candidate.** The answers are `[Supply, Quit]`.
        No event names the file, and the index holds nothing new.
      - **The described event is pending.** Read the question's
        description from `ScriptedAsker::questions_asked`. For a candidate
        whose titles agree, it carries the `candidate` line. This
        checks that the description is rendered from an event whose
        `acceptance` is `pending`; group 7 checks the wording.
      - **Outage, Retry, found.** Crossref and OpenAlex fail
        `Unavailable` once, then OpenAlex answers. The answers are
        `[Retry, Rename]`. The `resolved` event's `lookup` has `origin`
        `extracted`, `attempts` ending with OpenAlex `found`, and
        `earlier` one `attempted` round with both services
        `unavailable` and their messages. Its `acceptance` is
        `automatic`. Use `FlakySource` (near line 15493), which fails
        `Unavailable` a set number of times and then answers.
      - **Outage, Retry, all not-found.** Crossref fails `Unavailable`
        once, then every service says `NotFound` (`SequencedSource`, near
        line 15534). This is the setup of the existing
        `an_outage_then_not_found_on_retry_stops_offering_a_retry`. That
        test asserts the current attempts through a `..` pattern and
        keeps passing unchanged; the new assertions go in a new test
        beside it. The answers are
        `[Retry, Skip]`. The second question offers no Retry: its
        choices are exactly `[Supply, Skip, Quit]`, read from
        `ScriptedAsker::questions_asked`. The `skipped` event (reason
        `unresolvable`) has every current attempt `not-found` and
        `earlier` one round with Crossref `unavailable`.
      - **Two retries accumulate.** Outage, Retry, outage, Retry, then
        not-found. `earlier` holds both outage rounds, oldest first.
      - **Retry, then a correction.** Outage, Retry (still unavailable),
        then the operator supplies a DOI and renames.
        `displaced.lookup.earlier` holds the first round, and the
        event's own `lookup` (origin `operator`) has no `earlier`.
      - Kind (b): the `sections_for` helper per D11.
- [x] 5.2 Green, in `crates/borax/src/run.rs`:
      - the driver records submissions (D5), including refused texts
        from `supplied_identifier`;
      - it marks candidates `Rejected` only when a later record takes
        their place on offer or on Skip, and marks submissions `NoMove`
        or `NoRecord`;
      - restoration: the Supply arm's `Err(unheld)` branch, and the
        `elsewhere` reset after a candidate, put back the record that
        was on offer before the supply, in place of `own.clone()`;
      - the collision notice leaves the candidate open;
      - it puts the input as it stands on every event it builds;
      - `described` stops calling `accept`;
      - `skipped` rebuilds the held verdict with the input.

      Correct the docstrings D5 lists.

## 6. Human lines

- [x] 6.1 Red, in `crates/borax/tests/event.rs` (`human_line`) and in
      `dispatch.rs` (human output of an interactive run):
      - **The round-two final skip**: its line equals the D6 string.
      - **Two rejected candidates**: the line ends `; candidates
        rejected: doi:A, doi:B`, in submission order.
      - **A `resolved` line with one rejected candidate**: the clause
        follows `from <where>`. With a library problem, it follows the
        library clause.
      - **No clause**: a line whose submissions hold none `rejected`
        has no clause. That covers `unparsed`, `no-record`, `no-move`,
        a used submission, and not attempted.
      - **Escaping**: an identifier carrying a control character is
        written escaped. (No real identifier carries one, so build the
        event directly.)
      - **Current round only** (D12): an `unresolvable` skip whose
        `lookup` has an `earlier` round of `unavailable` answers gives
        exactly the line it gives without `earlier`. Expected to pass at
        once; it pins behaviour.
- [x] 6.2 Green, in `event.rs`: append the clause in `human_line`'s
      `resolved` and `skipped` arms, inside the escaped text.

## 7. The interactive description

- [x] 7.1 Red, in `crates/borax/tests/describe.rs`:
      - **A pending event**: the `candidate` line reads exactly
        `candidate   pending; skipping leaves the file as it was`. It
        sits before `new name` and after any `rejected` lines.
      - **A pending event whose `match_check` is `conflict`**: it shows
        the `conflict` line.
      - **An `automatic` event**: it shows neither the `candidate` line
        nor the conflict line.
      - **Rejected submissions**: each gives a `rejected` line with the
        identifier whole, in order. On a resolution they sit after
        `file says` and `conflict`. On a failed verdict they come last,
        after `library`.
      - **Escaping**: a `rejected` identifier carrying a control
        character is escaped.
      - **Nothing to show**: a description with no submissions is
        unchanged. The existing tests pass.
      - **Current round only** (D12): an `unresolvable` skip with an
        `earlier` round gives the same `no record` block as one without
        it. Expected to pass at once; it pins behaviour.
      - Kind (b): the `Fixture`'s `"supplied"` arm per D11.
      - In `dispatch.rs`, on what the terminal is shown: after a
        candidate is replaced on offer by a second DOI's record, the
        question about the second record carries `rejected` naming the
        first DOI. This shares its setup with 5.1's "A found, B found".
- [x] 7.2 Green, in `describe.rs`: add the two lines and the conflict
      condition, and update `describe`'s docstring list of lines.

## 8. The real binary

- [x] 8.1 Red, in `crates/borax/tests/end_to_end.rs`: extend
      `resolve_and_rename_over_the_real_backend_carry_no_removed_field`
      (near line 1340). Every `resolved` event and every resolution
      `skipped` event carries `identifier_input`. In these batch runs it
      is `not-attempted` with reason `not-asked`, or `content-duplicate`
      on a content duplicate. A line's raw text places it after
      `extraction` and before `lookup`.
- [x] 8.2 Green: nothing beyond groups 1–7 is expected. If something is
      needed, the fix goes in the group that owns it, and the report
      says so.

## 9. Audit and verification

- [x] 9.1 Audit every edit to a pre-existing test against the preamble's
      rules and D11. Report any test that needed anything else.
- [x] 9.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`.
- [x] 9.3 Search `crates/borax/src/` for what this change makes false.
      Each search must find nothing, or a hit the report justifies:
      - "reaches no event stream" and "nowhere in either answer";
      - `accept(` called from `described`;
      - `Acceptance::Automatic` produced for a record whose lookup
        origin is `Operator`.
- [x] 9.4 Run `openspec validate operator-input-evidence --strict` and
      `python3 scripts/check-spec-deltas.py`.
- [x] 9.5 By hand, run an interactive `borax rename` through a pty over
      one file of the real-PDF corpus, which lives outside the
      repository. Repeat the round-two session there, and check:
      - the `candidate` line;
      - the final skip line's clause;
      - the run log's `identifier_input`.

      If this is not run, say so in the report, as change 9 did.

## 10. Documents and state

- [x] 10.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Sources:
      this change's design.md (D1–D4, D6–D8 and the illustrative
      lines), proposal.md, and the implemented `human_line` and
      `describe`. Passages:
      - `*** The interactive session`:
        - the paragraph beginning "Choose ~Skip~ to leave the file in
          place": the skip still reports the file's own reason, and a
          rejected candidate adds `; candidate rejected: <identifier>`;
        - the *Supplying an identifier* paragraphs:
          - a refused text is kept as a submission in the run log;
          - a well-formed identifier, a truncated DOI included, is
            looked up exactly as typed;
          - a supplied record is shown as pending, with the `candidate`
            line, until you answer;
          - a candidate you set aside is named on a `rejected` line in
            later questions;
        - the paragraph beginning "borax writes this entry only when you
          accept the rename": a skipped candidate is still never stored
          or cited, and it is now reported in the file's verdict as a
          rejected submission.
      - `* Run logs`:
        - the paragraph beginning "Seven sections follow the record":
          eight sections, and the reasons `not-asked`, `not-supplied`,
          `unparsed` and `no-move`;
        - the sentence listing `acceptance` values: add `accepted` and
          `pending`, with when each appears and that no reported
          verdict is `pending`;
        - a new paragraph for `identifier_input`: its statuses,
          `submissions`, `syntax` and its reasons, each submission's
          outcome, and `used` and `displaced`;
        - the content-duplicate sentence: `identifier_input` is among
          the steps not attempted with reason `content-duplicate`;
        - the paragraph beginning "~run-started~ includes the Boolean
          ~interactive~": submissions are carried in the file's
          verdict, while questions are still not events;
        - the ~lookup~ paragraph: a lookup you asked borax to try again
          keeps the lookups before it in ~earlier~, oldest first, and
          the rest of ~lookup~ is the latest round.
      - `*** The interactive session`, the paragraph on ~Try the
        services again~: whether it is offered depends on the latest
        attempt only, and the run log keeps every attempt.
- [x] 10.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Sources: proposal.md and this
      change's design.md. Changes:
      - amend the Unreleased schema-4 entry: eight sections, the
        `accepted` and `pending` values, and the rejected-candidate
        clause on human lines;
      - add the API changes:
        - `identifier::supplied` returns
          `Result<Identifier, SuppliedError>`;
        - `IdentifierKind` is added;
        - `FileRecord::accepted` and `Evidence::identifier_input` are
          added;
        - the four `Unattempted` variants are added;
        - `earlier` on `LookupEvidence::Attempted` and `LookupStep`, and
          `LookupRound`, are added. A retry now keeps the lookups before
          it, where it used to replace them.

      Schema 4 is unreleased, so no entry describes the interim
      `automatic`.
- [x] 10.3 Orchestrator: read both diffs against the sources. Check for
      anything invented, and for anything dropped from the old text.
      Answer or remove every `TODO(docs)`.
- [x] 10.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change;
      - close the sentence at the end of the `sectioned-resolved-event`
        paragraph that calls `automatic` interim;
      - correct "An abandoned candidate leaves nothing anywhere: not
        reported, not indexed, not cited" in the
        `supply-identifiers-interactively` paragraph. It is now
        reported as evidence and still never indexed or cited;
      - correct "a retry replaces it, a supply never does" in the
        `expose-resolution-attempts` paragraph: a retry becomes the
        current round and keeps the ones before it;
      - update "Last reviewed".
