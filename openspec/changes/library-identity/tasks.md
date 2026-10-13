# Tasks: library-identity

Work on branch `change/library-identity`, cut from `main` at `c0ccf17`.
This is change 12 of the roadmap's Phase 4, and the second of its four
changes.

Groups 1 to 7 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D8 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.
- Write **one named test per bullet**. A bullet is not covered
  "implicitly" by an edited test or by another bullet's test.

These parts of the design fix the contract:

- the file and its parse rules: D1;
- what an identity means: D2;
- where it is ensured: D3;
- minting without replacing, and the reader's table: D4;
- failure and the warning: D5;
- validation: D6;
- the JSON shapes and the human strings: D7, and the illustrative lines
  at the end of design.md;
- the Rust interface: D8.

**Existing tests change.** Design D10 lists each one and the kind of
edit:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact.
  One is listed: `adopt_records_each_orphan_the_content_index_answers_for`,
  through its `outside_the_stores` helper;
- **(c)** a deletion must be listed. None is.

Sweep the tests for every changed type:

- `Event::LibraryStatus` and `Event::LibraryValidated`;
- `Survey`, `Validation`, `Reconciliation`, `Adoptions` and `Aftermath`;
- every test that compares a snapshot or lists a directory around a
  run that writes to the store.

Task 8.1 audits every edit to an existing test.

**Fixtures.** Reuse these:

- in `crates/borax-core/tests/library.rs`: the `UUID_A` to `UUID_D`
  constants, which are all version 7. A non-v7 canonical UUID is
  written inline, for example `550e8400-e29b-41d4-a716-446655440000`
  (version 4);
- in `crates/borax-sources/tests/store.rs`: `tempdir`, and the
  `write_atomically` tests near line 693 as the model;
- in `crates/borax/tests/library.rs`:
  - `tempdir`, `write_file`, `snapshot`, `stat`;
  - `artifact_record`, `fresh_record`, `write_artifact_record`;
  - `write_item`, `item_with_doi`, `record_with_doi`, `minimal_record`;
  - `hash`, `hash_entry`, `item_id`, `artifact_id`;
  - `Extractor` and `reconciling`;
  - `UUID_A` and the other constants, `TIMESTAMP`, `TOOL_VERSION`.

  For a `create` or `write` stand-in, record its calls in a
  `RefCell<Vec<PathBuf>>`, so that a test can assert both order and
  count;
- in `crates/borax/tests/dispatch.rs`:
  - `real_library`, `write_real_file`, `real_stat`, `snapshot`;
  - `write_lib_item`, `write_lib_artifact_record`,
    `seed_artifact_record`, `lib_item_id`, `lib_artifact_id`,
    `lib_hash_entry`, `hash_entry_for`, `hash_bytes`;
  - `adopt_over`, `adoptions`, `adopted_totals`,
    `assert_every_orphan_is_accounted_for`, `outside_the_stores`;
  - `FakeDocuments::with_file`, `pdf_with_embedded_doi`, `fake_source`,
    `record_by`;
  - `effective_with` (with `layer.record = Some(false)` for
    `--no-record`), `effective_with_default_template`, `fixed_now`;
  - `events_for`, and `dispatch` with `Streams`, `Session::batch`;
  - `RealFilesystem`, `ContentIndex::new(MemoryCache::new())`,
    `FakeBibFiles`.

  `Adapters { state_root: None, .. }` writes no run log, so nothing but
  the run's own writes reaches `.borax/`;
- in `crates/borax/tests/event.rs`: `all_events`, with
  `borax::event::json_line` and `human_line`.

**Read-only directories.** The `#[cfg(unix)]` tests that make a mint
fail follow `a_failed_write_after_an_accepted_change_reports_unwritten_alone`
(`dispatch.rs`, near line 13236):

1. Create `.borax/artifacts/` and `items/` first.
2. Set `.borax/` to mode 0o555, so that `library.toml` and its
   temporary cannot be created, while both stores below it stay
   writable.
3. Probe that a file cannot be created in `.borax/`. If it can (running
   as root), return early and say why.
4. Restore 0o755 before the assertions, so the temporary directory
   drops cleanly.

**Design tests.** Each case below is a test, in the group named:

| Case | Group |
|---|---|
| the file is `id = "<uuid>"\n`; an unknown key is ignored; a non-v7 canonical UUID parses; a non-canonical one does not | 1 |
| a file is never replaced; a lost race returns `AlreadyExists`; no temporary is left | 2 |
| the exclusive-create fallback when linking fails, and it still never replaces | 2 |
| concurrent creates leave one winner; concurrent ensures converge on one identity | 2, 3 |
| absent, present and faulty, a dangling symlink included | 3 |
| a lost race returns the winner's identity, or the winner's faulty file | 3 |
| the guard: identity before the write, no write when the mint fails, a faulty file written past and remembered | 3 |
| an admission that writes nothing mints nothing | 3 |
| reconcile and adopt mint on their first write, and not without one | 4 |
| a failed mint in reconcile and adopt reports `unwritten` naming the identity file | 4 |
| the `id` key and the human clause, present and absent | 5 |
| status and validate report the identity and mint none; validate's finding and its order | 6 |
| a rename mints, keeps the identity on later runs, and mints nothing in a preview or under `--no-record` | 7 |
| a failed mint costs the record and not the rename | 7 |
| a faulty identity file: writes proceed and the run warns exactly once; a preview does not warn | 7 |
| a copy is the same library | 7 |
| resolve and bib mint nothing; an identity decides nothing about resolution | 7 |

## 1. The library identity as a value

- [ ] 1.1 Red, in `crates/borax-core/tests/library.rs`:
      - `library_id_parses_a_canonical_uuid_and_renders_it_back_byte_identically`.
      - `library_id_from_uuid_renders_the_same_canonical_text`.
      - `library_id_refuses_an_uppercase_spelling`.
      - `library_id_refuses_a_braced_spelling`.
      - `library_id_refuses_a_bare_hex_run_with_no_hyphens`.
      - `library_id_accepts_a_canonical_uuid_of_another_version`: the
        version-4 literal parses (D1).
      - `library_identity_is_written_as_one_id_line`: `to_toml()`
        equals `format!("id = \"{UUID_A}\"\n")` exactly.
      - `library_identity_round_trips_through_toml`.
      - `library_identity_ignores_keys_and_tables_it_does_not_know`:
        `id`, `created = "2026-01-01"` and a `[later]` table parse to
        that `id`.
      - `library_identity_without_an_id_does_not_parse`: both `""` and
        `other = 1` fail with `Malformed`.
      - `library_identity_whose_id_is_not_a_string_does_not_parse`:
        `id = 7`.
      - `library_identity_whose_id_is_not_a_canonical_uuid_does_not_parse`:
        the uppercase spelling fails, and the message contains
        `not a canonical library uuid`.
      - `library_identity_that_is_not_toml_does_not_parse`.
- [ ] 1.2 Green: `LibraryId` and `LibraryIdentity` (D1, D8).

## 2. Creating a file without replacing one

- [ ] 2.1 Red, in `crates/borax-sources/tests/store.rs`:
      - `create_atomically_creates_the_file_and_its_parent_directories`.
      - `create_atomically_refuses_an_existing_file_and_leaves_it_byte_identical`:
        `ErrorKind::AlreadyExists`.
      - `create_atomically_leaves_no_temporary_behind_when_it_creates`:
        the directory holds the one file.
      - `create_atomically_leaves_no_temporary_behind_when_the_file_exists`.
      - `create_atomically_with_a_link_that_is_unsupported_falls_back_to_an_exclusive_create`:
        `link` returns `ErrorKind::Unsupported`. The file holds the
        content, and no temporary remains.
      - `create_atomically_with_a_link_that_is_unsupported_still_refuses_an_existing_file`:
        `AlreadyExists`, the file byte-identical, and no temporary.
      - `create_atomically_with_a_link_reporting_already_exists_does_not_fall_back`:
        `link` returns `AlreadyExists` while the destination is absent.
        The result is `AlreadyExists`, and the destination is still
        absent.
      - `concurrent_creates_of_one_path_leave_exactly_one_winner`: eight
        threads behind a `Barrier`, each with its own bytes. Exactly one
        `Ok`, the rest `AlreadyExists`. The file holds the winner's
        bytes, and no temporary remains.
- [ ] 2.2 Green: `create_atomically` and `create_atomically_with` (D4,
      D8).

## 3. Reading and ensuring a library's identity

- [ ] 3.1 Red, in `crates/borax/tests/library.rs`:
      - `identity_of_a_library_with_no_state_directory_is_absent`.
      - `identity_of_a_library_with_records_and_no_identity_file_is_absent`.
      - `identity_reads_the_id_the_file_holds`.
      - `identity_of_a_file_that_does_not_parse_is_faulty_naming_the_file`:
        `Faulty` whose `path` is `identity_file(root)`, and whose
        `message` is non-empty.
      - `identity_of_an_empty_file_is_faulty`.
      - `identity_of_a_directory_at_the_identity_path_is_faulty`.
      - `identity_of_a_dangling_symlink_is_faulty`, `#[cfg(unix)]`.
      - `ensure_identity_mints_a_v7_identity_into_an_absent_file`: it
        returns `Present(id)` with `id.uuid().get_version_num() == 7`.
        The file text is `id = "<id>"\n`, and `identity(root)` agrees.
      - `ensure_identity_creates_the_state_directory_when_there_is_none`.
      - `ensure_identity_returns_the_identity_already_there_and_calls_no_create`:
        the stand-in records 0 calls, and the snapshot is unchanged.
      - `ensure_identity_does_not_mint_over_a_faulty_file`: `Faulty`,
        the file byte-identical, and 0 calls.
      - `ensure_identity_reports_a_failed_mint_naming_the_identity_file`:
        the stand-in returns `io::Error::other("simulated")`. The `Err`'s
        text contains `cannot create the library's identity at`, the
        identity file's path, and `simulated`. The file is absent.
      - `ensure_identity_after_a_lost_race_returns_the_winners_identity`:
        the stand-in writes a file holding `UUID_B`, then returns
        `AlreadyExists`. The result is `Present(UUID_B)`.
      - `ensure_identity_after_a_lost_race_to_a_faulty_file_returns_it_faulty`.
      - `concurrent_ensures_converge_on_one_identity`: eight threads
        behind a `Barrier`, each calling
        `ensure_identity(root, &store_create)`. All return the same
        `Present(id)`, and the file holds it.
      - `store_writer_ensures_the_identity_before_the_write_it_guards`:
        the shared call log reads `[identity file, target]`.
      - `store_writer_does_not_write_when_the_mint_fails`: the `write`
        stand-in is never called, and the error names the identity file.
      - `store_writer_writes_into_a_library_whose_identity_file_is_faulty_and_remembers_the_fault`:
        `write` is called. `identity_fault()` is `Some`, with the
        identity file's path. The file is byte-identical.
      - `store_writer_ensures_the_identity_before_each_write`: two
        writes, with `library.toml` removed between them. It exists
        again after the second write.
      - `store_writer_that_writes_nothing_mints_nothing`.
      - `admit_through_a_store_writer_creates_the_identity_before_the_item_and_the_record`:
        an admission with no held record, through a `StoreWriter` over
        recording stand-ins that delegate to `store_create` and
        `store_write`. The log reads: identity file, then the item under
        `items/`, then the record under `.borax/artifacts/`.
      - `admit_through_a_store_writer_whose_mint_fails_writes_nothing_and_names_the_identity_file`:
        `Err(Unrecorded)` whose message names `.borax/library.toml`.
        No `items/` and no `.borax/artifacts/` exist afterwards.
      - `an_admission_that_rewrites_nothing_mints_nothing`:
        1. Admit a file twice through real seams.
        2. Remove `.borax/library.toml`.
        3. Read `held` again with `recorded_at`, and admit a third,
           identical time through a `StoreWriter` whose `write`
           stand-in counts its calls.

        The count is 0, and `identity(root)` is `Absent`. The third
        admission rewrites a document `rewrite` itself rendered, so it
        renders unchanged (D3).
      - `identity_fault_warning_names_the_file_and_validate`: the exact
        message from D5, with level `Warning`.
- [ ] 3.2 Green: `IDENTITY_FILE`, `identity_file`, `Identity`,
      `identity`, `ensure_identity`, `store_create`, `StoreWriter` and
      `identity_fault_warning` (D3, D4, D5, D8).

## 4. Reconciliation and adoption write through the guard

- [ ] 4.1 Red, in `crates/borax/tests/library.rs`:
      - `reconcile_that_repairs_a_record_mints_the_identity_first`: a
        moved artifact in a library with no identity file. The repair
        is written, and `identity(root)` is `Present` afterwards.
      - `reconcile_over_an_untouched_library_mints_nothing`.
      - `reconcile_that_finds_only_a_pending_change_mints_nothing`.
      - `reconcile_that_finds_only_missing_records_mints_nothing`.
      - `reconcile_into_a_library_with_an_identity_leaves_it_byte_identical`.
      - `reconcile_into_a_library_with_a_faulty_identity_file_writes_the_repair_and_reports_the_fault`:
        the repair is written, the identity file is byte-identical, and
        `identity_fault` is `Some`.
      - `reconcile_whose_mint_fails_writes_no_record_and_reports_unwritten_naming_the_identity_file`,
        `#[cfg(unix)]`, with the read-only `.borax/`. The record file is
        byte-identical, and its repair is `Repair::Unwritten` whose
        message contains `library.toml`.
      - `adopt_that_records_an_orphan_mints_the_identity`: call
        `library::adopt` directly, with a `lookup` closure answering
        for the orphan's hash.
      - `adopt_that_records_nothing_mints_nothing`: `lookup` answers
        nothing.
      - `adopt_into_a_library_with_a_faulty_identity_file_records_and_reports_the_fault`.
      - `adopt_whose_mint_fails_records_nothing_and_reports_unwritten_naming_the_identity_file`,
        `#[cfg(unix)]`. The orphan's adoption is `Adoption::Unwritten`
        whose message contains `library.toml`. No record and no item
        are written.
- [ ] 4.2 Green: `reconcile` and `adopt` write through a `StoreWriter`,
      and `Reconciliation::identity_fault` and `Adoptions::identity_fault`
      carry what it remembered (D3, D5).

## 5. The event vocabulary

- [ ] 5.1 Red, in `crates/borax/tests/event.rs`. Extend `all_events`
      with `LibraryStatus` and `LibraryValidated`, each once with `id`
      and once without, so the existing round-trip tests cover them.
      Make the D10 (a) edits in `crates/borax/tests/dispatch.rs`, `id:
      None` in each constructor:
      - (a) `LibraryStatus`:
        - `status_over_a_marked_directory_of_new_pdfs_reports_every_count_unopened`;
        - `status_identify_counts_artifacts_yielding_an_identifier_and_queries_no_source`;
        - `status_identify_reports_each_artifact_in_survey_order_before_the_totals`;
        - `status_identify_tells_the_two_controlled_cases_apart`;
        - `status_identify_counts_a_title_holding_a_doi_as_identifiable`;
        - `status_identify_tells_encrypted_and_unreadable_apart`;
        - `status_over_an_unmarked_directory_is_reported_as_given_and_writes_nothing`;
        - `status_names_only_the_orphan_and_opens_nothing`;
        - `status_over_a_library_with_a_pending_change_opens_and_names_nothing`.
      - (a) `LibraryValidated`:
        - `validate_emits_the_finding_then_the_orphan_and_missing_conditions_then_the_totals`;
        - `validate_names_all_three_conditions_in_kind_order_and_succeeds`;
        - `validate_names_conditions_beside_a_finding_and_is_partial`;
        - `validate_names_two_missing_conditions_for_two_records_of_one_identity`.

      The three `LibraryStatus { .. }` patterns need no edit. New tests:
      - `library_status_without_an_identity_is_written_without_an_id_key`:
        exact JSON, the same as 0.9.0's.
      - `library_status_carries_the_id_directly_after_the_root`: checked
        on the raw line text, not on a parsed `Value`, whose keys sort.
      - `library_validated_without_an_identity_is_written_without_an_id_key`.
      - `library_validated_carries_the_id_directly_after_the_root`.
      - `human_line_of_library_status_with_an_identity_ends_with_the_library_clause`.
      - `human_line_of_library_status_with_nested_libraries_and_an_identity_puts_the_identity_last`.
      - `human_line_of_library_status_without_an_identity_is_unchanged`.
      - `human_line_of_library_validated_with_an_identity_ends_with_the_library_clause`.
      - `human_line_of_library_validated_without_an_identity_is_unchanged`.

      Every expected string is D7's, exactly.
- [ ] 5.2 Green: `id` on both variants, and their human lines (D7, D8).

## 6. Status and validation

- [ ] 6.1 Red, in `crates/borax/tests/library.rs`:
      - `survey_carries_the_identity_the_library_holds`.
      - `survey_of_a_library_with_no_identity_file_carries_none`.
      - `survey_of_a_library_with_a_faulty_identity_file_carries_none`.
      - `status_event_carries_the_surveys_identity`.
      - `validate_reports_an_identity_file_that_does_not_parse_as_unreadable`:
        one finding whose path is `identity_file(root)` and whose kind
        is `Finding::Unreadable`. Its message contains the parser's
        reason.
      - `validate_reports_an_identity_path_holding_a_directory_as_unreadable`.
      - `validate_reports_the_identity_finding_before_the_item_and_record_findings`:
        a library holding a malformed identity file, an item file that
        does not parse, and an artifact record that does not parse.
      - `validate_reports_no_finding_and_no_condition_for_a_library_with_no_identity_file`.
      - `validate_reports_no_finding_for_an_identity_file_with_an_unknown_key`.
      - `validate_carries_the_identity_on_its_totals`: the last event of
        `validation_events` is `LibraryValidated` with `id` set.
      - `validate_mints_no_identity`: a library with records and no
        identity file. The snapshot is unchanged.

      And in `crates/borax/tests/dispatch.rs`:
      - `status_reports_the_identity_of_a_library_that_has_one`.
      - `status_over_a_library_with_no_identity_reports_none_and_mints_none`:
        the snapshot is unchanged, and `.borax/library.toml` is absent.
      - `status_human_output_ends_with_the_library_clause`: `dispatch`
        with `Streams`. The last stdout line ends with
        `; library <uuid>`.
      - `validate_over_a_malformed_identity_file_exits_partial_and_names_it`:
        `dispatch` with `Streams`. The outcome is partial success, and
        stdout carries `<identity file>: unreadable (`.
      - `validate_reports_the_identity_on_library_validated`.
- [ ] 6.2 Green: `contents` reads `identity(root)`. `Survey::identity`,
      `Validation::identity`, the identity finding first, and
      `status_event` and `validation_events` carrying `id` (D6, D7).
      Extend `Finding::Unreadable`'s and `Validation::findings`'
      docstrings.

## 7. Runs

- [ ] 7.1 Red, in `crates/borax/tests/dispatch.rs`, all through
      `events_for` or `dispatch` over `real_library`. Make the (b) edit
      D10 lists to `outside_the_stores` and
      `adopt_records_each_orphan_the_content_index_answers_for`. New
      tests:
      - `an_applying_rename_into_a_library_with_no_identity_mints_one`:
        afterwards `library::identity(&root)` is `Present`, version 7,
        and the artifact record exists.
      - `an_applying_rename_into_a_library_with_an_identity_leaves_it_byte_identical`.
      - `a_second_applying_rename_keeps_the_identity_the_first_minted`.
      - `a_library_an_earlier_release_wrote_gains_an_identity_on_its_next_recorded_rename`:
        seeded records and items with no identity file. Afterwards the
        identity is `Present`, and the seeded records the run did not
        touch are byte-identical.
      - `a_preview_mints_no_identity`.
      - `a_rename_with_no_record_mints_no_identity`.
      - `an_applying_rename_whose_mint_fails_renames_the_file_and_reports_unwritten`,
        `#[cfg(unix)]`, with the read-only `.borax/`. Check:
        - the file is at its target;
        - the admission is `Unwritten`, its message naming
          `.borax/library.toml`;
        - `items/` and `.borax/artifacts/` are unchanged.
      - `an_applying_rename_into_a_library_with_a_faulty_identity_file_records_and_warns_once`:
        `dispatch` with `Streams`, two files. Check:
        - both are recorded;
        - the identity file is byte-identical;
        - stderr contains D5's warning exactly once.
      - `a_reconcile_that_writes_into_a_library_with_a_faulty_identity_file_warns_once`.
      - `an_adoption_into_a_library_with_a_faulty_identity_file_warns_once`.
      - `a_preview_over_a_library_with_a_faulty_identity_file_does_not_warn`.
      - `a_reconcile_over_an_untouched_library_with_a_faulty_identity_file_does_not_warn`.
      - `a_copy_of_a_library_is_the_same_library`:
        1. Mint by an applying rename.
        2. Copy the tree recursively into a second `real_library`.
        3. `status` over each reports the same `id`.
        4. An applying rename in the copy leaves its identity file
           byte-identical.
      - `resolve_mints_no_identity`.
      - `bib_mints_no_identity`.
      - `a_library_with_no_identity_still_answers_for_its_own_files`:
        `resolve` over a tracked file in a seeded library with no
        identity file. The `library` section is `tracked`, and no
        identity file is created.
- [ ] 7.2 Green:
      - the rename driver records through one `StoreWriter` per run,
        and `Aftermath::identity` carries its warning;
      - `reconcile_events` and `adopt_events` return the warning from
        `identity_fault`;
      - `emit_events` places it, and `dispatch` writes it after
        `Aftermath::library` and before `Aftermath::diagnostic` (D3,
        D5).

## 8. Audit and verification

- [ ] 8.1 Audit every edit to a pre-existing test against the
      preamble's rules and D10. Report any test that needed anything
      else.
- [ ] 8.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`.
- [ ] 8.3 Search `crates/borax/src/` and `crates/borax-sources/src/`
      for what this change makes false. Each search must find nothing,
      or a hit the report justifies:
      - "Nothing but the two stores is written" in `adopt`'s docstring;
      - "two kinds of file" and "the two identities" in the module docs
        of `crates/borax-core/src/library.rs`;
      - `&store_write` passed directly to `admit` or `rewrite` anywhere
        but a test;
      - "store directory" and "one of the stores" in
        `Finding::Unreadable`'s docstring.
- [ ] 8.4 Run `openspec validate library-identity --strict` and
      `python3 scripts/check-spec-deltas.py`.
- [ ] 8.5 By hand, over a copy of a library made from the real-PDF
      corpus, which lives outside the repository:
      1. Run `borax status` (no identity, nothing written).
      2. Run `borax rename --apply` (identity minted).
      3. Copy the tree with `rsync -a` and run `borax status` over both
         (same identity).
      4. Corrupt `.borax/library.toml` and run `borax reconcile` after
         a `touch` (one warning), then `borax validate` (one finding).

      If this is not run, say so in the report.

## 9. Documents and state

- [ ] 9.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Keep every
      existing example name. Sources: this change's design.md (D1–D7
      and the illustrative lines), proposal.md, and the implemented
      `human_line` and `identity_fault_warning`. Passages:
      - `** Where a setting can be set`, the paragraph beginning "A
        /library/ root is the configured ~library-root~". `.borax/` also
        holds `library.toml`, the library's identity. Say what a copy
        is, and that the first store write mints the identity.
      - The library-model passage beginning "A library keeps two
        authoritative text stores". The identity file is authoritative
        too, and deleting it gives the library a new identity at its
        next write.
      - `** borax status`: the report line's closing `; library <uuid>`
        clause, and that `status` never mints one.
      - `** borax validate`: the identity-file finding, and that an
        absent identity is not a finding.
      - `** borax adopt`: adoption writes the identity before its first
        record.
      - `* Run logs`: the `library-status` and `library-validated`
        field sentence gains `id`, present only when the library has an
        identity.
- [ ] 9.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Write the Unreleased entry from
      proposal.md and design.md. It covers:
      - the identity file and when it is minted;
      - that a copy is the same library;
      - the validate finding and the run warning;
      - `id` on the two events, with the schema version unchanged.
- [ ] 9.3 Orchestrator: read both diffs against the sources, for
      anything invented and for anything dropped from the old text.
      Check the `#+AI_PASS` line the doc writer adds. Answer or remove
      every `TODO(docs)`.
- [ ] 9.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change after the
        `separate-change-from-acceptance` paragraph;
      - update "Last reviewed".
