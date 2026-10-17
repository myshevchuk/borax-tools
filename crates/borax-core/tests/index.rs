#![allow(clippy::unwrap_used)]

use borax_core::identifier::supplied;
use borax_core::index::{
    Fact, IndexEntry, extracted_from_json, extracted_to_json, operator_from_json, operator_to_json,
};
use borax_core::library::{EvidenceOrigin, IdentificationEvidence, Malformed};

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
