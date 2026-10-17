# Tasks: index-references-libraries

Work on branch `change/index-references-libraries`, cut from `main` at
`2ab0034`. This is change 13b of the roadmap's Phase 4, after 13a
(`reduce-content-index`) and before change 14.

Groups 1 to 6 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D9 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.
- Write **one named test per bullet**. A bullet is not covered
  "implicitly" by an edited test or by another bullet's test.

**Green points.** Every group ends with the workspace compiling and
`cargo test --workspace` green. Each type change lands in the same pair
as every consumer of that type (design D11). There is no interim state
to work around:

- groups 1 to 3 add code nothing calls;
- after group 4, references are read and nothing writes them;
- after group 5, every writer but `adopt` writes them.

These parts of the design fix the contract:

- a reference on disk, its name and its file: D1;
- who writes one, when, and with which identity and item: D2;
- the upsert and its repair: D3;
- reaching a reference, and the causes: D4;
- agreement, precedence, and inside a library: D5;
- adoption: D6;
- the schema, the JSON shapes and the order of events: D7, and the
  illustrative lines at the end of design.md;
- the human strings and the description: D8;
- the Rust interface: D9;
- the sequences each group's tests trace: D10;
- the group boundaries: D11.

**Existing tests change.** Design D13 lists each one, by group, with the
kind of edit:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact.
  D13 names the one that is renamed;
- **(c)** a deletion must be listed. None is.

Each group's red task makes that group's D13 edits and names them. D13
says how the sweep was made. A test that a group's change breaks and
D13 does not list is reported back, with the translation applied.

Task 7.1 audits every edit to an existing test.

**Fixtures.** Reuse these:

- `crates/borax-core/tests/index.rs`: the fact round-trip tests as the
  model for a reference's. The `UUID_*` constants are in
  `crates/borax-core/tests/library.rs`; copy what is needed.
- `crates/borax-sources/tests/store.rs`:
  - `tempdir`;
  - `put_extracted_writes_only_extracted_json` and
    `entry_ignores_a_temporary_file_in_the_directory` as models;
  - `content_index_fact_write_reports_failed_over_a_read_only_root` as
    the model for a `#[cfg(unix)]` failure.
- `crates/borax/tests/library.rs`:
  - `tempdir`, `write_file`, `snapshot`, `stat`;
  - `artifact_record`, `write_artifact_record`;
  - `write_item`, `item_with_doi`, `record_with_doi`, `minimal_record`;
  - `hash`, `hash_entry`, `item_id`, `artifact_id`;
  - the `UUID_*` constants and `TIMESTAMP`;
  - `Extractor` and `reconciling`.
- `crates/borax/tests/pipeline.rs`:
  - `FakeDocuments` with `with_open_error`, `pdf_with_embedded_doi`,
    `pdf_with_no_identifier`;
  - `fake_source`, `PanicSource`, `CountingCache`;
  - `hash_for`, `record_with_doi`, `config`;
  - `resolved_outcome`, `skipped_outcome`, `default_content_index`;
  - `library_item` and `tracked_library`.
- `crates/borax/tests/dispatch.rs`:
  - `real_library`, `write_real_file`, `real_stat`, `snapshot`,
    `copy_dir_recursive`;
  - `write_lib_item`, `write_lib_artifact_record`, `seed_artifact_record`,
    `seed_tracked_file`, `lib_item_id`, `lib_artifact_id`,
    `lib_hash_entry`, `hash_entry_for`, `record_by`;
  - `adopt_over`, `adoptions`, `adopted_totals`,
    `assert_every_orphan_is_accounted_for`, `seed_response_cache`,
    `doi_of`;
  - `reconcile_adapters`;
  - `fake_source`, `KeyedSource`, `PanicSource`, `FakeDocuments`;
  - `ScriptedAsker` and `Answer`;
  - `effective_with_default_template`, `fixed_now`;
  - `events_for`, `dispatch`, `Streams`, `Session::batch`,
    `Session::interactive`;
  - `RealFilesystem`, `FakeFilesystem`, `FakeBibFiles`.
- `crates/borax/tests/binary.rs`: `seed_tracked_binary_fixture`,
  `seed_operator_entry`, and the `env_clear` runs that set
  `XDG_CONFIG_HOME`, `APPDATA`, `XDG_CACHE_HOME` and `XDG_DATA_HOME`.
- `crates/borax/tests/event.rs`: `all_events`, `json_line`,
  `human_line`.
- `crates/borax/tests/describe.rs`: `Fixture`.

**Seeding.**

- **A referenced library.** A `tempdir` holding:
  - `.borax/library.toml` written as `LibraryIdentity { id }.to_toml()`;
  - its item files, through `write_lib_item` or `write_item`;
  - an artifact record where the test needs one.

  No marker is needed (D4).
- **A reference.** `index.put_reference(&hash, &LibraryReference {
  library, root, artifact, item })`, after group 1.
- **An unreadable reference file.** Bad text at `entry_path(root,
  &reference_key(&hash, "sha256-…"))` under a `FileCache` root.
- **A missing root.** A path under a `tempdir` that is never created.
- **A root holding no library.** A `tempdir` with no `.borax/`.
- **Another library at the root.** Another `LibraryIdentity`.

`r(L, R, A, I)` below is `LibraryReference { library: L, root: R,
artifact: A, item: I }`.

## 1. References on disk

Green point: additive except `IndexStore`'s two new methods. Every
implementor changes in this pair: `FileCache`, `MemoryCache`,
`run::IndexHome`, and the six test stores D13 lists.

- [ ] 1.1 Red. Make the group-1 D13 edits: the six test stores.

      In `crates/borax-core/tests/index.rs`:
      - `reference_round_trips_through_json`.
      - `reference_is_written_as_library_root_artifact_and_item`:
        checked on the text.
      - `reference_ignores_keys_it_does_not_know`.
      - `reference_missing_a_key_does_not_parse`: each of the four
        left out in turn.
      - `reference_with_an_identity_that_is_not_canonical_does_not_parse`.
      - `reference_name_is_the_sha256_of_library_root_and_artifact_joined_by_nul`:
        compared with `hash_bytes` over the same bytes.
      - `reference_name_does_not_depend_on_the_item`.
      - `reference_name_differs_for_another_root`.
      - `reference_with_a_root_that_is_not_utf8_has_no_name_and_no_json`,
        `#[cfg(unix)]`.

      In `crates/borax-sources/tests/store.rs`:
      - `reference_key_names_a_file_in_the_hashs_refs_directory`:
        `entry_path` gives `content/<hash>/refs/<name>.json`.
      - `put_reference_writes_one_file_named_for_its_key`.
      - `put_reference_replaces_the_file_of_the_same_key_whole`: another
        item, same file, one file in `refs/`.
      - `put_reference_for_another_root_writes_a_second_file`.
      - `put_reference_leaves_extracted_and_operator_json_byte_identical`.
      - `put_extracted_and_put_operator_leave_every_reference_byte_identical`.
      - `references_of_an_entry_with_no_refs_directory_are_none`.
      - `references_are_in_root_then_library_then_artifact_order`:
        three written out of order.
      - `references_skip_a_file_that_does_not_parse`.
      - `references_ignore_a_temporary_and_a_file_not_ending_json`.
      - `reference_reads_the_file_of_its_key_as_absent_held_or_unreadable`.
      - `entry_is_unchanged_by_references`: `entry()` with and without a
        reference in `refs/` is equal.
      - `remove_reference_removes_only_its_own_file`.
      - `remove_reference_of_an_absent_file_is_written`.
      - `memory_cache_lists_only_the_facts_directly_under_a_prefix`.
      - `memory_cache_removes_a_fact`.
      - `file_cache_list_facts_of_an_unreadable_directory_is_an_error`,
        `#[cfg(unix)]`.
      - `put_reference_with_a_root_that_is_not_utf8_fails_without_writing`,
        `#[cfg(unix)]`.
- [ ] 1.2 Green:
      - `LibraryReference`, `reference_to_json`, `reference_from_json`,
        `name` and `same_key`;
      - `references_prefix`, `reference_key`;
      - `IndexStore::list_facts` and `remove_fact` for `FileCache`,
        `MemoryCache` and `run::IndexHome`;
      - `ContentIndex::references`, `reference`, `put_reference` and
        `remove_reference`.

      (D1, D9.)

## 2. What a library owes the index

Green point: `Admitted` gains a field. Its four `src` literals and
patterns (`library.rs`, near lines 1356, 3027, 3054 and 3147) and its
three test literals change in this pair. The rest is additive.

- [ ] 2.1 Red. Make the group-2 D13 edits: the three `Admitted`
      literals.

      In `crates/borax/tests/library.rs`:
      - `admit_of_a_minted_record_answers_with_its_item`.
      - `admit_of_restored_bytes_answers_with_the_item_they_were_accepted_under`:
        an ordinary re-run over earlier bytes; `answers_with` is the
        entry's item and `item` the link.
      - `admit_of_a_reidentification_answers_with_the_new_link`.
      - `references_owed_names_every_hash_of_every_history`.
      - `references_owed_lists_each_hash_of_a_record_once`: a
        restatement in the history.
      - `references_owed_gives_restored_bytes_the_item_they_were_accepted_under`.
      - `references_owed_leaves_out_bytes_a_record_linking_no_item_answers_for_with_none`.
      - `references_owed_gives_an_unlinked_records_earlier_bytes_their_entrys_item`
        (D2).
      - `references_owed_names_the_full_path_the_record_names`.
      - `references_owed_skips_a_record_file_that_does_not_parse`.
      - `item_store_answer_is_the_one_item_carrying_the_identity`.
      - `item_store_answer_finds_an_item_whose_file_was_renamed_by_hand`.
      - `item_store_answer_is_dangling_when_no_file_carries_it`.
      - `item_store_answer_is_unreadable_when_a_file_claiming_it_cannot_be_read`.
      - `item_store_answer_is_unreadable_when_the_store_cannot_be_listed`,
        `#[cfg(unix)]`.
      - `item_store_answer_is_ambiguous_naming_every_file_carrying_it`.
- [ ] 2.2 Green: `Admitted::answers_with`, set by `admit` and
      `Stores::foresee`; `ItemStore::answer`, sharing `Stores::consult`'s
      attribution; `OwedReference` and `references_owed` (D2, D9).

## 3. The references module

Green point: additive. Nothing calls `borax::references` yet.

- [ ] 3.1 Red, in a new `crates/borax/tests/references.rs`.

      Reaching (D4):
      - `a_reference_to_a_missing_root_is_unavailable_missing`.
      - `a_reference_to_a_dangling_link_is_unavailable_missing`,
        `#[cfg(unix)]`.
      - `a_reference_to_an_unreadable_root_is_unavailable_unreadable_naming_the_root`,
        `#[cfg(unix)]`, under a parent with no permissions.
      - `a_reference_to_a_root_that_is_a_file_is_other_library_with_no_found`.
      - `a_reference_to_a_root_with_no_identity_is_other_library_with_no_found`.
      - `a_reference_to_a_root_whose_identity_is_faulty_is_unreadable_naming_the_identity_file`.
      - `a_reference_to_another_library_is_other_library_naming_it`.
      - `a_reference_whose_item_no_file_carries_is_item_unavailable_dangling`.
      - `a_reference_whose_item_store_cannot_be_listed_is_item_unavailable_unreadable`,
        `#[cfg(unix)]`.
      - `a_reference_whose_item_two_files_carry_is_item_unavailable_ambiguous`.
      - `a_reachable_reference_answers_with_its_stored_item_not_the_records_link`:
        the record re-linked by hand after the reference was written.
      - `a_reference_is_reachable_with_no_marker_at_its_root`.
      - `libraries_answer_a_second_reference_to_a_root_from_its_first_read`:
        the item file is edited between the two `reach` calls, and the
        second answers with the first read's record.
      - `reaching_a_reference_writes_nothing`: a `snapshot` of the root
        and of the index root, before and after.

      Settling (D5, pure):
      - `settle_one_reachable_reference_answers`.
      - `settle_references_whose_items_carry_one_preferred_identifier_agree_and_the_first_answers`.
      - `settle_references_whose_items_carry_different_identifiers_disagree_naming_each`.
      - `settle_compares_the_preferred_identifier_so_a_doi_outranks_an_arxiv_id`:
        one item with DOI X and arXiv A, one with arXiv A alone,
        disagree.
      - `settle_items_carrying_no_identifier_agree_only_as_one_item_of_one_library`.
      - `settle_an_item_carrying_an_identifier_and_one_carrying_none_disagree`.
      - `settle_keeps_each_unavailable_status_beside_an_answer`.
      - `settle_with_nothing_reachable_answers_nothing`.
      - `settle_two_locations_of_one_library_are_two_references`.

      Keeping (D3):
      - `keep_writes_a_reference_the_entry_lacks`.
      - `keep_writes_nothing_for_a_reference_held_with_the_same_item`:
        `None`, and the file is byte-identical.
      - `keep_rewrites_a_reference_whose_item_differs`.
      - `keep_rewrites_an_unreadable_reference_file`.
      - `keep_repairs_a_reference_to_the_same_artifact_at_a_missing_root`:
        `repaired_from` names the old root, and its file is gone.
      - `keep_repairs_a_reference_whose_root_holds_no_library`.
      - `keep_repairs_a_reference_whose_root_holds_another_library`.
      - `keep_repairs_every_stale_root_of_the_same_artifact`.
      - `keep_leaves_a_reference_at_a_root_holding_the_same_library`.
      - `keep_leaves_a_reference_at_an_unreadable_root`, `#[cfg(unix)]`.
      - `keep_leaves_a_stale_reference_when_its_own_key_was_already_held`.
      - `keep_leaves_references_to_other_artifacts_and_other_libraries`.
      - `keep_leaves_an_unreadable_reference_of_the_same_artifact`.
      - `keep_writes_the_new_reference_before_removing_a_stale_one`: a
        test store logs `write_fact` and `remove_fact` in order.
      - `keep_whose_write_fails_removes_nothing`.
      - `keep_whose_removal_fails_reports_failed_naming_the_file_and_leaves_it`.
      - `keep_reads_and_writes_no_evidence_fact`: the logging store sees
        no `extracted` or `operator` key.
- [ ] 3.2 Green: `borax::references` (`Unavailable`, `Reach`,
      `ReferenceStatus`, `ReferenceRead`, `Libraries`, `settle`, `read`,
      `Kept`, `keep`), and `pipeline::preferred_identifier` (D3, D4, D5,
      D9).

## 4. Resolution through references

Green point: every consumer of `IndexEvidence`, `IndexRead`,
`IndexReadStep`, `RecordRetrieval` and `RetrievedFrom` changes in this
pair: pipeline, evidence, event, describe and run, and the test sites D13
lists. Nothing writes references yet, so every existing run behaves as
before.

- [ ] 4.1 Red. Make the group-4 D13 edits.

      In `crates/borax/tests/pipeline.rs`:
      - `a_reachable_reference_answers_without_opening_the_file_or_asking_a_service`:
        `FakeDocuments::with_open_error` and `PanicSource`.
      - `a_reference_answer_reads_hit_with_its_references_and_no_evidence`.
      - `a_reference_answer_is_retrieved_from_the_library_reference`.
      - `a_reference_answer_marks_extraction_titles_lookup_match_check_and_the_write_not_attempted_for_content_index_hit`.
      - `a_reference_answer_is_accepted_automatically`.
      - `a_reference_answers_before_the_entrys_operator_evidence`.
      - `disagreeing_references_fall_to_the_entrys_evidence_and_report_each_identifier`.
      - `disagreeing_references_with_no_evidence_read_miss_and_extract`.
      - `an_unavailable_reference_falls_to_the_entrys_evidence_and_reports_its_cause`.
      - `an_item_unavailable_reference_falls_to_the_entrys_evidence`.
      - `a_reference_answer_goes_through_the_work_check`: an account
        whose library already holds a file of that work.
      - `a_tracked_file_reads_no_reference`: `CountingCache` stays at
        zero with a reference seeded.
      - `an_untracked_copy_inside_a_library_is_answered_by_its_reference_and_reported_untracked`.
      - `a_bypassed_index_reads_no_reference`.
      - `an_unhashable_file_reads_no_reference`.
      - `a_reference_answer_writes_nothing_to_the_content_index`: both
        facts and the reference byte-identical.
      - `identification_of_a_reference_answer_is_none`.
      - `remember_after_a_reference_answer_is_not_attempted_for_content_index_hit`.
      - `standing_in_answers_a_second_file_from_the_first_read_of_a_referenced_library`.
      - `sections_project_a_reference_hit_and_its_references`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_a_reference_hit_carries_references_and_no_evidence`.
      - `json_line_of_an_evidence_hit_beside_references_carries_both`.
      - `json_line_of_a_miss_with_references_carries_them`.
      - `json_line_of_a_read_with_no_references_has_no_references_key`.
      - `json_line_of_every_reference_status_and_cause_matches_design_d7`.
      - `reference_steps_round_trip_through_json_for_every_status_and_cause`.
      - `json_line_of_record_retrieval_library_reference_matches_design_d7`.
      - `human_line_of_a_reference_answer_ends_from_the_library_at_its_root`.
      - `human_line_names_each_reference_that_could_not_answer_after_the_library_clause`:
        one case per cause wording in D8.
      - `human_line_names_disagreeing_references_once_before_rejected_candidates`.
      - `human_line_of_a_skip_carries_the_reference_clauses`.
      - `human_line_of_a_reference_clause_escapes_control_characters`.

      In `crates/borax/tests/describe.rs`:
      - `a_reference_answer_names_the_records_identifier_without_an_origin`.
      - `a_reference_answers_record_line_names_the_library_at_its_root`.
      - `a_reference_answer_with_no_provenance_names_the_library_at_its_root_alone`.
      - `a_description_carries_a_reference_line_for_each_reference_that_could_not_answer`.
      - `a_description_carries_a_references_line_for_a_disagreement`.
      - `a_skips_description_carries_its_reference_lines_after_its_library_line`.

      In `crates/borax/tests/dispatch.rs`:
      - `resolve_outside_a_library_is_answered_by_a_reachable_reference_with_no_request`.
      - `resolve_batch_reads_a_referenced_library_once_for_two_files`:
        D4's snapshot rule, at concurrency 1.
      - `an_interactive_question_over_a_reference_answer_names_the_library_at_its_root`.
- [ ] 4.2 Green:
      - `IndexRead::Referenced`, `IndexEvidence::references`,
        `RecordRetrieval::LibraryReference`, and `Evidence::retrieval`
        for it;
      - `standing_in`, `beyond_library` reading references first, and
        one `Libraries` in `resolve_batch`, the rename driver and
        `bib`;
      - `IndexReadStep::Hit` and `Miss`, `ReferenceStep`,
        `RetrievedFrom::LibraryReference`, and the human clauses;
      - `describe.rs`.

      (D4, D5, D7, D8, D9.)

## 5. Writing references

Green point: additive behaviour through the event shapes group 4 fixed.
`WrittenEntry` gains a variant, and no test matches it exhaustively.

- [ ] 5.1 Red.

      In `crates/borax/tests/dispatch.rs`, the rename driver:
      - `an_applying_rename_writes_a_reference_for_the_admitted_bytes`.
      - `an_applying_rename_writes_the_reference_after_the_artifact_record`:
        a store logging `write_fact` sees the record on disk first, as
        `RecordOrderStore` does for the operator fact.
      - `an_applying_rename_reports_the_reference_after_the_admission_and_before_the_evidence`.
      - `an_already_named_tracked_file_admitted_by_an_applying_run_writes_its_missing_reference`.
      - `an_applying_rerun_over_a_held_reference_writes_and_reports_nothing`.
      - `a_reidentification_rewrites_the_reference_with_the_new_item`.
      - `a_separated_file_is_referenced_as_its_new_artifact_and_the_old_reference_is_kept`.
      - `restored_bytes_admitted_again_are_referenced_with_the_item_they_were_accepted_under`.
      - `a_pending_change_writes_no_reference`.
      - `a_file_renamed_outside_the_library_writes_no_reference`.
      - `an_unwritten_admission_writes_no_reference`.
      - `a_preview_writes_no_reference`.
      - `no_record_writes_no_reference`.
      - `an_applying_rename_into_a_library_with_no_identity_names_the_identity_it_minted`.
      - `an_unchanged_admission_into_a_library_with_no_identity_writes_no_reference_and_reports_nothing`:
        no event, empty stderr, no identity file.
      - `an_admission_into_a_library_with_a_faulty_identity_writes_no_reference_and_warns_once`.
      - `an_applying_admission_repairs_the_reference_of_a_moved_library`.
      - `a_failed_reference_write_is_reported_and_leaves_the_rename_standing`.

      In `crates/borax/tests/dispatch.rs`, reconcile:
      - `reconcile_restores_the_reference_an_interrupted_admission_left_unwritten`:
        D10 (a). Compare with the reference an uninterrupted run leaves
        in the same fixture.
      - `reconcile_writes_a_reference_for_every_hash_of_every_history`:
        D10 (g).
      - `reconcile_gives_restored_bytes_the_item_they_were_accepted_under`.
      - `reconcile_writes_no_reference_for_bytes_whose_record_links_no_item_and_reports_nothing`:
        D10 (h).
      - `reconcile_leaves_a_reference_whose_record_no_longer_links_an_item`.
      - `reconcile_rewrites_a_reference_whose_records_link_was_corrected_by_hand`.
      - `reconcile_repairs_every_reference_of_a_moved_library_historical_hashes_included`:
        D10 (d).
      - `reconcile_in_a_copy_beside_the_original_adds_a_second_reference`:
        D10 (b) step 3.
      - `reconcile_in_a_copy_after_the_original_is_deleted_repairs_rather_than_adds`:
        D10 (b) branch.
      - `reconcile_keeps_the_reference_of_a_deleted_copy`: D10 (b) step 6.
      - `reconcile_keeps_a_reference_whose_item_is_gone`: D10 (f).
      - `reconcile_over_a_library_with_no_identity_writes_no_reference_and_mints_none`.
      - `reconcile_whose_repair_minted_the_identity_writes_references_under_it`.
      - `reconcile_twice_writes_no_reference_the_second_time`.
      - `reconcile_reports_reference_writes_after_operator_restorations_and_before_its_totals`.
      - `reconcile_reference_writes_leave_every_store_file_byte_identical`.
      - `reconcile_whose_reference_write_fails_still_succeeds_and_reports_failed`,
        `#[cfg(unix)]`, over a read-only index root.

      In `crates/borax/tests/dispatch.rs`, the sequences across runs:
      - `two_copies_referenced_then_one_relinked_disagree_and_the_evidence_answers`:
        D10 (c).
      - `a_reference_whose_root_was_moved_is_missing_until_reconcile_repairs_it`:
        D10 (d), outside before and after.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_a_reference_write_matches_design_d7`.
      - `json_line_of_a_repair_carries_repaired_from`.
      - `json_line_of_a_reference_write_with_no_repair_has_no_repaired_from_key`.
      - `human_line_of_a_failed_reference_write_names_the_librarys_root`.
      - `human_line_of_a_written_reference_renders_nothing`.

      In `crates/borax/tests/binary.rs`, each setting `XDG_DATA_HOME` to
      a `tempdir`:
      - `reconcile_writes_references_under_xdg_data_home_borax_v2_content_refs`.
      - `resolve_outside_a_library_reads_a_reference_from_xdg_data_home`:
        the file is not a PDF and the binary has no transport, so only a
        reference can answer.
      - `cache_clear_leaves_the_content_index_so_a_reference_still_answers`.
- [ ] 5.2 Green:
      - `WrittenEntry::Reference`, and the failed-write human line;
      - the rename driver reads the identity and `keep`s after each
        admission it records, before `remember`;
      - `reconcile_events` reads the identity after its writes and
        `keep`s each of `references_owed`, after the operator
        restorations.

      (D2, D3, D7, D8.)

## 6. Adoption through references

Green point: `Adopted`, `Unadopted::Unanswered` and
`Adoption::Unanswered` change shape, with every consumer: `held_answer`,
`adopt`, `what_was_adopted` and the adoption tests D13 lists.

- [ ] 6.1 Red. Make the group-6 D13 edits, and the (b) rename of
      `adopt_writes_nothing_to_the_content_index`.

      In `crates/borax/tests/dispatch.rs`:
      - `adopt_records_an_orphan_from_a_reachable_reference_before_the_response_cache`.
      - `adopt_through_a_reference_reports_retrieval_library_reference`.
      - `adopt_through_a_reference_records_no_evidence_on_the_history_entry`.
      - `adopt_through_a_reference_after_the_cache_is_cleared_records_the_orphan`.
      - `adopt_with_disagreeing_references_uses_the_entrys_evidence`.
      - `adopt_with_references_none_answered_and_no_evidence_is_unanswered_naming_them_and_no_identifier`.
      - `adopt_with_evidence_no_cache_holds_and_an_unavailable_reference_is_unanswered_naming_both`.
      - `adopt_writes_a_reference_for_each_orphan_it_records`.
      - `adopt_writes_no_reference_for_an_orphan_it_leaves`.
      - `adopt_writes_no_reference_for_a_file_already_recorded`: the
        interrupted admission of D10 (a), then `adopt`.
      - `adopt_reports_reference_writes_after_the_adoptions_and_before_its_totals`.
      - `adopt_human_mode_names_an_unavailable_reference_on_an_unanswered_orphan`.

      In `crates/borax/tests/library.rs`:
      - `adopt_lists_the_reference_owed_for_each_orphan_it_records`.
      - `adopt_with_no_identifier_from_its_lookup_writes_no_evidence`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_library_adoption_recorded_through_a_reference`.
      - `json_line_of_library_adoption_unanswered_with_references_and_no_identifier`.
      - `human_line_of_library_adoption_unanswered_with_no_identifier_matches_design_d8`.
      - `human_line_of_library_adoption_unanswered_with_an_identifier_and_references_ends_with_the_clauses`.
- [ ] 6.2 Green:
      - `Adopted` and `Unadopted::Unanswered` reshaped, and
        `Adoptions::owed`;
      - `held_answer` settles the entry's references first;
      - `adopt` writes no evidence for a lookup with no identifier;
      - `adopt_events` reads the identity and `keep`s each owed
        reference, then the `missed` lookups, then the totals;
      - `Adoption::Unanswered` reshaped, and its human line.

      (D6, D7, D8.)

## 7. Audit and verification

- [ ] 7.1 Audit every edit to a pre-existing test against the preamble's
      rules and D13. Report any test that needed anything else.
- [ ] 7.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`. Run all
      three at the end of every group too, since each group is a green
      point.
- [ ] 7.3 Search `crates/*/src/` for what this change makes false. Each
      search must find nothing, or a hit the report justifies:
      - `refs/` described as reserved, or as written by nothing;
      - "evidence-first" in a docstring or comment;
      - `IndexReadStep::Hit`'s and `ContentIndexWrite`'s docstrings
        saying a hit always carries evidence, or that the event reports
        evidence writes alone;
      - `Adoption::Unanswered`'s docstring saying it always names an
        identifier;
      - `adopt`'s and `adopt_events`' docstrings saying adoption writes
        nothing to the content index;
      - `borax_core::index`'s module docstring naming two facts as the
        whole entry.
- [ ] 7.4 Run `openspec validate index-references-libraries --strict` and
      `python3 scripts/check-spec-deltas.py`.
- [ ] 7.5 By hand, over copies of a library built from the real-PDF
      corpus, which lives outside the repository:
      1. `borax reconcile` in the library, and check that `refs/`
         appears under the data directory for its files' hashes.
      2. `borax resolve` a copy of one file outside the library: it is
         answered `from the library at <root>`, with no request.
      3. `rsync` the library to a second root, `borax reconcile` there,
         and resolve the copy again: two references, one `agreed`.
      4. Move the second root, and resolve again: `missing`, and the
         first still answers.
      5. `borax reconcile` at the moved root: `repaired_from` names the
         old root.
      6. `borax cache --clear`, then `borax adopt` in a third library
         holding a copy: it is recorded through the reference.

      If this is not run, say so in the report.

## 8. Documents and state

- [ ] 8.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Keep every
      existing example name. Sources:
      - this change's design.md (D1–D8, D10, and the illustrative lines);
      - proposal.md;
      - the implemented `human_line` and `describe.rs`.

      Passages:
      - `* What a run does`: the order is the library, then a reference
        to another library, then the index's identifier, then
        extraction.
      - `** borax resolve`: the retrieval clause `from the library at
        <root>`, and the clauses for references that could not answer or
        disagreed.
      - `** borax rename`: an applying run writes a reference for each
        file it records, and the description's `reference` and
        `references` lines.
      - `** borax cache`: `cache --clear` keeps the references too.
      - `** borax reconcile`: it restores and repairs references for
        every hash, and keeps a deleted copy's reference.
      - `** borax adopt` and `** Choosing how to record existing files`:
        adoption through another library, before the response cache,
        and `unanswered` without an identifier.
      - `* Run logs`: `references` on `hit` and `miss`, the statuses and
        causes, `library-reference`, the `reference` entry kind with
        `repaired_from`, and the meanings D7 lists as changed.
- [ ] 8.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Extend the Unreleased entry from
      proposal.md and design.md. It covers:
      - references to libraries in the content index, and the order
        they answer in;
      - agreement and disagreement among several;
      - unavailable references kept and reported;
      - reconcile's restoration and repair;
      - adoption through another library;
      - the schema-5 additions and changed meanings, under Changed.
- [ ] 8.3 Orchestrator: read both diffs against the sources, for
      anything invented and for anything dropped from the old text.
      Check the `#+AI_PASS` line the doc writer adds. Answer or remove
      every `TODO(docs)`.
- [ ] 8.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change after the
        `reduce-content-index` paragraph;
      - in that paragraph, replace the closing sentence naming
        `index-references-libraries` as the change that follows;
      - in the `library-identity` paragraph, the sentence "Change 13
        must read the identity when it stores a reference rather than
        assume one" gains how it does, since
        `index-references-libraries`;
      - update "Last reviewed".
