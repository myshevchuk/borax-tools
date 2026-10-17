#![allow(clippy::unwrap_used)]

use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::supplied;
use borax_core::library::{
    ArtifactId, ArtifactRecord, EvidenceOrigin, HashEntry, IdentificationEvidence, Item, ItemId,
    LibraryId, LibraryIdentity, Malformed, RunId, is_library_relative, is_well_formed_hash,
    name_uuid,
};
use borax_core::record::{EntryType, Record};

// ---------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------

/// A canonical, lowercase, hyphenated UUID literal — this text is what
/// `ItemId`/`ArtifactId` parsing accepts and what rendering must
/// reproduce byte-identically.
const UUID_A: &str = "018f2b36-7f21-7abc-8def-0123456789ab";
/// A second, distinct canonical UUID, used wherever a test needs two
/// identities or wants to show one disagreeing with another.
const UUID_B: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef0";
/// A third distinct canonical UUID, for tests that need to tell apart a
/// record's link, an entry's own item, and a third item neither names.
const UUID_C: &str = "018f2b37-0000-7000-8000-000000000001";
/// A fourth distinct canonical UUID, for the same reason as [`UUID_C`].
const UUID_D: &str = "018f2b37-0000-7000-8000-000000000002";

fn hash(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A [`ContentHash`] built from arbitrary text rather than from real
/// bytes, bypassing `hash_bytes` so a malformed shape can be built for
/// [`is_well_formed_hash`] — deserialization does not re-validate the
/// shape, which is the point of this helper.
fn malformed_hash(text: &str) -> ContentHash {
    serde_json::from_value(serde_json::json!(text)).unwrap()
}

fn hash_entry(seed: &str, run: &str, timestamp: &str, tool_version: &str) -> HashEntry {
    HashEntry {
        hash: hash(seed),
        run: RunId::new(run),
        timestamp: timestamp.to_string(),
        tool_version: tool_version.to_string(),
        item: None,
        evidence: None,
    }
}

/// A minimal but valid record of `entry_type`, enough to round-trip.
fn minimal_record(entry_type: EntryType) -> Record {
    let mut record = Record::new(entry_type);
    record.title = Some("A Title".to_string());
    record
}

/// Sets `record.borax.source_fields[key]` to `value` on a fresh,
/// minimal `Article` record.
fn record_with_source_field(key: &str, value: serde_json::Value) -> Record {
    let mut record = minimal_record(EntryType::Article);
    record.borax.source_fields.insert(key.to_string(), value);
    record
}

/// Builds an `Item` carrying one source field, round-trips it through
/// `Item::to_toml`/`Item::from_toml`, asserts the round trip raised no
/// fault, and returns the field's value as it came back.
fn round_trip_source_field(value: serde_json::Value) -> serde_json::Value {
    let id = ItemId::parse(UUID_A).unwrap();
    let record = record_with_source_field("field", value);
    let item = Item { id, record };

    let parsed = Item::from_toml(&item.to_toml()).unwrap();
    assert!(
        parsed.faults.is_empty(),
        "unexpected fault(s): {:?}",
        parsed
            .faults
            .iter()
            .map(|fault| &fault.key)
            .collect::<Vec<_>>()
    );

    parsed
        .item
        .record
        .borax
        .source_fields
        .get("field")
        .expect("the field must survive a successful round trip")
        .clone()
}

// ---------------------------------------------------------------------
// 1.1 ItemId
// ---------------------------------------------------------------------

#[test]
fn item_id_parses_a_canonical_uuid_and_renders_it_back_byte_identically() {
    let id = ItemId::parse(UUID_A).unwrap();
    assert_eq!(id.to_string(), UUID_A);
}

#[test]
fn item_id_uuid_returns_the_parsed_uuid() {
    let id = ItemId::parse(UUID_A).unwrap();
    assert_eq!(id.uuid(), uuid::Uuid::parse_str(UUID_A).unwrap());
}

#[test]
fn item_id_from_uuid_renders_the_same_canonical_text() {
    let uuid = uuid::Uuid::parse_str(UUID_A).unwrap();
    let id = ItemId::from_uuid(uuid);
    assert_eq!(id.to_string(), UUID_A);
    assert_eq!(id.uuid(), uuid);
}

#[test]
fn item_id_refuses_an_uppercase_spelling() {
    assert_eq!(ItemId::parse(&UUID_A.to_uppercase()), None);
}

#[test]
fn item_id_refuses_a_braced_spelling() {
    assert_eq!(ItemId::parse(&format!("{{{UUID_A}}}")), None);
}

#[test]
fn item_id_refuses_a_urn_prefixed_spelling() {
    assert_eq!(ItemId::parse(&format!("urn:uuid:{UUID_A}")), None);
}

#[test]
fn item_id_refuses_a_bare_hex_run_with_no_hyphens() {
    assert_eq!(ItemId::parse(&UUID_A.replace('-', "")), None);
}

#[test]
fn item_id_refuses_a_string_that_is_not_a_uuid_at_all() {
    assert_eq!(ItemId::parse("not-a-uuid"), None);
}

#[test]
fn item_id_equality_holds_for_two_parses_of_the_same_text() {
    assert_eq!(
        ItemId::parse(UUID_A).unwrap(),
        ItemId::parse(UUID_A).unwrap()
    );
}

#[test]
fn item_id_compares_and_sorts_by_the_text_order_of_the_rendered_form() {
    let a = ItemId::parse(UUID_A).unwrap();
    let b = ItemId::parse(UUID_B).unwrap();

    assert_eq!(a.cmp(&b), UUID_A.cmp(UUID_B));

    let mut ids = [b, a];
    ids.sort();
    let rendered: Vec<String> = ids.iter().map(ToString::to_string).collect();

    let mut expected = vec![UUID_A.to_string(), UUID_B.to_string()];
    expected.sort();
    assert_eq!(rendered, expected);
}

// ---------------------------------------------------------------------
// 1.1 ArtifactId (identical surface to ItemId)
// ---------------------------------------------------------------------

#[test]
fn artifact_id_parses_a_canonical_uuid_and_renders_it_back_byte_identically() {
    let id = ArtifactId::parse(UUID_A).unwrap();
    assert_eq!(id.to_string(), UUID_A);
}

#[test]
fn artifact_id_uuid_returns_the_parsed_uuid() {
    let id = ArtifactId::parse(UUID_A).unwrap();
    assert_eq!(id.uuid(), uuid::Uuid::parse_str(UUID_A).unwrap());
}

#[test]
fn artifact_id_from_uuid_renders_the_same_canonical_text() {
    let uuid = uuid::Uuid::parse_str(UUID_A).unwrap();
    let id = ArtifactId::from_uuid(uuid);
    assert_eq!(id.to_string(), UUID_A);
    assert_eq!(id.uuid(), uuid);
}

#[test]
fn artifact_id_refuses_an_uppercase_spelling() {
    assert_eq!(ArtifactId::parse(&UUID_A.to_uppercase()), None);
}

#[test]
fn artifact_id_refuses_a_braced_spelling() {
    assert_eq!(ArtifactId::parse(&format!("{{{UUID_A}}}")), None);
}

#[test]
fn artifact_id_refuses_a_urn_prefixed_spelling() {
    assert_eq!(ArtifactId::parse(&format!("urn:uuid:{UUID_A}")), None);
}

#[test]
fn artifact_id_refuses_a_bare_hex_run_with_no_hyphens() {
    assert_eq!(ArtifactId::parse(&UUID_A.replace('-', "")), None);
}

#[test]
fn artifact_id_refuses_a_string_that_is_not_a_uuid_at_all() {
    assert_eq!(ArtifactId::parse("not-a-uuid"), None);
}

#[test]
fn artifact_id_equality_holds_for_two_parses_of_the_same_text() {
    assert_eq!(
        ArtifactId::parse(UUID_A).unwrap(),
        ArtifactId::parse(UUID_A).unwrap()
    );
}

#[test]
fn artifact_id_compares_and_sorts_by_the_text_order_of_the_rendered_form() {
    let a = ArtifactId::parse(UUID_A).unwrap();
    let b = ArtifactId::parse(UUID_B).unwrap();

    assert_eq!(a.cmp(&b), UUID_A.cmp(UUID_B));

    let mut ids = [b, a];
    ids.sort();
    let rendered: Vec<String> = ids.iter().map(ToString::to_string).collect();

    let mut expected = vec![UUID_A.to_string(), UUID_B.to_string()];
    expected.sort();
    assert_eq!(rendered, expected);
}

// ---------------------------------------------------------------------
// 1.3 Item: TOML round trip, every EntryType
// ---------------------------------------------------------------------

#[test]
fn item_round_trips_through_toml_for_every_entry_type() {
    let entry_types = [
        EntryType::Article,
        EntryType::Preprint,
        EntryType::Book,
        EntryType::Chapter,
        EntryType::Thesis,
        EntryType::Report,
        EntryType::Patent,
        EntryType::Standard,
    ];

    for entry_type in entry_types {
        let id = ItemId::parse(UUID_A).unwrap();
        let record = minimal_record(entry_type);
        let item = Item {
            id: id.clone(),
            record: record.clone(),
        };

        let parsed = Item::from_toml(&item.to_toml())
            .unwrap_or_else(|error| panic!("{entry_type:?} did not round-trip: {error}"));

        assert!(
            parsed.faults.is_empty(),
            "{entry_type:?} raised unexpected faults"
        );
        assert_eq!(parsed.item.id, id, "{entry_type:?} lost its identity");
        assert_eq!(parsed.item.record, record, "{entry_type:?} lost its record");
    }
}

#[test]
fn item_to_toml_writes_the_id_and_the_record_table_at_the_top_level() {
    let id = ItemId::parse(UUID_A).unwrap();
    let record = minimal_record(EntryType::Article);
    let item = Item { id, record };

    let text = item.to_toml();

    assert!(
        text.contains(&format!("\"{UUID_A}\"")),
        "the item's uuid must appear verbatim in the file: {text}"
    );
    assert!(
        text.contains("[record]"),
        "the record must live under a [record] table: {text}"
    );
}

// ---------------------------------------------------------------------
// 1.3 Item: source-fields, the JSON-text encoding of design D11
// ---------------------------------------------------------------------

#[test]
fn item_round_trip_preserves_a_null_source_field() {
    assert_eq!(
        round_trip_source_field(serde_json::Value::Null),
        serde_json::Value::Null
    );
}

#[test]
fn item_round_trip_distinguishes_a_string_from_the_number_it_looks_like() {
    let as_string = round_trip_source_field(serde_json::json!("42"));
    let as_number = round_trip_source_field(serde_json::json!(42));

    assert_eq!(as_string, serde_json::json!("42"));
    assert_eq!(as_number, serde_json::json!(42));
    assert_ne!(as_string, as_number);
}

#[test]
fn item_round_trip_distinguishes_the_string_null_from_an_actual_null() {
    let as_string = round_trip_source_field(serde_json::json!("null"));
    let as_null = round_trip_source_field(serde_json::Value::Null);

    assert_eq!(as_string, serde_json::json!("null"));
    assert_eq!(as_null, serde_json::Value::Null);
    assert_ne!(as_string, as_null);
}

#[test]
fn item_round_trip_distinguishes_a_string_from_a_nested_object_it_resembles() {
    let text = r#"{"a":1}"#;
    let as_string = round_trip_source_field(serde_json::json!(text));
    let as_object = round_trip_source_field(serde_json::json!({"a": 1}));

    assert_eq!(as_string, serde_json::json!(text));
    assert_eq!(as_object, serde_json::json!({"a": 1}));
    assert_ne!(as_string, as_object);
}

#[test]
fn item_round_trip_preserves_a_heterogeneous_array() {
    let value = serde_json::json!([1, "two", null, 4.5, {"k": "v"}, [true, false]]);
    assert_eq!(round_trip_source_field(value.clone()), value);
}

#[test]
fn item_round_trip_preserves_i64_min() {
    let value = serde_json::json!(i64::MIN);
    assert_eq!(round_trip_source_field(value.clone()), value);
}

#[test]
fn item_round_trip_preserves_u64_max() {
    let value = serde_json::json!(u64::MAX);
    assert_eq!(round_trip_source_field(value.clone()), value);
}

#[test]
fn item_round_trip_preserves_an_integral_float_as_a_float() {
    let value = serde_json::json!(3.0_f64);
    let result = round_trip_source_field(value.clone());

    assert_eq!(result, value);
    assert!(
        result.is_f64(),
        "3.0 must stay a JSON float, not become an integer: {result}"
    );
}

#[test]
fn item_round_trip_reflects_the_normalised_form_not_provider_bytes() {
    // The provider's own whitespace and key order are already gone
    // before the store ever sees the value: `source_fields` holds a
    // `serde_json::Value`, already parsed from the provider's response.
    // Parsing strips whitespace and orders an object's keys, so what a
    // round trip returns is that normalised form, not the provider's
    // original bytes.
    let raw = "{\"b\" : 1,\n  \"a\": 2}";
    let normalised: serde_json::Value = serde_json::from_str(raw).unwrap();

    let round_tripped = round_trip_source_field(normalised.clone());
    assert_eq!(round_tripped, normalised);

    let rendered = serde_json::to_string(&round_tripped).unwrap();
    assert_ne!(
        rendered, raw,
        "the round trip must not resurrect the provider's own bytes"
    );
    assert_eq!(rendered, serde_json::to_string(&normalised).unwrap());
}

#[test]
fn item_source_field_with_unparsable_json_text_is_a_fault_not_an_error() {
    let text = format!(
        "id = \"{}\"\n\n\
         [record]\n\
         type = \"article-journal\"\n\
         title = \"A Title\"\n\n\
         [record.borax.source_fields]\n\
         bad-key = \"not-json{{\"\n",
        UUID_A
    );

    let parsed = Item::from_toml(&text)
        .expect("a field whose JSON text does not parse is a fault, not an error");

    assert_eq!(parsed.faults.len(), 1);
    assert_eq!(parsed.faults[0].key, "bad-key");
    assert!(
        !parsed
            .item
            .record
            .borax
            .source_fields
            .contains_key("bad-key"),
        "a field that failed to parse must not survive into the record"
    );
    assert_eq!(parsed.item.id, ItemId::parse(UUID_A).unwrap());
    assert_eq!(parsed.item.record.title.as_deref(), Some("A Title"));
}

// ---------------------------------------------------------------------
// 1.5 ArtifactRecord: TOML round trip
// ---------------------------------------------------------------------

#[test]
fn artifact_record_round_trips_with_one_history_entry_and_an_item_link() {
    let id = ArtifactId::parse(UUID_A).unwrap();
    let item = ItemId::parse(UUID_B).unwrap();
    let entry = hash_entry("bytes-1", "run-1", "2026-08-19T00:00:00Z", "0.5.1");
    let original = ArtifactRecord {
        id,
        item: Some(item),
        history: vec![entry],
        path: "smith2024.pdf".to_string(),
        size: 123_456,
        modified_millis: 1_734_000_000_123,
    };

    let parsed = ArtifactRecord::from_toml(&original.to_toml()).unwrap();

    assert_eq!(parsed.id, original.id);
    assert_eq!(parsed.item, original.item);
    assert_eq!(parsed.history.len(), 1);
    assert_eq!(parsed.history[0].hash, original.history[0].hash);
    assert_eq!(
        parsed.history[0].run.as_str(),
        original.history[0].run.as_str()
    );
    assert_eq!(parsed.history[0].timestamp, original.history[0].timestamp);
    assert_eq!(
        parsed.history[0].tool_version,
        original.history[0].tool_version
    );
    assert_eq!(parsed.path, original.path);
    assert_eq!(parsed.size, original.size);
    assert_eq!(parsed.modified_millis, original.modified_millis);
}

#[test]
fn artifact_record_round_trips_several_history_entries_in_order() {
    let original = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![
            hash_entry("bytes-1", "run-1", "2026-08-19T00:00:00Z", "0.5.1"),
            hash_entry("bytes-2", "run-2", "2026-08-20T00:00:00Z", "0.5.2"),
            hash_entry("bytes-3", "run-3", "2026-08-21T00:00:00Z", "0.5.3"),
        ],
        path: "a/b/paper.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let parsed = ArtifactRecord::from_toml(&original.to_toml()).unwrap();

    let expected_hashes: Vec<_> = original.history.iter().map(|e| e.hash.clone()).collect();
    let actual_hashes: Vec<_> = parsed.history.iter().map(|e| e.hash.clone()).collect();
    assert_eq!(
        actual_hashes, expected_hashes,
        "history order must be preserved oldest first"
    );

    let runs: Vec<&str> = parsed.history.iter().map(|e| e.run.as_str()).collect();
    assert_eq!(runs, vec!["run-1", "run-2", "run-3"]);
}

#[test]
fn artifact_record_with_no_item_link_is_representable() {
    let original = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![hash_entry("bytes-1", "run-1", "t1", "v1")],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let parsed = ArtifactRecord::from_toml(&original.to_toml()).unwrap();

    assert_eq!(parsed.item, None);
}

#[test]
fn artifact_record_with_an_empty_history_parses_as_a_finding_not_an_error() {
    let original = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: Vec::new(),
        path: "x.pdf".to_string(),
        size: 0,
        modified_millis: 0,
    };

    let parsed = ArtifactRecord::from_toml(&original.to_toml())
        .expect("an empty history is a validator finding, not a parse error");

    assert!(parsed.history.is_empty());
}

#[test]
fn artifact_record_whose_history_entry_names_no_run_parses() {
    let original = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![hash_entry("bytes-1", "", "2026-08-19T00:00:00Z", "0.5.1")],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let parsed = ArtifactRecord::from_toml(&original.to_toml())
        .expect("a history entry naming no run is a finding, not a parse error");

    assert_eq!(parsed.history[0].run.as_str(), "");
}

#[test]
fn artifact_record_round_trips_path_size_and_modified_millis() {
    let original = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![hash_entry("bytes-1", "run-1", "t1", "v1")],
        path: "sub/dir/paper.pdf".to_string(),
        size: 9_876_543,
        modified_millis: 1_734_000_000_123,
    };

    let parsed = ArtifactRecord::from_toml(&original.to_toml()).unwrap();

    assert_eq!(parsed.path, original.path);
    assert_eq!(parsed.size, original.size);
    assert_eq!(parsed.modified_millis, original.modified_millis);
}

// ---------------------------------------------------------------------
// 1.5 ArtifactRecord: current_hash and holds
// ---------------------------------------------------------------------

#[test]
fn artifact_record_current_hash_is_the_newest_recorded_hash() {
    let first = hash_entry("bytes-1", "run-1", "t1", "v1");
    let second = hash_entry("bytes-2", "run-2", "t2", "v2");
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![first, second.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.current_hash(), Some(&second.hash));
}

#[test]
fn artifact_record_current_hash_is_none_for_an_empty_history() {
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: Vec::new(),
        path: "x.pdf".to_string(),
        size: 0,
        modified_millis: 0,
    };

    assert_eq!(record.current_hash(), None);
}

#[test]
fn artifact_record_holds_reports_any_hash_in_its_history() {
    let first = hash_entry("bytes-1", "run-1", "t1", "v1");
    let second = hash_entry("bytes-2", "run-2", "t2", "v2");
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![first.clone(), second.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert!(record.holds(&first.hash));
    assert!(record.holds(&second.hash));
    assert!(!record.holds(&hash("never-recorded")));
}

// ---------------------------------------------------------------------
// 1.7 name_uuid
// ---------------------------------------------------------------------

#[test]
fn name_uuid_reads_the_uuid_before_the_toml_extension() {
    let name = format!("milner1978.{UUID_A}.toml");
    assert_eq!(
        name_uuid(&name),
        Some(uuid::Uuid::parse_str(UUID_A).unwrap())
    );
}

#[test]
fn name_uuid_reads_a_bare_uuid_file_name() {
    let name = format!("{UUID_A}.toml");
    assert_eq!(
        name_uuid(&name),
        Some(uuid::Uuid::parse_str(UUID_A).unwrap())
    );
}

#[test]
fn name_uuid_is_none_for_a_name_with_no_uuid_at_all() {
    assert_eq!(name_uuid("notes-on-this.toml"), None);
}

#[test]
fn name_uuid_agrees_with_a_matching_identity() {
    let id = ItemId::parse(UUID_A).unwrap();
    let name = format!("milner1978.{UUID_A}.toml");
    let found = name_uuid(&name).expect("the name carries a uuid");
    assert_eq!(found, id.uuid());
}

#[test]
fn name_uuid_can_disagree_with_a_parsed_identity() {
    let id = ItemId::parse(UUID_A).unwrap();
    let name = format!("milner1978.{UUID_B}.toml");
    let found = name_uuid(&name).expect("the name carries a uuid");
    assert_ne!(found, id.uuid());
}

// ---------------------------------------------------------------------
// 1.7 is_library_relative
// ---------------------------------------------------------------------

#[test]
fn is_library_relative_accepts_a_well_formed_nested_path() {
    assert!(is_library_relative("sub/dir/paper.pdf"));
}

#[test]
fn is_library_relative_accepts_a_single_segment_path() {
    assert!(is_library_relative("paper.pdf"));
}

#[test]
fn is_library_relative_refuses_an_absolute_path() {
    assert!(!is_library_relative("/sub/paper.pdf"));
}

#[test]
fn is_library_relative_refuses_a_dot_dot_component() {
    assert!(!is_library_relative("sub/../paper.pdf"));
}

#[test]
fn is_library_relative_refuses_a_dot_component() {
    assert!(!is_library_relative("./paper.pdf"));
}

#[test]
fn is_library_relative_refuses_a_backslash() {
    assert!(!is_library_relative("sub\\paper.pdf"));
}

#[test]
fn is_library_relative_refuses_an_empty_segment() {
    assert!(!is_library_relative("sub//paper.pdf"));
}

#[test]
fn is_library_relative_refuses_a_trailing_slash() {
    assert!(!is_library_relative("sub/paper.pdf/"));
}

#[test]
fn is_library_relative_refuses_an_empty_path() {
    assert!(!is_library_relative(""));
}

// ---------------------------------------------------------------------
// 1.7 is_well_formed_hash
// ---------------------------------------------------------------------

#[test]
fn is_well_formed_hash_accepts_a_hash_from_hash_bytes() {
    assert!(is_well_formed_hash(&hash("anything")));
}

#[test]
fn is_well_formed_hash_refuses_the_wrong_length() {
    assert!(!is_well_formed_hash(&malformed_hash("sha256-deadbeef")));
}

#[test]
fn is_well_formed_hash_refuses_uppercase_hex() {
    let uppercase = hash("anything").as_str().to_uppercase();
    assert!(!is_well_formed_hash(&malformed_hash(&uppercase)));
}

#[test]
fn is_well_formed_hash_refuses_a_missing_prefix() {
    let without_prefix = hash("anything")
        .as_str()
        .strip_prefix("sha256-")
        .unwrap()
        .to_string();
    assert!(!is_well_formed_hash(&malformed_hash(&without_prefix)));
}

// ---------------------------------------------------------------------
// 1.1 History entries name their item (design D6, D7, D11)
// ---------------------------------------------------------------------

#[test]
fn hash_entry_item_round_trips_through_toml() {
    let item = ItemId::parse(UUID_B).unwrap();
    let mut entry = hash_entry("bytes-1", "run-1", "t1", "v1");
    entry.item = Some(item.clone());
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(item.clone()),
        history: vec![entry],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let parsed = ArtifactRecord::from_toml(&record.to_toml()).unwrap();

    assert_eq!(parsed.history[0].item, Some(item));
}

#[test]
fn a_hash_entry_with_no_item_writes_no_item_key() {
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(ItemId::parse(UUID_B).unwrap()),
        history: vec![hash_entry("bytes-1", "run-1", "t1", "v1")],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let text = record.to_toml();
    let history_start = text.find("[[history]]").expect("a history table");
    let history_text = &text[history_start..];

    assert!(
        !history_text.contains("item"),
        "a hash entry naming no item must write no item key: {text}"
    );
}

#[test]
fn a_record_written_before_entries_named_items_parses_with_no_item() {
    let h1 = hash("bytes-1");
    let h2 = hash("bytes-2");
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{h1}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\n\
         [[history]]\n\
         hash = \"{h2}\"\n\
         run = \"run-2\"\n\
         timestamp = \"t2\"\n\
         tool_version = \"v2\"\n",
    );

    let parsed = ArtifactRecord::from_toml(&text).expect("0.9.0-shaped text must still parse");

    assert_eq!(parsed.history.len(), 2);
    assert!(
        parsed.history.iter().all(|entry| entry.item.is_none()),
        "no 0.9.0 entry names an item"
    );
}

#[test]
fn a_hash_entry_item_that_is_not_a_canonical_uuid_does_not_parse() {
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\
         item = \"not-a-uuid\"\n",
        hash("bytes-1"),
    );

    let result = ArtifactRecord::from_toml(&text);

    assert!(
        matches!(result, Err(Malformed::Toml(_))),
        "an entry whose item is not a canonical uuid must not parse: {result:?}"
    );
}

#[test]
fn item_for_the_newest_hash_is_the_records_link_whatever_its_entry_names() {
    let link = ItemId::parse(UUID_B).unwrap();
    let other = ItemId::parse(UUID_C).unwrap();

    let mut entry = hash_entry("bytes-1", "run-1", "t1", "v1");
    entry.item = Some(other);

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(link.clone()),
        history: vec![entry.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.item_for(&entry.hash), Some(&link));
}

#[test]
fn item_for_an_earlier_hash_is_the_item_its_newest_entry_names() {
    let x = ItemId::parse(UUID_B).unwrap();
    let y = ItemId::parse(UUID_C).unwrap();

    let mut a = hash_entry("bytes-a", "run-1", "t1", "v1");
    a.item = Some(x.clone());
    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(y.clone());

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(y),
        history: vec![a.clone(), b],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.item_for(&a.hash), Some(&x));
}

#[test]
fn item_for_an_earlier_hash_whose_entry_names_none_is_the_records_link() {
    let link = ItemId::parse(UUID_D).unwrap();
    let b_item = ItemId::parse(UUID_C).unwrap();

    let a = hash_entry("bytes-a", "run-1", "t1", "v1");
    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(b_item);

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(link.clone()),
        history: vec![a.clone(), b],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.item_for(&a.hash), Some(&link));
}

#[test]
fn item_for_a_hash_the_history_does_not_hold_is_none() {
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(ItemId::parse(UUID_B).unwrap()),
        history: vec![hash_entry("bytes-a", "run-1", "t1", "v1")],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.item_for(&hash("never-recorded")), None);
}

#[test]
fn item_for_uses_the_newest_of_several_entries_holding_one_hash() {
    let x = ItemId::parse(UUID_B).unwrap();
    let y = ItemId::parse(UUID_C).unwrap();
    let z = ItemId::parse(UUID_D).unwrap();

    let mut a1 = hash_entry("bytes-a", "run-1", "t1", "v1");
    a1.item = Some(x);
    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(y.clone());
    let mut a2 = hash_entry("bytes-a", "run-3", "t3", "v3");
    a2.item = Some(z.clone());
    let mut c = hash_entry("bytes-c", "run-4", "t4", "v4");
    c.item = Some(y.clone());

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(y),
        history: vec![a1, b, a2.clone(), c],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.item_for(&a2.hash), Some(&z));
}

#[test]
fn append_restates_the_newest_bytes_under_the_link_as_read_when_their_entry_names_another_item() {
    let x = ItemId::parse(UUID_B).unwrap();
    let y = ItemId::parse(UUID_C).unwrap();

    let mut a = hash_entry("bytes-a", "run-1", "t1", "v1");
    a.item = Some(x);

    let mut record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(y.clone()),
        history: vec![a.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(y.clone());

    record.append(b.clone(), Some(&y));

    assert_eq!(record.history.len(), 3, "a restatement, then b");
    assert_eq!(record.history[0], a);
    assert_eq!(
        record.history[1].hash, a.hash,
        "the restatement names the newest (a's) bytes"
    );
    assert_eq!(record.history[1].item, Some(y));
    assert_eq!(
        record.history[1].run, b.run,
        "the restatement carries the appended entry's run"
    );
    assert_eq!(record.history[1].timestamp, b.timestamp);
    assert_eq!(record.history[1].tool_version, b.tool_version);
    assert_eq!(record.history[2], b);
}

#[test]
fn append_restates_the_newest_bytes_when_their_entry_names_no_item() {
    let y = ItemId::parse(UUID_B).unwrap();

    let a = hash_entry("bytes-a", "run-1", "t1", "v1");

    let mut record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(y.clone()),
        history: vec![a.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(y.clone());

    record.append(b.clone(), Some(&y));

    assert_eq!(record.history.len(), 3, "a restatement, then b");
    assert_eq!(record.history[1].hash, a.hash);
    assert_eq!(record.history[1].item, Some(y));
    assert_eq!(record.history[2], b);
}

#[test]
fn append_does_not_restate_for_an_entry_for_the_newest_bytes() {
    let x = ItemId::parse(UUID_B).unwrap();
    let y = ItemId::parse(UUID_C).unwrap();

    let mut a = hash_entry("bytes-a", "run-1", "t1", "v1");
    a.item = Some(x);

    let mut record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(y.clone()),
        history: vec![a.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let mut a_again = hash_entry("bytes-a", "run-2", "t2", "v2");
    a_again.item = Some(y.clone());

    record.append(a_again.clone(), Some(&y));

    assert_eq!(
        record.history,
        vec![a, a_again],
        "re-identifying the newest bytes restates nothing"
    );
}

#[test]
fn append_does_not_restate_when_the_newest_entry_names_the_link_as_read() {
    let y = ItemId::parse(UUID_B).unwrap();

    let mut a = hash_entry("bytes-a", "run-1", "t1", "v1");
    a.item = Some(y.clone());

    let mut record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(y.clone()),
        history: vec![a.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(y.clone());

    record.append(b.clone(), Some(&y));

    assert_eq!(record.history, vec![a, b]);
}

#[test]
fn append_does_not_restate_when_the_link_as_read_is_none() {
    let a = hash_entry("bytes-a", "run-1", "t1", "v1");

    let mut record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(ItemId::parse(UUID_B).unwrap()),
        history: vec![a.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(ItemId::parse(UUID_B).unwrap());

    record.append(b.clone(), None);

    assert_eq!(record.history, vec![a, b]);
}

#[test]
fn append_judges_the_link_as_read_not_the_records_current_link() {
    let x = ItemId::parse(UUID_B).unwrap();
    let y = ItemId::parse(UUID_C).unwrap();

    let mut a_old = hash_entry("bytes-a", "run-1", "t1", "v1");
    a_old.item = Some(x.clone());
    let mut b = hash_entry("bytes-b", "run-2", "t2", "v2");
    b.item = Some(y.clone());

    // As `admit` does: the caller has already assigned the new link
    // before calling `append`.
    let mut record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(x.clone()),
        history: vec![a_old.clone(), b.clone()],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let mut a_new = hash_entry("bytes-a", "run-3", "t3", "v3");
    a_new.item = Some(x);

    record.append(a_new.clone(), Some(&y));

    assert_eq!(
        record.history,
        vec![a_old, b, a_new],
        "judged against the link as read (y), not the record's current link (x), \
         so the newest entry (b, naming y) triggers no restatement"
    );
}

// ---------------------------------------------------------------------
// 1.1 LibraryId and LibraryIdentity (design D1, D8)
// ---------------------------------------------------------------------

#[test]
fn library_id_parses_a_canonical_uuid_and_renders_it_back_byte_identically() {
    let id = LibraryId::parse(UUID_A).unwrap();
    assert_eq!(id.to_string(), UUID_A);
}

#[test]
fn library_id_from_uuid_renders_the_same_canonical_text() {
    let uuid = uuid::Uuid::parse_str(UUID_A).unwrap();
    let id = LibraryId::from_uuid(uuid);
    assert_eq!(id.to_string(), UUID_A);
    assert_eq!(id.uuid(), uuid);
}

#[test]
fn library_id_refuses_an_uppercase_spelling() {
    assert_eq!(LibraryId::parse(&UUID_A.to_uppercase()), None);
}

#[test]
fn library_id_refuses_a_braced_spelling() {
    assert_eq!(LibraryId::parse(&format!("{{{UUID_A}}}")), None);
}

#[test]
fn library_id_refuses_a_bare_hex_run_with_no_hyphens() {
    assert_eq!(LibraryId::parse(&UUID_A.replace('-', "")), None);
}

#[test]
fn library_id_accepts_a_canonical_uuid_of_another_version() {
    // Version 4, not version 7: `LibraryId` checks spelling only, as
    // `canonical_uuid` does for items and artifacts (design D1).
    let version_4 = "550e8400-e29b-41d4-a716-446655440000";
    let id = LibraryId::parse(version_4).expect("a canonical uuid of any version must parse");
    assert_eq!(id.to_string(), version_4);
}

#[test]
fn library_identity_is_written_as_one_id_line() {
    let identity = LibraryIdentity {
        id: LibraryId::parse(UUID_A).unwrap(),
    };
    assert_eq!(identity.to_toml(), format!("id = \"{UUID_A}\"\n"));
}

#[test]
fn library_identity_round_trips_through_toml() {
    let identity = LibraryIdentity {
        id: LibraryId::parse(UUID_A).unwrap(),
    };
    let parsed = LibraryIdentity::from_toml(&identity.to_toml()).unwrap();
    assert_eq!(parsed, identity);
}

#[test]
fn library_identity_ignores_keys_and_tables_it_does_not_know() {
    let text = format!("id = \"{UUID_A}\"\ncreated = \"2026-01-01\"\n\n[later]\nkey = 1\n");
    let parsed = LibraryIdentity::from_toml(&text)
        .expect("unknown keys and tables must not stop this from parsing");
    assert_eq!(parsed.id, LibraryId::parse(UUID_A).unwrap());
}

#[test]
fn library_identity_without_an_id_does_not_parse() {
    assert!(matches!(
        LibraryIdentity::from_toml(""),
        Err(Malformed::Toml(_))
    ));
    assert!(matches!(
        LibraryIdentity::from_toml("other = 1\n"),
        Err(Malformed::Toml(_))
    ));
}

#[test]
fn library_identity_whose_id_is_not_a_string_does_not_parse() {
    assert!(matches!(
        LibraryIdentity::from_toml("id = 7\n"),
        Err(Malformed::Toml(_))
    ));
}

#[test]
fn library_identity_whose_id_is_not_a_canonical_uuid_does_not_parse() {
    let text = format!("id = \"{}\"\n", UUID_A.to_uppercase());
    let result = LibraryIdentity::from_toml(&text);
    assert!(matches!(result, Err(Malformed::Toml(_))));
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not a canonical library uuid"),
        "the message must say why the id was refused"
    );
}

#[test]
fn library_identity_that_is_not_toml_does_not_parse() {
    assert!(matches!(
        LibraryIdentity::from_toml("not { toml at all"),
        Err(Malformed::Toml(_))
    ));
}

// ---------------------------------------------------------------------
// 1.1 Identification evidence on history entries (design D3, D10)
// ---------------------------------------------------------------------

#[test]
fn evidence_origin_as_str_and_parse_round_trip_for_every_origin() {
    for (origin, word) in [
        (EvidenceOrigin::Extracted, "extracted"),
        (EvidenceOrigin::Supplied, "supplied"),
        (EvidenceOrigin::Overridden, "overridden"),
        (EvidenceOrigin::Adopted, "adopted"),
    ] {
        assert_eq!(origin.as_str(), word);
        assert_eq!(EvidenceOrigin::parse(word), Some(origin));
    }
}

#[test]
fn evidence_origin_parse_refuses_any_other_word() {
    for word in ["accepted", "Extracted", ""] {
        assert_eq!(
            EvidenceOrigin::parse(word),
            None,
            "{word:?} must not parse as an origin"
        );
    }
}

#[test]
fn evidence_origin_is_operator_for_supplied_and_overridden_alone() {
    assert!(!EvidenceOrigin::Extracted.is_operator());
    assert!(EvidenceOrigin::Supplied.is_operator());
    assert!(EvidenceOrigin::Overridden.is_operator());
    assert!(!EvidenceOrigin::Adopted.is_operator());
}

#[test]
fn hash_entry_evidence_round_trips_through_toml_for_each_identifier_kind() {
    let identifiers = [
        supplied("doi:10.1021/jacs.4c01234").unwrap(),
        supplied("arXiv:2401.12345").unwrap(),
        supplied("pmid:12345678").unwrap(),
        supplied("isbn:978-1-59327-828-1").unwrap(),
    ];

    for identifier in identifiers {
        let mut entry = hash_entry("bytes-1", "run-1", "t1", "v1");
        entry.evidence = Some(IdentificationEvidence {
            identifier: identifier.clone(),
            origin: EvidenceOrigin::Extracted,
        });
        let record = ArtifactRecord {
            id: ArtifactId::parse(UUID_A).unwrap(),
            item: None,
            history: vec![entry],
            path: "x.pdf".to_string(),
            size: 1,
            modified_millis: 0,
        };

        let parsed = ArtifactRecord::from_toml(&record.to_toml())
            .unwrap_or_else(|error| panic!("{identifier} did not round-trip: {error}"));

        assert_eq!(
            parsed.history[0].evidence,
            Some(IdentificationEvidence {
                identifier,
                origin: EvidenceOrigin::Extracted,
            })
        );
    }
}

#[test]
fn hash_entry_evidence_is_written_after_item_as_identifier_then_origin() {
    let mut entry = hash_entry("bytes-1", "run-1", "t1", "v1");
    entry.item = Some(ItemId::parse(UUID_B).unwrap());
    entry.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/x").unwrap(),
        origin: EvidenceOrigin::Supplied,
    });
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: Some(ItemId::parse(UUID_B).unwrap()),
        history: vec![entry],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let text = record.to_toml();
    let history_start = text.find("[[history]]").expect("a history table");
    let history_text = &text[history_start..];

    let item_at = history_text.find("item").expect("an item key");
    let identifier_at = history_text.find("identifier").expect("an identifier key");
    let origin_at = history_text.find("origin").expect("an origin key");

    assert!(
        item_at < identifier_at && identifier_at < origin_at,
        "evidence must be written after item, as identifier then origin: {history_text}"
    );
}

#[test]
fn hash_entry_without_evidence_is_written_with_neither_key() {
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![hash_entry("bytes-1", "run-1", "t1", "v1")],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    let text = record.to_toml();
    let history_start = text.find("[[history]]").expect("a history table");
    let history_text = &text[history_start..];

    assert!(
        !history_text.contains("identifier") && !history_text.contains("origin"),
        "a hash entry with no evidence must write neither key: {text}"
    );
}

#[test]
fn artifact_record_written_before_evidence_parses_with_none_and_renders_byte_identically() {
    let h1 = hash("bytes-1");
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{h1}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n",
    );

    let parsed =
        ArtifactRecord::from_toml(&text).expect("text written before evidence must still parse");

    assert_eq!(parsed.history[0].evidence, None);
    assert_eq!(
        parsed.to_toml(),
        text,
        "a record with no evidence must render byte-identically"
    );
}

#[test]
fn hash_entry_with_an_identifier_and_no_origin_does_not_parse() {
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\
         identifier = \"doi:10.1000/x\"\n",
        hash("bytes-1"),
    );

    let result = ArtifactRecord::from_toml(&text);

    assert!(
        matches!(result, Err(Malformed::Toml(_))),
        "an identifier with no origin must not parse: {result:?}"
    );
}

#[test]
fn hash_entry_with_an_origin_and_no_identifier_does_not_parse() {
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\
         origin = \"supplied\"\n",
        hash("bytes-1"),
    );

    let result = ArtifactRecord::from_toml(&text);

    assert!(
        matches!(result, Err(Malformed::Toml(_))),
        "an origin with no identifier must not parse: {result:?}"
    );
}

#[test]
fn hash_entry_with_an_unknown_origin_does_not_parse() {
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\
         identifier = \"doi:10.1000/x\"\n\
         origin = \"accepted\"\n",
        hash("bytes-1"),
    );

    let result = ArtifactRecord::from_toml(&text);

    assert!(
        matches!(result, Err(Malformed::Toml(_))),
        "an unknown origin must not parse: {result:?}"
    );
}

#[test]
fn hash_entry_with_an_identifier_that_does_not_parse_does_not_parse() {
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"x.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\
         identifier = \"not-an-identifier\"\n\
         origin = \"supplied\"\n",
        hash("bytes-1"),
    );

    let result = ArtifactRecord::from_toml(&text);

    assert!(
        matches!(result, Err(Malformed::Toml(_))),
        "an identifier that does not parse must not parse: {result:?}"
    );
}

#[test]
fn evidence_for_is_the_newest_entry_holding_the_hash_that_carries_evidence() {
    let h = hash("bytes-a");
    let mut first = hash_entry("bytes-a", "run-1", "t1", "v1");
    first.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/x").unwrap(),
        origin: EvidenceOrigin::Extracted,
    });
    let mut second = hash_entry("bytes-a", "run-2", "t2", "v2");
    second.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/y").unwrap(),
        origin: EvidenceOrigin::Supplied,
    });
    let third = hash_entry("bytes-b", "run-3", "t3", "v3");

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![first, second.clone(), third],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.evidence_for(&h), second.evidence.as_ref());
}

#[test]
fn evidence_for_is_none_when_no_entry_for_the_hash_carries_any() {
    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![hash_entry("bytes-a", "run-1", "t1", "v1")],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.evidence_for(&hash("bytes-a")), None);
}

#[test]
fn operator_evidence_for_skips_extracted_and_adopted_entries() {
    let h = hash("bytes-a");
    let mut extracted = hash_entry("bytes-a", "run-1", "t1", "v1");
    extracted.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/x").unwrap(),
        origin: EvidenceOrigin::Extracted,
    });
    let mut supplied_entry = hash_entry("bytes-a", "run-2", "t2", "v2");
    supplied_entry.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/y").unwrap(),
        origin: EvidenceOrigin::Supplied,
    });
    let mut adopted = hash_entry("bytes-a", "run-3", "t3", "v3");
    adopted.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/z").unwrap(),
        origin: EvidenceOrigin::Adopted,
    });

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![extracted, supplied_entry.clone(), adopted],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(
        record.operator_evidence_for(&h),
        supplied_entry.evidence.as_ref()
    );
}

#[test]
fn operator_evidence_for_is_none_with_no_operator_entry() {
    let mut extracted = hash_entry("bytes-a", "run-1", "t1", "v1");
    extracted.evidence = Some(IdentificationEvidence {
        identifier: supplied("doi:10.1000/x").unwrap(),
        origin: EvidenceOrigin::Extracted,
    });

    let record = ArtifactRecord {
        id: ArtifactId::parse(UUID_A).unwrap(),
        item: None,
        history: vec![extracted],
        path: "x.pdf".to_string(),
        size: 1,
        modified_millis: 0,
    };

    assert_eq!(record.operator_evidence_for(&hash("bytes-a")), None);
}
