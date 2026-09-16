use borax::event::{Counts, Format};
use borax::session::{FATAL, Mode, Outcome, PARTIAL, SUCCESS, mode, outcome_for};

// ---------------------------------------------------------------------
// Outcome::code
// ---------------------------------------------------------------------

#[test]
fn success_outcome_codes_as_success() {
    assert_eq!(Outcome::Success.code(), SUCCESS);
}

#[test]
fn partial_outcome_codes_as_partial() {
    assert_eq!(Outcome::Partial.code(), PARTIAL);
}

#[test]
fn fatal_outcome_codes_as_fatal() {
    assert_eq!(Outcome::Fatal.code(), FATAL);
}

#[test]
fn the_three_exit_codes_are_pairwise_distinct() {
    assert_ne!(SUCCESS, FATAL);
    assert_ne!(SUCCESS, PARTIAL);
    assert_ne!(FATAL, PARTIAL);
}

// The cli spec's "Batch with skips" scenario, asserted as it is worded:
// the code is the partial one, and it is neither of the other two.
#[test]
fn a_batch_with_eight_successes_and_two_skipped_files_exits_with_the_partial_code() {
    let counts = Counts {
        resolved: 8,
        renamed: 8,
        skipped: 2,
        named: 0,
        unmatched: 0,
        unreached: 0,
    };

    let code = outcome_for(&counts).code();

    assert_eq!(code, PARTIAL, "got {code}");
    assert_ne!(code, SUCCESS);
    assert_ne!(code, FATAL);
}

// ---------------------------------------------------------------------
// outcome_for
// ---------------------------------------------------------------------

#[test]
fn outcome_for_an_empty_run_with_every_count_zero_is_success() {
    let outcome = outcome_for(&Counts::default());
    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

#[test]
fn outcome_for_a_run_that_resolved_and_renamed_everything_with_nothing_skipped_is_success() {
    let counts = Counts {
        resolved: 5,
        renamed: 5,
        skipped: 0,
        named: 0,
        unmatched: 0,
        unreached: 0,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
}

#[test]
fn outcome_for_a_run_where_resolved_and_renamed_differ_but_skipped_is_zero_is_still_success() {
    let counts = Counts {
        resolved: 5,
        renamed: 3,
        skipped: 0,
        named: 0,
        unmatched: 0,
        unreached: 0,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(
        outcome,
        Outcome::Success,
        "only skipped should decide the outcome, got {outcome:?}"
    );
}

#[test]
fn outcome_for_a_run_with_a_single_skip_is_partial() {
    let counts = Counts {
        resolved: 5,
        renamed: 4,
        skipped: 1,
        named: 0,
        unmatched: 0,
        unreached: 0,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

#[test]
fn outcome_for_a_run_that_skipped_everything_is_partial() {
    let counts = Counts {
        resolved: 3,
        renamed: 0,
        skipped: 3,
        named: 0,
        unmatched: 0,
        unreached: 0,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

#[test]
fn outcome_for_eight_resolved_and_two_skipped_is_partial() {
    let counts = Counts {
        resolved: 8,
        renamed: 8,
        skipped: 2,
        named: 0,
        unmatched: 0,
        unreached: 0,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

/// design "Quitting an interactive run leaves the rest untouched": a run
/// that was quit exits with the partial-success code even when nothing
/// was skipped, because `unreached` files did not succeed either —
/// `outcome_for` sums `skipped` and `unreached`.
#[test]
fn outcome_for_a_run_with_unreached_files_and_nothing_skipped_is_partial() {
    let counts = Counts {
        resolved: 2,
        renamed: 2,
        skipped: 0,
        named: 0,
        unmatched: 0,
        unreached: 3,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}

// ---------------------------------------------------------------------
// mode: design D1 — batch := apply || setting(batch) || !terminal ||
// json
// ---------------------------------------------------------------------

/// The one combination that is interactive: a terminal, human output,
/// no `--apply`, and `batch` off — the default.
#[test]
fn a_terminal_with_human_output_no_apply_and_batch_off_is_interactive() {
    let result = mode(true, Format::Human, false, false);
    assert_eq!(result, Mode::Interactive, "got {result:?}");
}

#[test]
fn apply_selects_batch_on_a_terminal_with_batch_off() {
    let result = mode(true, Format::Human, false, true);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn the_batch_setting_alone_selects_batch_on_a_terminal() {
    let result = mode(true, Format::Human, true, false);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn no_batch_explicitly_set_still_asks_on_a_terminal_with_no_apply() {
    // `batch` off is the setting's default, so this is the same input as
    // the first case, pinned separately because it is the scenario
    // `--no-batch` on the command line produces.
    let result = mode(true, Format::Human, false, false);
    assert_eq!(result, Mode::Interactive, "got {result:?}");
}

#[test]
fn a_redirected_stdin_is_batch_even_with_no_apply_and_batch_off() {
    let result = mode(false, Format::Human, false, false);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn json_output_is_batch_even_on_a_terminal_with_no_apply_and_batch_off() {
    let result = mode(true, Format::Json, false, false);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn json_and_a_redirected_stdin_together_are_still_batch() {
    let result = mode(false, Format::Json, false, false);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn apply_selects_batch_even_off_a_terminal() {
    let result = mode(false, Format::Json, false, true);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn the_batch_setting_and_apply_together_are_batch() {
    let result = mode(true, Format::Human, true, true);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}

#[test]
fn every_disqualifying_input_off_a_terminal_with_json_batch_and_apply_all_set_is_batch() {
    let result = mode(false, Format::Json, true, true);
    assert_eq!(result, Mode::Batch, "got {result:?}");
}
