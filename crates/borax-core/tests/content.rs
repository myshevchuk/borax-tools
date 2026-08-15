#![allow(clippy::unwrap_used)]

use borax_core::content::{Hasher, hash_bytes};

// --- hash_bytes() against known vectors ---

#[test]
fn hash_bytes_of_empty_input_matches_the_known_sha256_vector() {
    assert_eq!(
        hash_bytes(b"").as_str(),
        "sha256-e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn hash_bytes_of_abc_matches_the_known_sha256_vector() {
    assert_eq!(
        hash_bytes(b"abc").as_str(),
        "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// --- rendered form ---

#[test]
fn rendered_form_has_the_sha256_prefix_and_64_lowercase_hex_digits() {
    let hash = hash_bytes(b"borax");
    let rendered = hash.as_str();
    let hex = rendered
        .strip_prefix("sha256-")
        .expect("rendered hash should start with sha256-");

    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn as_str_and_display_agree() {
    let hash = hash_bytes(b"borax");
    assert_eq!(hash.as_str(), hash.to_string());
}

// --- equality / ordering ---

#[test]
fn equal_inputs_give_equal_hashes() {
    assert_eq!(hash_bytes(b"same bytes"), hash_bytes(b"same bytes"));
}

#[test]
fn different_inputs_give_different_hashes() {
    assert_ne!(hash_bytes(b"left"), hash_bytes(b"right"));
}

#[test]
fn ordering_matches_the_rendered_strings_ordering() {
    let empty = hash_bytes(b"");
    let abc = hash_bytes(b"abc");

    assert_eq!(empty.cmp(&abc), empty.as_str().cmp(abc.as_str()));
    assert!(abc < empty, "\"ba78...\" should sort before \"e3b0...\"");
}

// --- Hasher agrees with hash_bytes over the concatenation ---

#[test]
fn hasher_with_no_updates_matches_hash_bytes_of_an_empty_stream() {
    assert_eq!(Hasher::new().finish(), hash_bytes(b""));
}

#[test]
fn hasher_fed_in_several_chunks_matches_hash_bytes_of_the_concatenation() {
    let mut hasher = Hasher::new();
    hasher.update(b"the quick ");
    hasher.update(b"brown fox jumps ");
    hasher.update(b"over the lazy dog");

    assert_eq!(
        hasher.finish(),
        hash_bytes(b"the quick brown fox jumps over the lazy dog")
    );
}

#[test]
fn hasher_treats_zero_length_chunks_as_no_ops() {
    let mut hasher = Hasher::new();
    hasher.update(b"");
    hasher.update(b"abc");
    hasher.update(b"");

    assert_eq!(hasher.finish(), hash_bytes(b"abc"));
}
