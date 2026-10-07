# Tasks: separate-change-from-acceptance

Work on branch `change/separate-change-from-acceptance`, cut from
`main` at `6d9305b`. This is change 15 of the roadmap's Phase 4, and the
first of its four changes.

Groups 1 to 9 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D11 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.
- Write **one named test per bullet**. A bullet is not covered
  "implicitly" by an edited test or by another bullet's test.

These parts of the design fix the contract:

- the gate and its bases: D1;
- the identifier comparison: D2;
- a pending change: D3;
- `--accept`: D4;
- the applying rename's outcomes: D5;
- entries' items and the item a record answers with: D6;
- pre-change entries: D7;
- the JSON shapes: D9 and the illustrative lines at the end of
  design.md;
- the human strings: D10;
- the Rust interface: D11.

**Existing tests change.** Design D13 lists each one and the kind of
edit:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. One is listed:
  `reconcile_refuses_page_limit_as_an_unknown_argument`.

Sweep the tests for every changed type: `HashEntry`, `Repair`,
`Event::LibraryReconciled`, `Admission`, `SkipReason`,
`LibraryAnswer::Tracked`, `RetrievedFrom::Library`,
`Command::Reconcile`, and every `library::reconcile` and
`work_duplicate` call. Task 10.1
audits every edit to an existing test.

**Fixtures.** Reuse these:

- in `crates/borax-core/tests/library.rs`: `hash`, `hash_entry` and
  `minimal_record`;
- in `crates/borax/tests/library.rs`:
  - `tempdir`, `write_file`, `snapshot`, `stat`;
  - `artifact_record`, `fresh_record`, `write_artifact_record`;
  - `write_item`, `item_at`, `item_with_doi`, `record_with_doi`,
    `item_file_name`;
  - `hash`, `hash_entry`, the `consult` helper;
  - `UUID_A` and the other constants, `TIMESTAMP`, `TOOL_VERSION`.

  Add one helper there. It builds a `Reconciling` from `rehash`, the run
  name, a list of accepted paths, and an extractor. The extractor is a
  closure over a `BTreeMap<PathBuf, Extraction>` that answers
  `Extraction::Unreadable { message: "not opened in this test" }` for a
  path it does not know, and counts its calls in a `Cell<usize>`;
- in `crates/borax/tests/dispatch.rs`:
  - `real_library`, `write_real_file`, `real_stat`,
    `edit_in_place_same_length`;
  - `hash_entry_for`, `lib_hash_entry`, `write_lib_item`,
    `write_lib_artifact_record`, `seed_artifact_record`, `lib_item_id`,
    `lib_artifact_id`;
  - `FakeDocuments::with_file` and `with_open_error`;
  - `pdf_with_embedded_doi`, `pdf_with_no_identifier`,
    `pdf_blank_with_title`, `pdf_with_text_arxiv`, and
    `FakePdf::with_pages`;
  - `fake_source`, `KeyedSource`, `record_by`, `effective_with`,
    `effective_with_default_template`, `fixed_now`;
  - `events_for`, `dispatch` with `Streams`, `ScriptedAsker::new` and
    `with_texts`, `Session::batch` and `Session::interactive`;
  - `RealFilesystem`, `ContentIndex::new(MemoryCache::new())`,
    `FakeBibFiles`.

  `FakeDocuments` is keyed by full path. Reconcile extracts from
  `root.join(relative)`, and a rename extracts before the move, at the
  file's original path. Register files under those paths;
- in `crates/borax/tests/event.rs`: `json_line`, `human_line`,
  `all_events` and `all_plain_skips`.

**Design tests.** Each case below is a test, in the group named:

| Case | Group |
|---|---|
| annotated paper accepted automatically (its DOI still extracted) | 4 (rewritten `fast_path_miss_…`), 7 |
| unrelated replacement left pending (the HPLC guide, in reconcile) | 4, 8 |
| unrelated replacement already named by a rename: pending, nothing written | 7 |
| unrelated replacement moved by a rename: a new artifact, old record untouched | 7 |
| replacement with no extractable identifier (blank page or scan) left pending | 4 |
| operator acceptance of a pending change (`--accept`) | 4, 8 |
| operator acceptance by a supplied identifier the linked item carries | 7 |
| re-identification as another work separates rather than accepts | 7 |
| bytes restored after a re-link answer with the item they were accepted under | 1, 5 |
| a pre-change history entry with no item still reads, validates and answers | 1, 4, 5, 9 |
| two swapped files still both repair (step order unchanged) and open nothing | 4 |
| the fast-path-invisible change under `--rehash`: accepted, or pending | 4 |
| `--accept` reaches the fast-path-invisible change without `--rehash` | 4 |
| `--accept` naming a path with nothing pending | 4, 8 |
| a link corrected by hand answers for the newest bytes | 1, 5 |
| arXiv compared without its version; ISBN lengths not converted | 2, 4, 7 |
| a pending file passed over in an interactive run is still shown | 7 |
| a preview foresees separation and pending without writing | 7 |
| a content-index answer at a recorded path is gated on extraction | 7 |
| `status` reports nothing about a pending change | 8 |
| D6 transitions (a)–(h): re-identify restored bytes then restore newer; hand-correct, edit, restore; re-link, restore, re-edit; correct while an older version is restored; first acceptance in a pre-change record; re-link of the newest bytes; ten ordinary reruns over restored bytes; restored bytes re-identified as the link's own item | 1, 6 |
| a released answer keeps its kind and fields; `accepted_under` only when the item differs | 3, 5 |
| a restored version is not a work duplicate of its own work; the link's work still meets it | 6, 7 |
| preview and apply agree on a later copy of a separated record's old bytes | 7 |

## 1. History entries name their item

- [ ] 1.1 Red, in `crates/borax-core/tests/library.rs`. Edit (a) the
      `hash_entry` helper to take `item: None`. New tests:
      - `hash_entry_item_round_trips_through_toml`: a record whose entry
        names an item reads back with that item.
      - `a_hash_entry_with_no_item_writes_no_item_key`: `to_toml`
        contains no `item` key inside `[[history]]`.
      - `a_record_written_before_entries_named_items_parses_with_no_item`:
        literal 0.9.0-shaped TOML text, entries with `hash`, `run`,
        `timestamp` and `tool_version` only, parses with every
        `item == None`.
      - `a_hash_entry_item_that_is_not_a_canonical_uuid_does_not_parse`:
        `from_toml` fails with `Malformed`.
      - `item_for_the_newest_hash_is_the_records_link_whatever_its_entry_names`.
      - `item_for_an_earlier_hash_is_the_item_its_newest_entry_names`.
      - `item_for_an_earlier_hash_whose_entry_names_none_is_the_records_link`.
      - `item_for_a_hash_the_history_does_not_hold_is_none`.
      - `item_for_uses_the_newest_of_several_entries_holding_one_hash`:
        history `[A(X), B(Y), A(Z), C(Y)]` answers `Z` for `A`.
      - `append_restates_the_newest_bytes_under_the_link_as_read_when_their_entry_names_another_item`:
        `[A(X)]`, with `link_as_read` Y and B appended, gives
        `[A(X), A(Y), B]`. The restatement carries B's run, timestamp
        and tool version.
      - `append_restates_the_newest_bytes_when_their_entry_names_no_item`.
      - `append_does_not_restate_for_an_entry_for_the_newest_bytes`.
      - `append_does_not_restate_when_the_newest_entry_names_the_link_as_read`.
      - `append_does_not_restate_when_the_link_as_read_is_none`.
      - `append_judges_the_link_as_read_not_the_records_current_link`:
        `[A(X), B(Y)]` whose `item` field the caller has already set to
        X, with `link_as_read` Y and A appended, gives
        `[A(X), B(Y), A]` with no restatement.
- [x] 1.2 Green: `HashEntry::item`, `ArtifactRecord::item_for` and
      `ArtifactRecord::append` (D6, D11).

## 2. Identifier comparison and the gate

- [ ] 2.1 Red, in `crates/borax/tests/library.rs`:
      - `carries_a_doi_the_item_record_holds`.
      - `carries_compares_only_the_field_of_the_identifiers_own_kind`: a
        record whose DOI field holds a value does not carry an arXiv
        identifier, or the reverse.
      - `carries_an_arxiv_identifier_whatever_its_version`: an item
        holding `2401.12345v2` carries `arXiv:2401.12345v1` and
        `arXiv:2401.12345`.
      - `does_not_carry_an_isbn_10_for_an_item_holding_the_isbn_13`.
      - `carries_a_pmid_and_an_isbn_of_the_same_length`.
      - `extracted_identifier_reads_back_every_form_display_writes`: one
        `Extraction::Found` each for `doi:`, `arXiv:` with a version,
        `pmid:` and `isbn:`, each equal to the `Identifier` it was
        rendered from.
      - `extracted_identifier_is_none_for_every_failure`: the four
        failure variants.
      - `accepted_by_is_extracted_when_the_linked_item_carries_the_extracted_identifier`.
      - `accepted_by_prefers_extracted_over_supplied`.
      - `accepted_by_is_supplied_when_only_the_supplied_identifier_is_carried`.
      - `accepted_by_is_none_when_neither_is_carried`.
      - `accepted_by_is_none_for_a_record_linking_no_item`.
      - `accepted_by_is_none_when_no_item_file_carries_the_linked_identity`.
      - `accepted_by_is_none_when_two_item_files_carry_the_linked_identity`.
- [x] 2.2 Green: `carries`, `extracted_identifier`, `accepted_by` (D2,
      D11).

## 3. The event vocabulary

- [ ] 3.1 Red, in `crates/borax/tests/event.rs`. Extend `all_events`
      with:
      - a `changed` repair of each basis;
      - a `pending` repair with and without an item;
      - `library-reconciled`;
      - each new admission.

      Extend `all_plain_skips` with `NotPending`. The existing
      round-trip tests then cover them. Add a `tracked` answer with
      `accepted_under` and an `accepted-item-unavailable` answer of
      each cause. Make the (a) edits D13 lists for
      `LibraryAnswer::Tracked` and `RetrievedFrom::Library` in
      `event.rs`, `pipeline.rs`, `describe.rs`, `dispatch.rs` and
      `library.rs`: `accepted_under: None` in constructors, and `..` in
      exact patterns. `RetrievedFrom::Library` and
      `RecordRetrieval::Library` keep their shape, so their sites need
      no edit. New tests:
      - `a_tracked_answer_without_accepted_under_is_written_as_in_0_9_0`:
        exact JSON with no `accepted_under` key.
      - `a_tracked_answer_carries_accepted_under_beside_the_link`.
      - `a_library_retrieval_is_written_with_artifact_and_item_alone`:
        `record_retrieval` `library` keeps its shape (D9).
      - `accepted_item_unavailable_serialises_each_cause`: `dangling`,
        `unreadable` with `path` and `message`, and `ambiguous` with
        `files`, each beside `artifact`, `item` and `accepted_under`.
      - `human_line_of_accepted_item_unavailable_names_the_artifact_and_both_items`.
      - `human_line_of_a_tracked_answer_with_accepted_under_is_the_tracked_line`.
      - `a_changed_repair_carries_how_it_was_accepted`: the exact JSON
        for `extracted` and for `operator` (D9).
      - `a_pending_repair_carries_hash_item_and_extraction`.
      - `a_pending_repair_with_no_item_omits_the_item_key`.
      - `library_reconciled_carries_pending_after_changed`: checked on
        the raw line text, not on a parsed `Value`, whose keys sort.
      - `admission_changed_carries_id_hash_and_accepted`.
      - `admission_pending_carries_id_and_hash`.
      - `admission_separated_carries_id_item_and_from`.
      - `not_pending_skip_carries_path_and_reason_alone`.
      - `not_pending_is_not_a_resolution_verdict`.
      - `human_line_of_a_changed_repair_names_the_identifier_it_was_accepted_on`.
      - `human_line_of_a_changed_repair_accepted_by_the_operator`.
      - `human_line_of_a_pending_repair_with_an_item_names_the_identifier_and_the_item`.
      - `human_line_of_a_pending_repair_with_no_item_says_the_record_links_none`.
      - `human_line_of_a_pending_repair_after_a_failure_uses_the_extraction_clause`:
        `no-text-layer`.
      - `human_line_of_a_pending_repair_escapes_the_path_and_the_readers_message`:
        an `unreadable` message and a path each holding `\x1b`.
      - `human_line_of_library_reconciled_counts_pending`.
      - `human_line_of_an_admission_accepted_on_an_extracted_identifier`.
      - `human_line_of_an_admission_accepted_on_a_supplied_identifier`.
      - `human_line_of_a_pending_admission`.
      - `human_line_of_a_separated_admission`.
      - `human_line_of_a_not_pending_skip`.
      - `human_line_of_a_stranding_skip_names_rehash_and_accept`.

      Every expected string is D10's, exactly.
- [x] 3.2 Green: the `event.rs` types and human lines (D9, D10, D11).

## 4. Reconciliation

- [ ] 4.1 Red, in `crates/borax/tests/library.rs`. These are the D13
      edits:
      - (a) every `reconcile(root, …)` call goes through the new
        helper;
      - (a) `hash_entry` and the three `HashEntry {` literals take
        `item: None`;
      - (a) the totals literal in
        `reconciliation_events_emits_one_repair_event_then_the_totals`
        takes `pending: 0`;
      - (b) `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded`;
      - (b) `a_change_the_fast_path_cannot_see_is_missed_without_rehash_and_found_with_it`.

      New tests:
      - `an_unrelated_replacement_is_left_pending_and_nothing_is_written`:
        the record links an item carrying one DOI, and the extractor
        finds another. Check:
        - the snapshot of `.borax/` is byte-identical afterwards;
        - the repair is `Pending` with the hash, the item and the
          extraction;
        - `pending == 1` and `confirmed == 0`.
      - `a_pending_change_is_reported_again_by_the_next_reconcile`: a
        second run hashes the file and reports `Pending` again, and
        writes nothing.
      - `a_replacement_with_no_text_layer_is_left_pending`.
      - `an_unreadable_replacement_is_left_pending`.
      - `a_change_at_a_record_linking_no_item_is_pending_naming_no_item`.
      - `a_change_whose_linked_item_no_file_carries_is_pending`.
      - `a_later_arxiv_version_is_accepted`.
      - `a_change_the_fast_path_hides_is_pending_under_rehash_when_its_identifier_is_not_carried`.
      - `swapped_files_both_repair_and_no_file_is_opened`: the extractor
        is called 0 times.
      - `an_untouched_library_opens_no_file`: the extractor is called 0
        times.
      - `accept_accepts_a_pending_change_under_the_records_link`. Check:
        - `Changed` with `accepted` `Operator`;
        - the appended entry names the record's link;
        - size and modification time are refreshed;
        - the next run settles the record on the fast path, hashing 0.
      - `accept_reaches_a_change_the_fast_path_hides_without_rehash`.
      - `accept_on_a_record_linking_no_item_appends_an_entry_naming_none`.
      - `accept_with_two_paths_accepts_both`.
      - `accept_naming_one_path_twice_accepts_it_once`: one entry.
      - `accept_normalises_dot_dot_in_a_named_path`: `root/sub/../a.pdf`.
      - `accept_of_a_change_the_identifier_accepted_reports_it_extracted`:
        `not_pending` stays empty.
      - `accept_of_a_confirmed_path_reports_nothing_and_writes_nothing`:
        no repair, and `not_pending` stays empty.
      - `accept_of_a_path_no_record_names_is_not_pending`.
      - `accept_of_a_path_whose_record_was_repaired_elsewhere_is_not_pending`.
      - `accept_of_a_path_outside_the_library_is_not_pending`.
      - `an_accepted_entry_is_appended_as_a_table_naming_its_item_and_earlier_entries_are_byte_identical`.
        Compare the record file's text before and after:
        - every `[[history]]` table that existed before is
          byte-identical, compared table by table;
        - a comment the fixture places in the record, and the `id`,
          `item` and `path` lines, are byte-identical;
        - only the top-level `size` and `modified_millis` values differ,
          since acceptance refreshes them, and they equal the file's;
        - exactly one new table follows the old ones, ending with
          `item = "<uuid>"`.

        Do not compare the whole prefix before the new table.
      - `an_acceptance_after_a_hand_correction_restates_the_newest_bytes`:
        `[A(X)]` whose link a person set to Y, and an extractor finding a
        DOI Y carries. The history becomes `[A(X), A(Y), B(Y)]`.
      - `a_record_written_before_entries_named_items_is_confirmed_without_being_rewritten`:
        literal 0.9.0-shaped record text, and the file is byte-identical
        after a reconcile that confirms it by hash.
- [x] 4.2 Green: `Reconciling`, `reconcile`, `Decided::Pending`,
      `history_table` and `reconciliation_events` (D3, D4, D11).

## 5. Resolution from the library

- [ ] 5.1 Red, in `crates/borax/tests/library.rs`. Edit
      `consult_reports_tracked_for_a_rollback_to_an_older_history_entry_interim_policy`'s
      docstring and message only (D13). New tests:
      - `consult_answers_restored_bytes_with_the_item_their_entry_names`.
        The history is `[A(item X), B(item Y)]`, the record links Y,
        and A is at the path. The answer is `Tracked` with `item` Y and
        `accepted_under` X, and `Consulted::item` is X's item.
      - `consult_answers_the_newest_bytes_with_no_accepted_under`.
      - `consult_reports_dangling_item_about_the_link_even_for_restored_bytes`:
        the link's item file is deleted, and the entry's item is held.
      - `consult_reports_no_item_for_restored_bytes_when_the_record_links_none`.
      - `consult_answers_the_newest_bytes_with_the_link_corrected_by_hand`:
        the history is `[A(X)]`, the record links Y, and A is at the
        path. The answer is Y.
      - `consult_reports_accepted_item_unavailable_for_restored_bytes_whose_item_is_gone`:
        `cause` `dangling`, `item` the link, `accepted_under` the
        deleted item.
      - `consult_reports_accepted_item_unavailable_when_two_files_carry_the_entrys_item`:
        `cause` `ambiguous`.
      - `consult_reports_accepted_item_unavailable_when_the_entrys_item_file_cannot_be_read`:
        `cause` `unreadable`, with that file and the parser's message.
      - `consult_reports_unreadable_item_about_the_link_over_a_valid_entry_item`:
        the link's item file does not parse, and the entry's item is
        held. The answer is `unreadable-item` naming the link, not
        `tracked`.
      - `consult_reports_ambiguous_item_about_the_link_over_a_valid_entry_item`:
        two files carry the link's identity, and the entry's item is
        held.
      - `evidence_retrieval_names_the_item_answered_with_for_restored_bytes`:
        `Evidence::retrieval()` is `RecordRetrieval::Library` with
        `item` X when the answer is `tracked` with `item` Y and
        `accepted_under` X.
      - `consult_reports_unrecognised_content_for_bytes_reconcile_left_pending`:
        run `reconcile` with an extractor whose DOI is not carried, then
        `consult`.

      And in `crates/borax/tests/dispatch.rs`:
      - `resolve_answers_restored_bytes_from_the_item_they_were_accepted_under`:
        `events_for` `resolve` over a real library with that history.
        The `library` section is `tracked` with `item` Y and
        `accepted_under` X, `record_retrieval` is `library` with `item`
        X and no `accepted_under`, and the record's title is X's item's.
- [x] 5.2 Green: `Stores::consult` checks the link, then answers
      through `item_for` (D6, D9), and `Evidence::retrieval`
      (`crates/borax/src/evidence.rs`) builds `RecordRetrieval::Library`
      with the item answered with.

## 6. Admission

- [ ] 6.1 Red, in `crates/borax/tests/library.rs`:
      - `admit_mints_a_record_whose_entry_names_its_item`.
      - `admit_of_bytes_already_held_appends_nothing`: the history is
        `[A, B]`, the file holds A, and the run is ordinary. The history
        is unchanged, the path, size and modification time are the
        file's, and the link is unchanged.
      - `admit_of_the_newest_bytes_on_a_rerun_appends_nothing`.
      - `admit_reidentifying_the_newest_bytes_moves_the_link_and_appends_nothing`:
        `[A(X)]`, re-identified as Y. The history is unchanged, and the
        link is Y. Rule 1 skips it, because A is newest and answers Y
        afterwards.
      - `admit_reidentifying_restored_bytes_appends_an_entry_under_the_new_item`:
        `[A(X), B(Y)]`, A at the path, re-identified as Z, gives
        `[A(X), B(Y), A(Z)]`.
      - `admit_reidentifying_the_newest_bytes_as_the_item_they_already_answer_with_appends_nothing`:
        `[A(X)]` linking X, re-identified as X.
      - `admit_reidentifying_restored_bytes_as_the_item_they_already_answer_with_appends_an_entry`:
        `[A(X), B(Y)]`, A re-identified as X, gives `[A(X), B(Y), A(X)]`,
        because A was not newest.
      - `admit_captures_the_link_as_read_before_assigning_the_new_one`:
        the same fixture yields no restatement of B.
      - `admit_of_accepted_changed_bytes_whose_record_write_fails_leaves_the_record_unchanged`:
        `admit` with a `write` stand-in that fails for the artifact
        record's path. `Err(Unrecorded)`, and the record file is
        byte-identical.
      - `admit_of_accepted_changed_bytes_appends_an_entry_naming_the_linked_item`.
      - `learn_takes_in_no_entry_for_bytes_already_held`: after
        `Stores::learn`, the learned record's history equals the held
        one.
      - `foresee_takes_in_no_entry_for_bytes_already_held`.
      - `admit_giving_a_record_with_no_link_an_item_appends_an_entry_under_it`
        (Rule 1).

      The D6 transitions, one test each, driven through `admit`,
      `reconcile` (with an extractor) and `Stores::consult` over a real
      directory, with "restore" meaning writing those bytes back at the
      path:
      - `transition_a_reidentifying_restored_bytes_leaves_newer_bytes_with_their_item`;
      - `transition_b_a_hand_correction_survives_an_edit_and_a_restore`;
      - `transition_c_relink_then_restore_then_re_edit`: A answers X, and
        A′ is pending;
      - `transition_d_a_correction_made_while_an_older_version_is_restored_is_credited_to_the_newest_bytes`;
      - `transition_e_a_pre_change_record_pins_its_newest_bytes_at_its_first_acceptance`;
      - `transition_f_relinking_the_newest_bytes_appends_nothing_until_later_bytes_restate_it`;
      - `transition_g_ten_ordinary_reruns_over_a_restored_file_append_nothing`;
      - `transition_h_restored_bytes_reidentified_as_the_links_own_item_are_appended`.

      Each asserts the history afterwards and every answer D6's table
      gives.

      The duplicate checks, through `Account`. Make the (a) edit to the
      six existing `work_duplicate` calls, passing `None`:
      - `work_duplicate_passes_over_the_item_the_own_record_answers_with_for_the_incoming_bytes`;
      - `work_duplicate_with_no_hash_passes_over_only_the_link`;
      - `work_duplicate_still_finds_a_recorded_path_of_the_link_item_holding_a_restored_version`.
- [x] 6.2 Green: `admit` and `Stores::take_in` append on D6's terms
      through `ArtifactRecord::append`, each capturing the link as read
      before assigning the new one, and `Account::work_duplicate`
      and `pipeline::second_copy` take the hash.

## 7. The applying rename

- [ ] 7.1 Red, in `crates/borax/tests/dispatch.rs`, all through
      `events_for` or `dispatch`. Edit (b)
      `no_record_refuses_a_move_when_the_edit_in_place_hash_is_stale`
      and (b)
      `an_accepted_interactive_rename_is_recorded_exactly_as_an_apply_run_records_it`,
      which compares the two histories with each entry's `item` set
      aside and asserts that every entry names its own run's item. Make
      the D13 (a) edits. New tests:
      - `an_annotated_file_renamed_is_accepted_on_its_extracted_doi`.
        Check:
        - a `library-admission` `changed` with `accepted` `extracted`;
        - the appended entry names the item;
        - the record names the target.
      - `an_unrelated_replacement_renamed_is_recorded_as_a_new_artifact`.
        Check:
        - a `separated` admission naming the new artifact, its item and
          the old one;
        - the old record file is byte-identical;
        - the new record's one entry names its item.
      - `the_next_reconcile_reports_the_separated_record_missing`.
      - `an_unrelated_replacement_already_named_writes_nothing_and_is_reported_pending`:
        `.borax/` and `items/` byte-identical, and a `pending`
        admission.
      - `a_pending_already_named_file_passed_over_is_still_shown`:
        `dispatch` with `Streams`, interactive, `skip_named` on. The
        terminal text contains the pending line.
      - `a_supplied_identifier_the_linked_item_carries_accepts_a_scan`:
        interactive, `ScriptedAsker::with_texts`, and `KeyedSource`.
        `changed` with `accepted` `supplied`.
      - `a_supplied_isbn_10_for_an_item_holding_the_isbn_13_is_not_accepted`:
        the file keeps its name, so the outcome is `pending`.
      - `a_supplied_identifier_for_another_work_separates_the_file`:
        `separated`, no `relinked`, and the old record byte-identical.
      - `a_content_index_answer_at_a_recorded_path_with_changed_bytes_is_gated_on_extraction`:
        the index holds the file's hash, and `FakeDocuments` registers
        it with the item's DOI. The outcome is `changed` as `extracted`.
      - `an_untracked_file_answered_by_the_content_index_is_not_opened_for_the_gate`:
        `with_open_error`, and the file resolves and is renamed.
      - `a_preview_of_a_replacement_foresees_a_new_artifact_and_writes_nothing`.
        A byte-identical copy later in the same preview is a content
        duplicate of the foreseen target, and nothing is written.
      - `a_preview_of_a_pending_file_learns_nothing`. A byte-identical
        copy later in the same preview is not a content duplicate.
      - `a_preview_and_an_apply_agree_on_a_later_copy_of_the_separated_records_old_bytes`.
        R records A at P, P holds B, and a later input elsewhere holds
        A. Run a preview and then an applying run over two fresh copies
        of the fixture. Neither reports the copy of A as a content
        duplicate of P, and both warn that the library has paths to
        reconcile.
      - `a_batch_rerun_over_a_restored_version_does_not_skip_it_as_a_work_duplicate`:
        another recorded artifact of the entry's item X holds a file.
      - `restored_bytes_renamed_by_an_ordinary_run_append_nothing_and_still_resolve_from_their_item`.
      - `a_reidentified_restored_version_gains_an_entry_under_the_new_item`.
      - `a_reidentified_newest_version_moves_the_link_and_appends_nothing`.
      - `interactive_keep_of_a_replacement_reports_pending_and_writes_nothing`:
        a changed file at its recorded path, unaccepted and asked about,
        answered Keep. `.borax/` and `items/` are byte-identical, and a
        `pending` admission is reported.
      - `an_accepted_change_that_is_also_relinked_reports_changed_then_relinked`.
      - `a_failed_write_after_an_accepted_change_reports_unwritten_alone`,
        `#[cfg(unix)]`. The fixture writes the held record and its
        linked item first. Then it makes `.borax/artifacts/` read-only
        (mode 0o555), so `recorded_at` still reads the held record and
        the gate still accepts the file, while the atomic record write,
        which creates a temporary file in that directory, fails. Expect
        `unwritten` and no `changed`, and the record file byte-identical.
        Restore the mode before the temporary directory drops. The test
        first probes that a file cannot be created in the read-only
        directory, and returns early, saying why, when it can (running as
        root). `admit`'s own failing-`write` case is task 6.1's.
- [x] 7.2 Green: the rename driver settles the gate before the move,
      separates or reports pending, emits the admission events in D9's
      order, releases a pending admission through the hold, and the
      preview foresees the same and treats the paths it moves files away
      from as holding nothing (D5).

## 8. The reconcile command

- [ ] 8.1 Red, in `crates/borax/tests/cli.rs`:
      - (c) delete `reconcile_refuses_page_limit_as_an_unknown_argument`.
      - `reconcile_accepts_page_limit`.
      - `reconcile_accept_is_repeatable_and_keeps_the_order_given`.
      - `reconcile_refuses_no_accept_as_an_unknown_argument`.
      - `reconcile_without_accept_equals_command_reconcile`.

      In `crates/borax/tests/config.rs`:
      - `accept_is_not_a_configuration_key`: config load fails naming
        it.
- [ ] 8.2 Red, in `crates/borax/tests/dispatch.rs`. Make the D13 edits:
      - (b) `reconcile_rehash_flag_reaches_the_library`;
      - (b) the human totals line near line 14403;
      - (a) the `LibraryReconciled` literals;
      - (a) the two direct `library::reconcile` calls.

      New tests:
      - `reconcile_reports_a_pending_change_and_its_totals`.
      - `reconcile_accept_accepts_a_pending_change`
        (`Command::reconcile_accepting`).
      - `reconcile_accept_of_a_path_with_nothing_pending_skips_it_and_exits_partial`:
        `dispatch` with `Streams`. The outcome is partial success, and
        the human summary names the skip.
      - `reconcile_page_limit_reaches_the_extraction`: a fake PDF whose
        only identifier is on page 3. `--page-limit 2` leaves it
        pending, and the default accepts it.
      - `reconcile_human_output_names_the_pending_change_and_counts_it`.
      - `status_over_a_library_with_a_pending_change_opens_and_names_nothing`:
        `FakeDocuments::with_open_error` on the changed file, and no
        event names it.
- [x] 8.3 Green: `Command::Reconcile` gains `extraction` and `accept`,
      plus `Command::reconcile_accepting`, the settings wiring, and
      `reconcile_events` passing the documents, the run's extraction
      settings and the accepted paths, and emitting one `skipped` per
      `not_pending` path (D4, D11).

## 9. Validation

- [ ] 9.1 Red, in `crates/borax/tests/library.rs`:
      - `validate_reports_no_finding_for_an_entry_naming_no_item`.
      - `validate_reports_no_finding_for_an_entry_naming_an_item_the_library_does_not_hold`.
      - `validate_reports_a_record_whose_entry_item_is_not_a_canonical_uuid_as_unreadable`.
- [x] 9.2 Green: nothing beyond group 1 is expected. If something is
      needed, the fix goes in the group that owns it, and the report
      says so.

## 10. Audit and verification

- [x] 10.1 Audit every edit to a pre-existing test against the
      preamble's rules and D13. Report any test that needed anything
      else.
- [ ] 10.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`.
- [x] 10.3 Search `crates/borax/src/` and `crates/borax-core/src/` for
      what this change makes false. Each search must find nothing, or a
      hit the report justifies:
      - "edited in place" and "Edited in place" in docstrings and
        comments;
      - `current_hash() != Some` in `admit` and `take_in`;
      - "interim" in `library.rs`;
      - "links the item" in `LibraryAnswer`'s docs where the item
        answered with is meant.
- [x] 10.4 Run `openspec validate separate-change-from-acceptance
      --strict` and `python3 scripts/check-spec-deltas.py`.
- [x] 10.5 By hand, over a copy of a library made from the real-PDF
      corpus, which lives outside the repository:
      - replace one recorded PDF with an unrelated one, run `borax
        reconcile` (pending), and run it again (pending again);
      - run `borax reconcile --accept <path>` on a scan's change;
      - run `borax rename --apply` on the replacement (`separated`), and
        then `borax reconcile` (the old record `missing`).

      If this is not run, say so in the report.

## 11. Documents and state

- [ ] 11.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Keep every
      existing example name. Sources: this change's design.md (D1–D7,
      D9, D10 and the illustrative lines), proposal.md, and the
      implemented `human_line`. Passages:
      - `* What a run does`, the paragraph beginning "The library tracks
        a file when". Restored older bytes now resolve from the item
        they were accepted under. Entries written before this release
        name no item and still resolve from the current link. The
        newest bytes resolve from the link.
      - `** borax rename`, the `--no-record` sentence ending "run ~borax
        reconcile --rehash~ first". Add `--accept`. Add a sentence on
        what an applying run does with a file whose changed bytes its
        record has not accepted: a new artifact when moved, and pending
        when left in place.
      - `** borax reconcile`:
        - the synopsis gains `--accept PATH` and `--page-limit N`;
        - the step-3 sentence ("treats an unmatched file at a record's
          own path as an in-place edit and appends its new hash")
          becomes the gate, with pending and how to accept;
        - say that `status` does not report pending changes.
      - The artifact-record passage ("Each hash entry names the run,
        timestamp, and borax version that recorded it"): each new entry
        also names its item.
      - `* Run logs`:
        - the `library-repair` and `library-reconciled` field sentence
          gains `pending` and the `pending` repair;
        - the `library-admission` kinds sentence gains `changed`,
          `pending` and `separated`;
        - the `skipped` reasons gain `not-pending`;
        - the jq example for reconcile may add `.pending`.
- [ ] 11.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Write the Unreleased entry from
      proposal.md and design.md. It covers:
      - the gate;
      - `reconcile --accept` and `--page-limit`;
      - separation and pending in an applying rename;
      - entries' `item`;
      - the restored-bytes fix;
      - the additions to schema 4, with the version unchanged.
- [ ] 11.3 Orchestrator: read both diffs against the sources, for
      anything invented and for anything dropped from the old text.
      Check the `#+AI_PASS` line the doc writer adds. Answer or remove
      every `TODO(docs)`.
- [x] 11.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change;
      - close the sentences at the end of the `consult-library-first`
        paragraph beginning "Still open, and handed to the roadmap's
        change 15": the appending of unrecognised bytes, and history
        entries recording no item;
      - update "Last reviewed".
