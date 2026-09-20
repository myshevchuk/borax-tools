#![allow(clippy::unwrap_used)]

use borax_core::content::{ContentHash, hash_bytes};
use borax_core::library::{
    ArtifactId, ArtifactRecord, HashEntry, Item, ItemId, RunId, is_library_relative,
    is_well_formed_hash, name_uuid,
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
