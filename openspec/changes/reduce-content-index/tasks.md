# Tasks: reduce-content-index

Work on branch `change/reduce-content-index`, cut from `main` at
`da6706d`. This is change 13a of the roadmap's Phase 4. 13b
(`index-references-libraries`) follows it.

Groups 1 to 7 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D10 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.
- Write **one named test per bullet**. A bullet is not covered
  "implicitly" by an edited test or by another bullet's test.

**Green points.** Every group ends with the workspace compiling and
`cargo test --workspace` green. Each type change lands in the same pair
as every consumer of that type (design D12). Two interim states hold
between groups. Both are closed within this change, and neither is a
stopping point:

- After group 3 and before group 4, a real `adopt` still reads the
  record API, which nothing writes any more.
- After group 3 and before group 6, the real binary keeps the index's
  facts under the cache root.

These parts of the design fix the contract:

- where the index lives: D1;
- the entry, its files, writers, readers and precedence: D2;
- evidence, origins, which write records what, and the append rule:
  D3;
- when the index's evidence is written: D4;
- recovery: D5;
- what an entry answers: D6;
- adoption: D7;
- the schema, the JSON shapes and the human strings: D8, D9, and the
  illustrative lines at the end of design.md;
- the Rust interface: D10;
- the sequences each group's tests trace: D11;
- the group boundaries: D12.

**Existing tests change.** Design D14 lists each one, by group, with the
kind of edit:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact.
  D14 names the ones that are renamed;
- **(c)** a deletion must be listed. None is.

Each group's red task makes that group's D14 edits and names them. D14
says how the sweep was made. A test that a group's change breaks and
D14 does not list is reported back, with the translation applied.

Task 8.1 audits every edit to an existing test.

**Fixtures.** Reuse these:

- `crates/borax-core/tests/library.rs`: `hash_entry`, the `UUID_*`
  constants, and `hash_entry_item_round_trips_through_toml` (near line
  732) as the model for a history key's round trip;
- `crates/borax-sources/tests/store.rs`: `tempdir`, the `cache_root_*`
  tests as the model for `data_root_*`, and
  `entries_survive_a_fresh_file_cache_opened_on_the_same_root`;
- `crates/borax/tests/library.rs`:
  - `tempdir`, `write_file`, `snapshot`, `stat`;
  - `artifact_record`, `write_artifact_record`;
  - `write_item`, `item_with_doi`, `record_with_doi`, `minimal_record`;
  - `hash`, `hash_entry`, `item_id`, `artifact_id`;
  - `Extractor` and `reconciling`;
  - the `UUID_*` constants and `TIMESTAMP`;
- `crates/borax/tests/pipeline.rs`:
  - `FakeDocuments`, `pdf_with_embedded_doi`, `pdf_with_text_doi`,
    `pdf_with_no_identifier`;
  - `FakeSource`, `fake_source`, `PanicSource`, `CountingCache`;
  - `hash_for`, `record_with_doi`, `record_with_doi_and_title`,
    `config`;
  - `resolved_outcome`, `skipped_outcome`, `evidence_with`,
    `default_content_index`;
- `crates/borax/tests/dispatch.rs`:
  - `real_library`, `write_real_file`, `snapshot`;
  - `write_lib_item`, `write_lib_artifact_record`, `seed_artifact_record`,
    `seed_tracked_file`, `lib_item_id`, `lib_artifact_id`,
    `lib_hash_entry`, `hash_entry_for`, `lib_minimal_record`;
  - `adopt_over`, `adoptions`, `assert_every_orphan_is_accounted_for`;
  - `FakeDocuments`, `pdf_with_embedded_doi`, `pdf_with_no_identifier`,
    `fake_source`, `KeyedSource`, `record_by`, `hash_for`;
  - `ScriptedAsker` and `Answer`;
  - `effective_with_default_template`, `fixed_now`;
  - `events_for`, and `dispatch` with `Streams`, `Session::batch`,
    `Session::interactive`;
  - `RealFilesystem`, `FakeFilesystem`, `FakeBibFiles`;
- `crates/borax/tests/binary.rs`: `seed_tracked_binary_fixture`, and the
  `env_clear` runs that set `XDG_CONFIG_HOME` and `XDG_CACHE_HOME`.
  Every binary test that can write the index also sets `XDG_DATA_HOME`
  to a `tempdir`, so no test reaches a real data directory;
- `crates/borax/tests/event.rs`: `all_events`, with `json_line` and
  `human_line`.

**Seeding.**

- An index fact is seeded with `index.put_extracted(&hash, &id)` or
  `index.put_operator(&hash, &evidence)`.
- An unreadable fact is seeded by writing bad text at
  `entry_path(root, &operator_key(&hash))` under a `FileCache` root.
- A response-cache answer is seeded with
  `FileCache::new(root).put(&cache::key(SourceName::Crossref, &id),
  &record)` under a `tempdir` given as `Adapters::cache_root`.

`s(x)` below is `IdentificationEvidence { identifier: x, origin:
Supplied }`.

## 1. Evidence on history entries, and the append rule

Green point: `HashEntry` and `Admitting` change type here, and every
literal of both, in `src` and in tests, is edited in this pair. Nothing
else changes type.

- [ ] 1.1 Red. Make the group-1 D14 edits: the (a) `evidence: None`
      sites, and the (b) rewrites of
      `admit_reidentifying_the_newest_bytes_moves_the_link_and_appends_nothing`,
      `transition_f_relinking_the_newest_bytes_appends_nothing_until_later_bytes_restate_it`,
      `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded`
      and `a_reidentified_newest_version_moves_the_link_and_appends_nothing`,
      under their new names.

      In `crates/borax-core/tests/library.rs`:
      - `evidence_origin_as_str_and_parse_round_trip_for_every_origin`.
      - `evidence_origin_parse_refuses_any_other_word`: `accepted`,
        `Extracted` and `""` are `None`.
      - `evidence_origin_is_operator_for_supplied_and_overridden_alone`.
      - `hash_entry_evidence_round_trips_through_toml_for_each_identifier_kind`:
        a DOI, an arXiv identifier, a PMID and an ISBN.
      - `hash_entry_evidence_is_written_after_item_as_identifier_then_origin`:
        checked on the rendered text.
      - `hash_entry_without_evidence_is_written_with_neither_key`.
      - `artifact_record_written_before_evidence_parses_with_none_and_renders_byte_identically`.
      - `hash_entry_with_an_identifier_and_no_origin_does_not_parse`.
      - `hash_entry_with_an_origin_and_no_identifier_does_not_parse`.
      - `hash_entry_with_an_unknown_origin_does_not_parse`.
      - `hash_entry_with_an_identifier_that_does_not_parse_does_not_parse`.
      - `evidence_for_is_the_newest_entry_holding_the_hash_that_carries_evidence`.
      - `evidence_for_is_none_when_no_entry_for_the_hash_carries_any`.
      - `operator_evidence_for_skips_extracted_and_adopted_entries`:
        `[A(x extracted), A(s(y)), A(adopted z)]` gives `s(y)` for A.
      - `operator_evidence_for_is_none_with_no_operator_entry`.

      In `crates/borax/tests/library.rs`:
      - `admit_writes_the_evidence_it_is_given_on_a_minted_records_entry`.
      - `admit_writes_the_evidence_it_is_given_on_an_appended_entry`.
      - `admit_with_no_evidence_writes_an_entry_with_neither_key`.
      - `admit_reidentifying_the_newest_bytes_with_the_same_evidence_appends_nothing`:
        `[A(Y, s(y))]`, link Y, re-identified from `s(y)`.
      - `admit_reidentifying_the_newest_bytes_as_their_own_item_with_new_operator_evidence_appends_one_entry`:
        `[A(X, x)]`, link X, re-identified as X from `s(x2)`.
      - `admit_reidentifying_restored_bytes_with_evidence_still_appends_as_before`:
        change 15's sequence (a) with `s(x)`. B still answers Y.
      - `admit_writes_no_evidence_on_a_restatement`: Rule 2, where only
        the following entry carries the evidence.
      - `an_ordinary_rerun_with_extracted_evidence_appends_nothing`.
      - `reconcile_accepting_a_change_on_its_extracted_identifier_records_it_as_extracted`.
      - `reconcile_accepting_a_named_path_records_no_evidence`.
      - `validate_reports_no_finding_for_entries_with_and_without_evidence`.
      - `validate_reports_a_record_with_an_unknown_origin_as_unreadable`.

      In `crates/borax/tests/dispatch.rs`:
      - `an_applying_rename_records_the_extracted_identifier_on_the_new_entry`.
      - `an_interactive_rename_from_a_supplied_identifier_records_it_as_supplied`.
      - `an_interactive_rename_over_a_conflict_records_it_as_overridden`.
      - `supplying_the_same_identifier_twice_for_a_tracked_file_appends_one_entry`.
- [ ] 1.2 Green:
      - `EvidenceOrigin`, `IdentificationEvidence`, `HashEntry::evidence`
        and its parse rule;
      - `ArtifactRecord::evidence_for` and `operator_evidence_for`;
      - `history_table`'s two keys;
      - `Admitting::evidence`, and `settle_history`'s evidence
        condition;
      - reconcile's accepted-change entry;
      - `FileRecord::identification` for today's origins, passed by the
        rename driver. `adopt` passes `None` until group 4.

      (D3, D10.)

## 2. The evidence store, beside the record API

Green point: purely additive. `ContentIndex`'s evidence API is a new
`impl<S: IndexStore>` block next to the record API. No existing
consumer changes.

- [ ] 2.1 Red.

      In a new `crates/borax-core/tests/index.rs`:
      - `extracted_fact_round_trips_through_json_for_each_identifier_kind`.
      - `extracted_fact_is_written_as_its_identifier_alone`.
      - `operator_fact_round_trips_for_supplied_and_overridden`.
      - `operator_fact_with_origin_extracted_or_adopted_does_not_parse`.
      - `a_fact_ignores_keys_it_does_not_know`.
      - `answer_is_the_operators_evidence_when_both_are_held`.
      - `answer_is_the_extracted_identifier_when_only_it_is_held`.
      - `answer_is_none_when_operator_is_unreadable_even_beside_an_extracted_identifier`.
      - `answer_is_the_operators_when_extracted_is_unreadable`.
      - `answer_is_none_when_both_are_absent`.

      In `crates/borax-sources/tests/store.rs`:
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
      - `data_root_and_cache_root_differ_for_one_environment`.
      - `extracted_and_operator_keys_name_files_in_the_hashs_directory`:
        `entry_path` gives `content/<hash>/extracted.json` and
        `…/operator.json`.
      - `put_extracted_writes_only_extracted_json`: the directory holds
        that one file.
      - `put_operator_writes_only_operator_json`.
      - `put_operator_refuses_evidence_of_a_non_operator_origin_without_writing`.
      - `put_extracted_leaves_an_existing_operator_json_byte_identical`.
      - `put_operator_leaves_an_existing_extracted_json_byte_identical`.
      - `a_later_operator_write_replaces_the_earlier_one`.
      - `entry_of_an_absent_directory_has_both_facts_absent`.
      - `entry_ignores_a_temporary_file_in_the_directory`.
      - `entry_reports_an_operator_json_that_does_not_parse_as_unreadable`.
      - `memory_cache_holds_facts_apart_from_records`: a record under
        `content/<hash>` and a fact under `content/<hash>/extracted`
        both read back.
      - `content_index_fact_write_reports_failed_over_a_read_only_root`,
        `#[cfg(unix)]`.
      - `a_v1_record_file_beside_the_entry_is_not_read_as_a_fact`.
- [ ] 2.2 Green: `borax_core::index`; `INDEX_FORMAT_VERSION`,
      `data_root`, `default_data_root`, `extracted_key`, `operator_key`;
      `IndexStore` for `FileCache` and `MemoryCache`; and
      `ContentIndex::entry`, `put_extracted` and `put_operator` (D1, D2,
      D10).

## 3. Resolution through evidence

Green point: every consumer of `IndexRead`, `RecordRetrieval`,
`Origin`, `IndexReadStep`, `RetrievedFrom`, `IdentifierOrigin`,
`ContentIndexWrite` and `SCHEMA` changes in this pair. That covers
pipeline, evidence, event, describe and run, and their tests. `Adapters`
becomes `S: Cache + IndexStore`, and every custom test store gains an
`IndexStore` impl. `adopt` still uses the record API, which still
exists.

- [ ] 3.1 Red. Make the group-3 D14 edits ((a) store impls and schema
      literals, and (b) rewrites).

      In `crates/borax/tests/pipeline.rs`:
      - `a_resolution_writes_its_lookups_identifier_as_extracted_evidence`.
      - `a_refused_record_writes_no_evidence`.
      - `a_resolution_under_no_cache_writes_extracted_evidence_and_leaves_operator_evidence`.
      - `an_evidence_hit_looks_up_its_identifier_without_opening_the_file`:
        `FakeDocuments::with_open_error`.
      - `an_evidence_hit_reports_lookup_origin_content_index_and_the_evidence_on_the_read`.
      - `an_operator_fact_answers_before_an_extracted_one`.
      - `an_evidence_hit_makes_no_title_check_for_any_origin`, over
        `extracted`, `supplied` and `overridden`.
      - `an_overridden_evidence_hit_resolves_a_record_whose_title_conflicts_with_the_file`.
      - `an_evidence_hit_writes_no_evidence`.
      - `an_evidence_hit_no_service_holds_is_skipped_unresolvable_and_not_extracted`.
      - `an_evidence_hit_offline_is_skipped_unresolvable_and_the_entry_is_unchanged`.
      - `an_evidence_hit_is_answered_by_the_response_cache_without_a_request`:
        a `Cached` source over a `MemoryCache` holding the record,
        wrapping a `PanicSource`.
      - `an_unreadable_operator_fact_reads_as_a_miss_and_extracts`.
      - `a_late_extraction_write_erases_no_operator_evidence`: miss, then
        `put_operator`, then the first resolution's write; both facts
        are held, and the next resolution looks up the operator's.
      - `remember_writes_supplied_evidence_for_a_supplied_record`.
      - `remember_writes_overridden_evidence_for_a_record_accepted_over_its_conflict`.
      - `remember_leaves_extracted_evidence_byte_identical`.
      - `identification_of_an_index_origin_lookup_carries_the_answering_facts_origin`.
      - `sections_projects_a_hit_with_its_evidence`.
      - `retrieval_of_an_index_origin_lookup_is_its_service`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_a_hit_carries_its_evidence`.
      - `json_line_of_a_lookup_from_the_content_index_has_origin_content_index`.
      - `json_line_of_content_index_write_matches_design_d8`.
      - `human_line_of_an_evidence_answer_ends_from_the_response_cache`.
      - `human_line_of_a_failed_content_index_write_is_unchanged`.

      In `crates/borax/tests/describe.rs`:
      - `an_evidence_answer_names_its_identifier_as_found_by_an_earlier_run`.
      - `an_evidence_answer_names_a_supplied_identifier_as_supplied_in_an_earlier_run`.
      - `an_evidence_answer_names_an_overridden_identifier_as_accepted_over_a_title_conflict`.

      In `crates/borax/tests/dispatch.rs`:
      - `a_supplied_identifier_renamed_outside_a_library_is_resolved_by_a_later_batch_run_without_extraction_or_question`.
      - `an_operators_answer_survives_a_no_cache_resolve`.
      - `an_interactive_supplied_rename_in_a_library_writes_the_operator_fact_after_the_record`:
        a recording write stand-in that shares an ordered log with the
        index store.
- [ ] 3.2 Green:
      - `index_read` and `beyond_library` answer by evidence, and the
        resolution write is `put_extracted`;
      - `remember(index, file)` and `FileRecord::identification` for
        `Origin::Indexed`;
      - `IndexRead::Hit(evidence)`, `Origin::Indexed`, and
        `RecordRetrieval::ContentIndex` removed;
      - `SCHEMA = 5`, `IndexReadStep::Hit`, `IdentifierOrigin::ContentIndex`,
        `RetrievedFrom::ContentIndex` removed, `ContentIndexWrite`'s
        `hash` and `entry`, and the human lines;
      - `describe.rs`;
      - `Adapters<S: Cache + IndexStore>`, and `IndexStore` for `run.rs`'s
        store enum, still over the cache root.

      (D2, D4, D6, D8, D9.)

## 4. Adoption from evidence

Green point: the record API's last consumers, `adopt_events` and the
adoption tests, change here. The record API is removed, and `Adapters`
becomes `S: IndexStore`.

- [ ] 4.1 Red. Make the group-4 D14 edits. New tests in
      `crates/borax/tests/dispatch.rs`:
      - `adopt_records_an_orphan_from_the_response_cache_by_the_entrys_identifier`.
      - `adopt_uses_the_operators_identifier_before_the_extracted_one`.
      - `adopt_reads_the_response_cache_in_priority_order_whatever_the_sources_setting`:
        `sources` names only OpenAlex, and the Crossref answer is
        recorded.
      - `adopt_never_queries_a_service`: `PanicSource`.
      - `adopt_records_the_identifier_with_origin_adopted`.
      - `adopt_writes_nothing_to_the_content_index`: the index store
        records no write.
      - `adopt_with_evidence_the_response_cache_does_not_hold_is_unanswered_naming_the_identifier`.
      - `adopt_with_no_entry_is_unindexed`.
      - `adopt_with_an_unreadable_operator_fact_is_unindexed`.
      - `adopt_human_mode_lists_an_unanswered_orphan_before_the_totals_line`.

      In `crates/borax/tests/library.rs`:
      - `adopt_writes_the_lookups_identifier_as_adopted_on_the_adopted_entry`.
      - `adopt_reports_unanswered_as_the_lookup_says`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_library_adoption_recorded_carries_its_retrieval`.
      - `json_line_of_library_adoption_unanswered_matches_design_d8`.
      - `human_line_of_library_adoption_unanswered_matches_design_d9`.
- [ ] 4.2 Green: the `lookup` seam returns `Result<Adopted, Unadopted>`;
      `adopt_events` reads the response cache under `cache_root`;
      `Adoption::Recorded::retrieval` and `Adoption::Unanswered`;
      `ContentIndex`'s record API removed; `Adapters<S: IndexStore>`
      (D7, D8, D10).

## 5. Recovery: reconcile restores operator evidence

Green point: additive behaviour through the event shape group 3 fixed.

- [ ] 5.1 Red.

      In `crates/borax/tests/library.rs`:
      - `operator_decisions_lists_each_hashs_newest_operator_evidence_with_the_records_path`.
      - `operator_decisions_skips_hashes_with_no_operator_entry`.
      - `operator_decisions_takes_the_record_read_last_when_two_disagree`.

      In `crates/borax/tests/dispatch.rs`, through `events_for`:
      - `an_interrupted_operator_admission_is_completed_by_the_next_reconcile`:
        the critic's sequence (D5). Seed `extracted` X, and a record
        whose newest entry is `(H, I_Y, s(Y))`, with no `operator.json`.
        After reconcile, `operator` is `s(Y)` and `extracted` is still
        X. This equals the facts an uninterrupted interactive run leaves
        in the same fixture, which the test also builds and compares.
      - `reconcile_restores_operator_evidence_for_a_restored_earlier_hash`.
      - `reconcile_replaces_a_different_operator_fact_with_the_librarys`.
      - `reconcile_rewrites_an_unreadable_operator_fact`.
      - `reconcile_writes_nothing_for_bytes_with_no_operator_entry_and_leaves_their_operator_fact`.
      - `reconcile_twice_writes_and_reports_nothing_the_second_time`.
      - `reconcile_reports_each_restoration_before_its_totals`.
      - `reconcile_restoration_leaves_every_store_file_byte_identical`.
      - `reconcile_whose_restoration_fails_still_succeeds_and_reports_failed`,
        `#[cfg(unix)]`, over a read-only index root.
      - `reconcile_restores_after_the_index_is_deleted`.
      - `status_and_validate_write_nothing_to_the_content_index`.
- [ ] 5.2 Green: `library::operator_decisions`, and `reconcile_events`
      writing `put_operator` where `entry(hash).operator` is not
      `Held(e)`, reporting each write (D5).

## 6. The real run

Green point: one call site and the binary tests. The binary has no fake
transport, so no test here reaches a lookup.

- [ ] 6.1 Red, in `crates/borax/tests/binary.rs`. Make the D14 (b)
      rewrite of `seed_stale_content_index`. New tests:
      - `reconcile_writes_operator_evidence_under_xdg_data_home_borax_v2_content`:
        a library seeded with a record whose entry is `s(doi)`.
      - `reconcile_writes_nothing_under_the_cache_directory`.
      - `cache_clear_leaves_the_content_index`: after a restoration,
        `borax cache --clear`, and the fact file is byte-identical.
      - `a_v1_entry_in_the_cache_directory_is_not_read`: a `v1` record
        under `XDG_CACHE_HOME` for a file that is not a PDF. The file is
        skipped as unreadable, not resolved from the record.
      - `a_run_with_no_data_directory_writes_no_fact_and_succeeds`:
        `borax reconcile` with `env_clear` and `XDG_CACHE_HOME` alone.
- [ ] 6.2 Green: `execute` opens the index over `default_data_root()`,
      or a `MemoryCache` when there is none (D1).

## 7. Retry of an index-held lookup

Green point: additive. Until this lands, Retry on such a lookup does
nothing, as Retry with no own identifier does today.

- [ ] 7.1 Red, in `crates/borax/tests/dispatch.rs`:
      - `retry_of_an_index_held_lookup_after_an_outage_keeps_the_content_index_origin_and_its_rounds`.
      - `retry_of_an_index_held_lookup_is_offered_only_when_the_lookup_was_inconclusive`.
- [ ] 7.2 Green: the interactive driver's Retry repeats the file's own
      lookup whatever its non-operator origin, and `rounds_before` keeps
      the rounds for `Origin::Indexed` (D6).

## 8. Audit and verification

- [ ] 8.1 Audit every edit to a pre-existing test against the preamble's
      rules and D14. Report any test that needed anything else.
- [ ] 8.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`. Run all
      three at the end of every group too, since each group is a green
      point.
- [ ] 8.3 Search `crates/*/src/` for what this change makes false. Each
      search must find nothing, or a hit the report justifies:
      - `RetrievedFrom::ContentIndex`, `RecordRetrieval::ContentIndex`;
      - `the content index` in a human string as a place a record came
        from;
      - "served from the index" and "the record indexed for" in
        docstrings;
      - `FORMAT_VERSION`'s docstring naming the index;
      - `Adoption::Unindexed`'s docstring saying "no record".
- [ ] 8.4 Run `openspec validate reduce-content-index --strict` and
      `python3 scripts/check-spec-deltas.py`.
- [ ] 8.5 By hand, over a copy of a library built from the real-PDF
      corpus, which lives outside the repository:
      1. `borax resolve` a file outside the library, and check that
         `extracted.json` appears under the data directory.
      2. `borax cache --clear`, then `borax resolve` it again: no
         extraction, and a network lookup.
      3. In an interactive rename inside the library, supply an
         identifier: the history entry and `operator.json` hold it.
      4. Delete the data directory and run `borax reconcile`:
         `operator.json` returns.
      5. `borax adopt` in a second library holding a copy: it is
         recorded from the response cache.

      If this is not run, say so in the report.

## 9. Documents and state

- [ ] 9.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Keep every
      existing example name. Sources:
      - this change's design.md (D1–D9, D11, and the illustrative lines);
      - proposal.md;
      - the implemented `human_line` and `describe.rs`.

      Passages:
      - `* What a run does`: the content index holds identifiers, not
        records; the library answers first, then the index's
        identifier is looked up.
      - `** borax resolve`: the retrieval clause no longer says `the
        content index`.
      - `** borax rename`'s passages on remembering an answer (near "A
        later interactive or batch run can use that"): where the answer
        is kept, inside and outside a library.
      - `** borax cache`: it clears the response cache only, and where
        the content index lives on each platform.
      - `** borax reconcile`: it restores operators' identifiers to the
        index.
      - `** borax adopt` and `** Choosing how to record existing files`:
        D7, `unanswered`, and that `cache --clear` keeps the
        identifiers adoption needs.
      - The `cache` setting under `** The settings`: it skips reading
        the index, and never displaces an operator's identifier.
      - `* Run logs`: schema 5, `hit`'s `evidence`, `lookup.origin`
        `content-index`, `content-index-write`'s new fields, and the
        adoption kinds.
- [ ] 9.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Write the Unreleased entry from
      proposal.md and design.md. It covers:
      - the index's new home and its reduced entries;
      - that `v1` entries are not read, and `cache --clear` removes them
        with the responses;
      - evidence on history entries, and the append rule;
      - reconcile's restoration;
      - adoption's new source and kinds;
      - schema 5, with the released values that changed, under Changed.
- [ ] 9.3 Orchestrator: read both diffs against the sources, for
      anything invented and for anything dropped from the old text.
      Check the `#+AI_PASS` line the doc writer adds. Answer or remove
      every `TODO(docs)`.
- [ ] 9.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change after the `library-identity`
        paragraph;
      - in the `supply-identifiers-interactively` paragraph, the
        sentence "A record accepted this way is written to the content
        index on the move" gains "as its identifier, since
        `reduce-content-index`";
      - add the Windows state/cache root collision to "Known defects";
      - update "Last reviewed".
