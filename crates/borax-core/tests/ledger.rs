#![allow(clippy::unwrap_used)]

use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_core::ledger::{
    Duplicate, DuplicateReason, Entry, Index, Parsed, RunId, Unparsable, Warning, parse_jsonl,
    serialize_jsonl,
};
use borax_core::record::EntryType;

// ---------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------

fn hash(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A minimal entry: no identifiers, distinguished by `path` and the hash
/// of `hash_seed`'s bytes.
fn entry(path: &str, hash_seed: &str) -> Entry {
    Entry {
        hash: hash(hash_seed),
        doi: None,
        arxiv: None,
        pmid: None,
        isbn: None,
        path: path.to_string(),
        entry_type: EntryType::Article,
        run: RunId::new("run-1"),
        timestamp: "2026-08-19T00:00:00Z".to_string(),
        tool_version: "0.2.0-test".to_string(),
    }
}

fn doi(s: &str) -> Doi {
    Doi::parse(s).unwrap()
}

fn arxiv(s: &str) -> ArxivId {
    ArxivId::parse(s).unwrap()
}

fn pmid(s: &str) -> Pmid {
    Pmid::parse(s).unwrap()
}

fn isbn(s: &str) -> Isbn {
    Isbn::parse(s).unwrap()
}

/// A maximal entry: every optional field set, for round-trip coverage.
fn maximal_entry() -> Entry {
    Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        arxiv: Some(arxiv("2401.12345v2")),
        pmid: Some(pmid("12345678")),
        isbn: Some(isbn("978-1-59327-828-1")),
        entry_type: EntryType::Preprint,
        run: RunId::new("2026-08-19T00-00-00Z-rename-apply"),
        ..entry("smith2024.pdf", "maximal-content")
    }
}

// ---------------------------------------------------------------------
// 1.1 entry model + lossless JSONL round-trip
// ---------------------------------------------------------------------

#[test]
fn minimal_entry_round_trips_losslessly() {
    let original = entry("smith2024.pdf", "a");
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn maximal_entry_round_trips_losslessly() {
    let original = maximal_entry();
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn entry_round_trip_preserves_a_path_with_spaces_and_non_ascii() {
    let original = entry("papers/Bor\u{e1}x notes \u{2014} final.pdf", "b");
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.path, original.path);
    assert_eq!(parsed, original);
}

#[test]
fn entry_round_trip_preserves_run_id_verbatim() {
    let original = entry("a.pdf", "a");
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.run.as_str(), original.run.as_str());
}

#[test]
fn entry_round_trip_preserves_timestamp_and_tool_version_verbatim() {
    // Neither field is parsed or reformatted by the ledger core: they
    // are caller-rendered strings, like the journal's `at`.
    let mut original = entry("a.pdf", "a");
    original.timestamp = "not-strictly-rfc3339-but-verbatim".to_string();
    original.tool_version = "0.1.0+debug".to_string();

    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.timestamp, original.timestamp);
    assert_eq!(parsed.tool_version, original.tool_version);
}

#[test]
fn entry_with_multiple_identifiers_round_trips_and_reports_all_of_them() {
    // A preprint later published carries both an arXiv id and a DOI.
    let original = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        arxiv: Some(arxiv("2401.12345")),
        ..entry("smith2024.pdf", "c")
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: Entry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);

    let identifiers = parsed.identifiers();
    assert_eq!(identifiers.len(), 2);
    assert!(identifiers.contains(&Identifier::Doi(doi("10.1021/jacs.4c01234"))));
    assert!(identifiers.contains(&Identifier::Arxiv(arxiv("2401.12345"))));
}

#[test]
fn entry_with_no_identifiers_reports_an_empty_identifier_list() {
    let e = entry("a.pdf", "a");
    assert_eq!(e.identifiers(), Vec::new());
}

#[test]
fn entry_json_carries_the_documented_fields() {
    let value: serde_json::Value = serde_json::to_value(maximal_entry()).unwrap();
    let object = value.as_object().unwrap();
    for key in [
        "hash",
        "path",
        "entry_type",
        "run",
        "timestamp",
        "tool_version",
    ] {
        assert!(object.contains_key(key), "missing key {key:?}: {value}");
    }
}

// ---------------------------------------------------------------------
// 1.2 parsing: well-formed lines, torn trailing line, mid-file corruption
// ---------------------------------------------------------------------

fn line(e: &Entry) -> String {
    serde_json::to_string(e).unwrap()
}

#[test]
fn parsing_empty_text_yields_no_entries_and_no_warning() {
    let parsed = parse_jsonl("").unwrap();
    assert_eq!(
        parsed,
        Parsed {
            entries: Vec::new(),
            warning: None,
        }
    );
}

#[test]
fn parsing_well_formed_lines_yields_every_entry_in_file_order_with_no_warning() {
    let a = entry("a.pdf", "a");
    let b = entry("b.pdf", "b");
    let text = format!("{}\n{}\n", line(&a), line(&b));

    let parsed = parse_jsonl(&text).unwrap();

    assert_eq!(parsed.entries, vec![a, b]);
    assert_eq!(parsed.warning, None);
}

#[test]
fn a_torn_trailing_line_without_a_newline_is_ignored_with_a_warning() {
    let a = entry("a.pdf", "a");
    // The write was interrupted: the last line has no trailing newline
    // and is not valid JSON.
    let text = format!("{}\n{{\"hash\":\"sha256-dead", line(&a));

    let parsed = parse_jsonl(&text).unwrap();

    assert_eq!(parsed.entries, vec![a]);
    assert_eq!(parsed.warning, Some(Warning::TornTrailingLine));
}

#[test]
fn a_torn_trailing_line_as_the_only_line_yields_no_entries_and_a_warning() {
    let text = "{\"hash\":\"sha256-dead".to_string();

    let parsed = parse_jsonl(&text).unwrap();

    assert_eq!(parsed.entries, Vec::new());
    assert_eq!(parsed.warning, Some(Warning::TornTrailingLine));
}

#[test]
fn mid_file_corruption_before_the_last_line_is_reported_unparsable() {
    let a = entry("a.pdf", "a");
    let b = entry("b.pdf", "b");
    // Line 2 is corrupt but line 3 is a complete, valid entry: this is
    // not a partial write, it is damage to a line the writer finished.
    let text = format!("{}\nnot valid json\n{}\n", line(&a), line(&b));

    let result = parse_jsonl(&text);

    assert_eq!(result, Err(Unparsable { line: 2 }));
}

#[test]
fn a_malformed_last_line_that_is_newline_terminated_is_unparsable_not_torn() {
    // The last line is garbage, but it *is* terminated: a completed
    // append that was corrupted, not one that was cut off mid-write. It
    // must not be silently swallowed the way a torn line is.
    let a = entry("a.pdf", "a");
    let text = format!("{}\nnot valid json\n", line(&a));

    let result = parse_jsonl(&text);

    assert_eq!(result, Err(Unparsable { line: 2 }));
}

#[test]
fn mid_file_corruption_reports_the_one_based_line_number() {
    let a = entry("a.pdf", "a");
    let b = entry("b.pdf", "b");
    let c = entry("c.pdf", "c");
    let text = format!("{}\n{}\nnot valid json\n{}\n", line(&a), line(&b), line(&c));

    let result = parse_jsonl(&text);

    assert_eq!(result, Err(Unparsable { line: 3 }));
}

#[test]
fn parsing_is_deterministic_across_repeated_calls() {
    let a = entry("a.pdf", "a");
    let text = format!("{}\n", line(&a));

    assert_eq!(parse_jsonl(&text), parse_jsonl(&text));
}

// ---------------------------------------------------------------------
// 1.2 in-memory index: hash -> entry, identifier -> entry
// ---------------------------------------------------------------------

#[test]
fn index_looks_up_an_entry_by_its_content_hash() {
    let a = entry("a.pdf", "a");
    let index = Index::build(std::slice::from_ref(&a));

    assert_eq!(index.by_hash(&hash("a")), Some(&a));
}

#[test]
fn index_reports_no_entry_for_an_unknown_hash() {
    let a = entry("a.pdf", "a");
    let index = Index::build(&[a]);

    assert_eq!(index.by_hash(&hash("unknown")), None);
}

#[test]
fn index_looks_up_an_entry_by_identifier() {
    let a = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("a.pdf", "a")
    };
    let index = Index::build(std::slice::from_ref(&a));

    let found = index.by_identifier(&Identifier::Doi(doi("10.1021/jacs.4c01234")));
    assert_eq!(found, Some(&a));
}

#[test]
fn index_reports_no_entry_for_an_unknown_identifier() {
    let a = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("a.pdf", "a")
    };
    let index = Index::build(&[a]);

    let found = index.by_identifier(&Identifier::Doi(doi("10.1038/other")));
    assert_eq!(found, None);
}

#[test]
fn index_looks_up_an_entry_by_any_of_its_several_identifiers() {
    let a = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        arxiv: Some(arxiv("2401.12345")),
        ..entry("a.pdf", "a")
    };
    let index = Index::build(std::slice::from_ref(&a));

    assert_eq!(
        index.by_identifier(&Identifier::Doi(doi("10.1021/jacs.4c01234"))),
        Some(&a)
    );
    assert_eq!(
        index.by_identifier(&Identifier::Arxiv(arxiv("2401.12345"))),
        Some(&a)
    );
}

#[test]
fn index_built_from_an_empty_slice_answers_nothing() {
    let index = Index::build(&[]);
    assert_eq!(index.by_hash(&hash("a")), None);
    assert_eq!(
        index.by_identifier(&Identifier::Doi(doi("10.1021/jacs.4c01234"))),
        None
    );
}

#[test]
fn later_entry_wins_when_two_entries_share_a_hash() {
    // Not a shape a well-formed ledger produces, but the index must
    // still answer deterministically rather than pick arbitrarily.
    let older = entry("old.pdf", "same");
    let newer = entry("new.pdf", "same");

    let index = Index::build(&[older, newer.clone()]);

    assert_eq!(index.by_hash(&hash("same")), Some(&newer));
}

#[test]
fn later_entry_wins_when_two_entries_share_an_identifier() {
    let older = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("old.pdf", "a")
    };
    let newer = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("new.pdf", "b")
    };

    let index = Index::build(&[older, newer.clone()]);

    assert_eq!(
        index.by_identifier(&Identifier::Doi(doi("10.1021/jacs.4c01234"))),
        Some(&newer)
    );
}

// ---------------------------------------------------------------------
// 1.3 dual duplicate check with distinct reasons
// ---------------------------------------------------------------------

#[test]
fn content_duplicate_names_the_existing_path_when_hash_matches() {
    let existing = entry("archived.pdf", "same-bytes");
    let index = Index::build(&[existing]);

    let found = index.content_duplicate(&hash("same-bytes"));

    assert_eq!(
        found,
        Some(Duplicate {
            reason: DuplicateReason::Content,
            existing_path: "archived.pdf".to_string(),
        })
    );
}

#[test]
fn content_duplicate_is_none_when_the_hash_is_unknown() {
    let existing = entry("archived.pdf", "same-bytes");
    let index = Index::build(&[existing]);

    assert_eq!(index.content_duplicate(&hash("different-bytes")), None);
}

#[test]
fn work_duplicate_names_the_existing_path_when_an_identifier_matches() {
    let existing = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("archived.pdf", "original-bytes")
    };
    let index = Index::build(&[existing]);

    // A different file (different hash), same resolved DOI.
    let found = index.work_duplicate(&[Identifier::Doi(doi("10.1021/jacs.4c01234"))]);

    assert_eq!(
        found,
        Some(Duplicate {
            reason: DuplicateReason::Work,
            existing_path: "archived.pdf".to_string(),
        })
    );
}

#[test]
fn work_duplicate_is_none_when_no_identifier_matches() {
    let existing = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("archived.pdf", "original-bytes")
    };
    let index = Index::build(&[existing]);

    let found = index.work_duplicate(&[Identifier::Doi(doi("10.1038/other"))]);

    assert_eq!(found, None);
}

#[test]
fn work_duplicate_is_none_for_an_empty_identifier_list() {
    let existing = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("archived.pdf", "original-bytes")
    };
    let index = Index::build(&[existing]);

    assert_eq!(index.work_duplicate(&[]), None);
}

#[test]
fn work_duplicate_matches_on_a_later_identifier_when_an_earlier_one_is_unknown() {
    let existing = Entry {
        arxiv: Some(arxiv("2401.12345")),
        ..entry("archived.pdf", "original-bytes")
    };
    let index = Index::build(&[existing]);

    let found = index.work_duplicate(&[
        Identifier::Doi(doi("10.1021/jacs.4c01234")),
        Identifier::Arxiv(arxiv("2401.12345")),
    ]);

    assert_eq!(
        found,
        Some(Duplicate {
            reason: DuplicateReason::Work,
            existing_path: "archived.pdf".to_string(),
        })
    );
}

#[test]
fn content_and_work_duplicates_are_reported_with_distinct_reasons() {
    // "Re-downloaded identical file": same bytes.
    let archived_a = entry("a.pdf", "identical-bytes");
    // "Second PDF of an archived paper": same DOI, different bytes.
    let archived_b = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("b.pdf", "b-bytes")
    };
    let index = Index::build(&[archived_a, archived_b]);

    let content = index
        .content_duplicate(&hash("identical-bytes"))
        .expect("content duplicate");
    let work = index
        .work_duplicate(&[Identifier::Doi(doi("10.1021/jacs.4c01234"))])
        .expect("work duplicate");

    assert_eq!(content.reason, DuplicateReason::Content);
    assert_eq!(work.reason, DuplicateReason::Work);
    assert_ne!(content.reason, work.reason);
    assert_eq!(content.existing_path, "a.pdf");
    assert_eq!(work.existing_path, "b.pdf");
}

#[test]
fn a_file_with_unknown_hash_and_no_matching_identifier_is_not_a_duplicate_of_either_kind() {
    let existing = Entry {
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..entry("archived.pdf", "original-bytes")
    };
    let index = Index::build(&[existing]);

    assert_eq!(index.content_duplicate(&hash("new-bytes")), None);
    assert_eq!(
        index.work_duplicate(&[Identifier::Doi(doi("10.1038/other"))]),
        None
    );
}

// ---------------------------------------------------------------------
// 1.4 deterministic rebuild serialization
// ---------------------------------------------------------------------

#[test]
fn serializing_an_empty_slice_yields_an_empty_string() {
    assert_eq!(serialize_jsonl(&[]), String::new());
}

#[test]
fn serialized_entries_are_sorted_by_path() {
    let z = entry("z.pdf", "z");
    let a = entry("a.pdf", "a");
    let m = entry("m.pdf", "m");

    let text = serialize_jsonl(&[z, a, m]);
    let parsed = parse_jsonl(&text).unwrap();

    let paths: Vec<&str> = parsed.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec!["a.pdf", "m.pdf", "z.pdf"]);
}

#[test]
fn serialization_is_byte_identical_regardless_of_input_order() {
    let a = entry("a.pdf", "a");
    let b = entry("b.pdf", "b");
    let c = entry("c.pdf", "c");

    let first = serialize_jsonl(&[a.clone(), b.clone(), c.clone()]);
    let second = serialize_jsonl(&[c, a, b]);

    assert_eq!(first, second);
}

#[test]
fn serializing_the_same_entries_twice_gives_identical_bytes() {
    let entries = vec![entry("a.pdf", "a"), entry("b.pdf", "b")];

    let first = serialize_jsonl(&entries);
    let second = serialize_jsonl(&entries);

    assert_eq!(first, second);
}

#[test]
fn serialized_output_round_trips_with_no_warning() {
    let entries = vec![entry("a.pdf", "a"), entry("b.pdf", "b")];
    let text = serialize_jsonl(&entries);

    let parsed = parse_jsonl(&text).unwrap();

    assert_eq!(parsed.warning, None);
    assert_eq!(parsed.entries.len(), 2);
}

#[test]
fn serialized_output_preserves_every_entry_losslessly() {
    let entries = vec![maximal_entry(), entry("a.pdf", "a")];
    let mut expected = entries.clone();
    expected.sort_by(|a, b| a.path.cmp(&b.path));

    let text = serialize_jsonl(&entries);
    let parsed = parse_jsonl(&text).unwrap();

    assert_eq!(parsed.entries, expected);
}

#[test]
fn paths_with_spaces_and_non_ascii_sort_lexicographically() {
    let umlaut = entry("\u{fc}ber.pdf", "u");
    let space = entry("a file with spaces.pdf", "s");
    let plain = entry("archive.pdf", "p");

    let text = serialize_jsonl(&[umlaut.clone(), space.clone(), plain.clone()]);
    let parsed = parse_jsonl(&text).unwrap();

    let mut expected = vec![plain.path.clone(), space.path.clone(), umlaut.path.clone()];
    expected.sort();
    let paths: Vec<String> = parsed.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(paths, expected);
}

#[test]
fn each_serialized_line_is_newline_terminated() {
    let entries = vec![entry("a.pdf", "a"), entry("b.pdf", "b")];
    let text = serialize_jsonl(&entries);

    assert!(text.ends_with('\n'));
    assert_eq!(text.lines().count(), 2);
}
