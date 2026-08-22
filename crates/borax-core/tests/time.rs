#![allow(clippy::unwrap_used)]

use borax_core::time::utc_basic;

// --- shape ---

#[test]
fn utc_basic_of_the_unix_epoch() {
    assert_eq!(utc_basic(0), "19700101T000000Z");
}

#[test]
fn utc_basic_of_a_known_recent_instant() {
    // 2026-08-22T07:54:00Z
    assert_eq!(utc_basic(1_787_385_240_000), "20260822T075400Z");
}

#[test]
fn utc_basic_zero_pads_every_field() {
    // 2024-01-02T03:04:05Z: day, month, hour, minute and second are all
    // single digits and each needs a leading zero.
    assert_eq!(utc_basic(1_704_164_645_000), "20240102T030405Z");
}

#[test]
fn utc_basic_discards_sub_second_precision_rather_than_rounding() {
    // 2024-01-01T00:00:00.999Z must render as :00, not round up to
    // the next second.
    assert_eq!(utc_basic(1_704_067_200_999), "20240101T000000Z");
}

// --- leap years and century rules ---

#[test]
fn utc_basic_of_a_leap_day() {
    // 2024 is a leap year: February has a 29th.
    assert_eq!(utc_basic(1_709_208_000_000), "20240229T120000Z");
}

#[test]
fn utc_basic_2000_is_a_leap_year_despite_being_a_century_year() {
    // Divisible by 400, so the century rule's exception applies. 1900
    // fails the same rule but predates the epoch, so it cannot be
    // reached from an unsigned millisecond count.
    assert_eq!(utc_basic(951_782_400_000), "20000229T000000Z");
}

// --- month, day and year boundaries ---

#[test]
fn utc_basic_a_second_before_a_non_leap_february_ends() {
    assert_eq!(utc_basic(1_677_628_799_000), "20230228T235959Z");
}

#[test]
fn utc_basic_the_second_march_begins_in_a_non_leap_year() {
    assert_eq!(utc_basic(1_677_628_800_000), "20230301T000000Z");
}

#[test]
fn utc_basic_a_30_day_months_last_second() {
    assert_eq!(utc_basic(1_714_521_599_000), "20240430T235959Z");
}

#[test]
fn utc_basic_the_day_after_a_30_day_month_ends() {
    assert_eq!(utc_basic(1_714_521_600_000), "20240501T000000Z");
}

#[test]
fn utc_basic_a_second_before_midnight_on_the_last_day_of_the_year() {
    assert_eq!(utc_basic(1_704_067_199_000), "20231231T235959Z");
}

#[test]
fn utc_basic_midnight_on_new_years_day() {
    assert_eq!(utc_basic(1_704_067_200_000), "20240101T000000Z");
}
