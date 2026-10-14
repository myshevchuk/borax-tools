# Tasks: index-references-libraries

Work on branch `change/index-references-libraries`, cut from `main` at
`da6706d`. This is change 13 of the roadmap's Phase 4, and the third of
its four changes.

Groups 1 to 13 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D12 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.
- Write **one named test per bullet**. A bullet is not covered
  "implicitly" by an edited test or by another bullet's test.

Groups 1 to 7 change the index from records to evidence and give
history entries their evidence. Groups 8 to 11 add references. Groups 12
and 13 wire the real run and the operator's view. Each group leaves the
suite green, so the work can stop, or be split, at the boundary after
group 7.

These parts of the design fix the contract:

- where the index lives: D1;
- the entry and its storage seam: D2;
- evidence, origins and which write records what: D3, D4;
- writing references, restoring them, and repairing roots: D5, D6;
- what an entry answers, and reading a library: D7, D8;
- adoption: D9;
- the schema, the JSON shapes and the human strings: D10, D11, and the
  illustrative lines at the end of design.md;
- the Rust interface: D12;
- the sequences each group's tests trace: D13.

**Existing tests change.** Design D15 lists each one and the kind of
edit:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact.
  Six are also renamed, and D15 names them;
- **(c)** a deletion must be listed. None is.

Each group's red task makes the D15 edits for the types it changes, and
names them. Sweep the tests for every changed type:

- `HashEntry`, `Admitting`, `ContentIndex::put`/`get`, `IndexRead`,
  `RecordRetrieval`, `Origin`, `Unattempted`;
- `IndexReadStep`, `RetrievedFrom`, `IdentifierOrigin`,
  `Event::ContentIndexWrite`, `Adoption`, `SCHEMA`;
- `pipeline::remember`, `library::adopt`;
- every test that runs two resolutions over one index and expects the
  second to ask no service. Report each one found that D15 does not
  list, with the translation applied.

Task 14.1 audits every edit to an existing test.

**Fixtures.** Reuse these:

- `crates/borax-core/tests/library.rs`: `hash_entry`, the `UUID_*`
  constants, and the `hash_entry_item_round_trips_through_toml` test
  (near line 732) as the model for a history key's round trip;
- `crates/borax-sources/tests/store.rs`: `tempdir`, the `cache_root_*`
  tests as the model for `data_root_*`, and the
  `entries_survive_a_fresh_file_cache_opened_on_the_same_root` test;
- `crates/borax/tests/library.rs`:
  - `tempdir`, `write_file`, `snapshot`, `stat`;
  - `artifact_record`, `write_artifact_record`;
  - `write_item`, `item_with_doi`, `record_with_doi`, `minimal_record`;
  - `hash`, `hash_entry`, `item_id`, `artifact_id`;
  - `Extractor` and `reconciling`;
  - `UUID_A` and the other constants, `TIMESTAMP`;
- `crates/borax/tests/pipeline.rs`:
  - `FakeDocuments`, `pdf_with_embedded_doi`, `pdf_with_text_doi`,
    `pdf_with_no_identifier`;
  - `FakeSource`, `fake_source`, `PanicSource`;
  - `hash_for`, `record_with_doi`, `record_with_doi_and_title`,
    `config`;
  - `resolved_outcome`, `skipped_outcome`, `evidence_with`,
    `default_content_index`;
- `crates/borax/tests/dispatch.rs`:
  - `real_library`, `write_real_file`, `snapshot`;
  - `write_lib_item`, `write_lib_artifact_record`, `seed_artifact_record`,
    `seed_tracked_file`, `lib_item_id`, `lib_artifact_id`,
    `lib_hash_entry`, `hash_entry_for`, `lib_minimal_record`;
  - `adopt_over`, `adoptions`, `assert_every_orphan_is_accounted_for`,
    `outside_the_stores`;
  - `FakeDocuments`, `pdf_with_embedded_doi`, `fake_source`,
    `record_by`, `hash_for`;
  - `effective_with_default_template`, `fixed_now`;
  - `events_for`, and `dispatch` with `Streams`, `Session::batch`;
  - `RealFilesystem`, `FakeFilesystem`, `FakeBibFiles`;
- `crates/borax/tests/binary.rs`: `seed_tracked_binary_fixture`, and
  the `env_clear` runs that set `XDG_CONFIG_HOME` and `XDG_CACHE_HOME`.
  Every binary test that writes the index sets `XDG_DATA_HOME` to a
  `tempdir`, so no test reaches a real data directory;
- `crates/borax/tests/event.rs`: `all_events`, with `json_line` and
  `human_line`.

**Seeding.** An entry is seeded with `index.put(&hash, &IndexEntry {
… })`. A response-cache answer is seeded with
`FileCache::new(root).put(&cache::key(SourceName::Crossref, &id),
&record)` under a `tempdir` given as `Adapters::cache_root`. A
referenced library is a `real_library` (or a `tempdir` with a
`.borax.toml`) whose `.borax/library.toml` is written with
`LibraryIdentity { id }.to_toml()` and whose item is written with
`write_lib_item`.

**Read-only directories.** The `#[cfg(unix)]` tests that make a
reference write fail give `Adapters::index` a `ContentIndex` over a
`FileCache` whose root is a directory set to mode 0o555. They probe
that a file cannot be created there first, and return early saying why
when running as root. They restore 0o755 before the assertions, as
change 12's tests do.

**Design tests.** Each case below is a test, in the group named:

| Case | Group |
|---|---|
| origins round-trip; evidence on a history entry, both or neither; malformed evidence does not parse; `evidence_for` | 1 |
| the entry's JSON; the operator-evidence rule; the upsert's three steps, the evidence fill, no removal | 2 |
| `data_root` on each platform; entries over `FileCache` and `MemoryCache`; a `v1` entry is not read | 3 |
| schema 5; every new key and kind in JSON and in human lines | 4 |
| the sections projection of a hit, an index-held lookup, a reference retrieval; `identification()` | 5 |
| evidence written; an evidence answer looked up without a title check; `unresolvable`; `operator-evidence`; `remember` | 6 |
| evidence on admitted and reconciled entries; none for `--accept` and restatements | 7 |
| each reference outcome; agreement and disagreement | 8 |
| a reference answer; fallbacks; an untracked copy inside a library; the human clauses | 9 |
| references written after admission, adoption and reconcile; identity absent and faulty; order; repair; no pruning; failure | 10 |
| adoption from a reference, from the response cache, `unanswered`, `unindexed` | 11 |
| the real run's data root; `cache --clear` spares it | 12 |
| the description's lines; retry of an index-held lookup | 13 |

## 1. Identification evidence on history entries

- [ ] 1.1 Red, in `crates/borax-core/tests/library.rs`. Make the D15 (a)
      edit to `hash_entry`. New tests:
      - `evidence_origin_as_str_and_parse_round_trip_for_every_origin`:
        `extracted`, `supplied`, `overridden`, `adopted`.
      - `evidence_origin_parse_refuses_any_other_word`: `accepted`,
        `Extracted` and `""` are `None`.
      - `evidence_origin_is_operator_for_supplied_and_overridden_alone`.
      - `hash_entry_evidence_round_trips_through_toml_for_each_identifier_kind`:
        a DOI, an arXiv identifier, a PMID and an ISBN, each read back
        equal.
      - `hash_entry_evidence_is_written_after_item_as_identifier_then_origin`:
        checked on the rendered text.
      - `hash_entry_without_evidence_is_written_with_neither_key`.
      - `artifact_record_written_before_evidence_parses_with_none_and_renders_byte_identically`.
      - `hash_entry_with_an_identifier_and_no_origin_does_not_parse`.
      - `hash_entry_with_an_origin_and_no_identifier_does_not_parse`.
      - `hash_entry_with_an_unknown_origin_does_not_parse`.
      - `hash_entry_with_an_identifier_that_does_not_parse_does_not_parse`.
      - `evidence_for_is_the_newest_entry_holding_the_hash_that_carries_evidence`:
        `[A{doi:x}, B, A]` gives `doi:x` for A.
      - `evidence_for_is_none_when_no_entry_for_the_hash_carries_any`.
- [ ] 1.2 Green: `EvidenceOrigin`, `IdentificationEvidence`,
      `HashEntry::evidence` with its parse rule, and
      `ArtifactRecord::evidence_for` (D3, D12). In
      `crates/borax/src/library.rs`, `history_table` writes the two keys.
      Every `HashEntry` literal in `src` gains `evidence: None` for now.

## 2. The index entry as a value

- [ ] 2.1 Red, in a new `crates/borax-core/tests/index.rs`:
      - `index_entry_round_trips_through_json`.
      - `index_entry_with_no_evidence_and_no_references_is_written_as_an_empty_object`.
      - `index_entry_ignores_keys_it_does_not_know`.
      - `index_entry_with_an_unknown_origin_does_not_parse`.
      - `index_entry_with_a_reference_missing_its_item_does_not_parse`.
      - `with_evidence_sets_evidence_on_an_entry_that_has_none_and_keeps_references`.
      - `with_evidence_replaces_extracted_evidence_with_extracted_evidence`.
      - `with_evidence_replaces_extracted_evidence_with_an_operators`.
      - `with_evidence_replaces_an_operators_evidence_with_another_operators`.
      - `with_evidence_withholds_extracted_evidence_over_supplied_evidence`.
      - `with_evidence_withholds_extracted_evidence_over_overridden_evidence`.
      - `with_reference_appends_to_an_entry_holding_none`.
      - `with_reference_already_held_with_the_same_item_changes_nothing`:
        `changed` is `false`.
      - `with_reference_already_held_with_another_item_updates_only_the_item`.
      - `with_reference_repairs_the_first_stale_root_of_the_same_library_and_artifact`:
        `repaired_from` is the old root, and one reference remains.
      - `with_reference_appends_beside_a_root_that_is_not_stale`: two
        references afterwards.
      - `with_reference_does_not_ask_about_references_of_other_libraries_or_artifacts`:
        the `stale` stand-in records no call for them.
      - `with_reference_never_removes_a_stale_reference_when_the_new_one_is_already_held`.
      - `with_reference_fills_evidence_on_an_entry_holding_none`:
        `filled` is `true`.
      - `with_reference_keeps_the_evidence_an_entry_already_holds`.
- [ ] 2.2 Green: `borax_core::index` with `LibraryReference`,
      `IndexEntry` and `Referred` (D2, D4, D6, D12).

## 3. Where the index lives, and its storage

- [ ] 3.1 Red, in `crates/borax-sources/tests/store.rs`. Make the D15
      (b) rewrites there. New tests:
      - `data_root_honors_xdg_data_home_on_unix`.
      - `data_root_falls_back_to_home_dot_local_share_on_unix`.
      - `data_root_prefers_xdg_data_home_over_home_on_unix`.
      - `data_root_skips_an_empty_xdg_data_home_on_unix`.
      - `data_root_skips_a_relative_xdg_data_home_on_unix`.
      - `data_root_honors_appdata_on_windows`.
      - `data_root_falls_back_to_xdg_data_home_on_windows`.
      - `data_root_prefers_appdata_over_xdg_data_home_on_windows`.
      - `data_root_skips_an_empty_appdata_on_windows`.
      - `data_root_skips_a_relative_appdata_on_windows`.
      - `data_root_ends_in_borax_v2`.
      - `data_root_is_none_when_no_candidate_qualifies`.
      - `data_root_and_cache_root_differ_for_one_environment`: with
        every variable set, on the platform the test runs on.
      - `file_cache_stores_an_index_entry_at_content_hash_json`.
      - `file_cache_reads_an_entry_that_does_not_parse_as_none`.
      - `file_cache_index_write_replaces_an_entry_that_does_not_parse`.
      - `memory_cache_holds_index_entries_apart_from_records`: a record
        and an entry under the same key both read back.
      - `content_index_put_reports_failed_over_a_read_only_root`,
        `#[cfg(unix)]`.
      - `a_v1_record_under_the_cache_root_is_not_read_as_an_entry`: a
        `Record` written at `content/<hash>.json` under one root, and a
        `ContentIndex` over another root, reads `None`.
- [ ] 3.2 Green: `INDEX_FORMAT_VERSION`, `data_root`,
      `default_data_root`, `IndexStore` for `FileCache` and
      `MemoryCache`, and `ContentIndex` over it (D1, D2, D12).
      `FORMAT_VERSION`'s docstring stops naming the index.

## 4. The event vocabulary

- [ ] 4.1 Red, in `crates/borax/tests/event.rs`. Make the D15 (b) edits
      there (`schema_is_five`, `"schema":5`, the retrieval and
      `ContentIndexWrite` shapes). Extend `all_events` with one event
      for each new key and kind, so the round-trip tests cover them. New
      tests:
      - `json_line_of_a_hit_carries_evidence_and_references_in_order`.
      - `json_line_of_a_hit_with_no_evidence_omits_the_key_and_keeps_an_empty_references`.
      - `json_line_of_each_reference_status_and_cause_matches_design_d10`:
        `answered`, `agreed`, `disagreeing` with and without
        `identifier`, `unavailable` × `missing` / `other-library` with
        and without `found` / `unreadable`, and `item-unavailable` ×
        `dangling` / `unreadable` / `ambiguous`.
      - `json_line_of_a_lookup_from_the_content_index_has_origin_content_index`.
      - `json_line_of_a_library_reference_retrieval_matches_design_d10`.
      - `json_line_of_content_index_write_of_evidence_matches_design_d10`.
      - `json_line_of_content_index_write_of_a_reference_matches_design_d10`.
      - `json_line_of_content_index_write_of_a_repair_carries_repaired_from`.
      - `json_line_of_library_adoption_recorded_carries_its_retrieval`.
      - `json_line_of_library_adoption_unanswered_with_and_without_an_identifier`.
      - `json_line_of_operator_evidence_write_not_attempted`.
      - `human_line_of_a_reference_answer_ends_from_the_library_at_its_root`.
      - `human_line_of_an_unavailable_reference_appends_its_clause_for_each_cause`.
      - `human_line_of_disagreeing_references_appends_one_clause_naming_each`.
      - `human_line_puts_reference_clauses_after_the_library_clause_and_before_rejected_candidates`.
      - `human_line_of_a_root_with_a_control_character_is_escaped`.
      - `human_line_of_a_failed_reference_write_names_the_library_root`.
      - `human_line_of_a_failed_evidence_write_is_unchanged`.
      - `human_line_of_a_written_content_index_write_is_nothing`.
      - `human_line_of_library_adoption_unanswered_matches_design_d11`,
        with and without an identifier.

      Every expected string is D10's or D11's, exactly.
- [ ] 4.2 Green: `SCHEMA = 5`, `ReferenceOutcome` and its statuses,
      `IndexReadStep::Hit`'s fields, `IdentifierOrigin::ContentIndex`,
      `RetrievedFrom::LibraryReference` (and `ContentIndex` removed),
      `ContentIndexWrite`, `Adoption::Recorded::retrieval`,
      `Adoption::Unanswered`, and their human lines (D10, D11, D12).

## 5. The evidence engine

- [ ] 5.1 Red, in `crates/borax/tests/pipeline.rs`. Make the D15 (b)
      edits to the `sections_projects_*`, `prior_content_index_hit` and
      `evidence_via_*` tests. New tests:
      - `sections_projects_a_hit_with_its_evidence_and_reference_outcomes`.
      - `sections_projects_an_index_origin_lookup_as_content_index`.
      - `sections_projects_a_library_reference_retrieval`.
      - `retrieval_of_an_answering_reference_is_library_reference`.
      - `retrieval_of_an_index_origin_lookup_is_its_service`.
      - `unattempted_operator_evidence_as_str_is_operator_evidence`.
      - `identification_of_an_extracted_lookup_is_extracted`.
      - `identification_of_a_supplied_lookup_accepted_is_supplied`.
      - `identification_of_an_overridden_record_is_overridden`, for an
        extracted and for a supplied lookup.
      - `identification_of_an_index_origin_lookup_carries_the_index_origin`.
      - `identification_of_a_reference_answer_is_the_entrys_evidence`.
      - `identification_of_a_reference_answer_with_no_entry_evidence_is_none`.
      - `identification_of_a_library_answer_is_none`.
- [ ] 5.2 Green: `IndexRead::Hit(IndexHit)`, `Origin::Indexed`,
      `RecordRetrieval::LibraryReference`, `Unattempted::OperatorEvidence`,
      `Evidence::sections` and `Evidence::retrieval`, and
      `FileRecord::identification` (D3, D10, D12).

## 6. Resolution through evidence

- [ ] 6.1 Red, in `crates/borax/tests/pipeline.rs`. Make the D15 (b)
      rewrites there for the index's read and write and for `remember`.
      New tests:
      - `a_resolution_writes_its_lookups_identifier_as_extracted_evidence`.
      - `a_refused_record_writes_no_evidence`.
      - `an_evidence_hit_looks_up_its_identifier_without_opening_the_file`:
        `FakeDocuments::with_open_error`.
      - `an_evidence_hit_reports_lookup_origin_content_index_and_the_evidence_on_the_read`.
      - `an_evidence_hit_makes_no_title_check_for_any_origin`: one test
        over all four origins, each checked.
      - `an_overridden_evidence_hit_resolves_a_record_whose_title_conflicts_with_the_file`:
        the file's titles would conflict, and it resolves.
      - `an_evidence_hit_writes_no_evidence`.
      - `an_evidence_hit_no_service_holds_is_skipped_unresolvable_with_its_attempts`.
      - `an_evidence_hit_offline_is_skipped_unresolvable_and_the_entry_is_unchanged`.
      - `an_evidence_hit_is_answered_by_the_response_cache_without_a_request`:
        a `Cached` source over a `MemoryCache` holding the record,
        wrapping a `PanicSource`.
      - `a_hit_with_neither_answering_reference_nor_evidence_extracts_and_writes_evidence_beside_the_reference`.
      - `no_cache_over_supplied_evidence_does_not_write_and_reports_operator_evidence`.
      - `no_cache_over_extracted_evidence_replaces_it`.
      - `remember_writes_supplied_evidence_for_a_supplied_record`.
      - `remember_writes_overridden_evidence_for_a_record_accepted_over_its_conflict`.
      - `remember_keeps_the_references_an_entry_holds`.

      And in `crates/borax/tests/dispatch.rs`, with the D15 (b)
      rewrites of the supplied-identifier and adoption-free tests:
      - `a_supplied_identifier_renamed_outside_a_library_is_resolved_by_a_later_batch_run_without_extraction`.
      - `an_operators_answer_survives_a_no_cache_resolve`.
- [ ] 6.2 Green: `index_read` and `beyond_library` answer through the
      evidence, and the resolution write is of evidence under the
      operator-evidence rule. `remember(index, file)` (D4, D7). The
      rename driver passes the file record to `remember`.

## 7. Evidence on history entries the library writes

- [ ] 7.1 Red, in `crates/borax/tests/library.rs`. Make the D15 (a)
      `Admitting` edits (`evidence: None`, 37 sites) and the (b) edit to
      `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded`.
      New tests:
      - `admit_writes_the_evidence_it_is_given_on_a_minted_records_entry`.
      - `admit_writes_the_evidence_it_is_given_on_an_appended_entry`.
      - `admit_with_no_evidence_writes_an_entry_with_neither_key`.
      - `admit_writes_no_evidence_on_a_restatement`: change 15's Rule 2,
        and only the following entry carries it.
      - `reconcile_accepting_a_change_on_its_extracted_identifier_records_it_as_extracted`.
      - `reconcile_accepting_a_named_path_records_no_evidence`.
      - `reconcile_writes_no_evidence_onto_a_confirmed_or_repaired_record`:
        the record's history is unchanged.
      - `validate_reports_no_finding_for_entries_with_and_without_evidence`.
      - `validate_reports_a_record_with_an_unknown_origin_as_unreadable`.

      And in `crates/borax/tests/dispatch.rs`:
      - `an_applying_rename_records_the_extracted_identifier_on_the_new_entry`.
      - `an_interactive_rename_from_a_supplied_identifier_records_it_as_supplied`.
      - `an_interactive_rename_over_a_conflict_records_it_as_overridden`.
      - `an_applying_rename_answered_by_index_evidence_records_its_origin`:
        `supplied` evidence in the index gives a `supplied` entry.
- [ ] 7.2 Green: `Admitting::evidence`, written by `admit` and
      `settle_history`; reconcile's accepted-change entry; the rename
      driver and `adopt` pass `file.identification()` or their own (D3,
      D12).

## 8. Reading a referenced library

- [ ] 8.1 Red, in `crates/borax/tests/library.rs`, through
      `references::answer` with a fresh `Libraries`:
      - `a_reference_whose_root_does_not_exist_is_unavailable_missing`.
      - `a_reference_whose_root_is_a_file_is_unavailable_other_library_without_found`.
      - `a_reference_whose_root_has_no_identity_is_unavailable_other_library_without_found`.
      - `a_reference_whose_root_has_another_identity_is_unavailable_other_library_naming_it`.
      - `a_reference_whose_identity_file_does_not_parse_is_unavailable_unreadable_naming_it`.
      - `a_reference_whose_root_cannot_be_read_is_unavailable_unreadable`,
        `#[cfg(unix)]`, a root under a 0o000 parent.
      - `a_reference_whose_item_is_gone_is_item_unavailable_dangling`.
      - `a_reference_whose_item_file_does_not_parse_is_item_unavailable_unreadable`.
      - `a_reference_whose_item_two_files_carry_is_item_unavailable_ambiguous`.
      - `a_reachable_reference_answers_with_its_items_record`.
      - `a_reference_answers_with_its_own_item_not_the_records_current_link`:
        the record was re-linked by hand afterwards.
      - `two_reachable_references_carrying_one_doi_agree_and_the_first_answers`.
      - `two_reachable_references_carrying_different_dois_disagree_and_none_answers`.
      - `two_references_to_one_item_carrying_no_identifier_agree`.
      - `two_items_carrying_no_identifier_in_different_libraries_disagree`.
      - `one_reachable_reference_beside_unavailable_ones_answers`.
      - `answering_writes_nothing_to_the_referenced_library`: the
        snapshot is unchanged.
      - `a_libraries_reads_each_root_once`: a second answer after the
        item file is deleted still answers from the first reading.
- [ ] 8.2 Green: `references::Libraries` and `references::answer` (D7,
      D8, D12).

## 9. Resolution through references

- [ ] 9.1 Red, in `crates/borax/tests/pipeline.rs`, through `standing`
      with entries naming `tempdir` libraries:
      - `a_reference_answer_opens_nothing_and_asks_no_service`: a
        `PanicSource`.
      - `a_reference_answer_reports_library_reference_and_content_index_hit_for_every_later_step`.
      - `a_reference_answer_writes_nothing_to_the_index`.
      - `disagreeing_references_fall_back_to_the_entrys_evidence`.
      - `disagreeing_references_with_no_evidence_fall_back_to_extraction`.
      - `an_unavailable_reference_falls_back_to_the_evidence_and_is_reported`.
      - `a_reference_answer_goes_through_the_work_check`: with an
        `Account` holding the item's work, the verdict is a work
        duplicate.

      And in `crates/borax/tests/dispatch.rs`:
      - `resolve_outside_any_library_is_answered_by_a_library_reference`.
      - `resolve_of_an_untracked_copy_inside_the_library_is_answered_by_its_reference_and_reports_untracked`.
      - `resolve_of_a_tracked_file_reads_no_entry_whatever_its_references_say`:
        the entry names another library, and the run's library answers.
      - `resolve_human_line_names_a_missing_library_and_answers_from_the_other`.
      - `resolve_human_line_names_disagreeing_libraries`.
- [ ] 9.2 Green: `beyond_library` answers through `references::answer`
      before the evidence. `resolve_batch` and the rename driver hold a
      `Libraries` for the run, and `standing` uses a fresh one (D7, D8).

## 10. Writing references

- [ ] 10.1 Red, in `crates/borax/tests/dispatch.rs`. Make the D15 (b)
      edits to the `ContentIndexWrite` sequences. All through
      `events_for` or `dispatch` over `real_library`, with an identity
      written first unless the test says otherwise:
      - `an_applying_rename_writes_a_reference_after_the_record_naming_the_library_artifact_and_item`.
      - `an_applying_rename_reports_the_reference_after_renamed_and_any_admission`.
      - `a_supplied_rename_in_a_library_reports_the_reference_then_the_evidence`.
      - `an_applying_rename_over_a_recorded_file_restores_a_missing_reference`.
      - `an_applying_rename_over_a_referenced_file_writes_and_reports_nothing_for_it`.
      - `a_separated_file_is_referenced_under_its_new_artifact_and_the_old_reference_is_kept`.
      - `a_pending_change_writes_no_reference`.
      - `a_preview_writes_no_reference`.
      - `a_rename_with_no_record_writes_no_reference`.
      - `a_rename_into_a_library_with_no_identity_whose_admission_writes_nothing_writes_no_reference_and_warns_once`:
        two such files, one warning naming 2.
      - `a_rename_into_a_library_with_a_faulty_identity_writes_no_reference_and_warns_once`.
      - `a_failed_reference_write_reports_failed_and_the_rename_and_record_stand`,
        `#[cfg(unix)]`.
      - `a_reference_write_names_the_normalised_root_for_a_relative_library_root`.
      - `reconcile_restores_the_reference_of_every_record`.
      - `reconcile_restores_a_reference_with_the_records_evidence`.
      - `reconcile_twice_writes_and_reports_nothing_the_second_time`.
      - `reconcile_refreshes_a_references_item_after_a_hand_relink`.
      - `reconcile_repairs_a_reference_to_a_moved_library`: the
        `repaired_from` event.
      - `reconcile_in_a_copy_adds_a_second_reference_beside_the_original`.
      - `reconcile_keeps_a_reference_to_a_deleted_copy`.
      - `reconcile_leaves_a_reference_to_an_unreadable_root_alone`,
        `#[cfg(unix)]`.
      - `reconcile_over_a_library_with_no_identity_writes_no_reference_mints_none_and_warns`.
      - `reconcile_reports_reference_writes_before_its_totals`.
      - `status_validate_resolve_and_bib_write_no_reference`.
      - `an_interrupted_admission_is_referenced_by_the_next_reconcile`:
        seed an item and record as `admit` writes them, with no entry.
        The reconcile writes it, and the record is byte-identical.

      And in `crates/borax/tests/library.rs`:
      - `restore_with_an_identity_writes_one_reference_per_record`.
      - `restore_with_no_identity_reports_how_many_were_withheld`.
      - `unreferenced_warning_matches_design_d11`.
- [ ] 10.2 Green: `references::refer` and `references::restore`.
      `recorded` writes a reference after each `Ok` admission.
      `reconcile_events` restores after the pass. `Aftermath::unreferenced`,
      and `dispatch` writes it after `Aftermath::identity` (D5, D6, D11).

## 11. Adoption

- [ ] 11.1 Red, in `crates/borax/tests/dispatch.rs`. Make the D15 (b)
      edits to the `adopt_*` tests. New tests:
      - `adopt_records_an_orphan_from_a_reachable_reference_with_no_response_cache`.
      - `adopt_from_a_reference_records_the_items_identifier_as_adopted`.
      - `adopt_records_an_orphan_from_the_response_cache_by_the_entrys_identifier`.
      - `adopt_reads_the_response_cache_in_priority_order_whatever_the_sources_setting`:
        `sources` names only OpenAlex, and the Crossref answer is the one
        recorded.
      - `adopt_never_queries_a_service`: `PanicSource`.
      - `adopt_with_disagreeing_references_uses_the_evidence`.
      - `adopt_with_only_unavailable_references_and_no_evidence_is_unanswered_with_their_outcomes`.
      - `adopt_with_evidence_the_response_cache_does_not_hold_is_unanswered_naming_the_identifier`.
      - `adopt_with_no_entry_is_unindexed`.
      - `adopt_references_each_adopted_orphan_after_the_pass`: the
        `content-index-write` events follow the `library-adoption`
        events and precede the totals.
      - `adopt_after_the_cache_is_cleared_records_what_a_reference_answers`.
      - `adopt_human_mode_lists_an_unanswered_orphan_before_the_totals_line`.

      And in `crates/borax/tests/library.rs`, with the D15 (b) edits to
      the four `library::adopt(` calls:
      - `adopt_writes_the_lookups_evidence_on_the_adopted_entry`.
      - `adopt_reports_unanswered_as_the_lookup_says`.
- [ ] 11.2 Green: `references::adoptable`, `adopt`'s `lookup` returning
      `Result<Adopted, Unadopted>`, `Adoption::Recorded::retrieval`,
      `Adoption::Unanswered`, and `adopt_events` writing references
      (D9).

## 12. The real run

- [ ] 12.1 Red, in `crates/borax/tests/binary.rs`. Make the D15 (b)
      rewrite of `seed_stale_content_index`. The binary has no fake
      transport, so none of these may reach a lookup: each is answered by
      a library or a reference, or ends before a lookup. New tests:
      - `reconcile_writes_references_under_xdg_data_home_borax_v2_content`:
        a library with an identity, an item and a record, seeded with
        `seed_tracked_binary_fixture`.
      - `reconcile_writes_nothing_under_the_cache_directory`.
      - `resolve_outside_a_library_reads_a_reference_from_xdg_data_home`:
        an entry seeded under `XDG_DATA_HOME/borax/v2/content/`, naming a
        seeded library; `record_retrieval.kind` is `library-reference`.
      - `cache_clear_leaves_the_content_index_so_a_reference_still_answers`.
      - `a_v1_entry_in_the_cache_directory_is_not_read`: a `v1` record
        seeded under `XDG_CACHE_HOME` for a file that is not a PDF. The
        file is skipped as unreadable, not resolved from that record.
      - `a_run_with_no_data_directory_writes_no_entry_and_succeeds`:
        `borax reconcile` with `env_clear` and `XDG_CACHE_HOME` alone.
- [ ] 12.2 Green: `execute` opens the index over `default_data_root()`,
      or a `MemoryCache` when there is none. `Adapters<S: IndexStore>`
      (D1).

## 13. The operator's view

- [ ] 13.1 Red, in `crates/borax/tests/describe.rs`. Make the D15 (b)
      edits. New tests:
      - `an_evidence_answer_names_its_identifier_as_found_by_an_earlier_run`.
      - `an_evidence_answer_names_a_supplied_identifier_as_supplied_in_an_earlier_run`.
      - `an_evidence_answer_names_an_overridden_identifier_as_accepted_over_a_title_conflict`.
      - `an_evidence_answer_names_an_adopted_identifier_as_adopted_in_an_earlier_run`.
      - `a_reference_answer_names_the_records_identifier_with_no_origin`.
      - `a_reference_answer_record_line_names_its_services_and_the_library_root`.
      - `a_reference_answer_with_no_services_names_the_library_root_alone`.

      And in `crates/borax/tests/dispatch.rs`:
      - `retry_of_an_index_held_lookup_after_an_outage_keeps_the_content_index_origin_and_its_rounds`.
- [ ] 13.2 Green: `describe.rs`'s identifier and record lines, and the
      interactive driver's retry of the file's own lookup whatever its
      origin (D7, D11).

## 14. Audit and verification

- [ ] 14.1 Audit every edit to a pre-existing test against the
      preamble's rules and D15. Report any test that needed anything
      else, and every test found by the two-run sweep.
- [ ] 14.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`.
- [ ] 14.3 Search `crates/*/src/` for what this change makes false. Each
      search must find nothing, or a hit the report justifies:
      - `RetrievedFrom::ContentIndex` and `RecordRetrieval::ContentIndex`;
      - `the content index` in a human string as a place a record came
        from;
      - `index.put(` given a `Record`;
      - "the content index holds the record" and "served from the index"
        in docstrings;
      - `FORMAT_VERSION`'s "Entries are serialized [`Record`]s" naming
        the index;
      - `cache::clear`'s docstring, if it says the index is cleared;
      - `Adoption::Unindexed`'s docstring saying "no record".
- [ ] 14.4 Run `openspec validate index-references-libraries --strict`
      and `python3 scripts/check-spec-deltas.py`.
- [ ] 14.5 By hand, over copies of two libraries built from the
      real-PDF corpus, which lives outside the repository:
      1. Run `borax rename --apply` in library A, so references are
         written.
      2. Run `borax resolve` over a copy of one file outside both
         libraries: it is answered from A.
      3. `rsync` A to A′, then run `borax reconcile` in A′: there are two
         references.
      4. Correct an item's DOI in A′, then resolve the copy: the
         references disagree and the evidence is looked up.
      5. Remove A, then resolve the copy: A is missing and A′ answers.
      6. Run `borax cache --clear`, then `borax adopt` in a third library
         holding the file: it is recorded from A′.

      If this is not run, say so in the report.

## 15. Documents and state

- [ ] 15.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Keep every
      existing example name. Sources: this change's design.md (D1–D11,
      D13 and the illustrative lines), proposal.md, and the implemented
      `human_line`, `describe.rs` and `unreferenced_warning`. Passages:
      - `* What a run does`: the content index holds identifiers and
        references, the library answers first, then a reference, then
        the identifier's lookup.
      - `** borax resolve`: `the library at <root>` in the retrieval
        clause, and the reference clauses.
      - `** borax rename`'s interactive passages on remembering an
        answer (the paragraphs near "A later interactive or batch run
        can use that"): where the answer is kept, inside and outside a
        library, and the reference's `content-index-write`.
      - `** borax cache`: it clears the response cache only. Say where
        the content index lives on each platform.
      - `** borax reconcile`: it restores and repairs references.
      - `** borax adopt` and `** Choosing how to record existing files`:
        D9, `unanswered`, and that `cache --clear` no longer empties
        what adoption reads from libraries.
      - The `cache` setting under `** The settings` (the sentence that
        it "also bypasses the content index"): it skips reading the
        index, and does not replace an operator's identifier.
      - `* Run logs`: schema 5, `content_index` `hit`'s fields, the
        reference outcomes, `library-reference`, `lookup.origin`
        `content-index`, `content-index-write`'s new fields, and the
        adoption kinds.
- [ ] 15.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Write the Unreleased entry from
      proposal.md and design.md. It covers:
      - the index's new home and its reduced entries;
      - that `v1` entries are not read and `cache --clear` removes
        them with the responses;
      - references and what answers through them;
      - evidence on history entries;
      - adoption's new sources and kinds;
      - the warning;
      - the bump to schema 5, with the released values that changed,
        under Changed.
- [ ] 15.3 Orchestrator: read both diffs against the sources, for
      anything invented and for anything dropped from the old text.
      Check the `#+AI_PASS` line the doc writer adds. Answer or remove
      every `TODO(docs)`.
- [ ] 15.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change after the `library-identity`
        paragraph;
      - in the `supply-identifiers-interactively` paragraph, the
        sentence "A record accepted this way is written to the content
        index on the move" gains "as its identifier, since
        `index-references-libraries`";
      - in "Not built yet", the title-search bullet's "the content-index
        memory — a chosen candidate is remembered exactly as a supplied
        identifier is" stays true; check it;
      - add the Windows state/cache root collision to "Known defects";
      - update "Last reviewed".
