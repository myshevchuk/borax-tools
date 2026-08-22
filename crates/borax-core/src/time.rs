//! Turning a moment into the string borax stamps things with.
//!
//! One format, used for ledger entry timestamps, run identifiers, and
//! run-log filenames: ISO 8601's basic form, which is fixed width and
//! made only of digits and two letters. That is what lets a directory
//! listing sort by time, and it is the one spelling of a timestamp that
//! is a legal filename on every platform borax runs on.
//!
//! The arithmetic is done here rather than taken from a date library
//! because this is the whole of borax's interest in calendars: a
//! civil date from a count of days, for instants at or after the epoch,
//! in UTC. No local time, no parsing, no leap seconds — the Unix epoch
//! count does not have them.

/// `millis` since the Unix epoch as a UTC timestamp in ISO 8601 basic
/// format: `YYYYMMDDThhmmssZ`.
///
/// Sub-second precision is discarded rather than rounded, so an instant
/// and the last millisecond before the next second render the same:
/// the stamp names the second an event fell in, and rounding could name
/// a second that had not begun.
///
/// The year is padded to four digits and every other field to two, so
/// two stamps compare as strings exactly as the instants they name
/// compare — which is what makes sorting a runs directory by name sort
/// it by time.
pub fn utc_basic(millis: u128) -> String {
    let seconds = (millis / 1_000) as i128;
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let second_of_day = seconds.rem_euclid(86_400);

    let hour = second_of_day / 3_600;
    let minute = (second_of_day / 60) % 60;
    let second = second_of_day % 60;

    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// The civil date `days` after 1970-01-01, as year, month and day.
///
/// Howard Hinnant's `civil_from_days`, which counts from an era of 400
/// years starting on 1 March. Shifting the year to begin in March puts
/// the leap day at the end of it, so a leap year changes only the
/// length of the era's last month and every month before it keeps a
/// fixed offset — which is what reduces the calendar to the divisions
/// below rather than a table and a special case for February.
///
/// The Gregorian rules fall out of the era: 146097 days is exactly 400
/// years, so a year divisible by 4 is a leap year unless it is
/// divisible by 100 unless it is divisible by 400.
fn civil_from_days(days: i128) -> (i128, i128, i128) {
    // Days from 0000-03-01, the start of the era the epoch falls in.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);

    // The month index counts from March, so its length pattern repeats
    // every five months and the day within it is one division away.
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = match march_month < 10 {
        true => march_month + 3,
        false => march_month - 9,
    };

    // January and February belong to the calendar year after the
    // March-based one they were counted in.
    (year_of_era + era * 400 + i128::from(month <= 2), month, day)
}
