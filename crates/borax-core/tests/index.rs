#![allow(clippy::unwrap_used)]

use borax_core::identifier::supplied;
use std::path::PathBuf;

use borax_core::content::hash_bytes;
use borax_core::index::{
    Fact, IndexEntry, LibraryReference, extracted_from_json, extracted_to_json, operator_from_json,
    operator_to_json, reference_from_json, reference_to_json,
};
use borax_core::library::{
    ArtifactId, EvidenceOrigin, IdentificationEvidence, ItemId, LibraryId, Malformed,
};

// ---------------------------------------------------------------------
// 2.1 extracted.json: `{"identifier":"…"}` (design D2, D10)
// ---------------------------------------------------------------------

#[test]
fn extracted_fact_round_trips_through_json_for_each_identifier_kind() {
    let identifiers = [
        supplied("doi:10.1021/jacs.4c01234").unwrap(),
        supplied("arXiv:2401.12345").unwrap(),
        supplied("pmid:12345678").unwrap(),
        supplied("isbn:978-1-59327-828-1").unwrap(),
    ];

    for identifier in identifiers {
        let text = extracted_to_json(&identifier);
        let parsed = extracted_from_json(&text)
            .unwrap_or_else(|error| panic!("{identifier} did not round-trip: {error}"));
        assert_eq!(parsed, identifier);
    }
}

#[test]
fn extracted_fact_is_written_as_its_identifier_alone() {
    let identifier = supplied("doi:10.1000/x").unwrap();

    assert_eq!(
        extracted_to_json(&identifier),
        r#"{"identifier":"doi:10.1000/x"}"#
    );
}

// ---------------------------------------------------------------------
// 2.1 operator.json: `{"identifier":"…","origin":"supplied"|"overridden"}`
// ---------------------------------------------------------------------

#[test]
fn operator_fact_round_trips_for_supplied_and_overridden() {
    for origin in [EvidenceOrigin::Supplied, EvidenceOrigin::Overridden] {
        let evidence = IdentificationEvidence {
            identifier: supplied("doi:10.1000/x").unwrap(),
            origin,
        };

        let text = operator_to_json(&evidence);
        let parsed = operator_from_json(&text)
            .unwrap_or_else(|error| panic!("{origin:?} did not round-trip: {error}"));
        assert_eq!(parsed, evidence);
    }
}

#[test]
fn operator_fact_with_origin_extracted_or_adopted_does_not_parse() {
    for origin in ["extracted", "adopted"] {
        let text = format!(r#"{{"identifier":"doi:10.1000/x","origin":"{origin}"}}"#);

        assert!(
            matches!(operator_from_json(&text), Err(Malformed::Toml(_))),
            "{origin} must not parse as an operator fact"
        );
    }
}

#[test]
fn a_fact_ignores_keys_it_does_not_know() {
    let text = r#"{"identifier":"doi:10.1000/x","unknown":"value"}"#;

    let parsed = extracted_from_json(text).expect("an unknown key must not stop parsing");

    assert_eq!(parsed, supplied("doi:10.1000/x").unwrap());
}

// ---------------------------------------------------------------------
// 2.1 IndexEntry::answer() (design D2's table)
// ---------------------------------------------------------------------

#[test]
fn answer_is_the_operators_evidence_when_both_are_held() {
    let operator = IdentificationEvidence {
        identifier: supplied("doi:10.1000/op").unwrap(),
        origin: EvidenceOrigin::Supplied,
    };
    let entry = IndexEntry {
        extracted: Fact::Held(supplied("doi:10.1000/ext").unwrap()),
        operator: Fact::Held(operator.clone()),
    };

    assert_eq!(entry.answer(), Some(operator));
}

#[test]
fn answer_is_the_extracted_identifier_when_only_it_is_held() {
    let identifier = supplied("doi:10.1000/ext").unwrap();
    let entry = IndexEntry {
        extracted: Fact::Held(identifier.clone()),
        operator: Fact::Absent,
    };

    assert_eq!(
        entry.answer(),
        Some(IdentificationEvidence {
            identifier,
            origin: EvidenceOrigin::Extracted,
        })
    );
}

#[test]
fn answer_is_none_when_operator_is_unreadable_even_beside_an_extracted_identifier() {
    let entry = IndexEntry {
        extracted: Fact::Held(supplied("doi:10.1000/ext").unwrap()),
        operator: Fact::Unreadable("broken".to_string()),
    };

    assert_eq!(
        entry.answer(),
        None,
        "an unreadable operator.json withholds the extracted identifier beside it"
    );
}

#[test]
fn answer_is_the_operators_when_extracted_is_unreadable() {
    let operator = IdentificationEvidence {
        identifier: supplied("doi:10.1000/op").unwrap(),
        origin: EvidenceOrigin::Overridden,
    };
    let entry = IndexEntry {
        extracted: Fact::Unreadable("broken".to_string()),
        operator: Fact::Held(operator.clone()),
    };

    assert_eq!(entry.answer(), Some(operator));
}

#[test]
fn answer_is_none_when_both_are_absent() {
    let entry = IndexEntry {
        extracted: Fact::Absent,
        operator: Fact::Absent,
    };

    assert_eq!(entry.answer(), None);
}

// ---------------------------------------------------------------------
// 1.1 a reference: `{"library","root","artifact","item"}` (design D1)
// ---------------------------------------------------------------------

const LIBRARY: &str = "018f2b36-7f21-7abc-8def-0123456789ab";
const ARTIFACT: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef0";
const ITEM: &str = "018f2b37-0000-7000-8000-000000000001";

fn reference_at(root: &str) -> LibraryReference {
    LibraryReference {
        library: LibraryId::parse(LIBRARY).unwrap(),
        root: PathBuf::from(root),
        artifact: ArtifactId::parse(ARTIFACT).unwrap(),
        item: ItemId::parse(ITEM).unwrap(),
    }
}

#[test]
fn reference_round_trips_through_json() {
    let reference = reference_at("/home/u/papers");

    let text = reference_to_json(&reference).expect("a UTF-8 root has JSON");
    let parsed = reference_from_json(&text)
        .unwrap_or_else(|error| panic!("{text} did not round-trip: {error}"));

    assert_eq!(parsed, reference);
}

#[test]
fn reference_is_written_as_library_root_artifact_and_item() {
    let text = reference_to_json(&reference_at("/home/u/papers")).unwrap();

    assert_eq!(
        text,
        format!(
            r#"{{"library":"{LIBRARY}","root":"/home/u/papers","artifact":"{ARTIFACT}","item":"{ITEM}"}}"#
        )
    );
}

#[test]
fn reference_ignores_keys_it_does_not_know() {
    let text = format!(
        r#"{{"library":"{LIBRARY}","root":"/r","artifact":"{ARTIFACT}","item":"{ITEM}","unknown":"value"}}"#
    );

    let parsed = reference_from_json(&text).expect("an unknown key must not stop parsing");

    assert_eq!(parsed, reference_at("/r"));
}

#[test]
fn reference_missing_a_key_does_not_parse() {
    for missing in ["library", "root", "artifact", "item"] {
        let mut keys = vec![
            ("library", LIBRARY),
            ("root", "/r"),
            ("artifact", ARTIFACT),
            ("item", ITEM),
        ];
        keys.retain(|(key, _)| *key != missing);
        let body: Vec<String> = keys
            .iter()
            .map(|(key, value)| format!(r#""{key}":"{value}""#))
            .collect();
        let text = format!("{{{}}}", body.join(","));

        assert!(
            matches!(reference_from_json(&text), Err(Malformed::Toml(_))),
            "a reference without {missing} must not parse: {text}"
        );
    }
}

#[test]
fn reference_with_an_identity_that_is_not_canonical_does_not_parse() {
    let upper = LIBRARY.to_uppercase();
    let unhyphenated = ARTIFACT.replace('-', "");
    let cases = [
        ("library", upper.as_str()),
        ("library", "not-a-uuid"),
        ("artifact", unhyphenated.as_str()),
        ("item", ""),
    ];

    for (key, bad) in cases {
        let mut values = [
            ("library", LIBRARY),
            ("root", "/r"),
            ("artifact", ARTIFACT),
            ("item", ITEM),
        ];
        for entry in &mut values {
            if entry.0 == key {
                entry.1 = bad;
            }
        }
        let body: Vec<String> = values
            .iter()
            .map(|(key, value)| format!(r#""{key}":"{value}""#))
            .collect();
        let text = format!("{{{}}}", body.join(","));

        assert!(
            matches!(reference_from_json(&text), Err(Malformed::Toml(_))),
            "{key} {bad:?} is not canonical and must not parse"
        );
    }
}

#[test]
fn reference_name_is_the_sha256_of_library_root_and_artifact_joined_by_nul() {
    let reference = reference_at("/home/u/papers");
    let joined = format!("{LIBRARY}\0/home/u/papers\0{ARTIFACT}");

    assert_eq!(
        reference.name(),
        Some(hash_bytes(joined.as_bytes()).as_str().to_string())
    );
}

#[test]
fn reference_name_does_not_depend_on_the_item() {
    let reference = reference_at("/r");
    let other = LibraryReference {
        item: ItemId::parse("018f2b37-0000-7000-8000-000000000002").unwrap(),
        ..reference.clone()
    };

    assert!(reference.name().is_some());
    assert_eq!(reference.name(), other.name());
    assert!(reference.same_key(&other));
}

#[test]
fn reference_name_differs_for_another_root() {
    let first = reference_at("/one");
    let second = reference_at("/two");

    assert!(first.name().is_some());
    assert_ne!(first.name(), second.name());
    assert!(!first.same_key(&second));
}

#[cfg(unix)]
#[test]
fn reference_with_a_root_that_is_not_utf8_has_no_name_and_no_json() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let reference = LibraryReference {
        root: PathBuf::from(OsStr::from_bytes(b"/home/\xff/papers")),
        ..reference_at("/r")
    };

    assert_eq!(reference.name(), None);
    assert_eq!(reference_to_json(&reference), None);
}
