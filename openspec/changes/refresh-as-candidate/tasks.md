# Tasks: refresh-as-candidate

Work on branch `change/refresh-as-candidate`, cut from `main` at
`786628f`. This is change 14 of the roadmap's Phase 4, its last, after
13b (`index-references-libraries`). Release 0.10.0 follows it.

Groups 1 to 6 are each a red/green pair:

- The red task is written and run first, and it touches test files
  only.
- The green task then makes it pass without editing those tests.
- A new test may open red on a name from design D12 that does not exist
  yet. Write it so that it fails on its assertion once the name exists.
- Write **one named test per bullet**. A bullet is not covered
  "implicitly" by an edited test or by another bullet's test.
- A few bullets pin behaviour that already holds and will open green:
  - 1.1's `new_cached_source_still_serves_a_held_answer_without_asking`;
  - 2.1's refusal tests and its unknown-key test, since `--refresh` is
    unknown everywhere today;
  - 4.1's `…_without_refresh_…` tests.

  The red report names them as pins, not as failures to explain. They
  hold the green task to adding the flag and the key where D1 and D8
  say, and nowhere else.

**Green points.** Every group ends with the workspace compiling and
`cargo test --workspace` green. Each type change lands in the same pair
as every consumer of that type (design D14):

- groups 1 and 3 add code nothing calls;
- after group 2 the flag parses and `run-started` reports it, and no
  resolution reads it;
- after group 4 a refresh is reported everywhere and nothing asks;
- after group 5 the write exists and no driver calls it;
- after group 6 an interactive rename asks.

These parts of the design fix the contract:

- the flag, which files, and what it is not: D1;
- what a refresh produces, and when it produces none: D2;
- where acceptance is offered, and the reasons where it is not: D3, D4;
- the schema additions: D5, D8, and the illustrative lines at the end
  of design.md;
- the refreshing response cache: D6;
- the compared fields, their order and their equality: D7;
- the replacement write and its event: D9;
- the question, its timing, once per item, passing over, Quit, Retry
  and Supply: D10;
- the human strings and the description: D11;
- the Rust interface: D12;
- the sequences each group's tests trace: D13.

**Existing tests change.** Design D15 lists each one, by group, with the
kind of edit:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact.
  There is none;
- **(c)** a deletion must be listed. None is.

Each group's red task makes that group's D15 edits and names them. A
test that a group's change breaks and D15 does not list is reported
back, with the translation applied. Task 7.1 audits every edit to an
existing test.

**Fixtures.** Reuse these:

- `crates/borax-sources/tests/cache.rs`: the `Cached` tests and their
  counting source and `MemoryCache`, as models.
- `crates/borax/tests/pipeline.rs`:
  - `FakeDocuments`, `fake_source`, `PanicSource`, `CountingCache`;
  - `hash_for`, `record_with_doi`, `config`, `resolved_outcome`;
  - `library_item` and `tracked_library`.
- `crates/borax/tests/library.rs`:
  - `tempdir`, `write_file`, `snapshot`;
  - `write_item`, `item_with_doi`, `record_with_doi`, `item_id`;
  - the `UUID_*` constants.
- `crates/borax/tests/dispatch.rs`:
  - `real_library`, `write_real_file`, `snapshot`;
  - `write_lib_item`, `write_lib_artifact_record`, `seed_tracked_file`,
    `lib_item_id`, `lib_artifact_id`, `lib_hash_entry`;
  - `fake_source`, `KeyedSource`, `PanicSource`, `FakeDocuments`;
  - `ScriptedAsker` and `Answer`;
  - `effective_with_default_template`, `fixed_now`;
  - `events_for`, `dispatch`, `Streams`, `Session::batch`,
    `Session::interactive`;
  - `RealFilesystem`, `FakeFilesystem`, `FakeBibFiles`.
- `crates/borax/tests/references.rs` and `dispatch.rs`'s reference
  fixtures (13b), for a referenced library.
- `crates/borax/tests/binary.rs`: `seed_tracked_binary_fixture` and the
  `env_clear` runs that set `XDG_CONFIG_HOME`, `APPDATA`,
  `XDG_CACHE_HOME` and `XDG_DATA_HOME`.
- `crates/borax/tests/event.rs`: `all_events`, `json_line`,
  `human_line`.
- `crates/borax/tests/describe.rs`: `Fixture`.

**Seeding.**

- **A hand-corrected item.** A tracked file whose item's `title` differs
  from what the fake source returns for the item's DOI.
- **A source that disagrees.** `fake_source` or `KeyedSource` returning,
  for the item's DOI, a record that differs from the item in the fields
  a test names.
- **An outage.** A source answering `SourceError::Unavailable`.
- **An item edited during a question.** An `Asker` whose `choose` writes
  to the item file before answering `Answer::Replace`.

No test reaches the network. A binary test that needs a refresh to make
no request uses `--sources arxiv` with an item carrying only a DOI.

## 1. A response cache that writes and never reads

Green point: additive.

- [ ] 1.1 Red, in `crates/borax-sources/tests/cache.rs`:
      - `refreshing_cached_source_asks_the_source_when_the_cache_holds_the_key`.
      - `refreshing_cached_source_stores_the_fresh_answer_over_the_one_held`.
      - `refreshing_cached_source_reports_a_network_retrieval_with_its_write`.
      - `refreshing_cached_source_reports_a_failed_write_as_stored_failed`.
      - `refreshing_cached_source_stores_nothing_for_a_failure`.
      - `new_cached_source_still_serves_a_held_answer_without_asking`.
- [ ] 1.2 Green: `Cached::refreshing`, and `fetch` honouring it (D6,
      D12).

## 2. The selector

Green point: `ResolveConfig` gains a field, read by nothing yet.
`RunStarted` gains a field. Their test literals change in this pair
(D15).

- [ ] 2.1 Red. Make the group-2 D15 edits: the `config` helper in
      `pipeline.rs`, and `run_started` and `run_started_with_a_table`
      in `event.rs`.

      In `crates/borax/tests/cli.rs`:
      - `resolve_accepts_refresh`.
      - `rename_accepts_refresh`.
      - `bib_accepts_refresh`.
      - `config_refuses_refresh_as_an_unknown_argument`.
      - `status_refuses_refresh_as_an_unknown_argument`.
      - `rename_refuses_no_refresh_as_an_unknown_argument`.
      - `refresh_sets_no_setting`: `rename --refresh f.pdf` has
        `Settings::default()`.
      - `refresh_follows_its_subcommand`: `borax --refresh rename f.pdf`
        is refused.
      - `command_refreshing_sets_refresh_on_resolve_rename_and_bib_only`.
      - `command_constructors_do_not_refresh`.

      In `crates/borax/tests/config.rs`:
      - `a_configuration_file_setting_refresh_is_an_unknown_key`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_run_started_carries_refresh_true_only_when_set`.
      - `json_line_of_run_started_without_refresh_has_no_refresh_key`.
      - `run_started_round_trips_with_and_without_refresh`.

      In `crates/borax/tests/dispatch.rs`:
      - `a_run_given_refresh_reports_refresh_on_run_started`, for each
        of `resolve`, `rename` and `bib`.
      - `a_run_not_given_refresh_carries_no_refresh_key_on_run_started`.

      In `crates/borax/tests/binary.rs`:
      - `resolve_refresh_over_an_empty_directory_reports_refresh_on_run_started`.
- [ ] 2.2 Green:
      - `refresh` on `Command::Resolve`, `Rename` and `Bib`, with its
        help text, and `Command::refreshing` and `refreshes`;
      - `ResolveConfig::refresh`, and `resolving` taking the selector;
      - `Event::RunStarted::refresh`, set in `dispatch`;
      - `polite` choosing `Cached::refreshing` under `--refresh` with the
        cache on.

      (D1, D6, D8, D12.)

## 3. The refresh module

Green point: additive. Nothing calls `borax::refresh` or the new step
types yet, and `Event` is unchanged.

- [ ] 3.1 Red, in a new `crates/borax/tests/refresh.rs`.

      The comparison (D7):
      - `fields_are_the_records_own_keys_in_record_order`: `Field::ALL`
        and `as_str`.
      - `field_value_is_the_fields_json_as_the_record_serializes_it`.
      - `differences_of_equal_records_are_none`.
      - `differences_ignore_provenance_confidence_and_source_fields`.
      - `differences_report_a_changed_title_with_both_values`.
      - `differences_report_a_field_the_item_lacks_with_no_item_value`.
      - `differences_report_a_field_the_candidate_lacks_with_no_candidate_value`.
      - `differences_compare_authors_in_order`.
      - `differences_compare_issued_to_its_precision`.
      - `differences_compare_the_arxiv_identifier_with_its_version`.
      - `differences_do_not_normalise_whitespace_or_case`.
      - `differences_come_in_field_order`.

      The replacement record (D9):
      - `replaced_takes_each_differing_field_from_the_candidate`.
      - `replaced_removes_a_field_the_candidate_lacks`.
      - `replaced_takes_the_provenance_entries_of_the_fields_it_changes`.
      - `replaced_keeps_source_fields_confidence_and_every_other_provenance_entry`.
      - `replaced_has_no_difference_from_the_candidate`.

      The lookup (D2):
      - `refresh_of_a_record_with_no_identifier_asks_nothing_and_is_no_identifier`,
        over `PanicSource`.
      - `refresh_looks_up_the_preferred_identifier`: a record with a DOI
        and an arXiv id is looked up by the DOI.
      - `refresh_with_no_service_supporting_the_identifier_is_no_eligible_service`.
      - `refresh_no_service_answers_is_unresolved_with_every_attempt_in_order`.
      - `refresh_of_an_identifier_no_service_holds_is_unresolved_with_not_found_attempts`.
      - `refresh_equal_in_every_compared_field_is_current`.
      - `refresh_differing_in_provenance_alone_is_current`.
      - `refresh_differing_is_differs_with_the_candidate_and_its_differences`.
      - `refresh_keeps_a_failed_service_ahead_of_the_one_that_answered`.

      The step (D8):
      - `refresh_step_of_differs_carries_its_decision_as_acceptance`.
      - `refresh_step_of_current_carries_no_acceptance`.
      - `refresh_step_of_no_identifier_is_not_attempted_no_identifier`.

      In `crates/borax/tests/pipeline.rs`:
      - `unattempted_names_the_four_refresh_reasons`.

      In `crates/borax/tests/event.rs`:
      - `refresh_step_differs_serializes_as_design_d8`.
      - `refresh_step_of_each_status_serializes_as_design_d8`.
      - `refresh_acceptance_of_each_status_serializes_as_design_d8`.
      - `difference_step_leaves_out_an_absent_side`.
      - `refresh_step_round_trips_every_status_and_acceptance`.
- [ ] 3.2 Green:
      - `borax::refresh`: `Field`, `Difference`, `differences`,
        `replaced`, `Fresh`, `Decision`, `Whose`, `Refresh`, `refresh`,
        `Refresh::step`;
      - the four `Unattempted` values;
      - `RefreshStep`, `DifferenceStep` and `RefreshAcceptance`.

      (D2, D7, D8, D9, D12.)

## 4. Resolution reports a refresh

Green point: `FileRecord` and `Event::Resolved` gain a field. Every
literal and full pattern of either changes in this pair (D15), and so
does every `src` site. The behaviour is behind `ResolveConfig::refresh`,
which every existing test leaves false.

- [ ] 4.1 Red. Make the group-4 D15 edits: the `FileRecord` literals and
      the `Event::Resolved` literals and patterns.

      In `crates/borax/tests/pipeline.rs`:
      - `a_tracked_file_resolved_without_refresh_carries_no_refresh`.
      - `a_tracked_file_resolved_with_refresh_still_resolves_to_its_items_record`.
      - `a_tracked_file_with_refresh_looks_up_its_items_preferred_identifier`:
        a counting source is asked once, for the DOI.
      - `a_tracked_file_with_refresh_opens_nothing_and_reads_no_index`:
        `CountingCache` stays at zero, and `FakeDocuments` is never
        opened.
      - `a_tracked_file_with_refresh_writes_nothing_to_the_content_index`.
      - `a_tracked_file_with_refresh_reports_decision_not_asked`.
      - `a_tracked_file_answered_under_an_earlier_item_refreshes_that_item`.
      - `a_reference_answer_with_refresh_refreshes_the_referenced_item_with_another_library`.
      - `an_evidence_answer_with_refresh_carries_no_refresh`.
      - `an_extracted_file_with_refresh_carries_no_refresh`.
      - `a_work_duplicate_verdict_with_refresh_carries_no_refresh`: a
        reference answer whose work the run's account already holds.
      - `resolved_event_carries_the_files_refresh_as_its_step`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_resolved_without_refresh_has_no_refresh_key`.
      - `json_line_of_resolved_with_refresh_puts_refresh_after_acceptance`.
      - `human_line_of_resolved_with_a_current_refresh_matches_design_d11`.
      - `human_line_of_resolved_with_a_differing_refresh_not_asked_matches_design_d11`.
      - `human_line_of_resolved_with_an_unresolved_refresh_matches_design_d11`.
      - `human_line_of_resolved_with_no_eligible_service_matches_design_d11`.
      - `human_line_of_resolved_with_no_identifier_matches_design_d11`.
      - `human_line_of_resolved_with_each_acceptance_matches_design_d11`.
      - `human_line_of_another_library_names_the_references_root`.
      - `human_line_refresh_clause_precedes_the_rejected_candidate_clause`.
      - `human_line_refresh_clause_escapes_record_values`.

      In `crates/borax/tests/dispatch.rs`:
      - `resolve_with_refresh_reports_a_hand_corrected_item_as_differing_and_leaves_it_byte_identical`
        (D13 a).
      - `resolve_with_refresh_reports_current_for_an_item_the_services_agree_with`.
      - `resolve_with_refresh_reports_an_outage_as_unresolved_and_the_item_stands`
        (D13 d).
      - `resolve_with_refresh_reports_not_found_as_unresolved`.
      - `resolve_with_refresh_reports_in_input_order_under_concurrency`.
      - `resolve_with_refresh_through_a_reference_writes_nothing_to_the_referenced_library`
        (D13 e).
      - `rename_apply_with_refresh_writes_no_item_and_reports_not_asked`
        (D13 a).
      - `rename_apply_with_refresh_and_no_record_reports_not_asked`
        (D13 f).
      - `rename_preview_with_refresh_writes_nothing_to_the_library`
        (D13 g).
      - `bib_with_refresh_cites_the_items_record_not_the_candidate`.
      - `refresh_with_no_cache_extracts_an_untracked_file_and_carries_no_refresh`
        (D13 k).

      In `crates/borax/tests/binary.rs`:
      - `resolve_refresh_reports_no_eligible_service_for_a_tracked_doi_with_only_arxiv`:
        `resolve --refresh --json --sources arxiv` over
        `seed_tracked_binary_fixture`'s file, with a DOI on its item.
- [ ] 4.2 Green:
      - `FileRecord::refresh`;
      - `standing_in` attaching a `Refresh` to a resolved verdict from
        an item;
      - `Event::Resolved::refresh`, projected by `resolved_event`;
      - the human clause.

      (D2, D3, D4, D8, D11.)

## 5. The replacement write

Green point: additive. No driver calls `replace_record` yet.
`human_line` gains the new event's arm.

- [ ] 5.1 Red.

      In `crates/borax/tests/library.rs`:
      - `replace_record_changes_only_the_lines_of_the_differing_keys`:
        the file's text before and after differ in the `title` line
        alone.
      - `replace_record_keeps_the_id_comments_and_unknown_keys`.
      - `replace_record_keeps_the_items_file_name`.
      - `replace_record_removes_a_key_the_record_no_longer_holds`.
      - `replace_record_adds_a_key_the_item_lacked`.
      - `replace_record_updates_the_provenance_entries_of_the_fields_it_changes`.
      - `replace_record_leaves_source_fields_and_confidence`.
      - `replace_record_read_back_is_the_replacement`.
      - `replace_record_refuses_a_file_changed_since_it_was_read`.
      - `replace_record_refuses_a_file_holding_another_item`.
      - `replace_record_refuses_a_file_that_cannot_be_read`.
      - `replace_record_reports_a_failed_write_and_leaves_the_file`.
      - `replace_record_through_a_store_writer_mints_an_absent_identity`.
      - `replace_record_whose_mint_fails_writes_nothing_and_names_the_identity_file`.
      - `stores_item_file_names_the_file_an_item_was_read_from`.
      - `stores_learn_item_answers_a_later_consultation_with_the_learned_record`.
      - `stores_learn_item_keeps_the_items_file`.
      - `stores_learn_item_of_an_identity_not_held_changes_nothing`.

      In `crates/borax/tests/event.rs`:
      - `json_line_of_library_replacement_matches_design_d9`.
      - `human_line_of_a_written_library_replacement_matches_design_d11`.
      - `human_line_of_a_failed_library_replacement_matches_design_d11`.
      - `human_line_of_library_replacement_escapes_after_the_path`.
      - `library_replacement_counts_toward_nothing`.
      - `library_replacement_round_trips`.
- [ ] 5.2 Green: `replace_record`, `Stores::item_file`,
      `Stores::learn_item`, `Event::LibraryReplacement` and its human
      line (D9, D11, D12).

## 6. The interactive question

Green point: `Answer` gains two variants, and `Choice`'s `Display` gains
their arms. No existing test passes `--refresh`, so no existing
question changes.

- [ ] 6.1 Red.

      In `crates/borax/tests/dispatch.rs`, each with `ScriptedAsker`
      over `RealFilesystem` and a tracked library:
      - `interactive_refresh_puts_the_refresh_question_first_offering_keep_replace_quit`.
      - `interactive_refresh_question_defaults_to_keeping_the_librarys_record`.
      - `interactive_refresh_question_names_the_file_as_its_target`.
      - `interactive_refresh_keep_leaves_the_item_byte_identical_and_reports_declined`
        (D13 b).
      - `interactive_refresh_replace_writes_the_item_and_reports_accepted`
        (D13 c).
      - `interactive_refresh_replace_emits_library_replacement_before_the_files_resolved_event`.
      - `interactive_refresh_replace_resolves_the_file_to_the_replaced_record`.
      - `interactive_refresh_replace_then_the_move_question_proposes_the_replaced_records_name`:
        a template rendering the title.
      - `interactive_refresh_replace_then_quit_keeps_the_replacement_and_counts_the_file_unreached`.
      - `interactive_refresh_quit_at_the_refresh_question_writes_nothing`.
      - `interactive_refresh_replace_of_an_item_edited_meanwhile_is_unwritten`
        (D13 i): the file goes on with the record as the run read it.
      - `interactive_refresh_failed_write_is_unwritten_with_its_message`,
        `#[cfg(unix)]`.
      - `interactive_refresh_replace_mints_the_identity_of_a_library_with_none`
        (D13 j).
      - `interactive_refresh_asks_about_an_already_named_file_skip_named_would_pass_over`.
      - `interactive_refresh_asks_no_already_named_question_after_the_refresh_question`.
      - `interactive_refresh_shows_an_already_named_file_whose_refresh_found_nothing`:
        no question, and not counted as not shown.
      - `interactive_refresh_passes_over_an_already_named_file_whose_item_is_current`.
      - `interactive_refresh_asks_once_per_item_and_a_second_file_after_replace_is_current`
        (D13 h).
      - `interactive_refresh_after_keep_reports_already_asked_for_the_second_file`.
      - `interactive_refresh_replacement_is_learned_by_the_work_check`:
        a later file resolving to the replaced record's new DOI is a
        work duplicate of the item.
      - `interactive_refresh_with_no_record_asks_nothing_and_reports_not_recording`
        (D13 f).
      - `interactive_refresh_of_a_reference_answer_asks_nothing_and_reports_another_library`
        (D13 e).
      - `interactive_refresh_offers_no_retry_after_a_refresh_outage`.
      - `interactive_refresh_replace_then_supply_keeps_the_refresh_on_the_supplied_verdict`.
      - `interactive_refresh_filed_work_duplicate_carries_no_refresh`: a
        reference answer the run's library holds the work of, filed as
        another artifact.

      In `crates/borax/tests/describe.rs`:
      - `description_of_a_current_refresh_matches_design_d11`.
      - `description_of_a_pending_differing_refresh_matches_design_d11`:
        `refresh`, its continuation, `differs` and `decision`.
      - `description_of_a_differing_author_list_writes_every_author`.
      - `description_of_an_absent_value_writes_nothing`.
      - `description_of_an_unresolved_refresh_writes_each_service_on_a_line`.
      - `description_of_no_identifier_writes_one_refresh_line`.
      - `description_of_each_decision_matches_design_d11`.
      - `description_refresh_lines_follow_file_says_and_precede_new_name`.
      - `description_refresh_writes_the_identifier_whole`.
      - `description_refresh_escapes_record_values`.
- [ ] 6.2 Green:
      - `Answer::KeepRecord` and `Answer::Replace`, with D11's labels;
      - in the rename driver:
        - the refresh question, first, with the hold ended;
        - the write at the answer, and its event;
        - the consulted stores and the account learning the
          replacement;
        - the per-run item memory;
        - the decision reasons `not-recording` and `already-asked`;
        - passing over only files with nothing to report;
        - the refresh carried onto a supplied verdict;
      - the description's `refresh`, `differs` and `decision` lines.

      (D3, D9, D10, D11.)

## 7. Audit and verification

- [ ] 7.1 Audit every edit to a pre-existing test against the preamble's
      rules and D15. Report any test that needed anything else.
- [ ] 7.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`. Run all
      three at the end of every group too, since each group is a green
      point.
- [ ] 7.3 Search `crates/*/src/` for what this change makes false. Each
      search must find nothing, or a hit the report justifies:
      - `admit`'s and `select_item`'s docstrings implying that nothing
        but a mint writes an item;
      - `standing_in`'s and `resolve_file`'s docstrings saying a tracked
        file asks no service, with no word on a refresh;
      - `ResolveConfig::cache`'s docstring as the only switch on what is
        asked;
      - `Cached`'s docstring saying an answer held is always served;
      - `Event::RunStarted`'s and `Event::Resolved`'s docstrings listing
        their keys without `refresh`;
      - `situation`'s and `rename_events`' docstrings saying an
        already-named file under `skip-named` is never shown.
- [ ] 7.4 Run `openspec validate refresh-as-candidate --strict` and
      `python3 scripts/check-spec-deltas.py`.
- [ ] 7.5 By hand, over a copy of a library built from the real-PDF
      corpus, which lives outside the repository, in a pty for the
      interactive steps:
      1. Edit one item's title by hand, then `borax resolve --refresh`
         its file: `differs` in `title`, and the item file is unchanged.
      2. `borax rename --refresh` interactively, answer Keep: the
         question is put though the file is already named, and nothing
         changes.
      3. Again, answer Replace: `git diff` of the item file, or `diff`
         against a copy, shows the `title` line alone, and a comment
         added above it survives.
      4. `borax rename --refresh` again: `current`, and nothing is
         asked.
      5. With the network off, `borax resolve --refresh`: `unresolved`,
         and the item stands.
      6. `borax resolve --refresh` a copy outside the library:
         `another-library`, naming the root.

      If this is not run, say so in the report.

## 8. Documents and state

- [ ] 8.1 Doc writer (`codex-docs`), update job on `docs/manual.org`
      (Org, for CLI users). This is not the implementer's. Keep every
      existing example name. Sources:
      - this change's design.md (D1 to D11, D13, and the illustrative
        lines);
      - proposal.md;
      - the implemented `human_line`, `describe.rs` and the menu labels.

      Passages:
      - `* What a run does`: step 2 says a tracked file asks no service
        "unless the run was given `--refresh`", and what a refresh does
        there.
      - `** borax resolve`: `--refresh`, the refresh clause, and that
        `resolve` never replaces anything.
      - `** borax rename` and `*** The interactive session`: the refresh
        question, its choices and default, what Replace writes and what
        survives, once per item, Quit after Replace, and that a
        differing candidate is asked about even for a file `skip-named`
        would pass over.
      - `** borax bib`: `--refresh` cites the item, and makes untracked
        files' lookups fresh.
      - `** borax cache`: `--refresh` stores fresh answers over held
        ones.
      - `** Settings on the command line`: `--refresh` is a selector, not
        a setting, with no `--no-refresh`, and refused by `config`.
      - `* Run logs`: `run-started.refresh`, `resolved.refresh` and its
        statuses and acceptance, the four reasons, and
        `library-replacement`.
- [ ] 8.2 Doc writer (`codex-docs`), update job on `CHANGELOG.md`
      (Markdown, Keep a Changelog). Extend the Unreleased entry from
      proposal.md and design.md. It covers:
      - `--refresh` on `resolve`, `rename` and `bib`;
      - candidates reported beside library items, and what is compared;
      - acceptance in an interactive `rename`, what a replacement writes
        and what it keeps;
      - the schema-5 additions, under Added.
- [ ] 8.3 Orchestrator: read both diffs against the sources, for
      anything invented and for anything dropped from the old text.
      Check the `#+AI_PASS` line the doc writer adds. Answer or remove
      every `TODO(docs)`.
- [ ] 8.4 Implementer, in `openspec/STATE.md`:
      - add a paragraph for this change after the
        `index-references-libraries` paragraphs, as change 14 and the
        last of Phase 4;
      - in the `consult-library-first` paragraph, the sentence on
        `--no-cache` and `--no-record` gains that `--refresh` asks a
        service for a candidate and still answers from the item;
      - update "Last reviewed".
