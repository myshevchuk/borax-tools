use borax::event::Counts;
use borax::session::{FATAL, Outcome, PARTIAL, SUCCESS, outcome_for};

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
        unmatched: 0,
        unreached: 0,
    };

    let outcome = outcome_for(&counts);

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
}
