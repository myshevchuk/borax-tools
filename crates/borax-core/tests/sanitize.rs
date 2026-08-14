#![allow(clippy::unwrap_used)]

use borax_core::sanitize::{MAX_COMPONENT_BYTES, MAX_PATH_BYTES, sanitize};

// ---------------------------------------------------------------------
// Spec scenario
// ---------------------------------------------------------------------

#[test]
fn spec_scenario_windows_hostile_title() {
    // ':' and '?' are forbidden characters (rule 1); the resulting stem
    // "CON_ a study_ of borax" is not itself a reserved device name, so
    // rule 3 does not fire again.
    assert_eq!(
        sanitize("CON: a study? of borax.pdf"),
        "CON_ a study_ of borax.pdf"
    );
}

// ---------------------------------------------------------------------
// Reserved device names
// ---------------------------------------------------------------------

#[test]
fn reserved_stem_gets_underscore_appended_before_extension() {
    assert_eq!(sanitize("CON.pdf"), "CON_.pdf");
}

#[test]
fn reserved_stem_check_is_case_insensitive_but_keeps_original_case() {
    assert_eq!(sanitize("con.pdf"), "con_.pdf");
}

#[test]
fn reserved_stem_with_no_extension_gets_underscore_appended() {
    assert_eq!(sanitize("NUL"), "NUL_");
}

#[test]
fn reserved_com_device_name_is_rewritten() {
    assert_eq!(sanitize("COM7.txt"), "COM7_.txt");
}

#[test]
fn stem_that_only_starts_with_a_reserved_name_is_unchanged() {
    assert_eq!(sanitize("CONX.pdf"), "CONX.pdf");
}

#[test]
fn com_ten_is_not_a_reserved_device_name() {
    // Only COM1-COM9 (and LPT1-LPT9) are reserved; COM10 is a normal
    // stem.
    assert_eq!(sanitize("COM10.txt"), "COM10.txt");
}

// ---------------------------------------------------------------------
// Forbidden characters
// ---------------------------------------------------------------------

#[test]
fn each_windows_forbidden_character_becomes_underscore() {
    assert_eq!(sanitize("a<b>c:d\"e\\f|g?h*i"), "a_b_c_d_e_f_g_h_i");
}

#[test]
fn control_characters_become_underscore() {
    assert_eq!(sanitize("a\u{0007}b"), "a_b");
}

// ---------------------------------------------------------------------
// Trailing dots and spaces
// ---------------------------------------------------------------------

#[test]
fn trailing_dot_and_space_are_trimmed_from_a_component() {
    assert_eq!(sanitize("name. "), "name");
}

#[test]
fn trailing_dot_and_space_are_trimmed_per_component() {
    assert_eq!(sanitize("dir./file. "), "dir/file");
}

// ---------------------------------------------------------------------
// '/' as separator
// ---------------------------------------------------------------------

#[test]
fn forward_slash_separated_components_pass_through_unchanged() {
    assert_eq!(sanitize("a/b/c.pdf"), "a/b/c.pdf");
}

#[test]
fn empty_components_from_repeated_slashes_are_dropped() {
    assert_eq!(sanitize("a//b/"), "a/b");
}

#[test]
fn leading_slash_produces_no_leading_empty_component() {
    assert_eq!(sanitize("/a"), "a");
}

// ---------------------------------------------------------------------
// Empty components
// ---------------------------------------------------------------------

#[test]
fn component_emptied_by_trimming_becomes_underscore() {
    // Trimming trailing dots leaves nothing behind.
    assert_eq!(sanitize("..."), "_");
}

#[test]
fn empty_input_sanitizes_to_underscore() {
    assert_eq!(sanitize(""), "_");
}

#[test]
fn input_of_only_slashes_sanitizes_to_underscore() {
    assert_eq!(sanitize("///"), "_");
}

// ---------------------------------------------------------------------
// Length limits
// ---------------------------------------------------------------------

#[test]
fn overlong_component_is_truncated_to_max_component_bytes_keeping_extension() {
    let stem = "x".repeat(300);
    let input = format!("{stem}.pdf");
    let result = sanitize(&input);
    assert_eq!(result.len(), MAX_COMPONENT_BYTES);
    assert!(result.ends_with(".pdf"));
    assert!(result.trim_end_matches(".pdf").chars().all(|c| c == 'x'));
}

#[test]
fn overlong_multibyte_component_is_truncated_at_a_char_boundary() {
    let stem = "ü".repeat(300);
    let input = format!("{stem}.pdf");
    let result = sanitize(&input);
    // `String` guarantees valid UTF-8; the remaining assertions confirm
    // the truncation actually enforced the byte cap.
    assert!(result.len() <= MAX_COMPONENT_BYTES);
    assert!(result.ends_with(".pdf"));
}

#[test]
fn overlong_total_path_is_shortened_by_truncating_the_last_component() {
    // 400 single-character directory components keep the directory
    // portion well under MAX_PATH_BYTES on its own, so the overflow can
    // only be resolved by truncating the last component's stem.
    let dirs = "d/".repeat(400);
    let last_stem = "a".repeat(300);
    let input = format!("{dirs}{last_stem}.pdf");
    let result = sanitize(&input);
    assert!(result.len() <= MAX_PATH_BYTES);
    assert!(result.ends_with(".pdf"));
}

// ---------------------------------------------------------------------
// Unicode
// ---------------------------------------------------------------------

#[test]
fn unicode_characters_outside_the_forbidden_set_pass_through() {
    assert_eq!(sanitize("Grüße – borax.pdf"), "Grüße – borax.pdf");
}

// ---------------------------------------------------------------------
// Idempotence
// ---------------------------------------------------------------------

#[test]
fn sanitize_is_idempotent() {
    let inputs = [
        "CON: a study? of borax.pdf",
        "CON.pdf",
        "a<b>c:d\"e\\f|g?h*i",
        "name. ",
        "dir./file. ",
        "a//b/",
        "...",
        "",
        "Grüße – borax.pdf",
    ];
    for input in inputs {
        let once = sanitize(input);
        let twice = sanitize(&once);
        assert_eq!(twice, once, "input {input:?}");
    }
}
