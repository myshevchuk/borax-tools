#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use borax::event::{
    Adoption, Attempt, Claim, ClaimOrigin, Condition, Counts, Diagnostic, Event, Extraction,
    Format, Level, LibraryAnswer, SCHEMA, SkipReason, Summary, TableUsed, human_line,
    human_summary, json_line, render,
};
use borax::evidence::{
    Consultation, Evidence, ExtractionEvidence, ExtractionStep, IndexEvidence, IndexRead,
    IndexWrite, LookupEvidence, MatchCheck, Origin, ServiceAttempt, Titles, Unattempted,
};
use borax::pipeline::{FileOutcome, FileRecord, event_for};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_core::record::{BoraxExt, EntryType, Record, Source};
use borax_pdf::tiered::Tier;
use borax_sources::cache::CacheWrite;
use borax_sources::source::{Retrieval, SourceName};
use serde_json::Value;

/// The evidence of a record reached through a lookup: `source()` and
/// `tier()` name `service` and the extraction pass that read
/// `identifier`, and `cached()` is `false`.
fn evidence_via_lookup(service: SourceName, identifier: Identifier, tier: Tier) -> Evidence {
    Evidence {
        library: Consultation::NotConsulted(Unattempted::NoLibrary),
        content_index: IndexEvidence {
            read: IndexRead::Miss,
            write: IndexWrite::Attempted(CacheWrite::Written),
        },
        extraction: ExtractionEvidence {
            result: ExtractionStep::Ran(Extraction::Found {
                identifier: identifier.to_string(),
                tier: tier.as_str().to_string(),
            }),
            titles: Titles::Read(Vec::new()),
        },
        lookup: LookupEvidence::Attempted {
            identifier,
            origin: Origin::Extracted(tier),
            attempts: vec![ServiceAttempt {
                service,
                outcome: Ok(Retrieval::Network { stored: None }),
            }],
        },
        match_check: MatchCheck::Agreed,
    }
}

/// The evidence of a record the content index answered for: `source()`
/// and `tier()` are both `None`, and `cached()` is `true`.
fn evidence_via_content_index_hit() -> Evidence {
    Evidence {
        library: Consultation::NotConsulted(Unattempted::NoLibrary),
        content_index: IndexEvidence {
            read: IndexRead::Hit,
            write: IndexWrite::NotAttempted(Unattempted::ContentIndexHit),
        },
        extraction: ExtractionEvidence {
            result: ExtractionStep::NotAttempted(Unattempted::ContentIndexHit),
            titles: Titles::NotAttempted(Unattempted::ContentIndexHit),
        },
        lookup: LookupEvidence::NotAttempted(Unattempted::ContentIndexHit),
        match_check: MatchCheck::NotAttempted(Unattempted::ContentIndexHit),
    }
}

// --- event constructors ---

fn run_started() -> Event {
    Event::RunStarted {
        command: "rename".to_string(),
        version: "0.1.0".to_string(),
        applying: true,
        interactive: false,
        tables: Vec::new(),
    }
}

fn run_started_with_a_table() -> Event {
    Event::RunStarted {
        command: "rename".to_string(),
        version: "0.1.0".to_string(),
        applying: true,
        interactive: false,
        tables: vec![TableUsed {
            name: "jcode".to_string(),
            path: PathBuf::from("/collection/journals.tsv"),
            digest: "sha256-abc123".to_string(),
        }],
    }
}

fn resolved() -> Event {
    Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        source: "crossref".to_string(),
        found: "10.1000/xyz123".to_string(),

        claims: Vec::new(),

        tier: Some("first-page".to_string()),
        cached: false,
        overrode: None,
        library: None,
    }
}

fn planned() -> Event {
    Event::Planned {
        path: PathBuf::from("paper.pdf"),
        target: PathBuf::from("smith2024_borax.pdf"),
    }
}

/// A fixed hash for fixtures that need one but do not test its value.
fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

fn renamed() -> Event {
    Event::Renamed {
        path: PathBuf::from("paper.pdf"),
        target: PathBuf::from("smith2024_borax.pdf"),
        hash: hash_of("paper.pdf"),
    }
}

fn skipped(reason: SkipReason) -> Event {
    Event::Skipped {
        path: PathBuf::from("mystery.pdf"),
        reason,
        library: None,
    }
}

fn bib_entry() -> Event {
    Event::BibEntry {
        path: PathBuf::from("paper.pdf"),
        key: "smith2024".to_string(),
        outcome: "added".to_string(),
    }
}

fn sidecar() -> Event {
    Event::Sidecar {
        path: PathBuf::from("paper.pdf"),
        target: PathBuf::from("paper.bib"),
    }
}

fn config_setting() -> Event {
    Event::ConfigSetting {
        key: "mailto".to_string(),
        value: "\"test@example.org\"".to_string(),
        origin: "defaults".to_string(),
    }
}

fn cache_status() -> Event {
    Event::CacheStatus {
        root: PathBuf::from("/cache"),
        entries: 4,
        bytes: 1024,
    }
}

fn cache_cleared() -> Event {
    Event::CacheCleared {
        root: PathBuf::from("/cache"),
        entries: 4,
        bytes: 1024,
    }
}

fn lookup_missed() -> Event {
    Event::LookupMissed {
        table: "jcode".to_string(),
        input: "Amino Acids".to_string(),
    }
}

fn run_finished() -> Event {
    Event::RunFinished {
        counts: Counts {
            resolved: 3,
            renamed: 2,
            skipped: 1,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
        },
    }
}

/// Fixed UUID text for condition fixtures below, following
/// `library_extraction`'s own fixed hashes: the value is never
/// validated as a real UUID by `Condition`, which carries `id` and
/// `record` as plain strings (design D10).
const COND_UUID_MISSING: &str = "0190c3a2-1111-7000-8000-000000000001";
const COND_UUID_UNLINKED: &str = "0190c3a3-2222-7000-8000-000000000002";

fn library_condition(path: &str, condition: Condition) -> Event {
    Event::LibraryCondition {
        path: path.to_string(),
        condition,
    }
}

fn library_adoption(path: &str, adoption: Adoption) -> Event {
    Event::LibraryAdoption {
        path: path.to_string(),
        adoption,
    }
}

/// One instance of every [`Condition`] variant, paired with the `path`
/// design D3 shows for it.
fn all_conditions() -> Vec<(&'static str, Condition)> {
    vec![
        ("sub/new.pdf", Condition::Orphan),
        (
            "gone.pdf",
            Condition::Missing {
                id: COND_UUID_MISSING.to_string(),
                record: format!(".borax/artifacts/{COND_UUID_MISSING}.toml"),
            },
        ),
        (
            "items/milner1978.unlinked.toml",
            Condition::Unlinked {
                id: COND_UUID_UNLINKED.to_string(),
            },
        ),
    ]
}

/// Every `Event` variant, so coverage-oriented tests can iterate once.
fn all_events() -> Vec<Event> {
    vec![
        run_started(),
        resolved(),
        planned(),
        renamed(),
        library_condition("sub/new.pdf", Condition::Orphan),
        library_condition(
            "gone.pdf",
            Condition::Missing {
                id: COND_UUID_MISSING.to_string(),
                record: format!(".borax/artifacts/{COND_UUID_MISSING}.toml"),
            },
        ),
        library_condition(
            "items/milner1978.unlinked.toml",
            Condition::Unlinked {
                id: COND_UUID_UNLINKED.to_string(),
            },
        ),
        library_adoption("new.pdf", Adoption::Unindexed),
        skipped(SkipReason::NoIdentifier),
        skipped(SkipReason::Unresolvable {
            found: "doi:10.1000/xyz123".to_string(),
            tier: Some("text-layer".to_string()),
            attempts: vec![Attempt {
                source: "crossref".to_string(),
                error: "not found".to_string(),
            }],
        }),
        skipped(SkipReason::Conflict {
            field: "year".to_string(),
            extracted: "2023".to_string(),
            resolved: "2024".to_string(),
            similarity: 0.0,
        }),
        skipped(SkipReason::TargetTaken {
            target: PathBuf::from("smith2024_borax.pdf"),
        }),
        skipped(SkipReason::Unreadable {
            message: "not a PDF".to_string(),
        }),
        skipped(SkipReason::BibWriteFailed {
            message: "disk full".to_string(),
        }),
        skipped(SkipReason::Unciteable),
        skipped(SkipReason::Unrecordable {
            message: "the file's content hash is unknown".to_string(),
        }),
        bib_entry(),
        sidecar(),
        config_setting(),
        cache_status(),
        cache_cleared(),
        lookup_missed(),
        run_finished(),
    ]
}

/// Every `SkipReason` variant, so nesting tests can iterate once.
fn all_skip_reasons() -> Vec<SkipReason> {
    vec![
        SkipReason::NoIdentifier,
        SkipReason::Unresolvable {
            found: "doi:10.1000/xyz123".to_string(),
            tier: Some("text-layer".to_string()),
            attempts: vec![
                Attempt {
                    source: "crossref".to_string(),
                    error: "not found".to_string(),
                },
                Attempt {
                    source: "arxiv".to_string(),
                    error: "timed out".to_string(),
                },
            ],
        },
        SkipReason::Conflict {
            field: "year".to_string(),
            extracted: "2023".to_string(),
            resolved: "2024".to_string(),
            similarity: 0.0,
        },
        SkipReason::TargetTaken {
            target: PathBuf::from("smith2024_borax.pdf"),
        },
        SkipReason::Unreadable {
            message: "not a PDF".to_string(),
        },
        SkipReason::BibWriteFailed {
            message: "disk full".to_string(),
        },
        SkipReason::Unciteable,
        SkipReason::Unrecordable {
            message: "the file's content hash is unknown".to_string(),
        },
    ]
}

// --- json_line() renders every variant as one well-formed JSON object ---

#[test]
fn json_line_of_every_event_is_a_single_line_with_no_embedded_newline() {
    for event in all_events() {
        let line = json_line(&event);
        assert!(!line.contains('\n'), "event {event:?} produced {line:?}");
    }
}

#[test]
fn json_line_of_every_event_parses_as_a_json_object_carrying_schema_and_event_tag() {
    for event in all_events() {
        let line = json_line(&event);
        let value: Value = serde_json::from_str(&line).unwrap();
        let object = value
            .as_object()
            .unwrap_or_else(|| panic!("event {event:?} did not render as a JSON object: {line}"));

        assert_eq!(object["schema"], Value::from(SCHEMA));
        assert!(object["event"].is_string(), "event {event:?}: {line}");
    }
}

#[test]
fn json_line_event_tag_is_the_variant_name_in_kebab_case() {
    let cases: Vec<(Event, &str)> = vec![
        (run_started(), "run-started"),
        (resolved(), "resolved"),
        (planned(), "planned"),
        (renamed(), "renamed"),
        (skipped(SkipReason::NoIdentifier), "skipped"),
        (bib_entry(), "bib-entry"),
        (sidecar(), "sidecar"),
        (config_setting(), "config-setting"),
        (cache_status(), "cache-status"),
        (cache_cleared(), "cache-cleared"),
        (lookup_missed(), "lookup-missed"),
        (run_finished(), "run-finished"),
    ];

    for (event, expected_tag) in cases {
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        assert_eq!(value["event"], Value::from(expected_tag));
    }
}

#[test]
fn json_line_of_resolved_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&resolved())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "cached",
            "claims",
            "event",
            "found",
            "identifier",
            "library",
            "overrode",
            "path",
            "record",
            "schema",
            "source",
            "tier"
        ]
    );

    assert_eq!(object["path"], Value::from("paper.pdf"));
    assert_eq!(object["identifier"], Value::from("10.1000/xyz123"));
    assert_eq!(object["source"], Value::from("crossref"));
    assert_eq!(object["tier"], Value::from("first-page"));
    assert_eq!(object["cached"], Value::from(false));
}

#[test]
fn json_line_of_skipped_has_exactly_the_documented_field_set() {
    let value: Value =
        serde_json::from_str(&json_line(&skipped(SkipReason::NoIdentifier))).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "library", "path", "reason", "schema"]);

    assert_eq!(object["path"], Value::from("mystery.pdf"));
    assert!(object["reason"].is_object());
    assert_eq!(object["library"], Value::Null);
}

#[test]
fn json_line_of_run_finished_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&run_finished())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["counts", "event", "schema"]);

    assert_eq!(
        object["counts"],
        serde_json::json!({"resolved": 3, "renamed": 2, "skipped": 1, "named": 0, "unmatched": 0, "unreached": 0, "findings": 0})
    );
}

#[test]
fn json_line_of_sidecar_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&sidecar())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "path", "schema", "target"]);

    assert_eq!(object["path"], Value::from("paper.pdf"));
    assert_eq!(object["target"], Value::from("paper.bib"));
}

#[test]
fn json_line_of_config_setting_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&config_setting())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "key", "origin", "schema", "value"]);

    assert_eq!(object["key"], Value::from("mailto"));
    assert_eq!(object["value"], Value::from("\"test@example.org\""));
    assert_eq!(object["origin"], Value::from("defaults"));
}

#[test]
fn json_line_of_cache_status_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&cache_status())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["bytes", "entries", "event", "root", "schema"]);

    assert_eq!(object["root"], Value::from("/cache"));
    assert_eq!(object["entries"], Value::from(4));
    assert_eq!(object["bytes"], Value::from(1024));
}

#[test]
fn json_line_of_cache_cleared_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&cache_cleared())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["bytes", "entries", "event", "root", "schema"]);

    assert_eq!(object["root"], Value::from("/cache"));
    assert_eq!(object["entries"], Value::from(4));
    assert_eq!(object["bytes"], Value::from(1024));
}

/// design: `Renamed` carries the hash the file resolved to, required
/// rather than optional — an applying rename already refuses to move a
/// file whose hash is unknown, so this is the shape the invariant
/// takes in the type.
#[test]
fn json_line_of_renamed_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&renamed())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "hash", "path", "schema", "target"]);

    assert_eq!(object["path"], Value::from("paper.pdf"));
    assert_eq!(object["target"], Value::from("smith2024_borax.pdf"));
    assert_eq!(object["hash"], Value::from(hash_of("paper.pdf").as_str()));
}

/// `Planned` moves nothing, so unlike `Renamed` it has no hash to
/// carry — pinned so a future edit cannot quietly add one to match.
#[test]
fn json_line_of_planned_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&planned())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "path", "schema", "target"]);
}

/// The component `Unrecordable` replaces is gone from every rendering,
/// in either format — a name a future skip reason must not resurrect.
#[test]
fn unjournalable_appears_nowhere_in_any_rendering() {
    for event in all_events() {
        let json = json_line(&event);
        assert!(
            !json.to_lowercase().contains("unjournalable"),
            "got {json:?}"
        );
        if let Some(line) = human_line(&event) {
            assert!(
                !line.to_lowercase().contains("unjournalable"),
                "got {line:?}"
            );
        }
    }
}

// --- SkipReason nesting under Skipped.reason, tagged by kind ---

#[test]
fn skipped_nests_the_reason_under_reason_with_a_kebab_case_kind_tag() {
    let cases: Vec<(SkipReason, &str)> = vec![
        (SkipReason::NoIdentifier, "no-identifier"),
        (
            SkipReason::Unresolvable {
                found: "doi:10.1000/xyz123".to_string(),
                tier: Some("text-layer".to_string()),
                attempts: Vec::new(),
            },
            "unresolvable",
        ),
        (
            SkipReason::Conflict {
                field: "year".to_string(),
                extracted: "2023".to_string(),
                resolved: "2024".to_string(),
                similarity: 0.0,
            },
            "conflict",
        ),
        (
            SkipReason::TargetTaken {
                target: PathBuf::from("x.pdf"),
            },
            "target-taken",
        ),
        (
            SkipReason::Unreadable {
                message: "bad".to_string(),
            },
            "unreadable",
        ),
        (
            SkipReason::BibWriteFailed {
                message: "disk full".to_string(),
            },
            "bib-write-failed",
        ),
        (SkipReason::Unciteable, "unciteable"),
        (
            SkipReason::Unrecordable {
                message: "the file's content hash is unknown".to_string(),
            },
            "unrecordable",
        ),
    ];

    for (reason, expected_kind) in cases {
        let value: Value = serde_json::from_str(&json_line(&skipped(reason))).unwrap();
        assert_eq!(value["reason"]["kind"], Value::from(expected_kind));
    }
}

#[test]
fn no_identifier_reason_carries_nothing_but_its_kind() {
    let value: Value =
        serde_json::from_str(&json_line(&skipped(SkipReason::NoIdentifier))).unwrap();
    let reason = value["reason"].as_object().unwrap();
    assert_eq!(reason.keys().collect::<Vec<_>>(), vec!["kind"]);
}

#[test]
fn unciteable_reason_carries_nothing_but_its_kind() {
    let value: Value = serde_json::from_str(&json_line(&skipped(SkipReason::Unciteable))).unwrap();
    let reason = value["reason"].as_object().unwrap();
    assert_eq!(reason.keys().collect::<Vec<_>>(), vec!["kind"]);
}

#[test]
fn unresolvable_reason_carries_an_attempts_array_of_source_and_error_objects() {
    let reason = SkipReason::Unresolvable {
        found: "doi:10.1000/xyz123".to_string(),
        tier: Some("text-layer".to_string()),
        attempts: vec![
            Attempt {
                source: "crossref".to_string(),
                error: "not found".to_string(),
            },
            Attempt {
                source: "arxiv".to_string(),
                error: "timed out".to_string(),
            },
        ],
    };
    let value: Value = serde_json::from_str(&json_line(&skipped(reason))).unwrap();
    let attempts = value["reason"]["attempts"].as_array().unwrap();

    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["source"], Value::from("crossref"));
    assert_eq!(attempts[0]["error"], Value::from("not found"));
    assert_eq!(attempts[1]["source"], Value::from("arxiv"));
    assert_eq!(attempts[1]["error"], Value::from("timed out"));

    let mut keys: Vec<&str> = value["reason"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["attempts", "found", "kind", "tier"]);
}

// --- round-trip through Event's own (de)serialization ---
//
// `json_line` adds an extra top-level `schema` field alongside the
// event's own fields. `Event`'s derive has no `deny_unknown_fields`, so
// parsing `json_line`'s own output back into `Event` still round-trips;
// this is asserted directly rather than routing around it.

#[test]
fn every_event_round_trips_through_json_line_and_back() {
    for event in all_events() {
        let line = json_line(&event);
        let parsed: Event = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed, event);
    }
}

#[test]
fn every_event_round_trips_through_plain_serde_json_to_string() {
    for event in all_events() {
        let text = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, event);
    }
}

// --- render() dispatches to the right renderer ---

#[test]
fn render_json_equals_json_line_for_every_event() {
    for event in all_events() {
        assert_eq!(render(Format::Json, &event), Some(json_line(&event)));
    }
}

#[test]
fn render_human_equals_human_line_for_every_event() {
    for event in all_events() {
        assert_eq!(render(Format::Human, &event), human_line(&event));
    }
}

// --- Json is never silent ---

#[test]
fn render_json_is_some_for_every_event_including_run_started_and_planned() {
    for event in all_events() {
        assert!(
            render(Format::Json, &event).is_some(),
            "event {event:?} rendered as None under --json"
        );
    }
}

// --- Human rendering ---

#[test]
fn human_line_of_run_started_is_silent() {
    assert_eq!(human_line(&run_started()), None);
}

#[test]
fn human_line_of_resolved_mentions_the_path_and_is_a_single_line() {
    let line = human_line(&resolved()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("paper.pdf"));
}

#[test]
fn human_line_of_renamed_mentions_the_path_and_the_target() {
    let line = human_line(&renamed()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("paper.pdf"));
    assert!(line.contains("smith2024_borax.pdf"));
}

#[test]
fn human_line_of_bib_entry_mentions_the_path_and_the_key() {
    let line = human_line(&bib_entry()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("paper.pdf"));
    assert!(line.contains("smith2024"));
}

#[test]
fn human_line_of_sidecar_mentions_the_path_and_the_target() {
    let line = human_line(&sidecar()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("paper.pdf"));
    assert!(line.contains("paper.bib"));
}

#[test]
fn human_line_of_config_setting_mentions_the_key_value_and_origin() {
    let line = human_line(&config_setting()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("mailto"));
    assert!(line.contains("test@example.org"));
    assert!(line.contains("defaults"));
}

#[test]
fn human_line_of_cache_status_mentions_the_root_and_the_counts() {
    let line = human_line(&cache_status()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("/cache"));
    assert!(line.contains('4'));
    assert!(line.contains("1024"));
}

#[test]
fn human_line_of_cache_cleared_mentions_the_root_and_the_counts() {
    let line = human_line(&cache_cleared()).unwrap();
    assert!(!line.contains('\n'));
    assert!(line.contains("/cache"));
    assert!(line.contains('4'));
    assert!(line.contains("1024"));
}

/// design D2: a context-free rendering of `run-finished` has nothing
/// correct to say, because it cannot know which command's shape fits.
/// `run-finished` joins `run-started` as a silent event; the summary
/// comes from `human_summary`, given the command's `Summary`.
#[test]
fn human_line_of_run_finished_is_silent() {
    assert_eq!(human_line(&run_finished()), None);
}

#[test]
fn human_line_of_skipped_mentions_the_path_for_every_reason() {
    for reason in all_skip_reasons() {
        let line = human_line(&skipped(reason.clone())).unwrap();
        assert!(!line.contains('\n'));
        assert!(
            line.contains("mystery.pdf"),
            "reason {reason:?} produced line without the path: {line}"
        );
    }
}

#[test]
fn human_line_of_skipped_makes_the_reason_legible_for_every_variant() {
    let cases: Vec<(SkipReason, &str)> = vec![
        (SkipReason::NoIdentifier, "identifier"),
        (
            SkipReason::Unresolvable {
                found: "doi:10.1000/xyz123".to_string(),
                tier: Some("text-layer".to_string()),
                attempts: vec![Attempt {
                    source: "crossref".to_string(),
                    error: "not found".to_string(),
                }],
            },
            "crossref",
        ),
        (
            SkipReason::Conflict {
                field: "year".to_string(),
                extracted: "2023".to_string(),
                resolved: "2024".to_string(),
                similarity: 0.0,
            },
            "year",
        ),
        (
            SkipReason::TargetTaken {
                target: PathBuf::from("smith2024_borax.pdf"),
            },
            "smith2024_borax.pdf",
        ),
        (
            SkipReason::Unreadable {
                message: "not a PDF".to_string(),
            },
            "not a PDF",
        ),
        (
            SkipReason::BibWriteFailed {
                message: "disk full".to_string(),
            },
            "disk full",
        ),
        (SkipReason::Unciteable, "citation key"),
        (
            SkipReason::Unrecordable {
                message: "the file's content hash is unknown".to_string(),
            },
            "content hash",
        ),
    ];

    for (reason, must_contain) in cases {
        let line = human_line(&skipped(reason.clone())).unwrap();
        assert!(
            line.contains(must_contain),
            "reason {reason:?} produced line missing {must_contain:?}: {line}"
        );
    }
}

// --- Planned: what a preview run shows ---

/// Preview is the default for `borax rename`, so a silent `Planned`
/// would make the default invocation print nothing at all.
#[test]
fn human_line_of_planned_shows_both_names() {
    let line = human_line(&planned()).unwrap();

    assert!(!line.contains('\n'));
    assert!(line.contains("paper.pdf"));
    assert!(line.contains("smith2024_borax.pdf"));
}

// --- Counts ---

#[test]
fn counts_default_is_all_zeroes() {
    assert_eq!(
        Counts::default(),
        Counts {
            resolved: 0,
            renamed: 0,
            skipped: 0,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
        }
    );
}

#[test]
fn counts_serializes_with_all_six_fields() {
    let counts = Counts {
        resolved: 1,
        renamed: 2,
        skipped: 3,
        named: 0,
        unmatched: 4,
        unreached: 0,
        findings: 0,
    };
    let value: Value = serde_json::to_value(counts).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"resolved": 1, "renamed": 2, "skipped": 3, "named": 0, "unmatched": 4, "unreached": 0, "findings": 0})
    );
}

// --- Event::LookupMissed ---

#[test]
fn json_line_of_lookup_missed_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&lookup_missed())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "input", "schema", "table"]);

    assert_eq!(object["table"], Value::from("jcode"));
    assert_eq!(object["input"], Value::from("Amino Acids"));
}

/// The point of the line is which line to add to which file, so it says
/// both, and quotes the input so a title with trailing punctuation is
/// legible.
#[test]
fn human_line_of_lookup_missed_names_the_table_and_the_input() {
    let line = human_line(&lookup_missed()).unwrap();

    assert!(!line.contains('\n'));
    assert!(line.contains("jcode"), "got {line:?}");
    assert!(line.contains("Amino Acids"), "got {line:?}");
}

#[test]
fn counts_observe_counts_a_lookup_missed_as_unmatched() {
    let mut counts = Counts::default();
    counts.observe(&lookup_missed());
    counts.observe(&lookup_missed());

    assert_eq!(
        counts,
        Counts {
            resolved: 0,
            renamed: 0,
            skipped: 0,
            named: 0,
            unmatched: 2,
            unreached: 0,
            findings: 0,
        }
    );
}

/// Spec scenario: "An unmatched journal is named once" — the summary
/// half of it.
#[test]
fn the_summary_line_names_unmatched_lookups_when_there_were_any() {
    let line = human_summary(
        Summary::Renaming,
        &Counts {
            resolved: 12,
            renamed: 12,
            skipped: 0,
            named: 0,
            unmatched: 1,
            unreached: 0,
            findings: 0,
        },
        0,
    )
    .unwrap();

    assert!(line.contains("1 unmatched"), "got {line:?}");
}

/// Spec scenario: "A run with no misses reports none" — a zero count is
/// carried in the JSON summary and left out of the prose one, there
/// being nothing for a person to do about it.
#[test]
fn the_summary_line_says_nothing_about_unmatched_lookups_when_there_were_none() {
    let line = human_summary(
        Summary::Renaming,
        &Counts {
            resolved: 3,
            renamed: 2,
            skipped: 1,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
        },
        0,
    )
    .unwrap();

    assert_eq!(line, "3 resolved, 2 renamed, 1 skipped");
}

// ---------------------------------------------------------------------
// human_summary: the shapes fixed by design D3 — task 1.1
// ---------------------------------------------------------------------

/// A `Counts` with every field nonzero, so the optional-clause tests
/// below can pin the order and presence of each one at once.
fn full_counts() -> Counts {
    Counts {
        resolved: 5,
        renamed: 4,
        skipped: 2,
        named: 3,
        unmatched: 1,
        unreached: 1,
        findings: 1,
    }
}

/// design D3: `Renaming` reproduces today's `human_summary` output,
/// byte for byte, with every optional clause present when its count is
/// nonzero: `already named`, `unmatched`, `not reached` and `findings`.
#[test]
fn human_summary_renaming_includes_every_optional_clause_when_all_are_nonzero() {
    let line = human_summary(Summary::Renaming, &full_counts(), 0).unwrap();

    assert_eq!(
        line,
        "5 resolved, 4 renamed, 2 skipped, 3 already named, \
         1 unmatched, 1 not reached, 1 findings",
        "got {line:?}"
    );
}

/// design D3: an interactive run that hid every already-named file it
/// passed over says so instead of naming them, `(not shown)`.
#[test]
fn human_summary_renaming_marks_already_named_not_shown_when_all_are_hidden() {
    let line = human_summary(Summary::Renaming, &full_counts(), 3).unwrap();

    assert!(line.contains("3 already named (not shown)"), "got {line:?}");
}

/// design D3: an interactive run that hid only some of the already-named
/// files it passed over names how many, `(N not shown)`.
#[test]
fn human_summary_renaming_marks_already_named_partly_shown_when_some_are_hidden() {
    let line = human_summary(Summary::Renaming, &full_counts(), 1).unwrap();

    assert!(
        line.contains("3 already named (1 not shown)"),
        "got {line:?}"
    );
}

/// design D3: `Renaming` is always `Some`, even for a run with no
/// activity at all.
#[test]
fn human_summary_renaming_is_always_some() {
    assert!(human_summary(Summary::Renaming, &Counts::default(), 0).is_some());
}

/// design D3: `Resolution` gives `1 resolved, 0 skipped` for one
/// resolution.
#[test]
fn human_summary_resolution_reports_one_resolution() {
    let counts = Counts {
        resolved: 1,
        ..Counts::default()
    };

    assert_eq!(
        human_summary(Summary::Resolution, &counts, 0),
        Some("1 resolved, 0 skipped".to_string())
    );
}

/// design D3: `Resolution` gives `0 resolved, 0 skipped` for an empty
/// run. Resolved and skipped are always written, even at zero, because
/// they answer the question the command was run to ask.
#[test]
fn human_summary_resolution_reports_zero_and_zero_for_an_empty_run() {
    assert_eq!(
        human_summary(Summary::Resolution, &Counts::default(), 0),
        Some("0 resolved, 0 skipped".to_string())
    );
}

/// design D3: `Resolution` adds the `unmatched`, `not reached` and
/// `findings` clauses only when nonzero, and its line never contains
/// `renamed` or `already named`, even with those counts set.
#[test]
fn human_summary_resolution_adds_optional_clauses_and_drops_renaming_fields() {
    let line = human_summary(Summary::Resolution, &full_counts(), 0).unwrap();

    assert_eq!(
        line, "5 resolved, 2 skipped, 1 unmatched, 1 not reached, 1 findings",
        "got {line:?}"
    );
    assert!(!line.contains("renamed"), "got {line:?}");
    assert!(!line.contains("already named"), "got {line:?}");
}

/// design D3: `Silent` is `None` for all-zero counts.
#[test]
fn human_summary_silent_is_none_for_all_zero_counts() {
    assert_eq!(human_summary(Summary::Silent, &Counts::default(), 0), None);
}

/// design D3: `Silent` is `None` for counts whose only nonzero totals
/// are resolved, renamed, named or unmatched — none of which decides a
/// partial-success exit.
#[test]
fn human_summary_silent_is_none_when_only_non_partial_success_totals_are_nonzero() {
    let counts = Counts {
        resolved: 4,
        renamed: 3,
        named: 2,
        unmatched: 1,
        skipped: 0,
        unreached: 0,
        findings: 0,
    };

    assert_eq!(human_summary(Summary::Silent, &counts, 0), None);
}

/// design D3: `Validation` is `None` when findings alone is nonzero —
/// `validate` already states the count on the `library-validated` line,
/// and repeating it there is the redundancy this change removes.
#[test]
fn human_summary_validation_is_none_when_findings_alone_is_nonzero() {
    let counts = Counts {
        findings: 2,
        ..Counts::default()
    };

    assert_eq!(human_summary(Summary::Validation, &counts, 0), None);
}

// ---------------------------------------------------------------------
// human_summary: a partial-success total is never hidden — design D3a,
// task 1.2
// ---------------------------------------------------------------------

/// design D3a: for every `Summary` variant, a nonzero `skipped`,
/// `unreached` or `findings` total, taken alone, always produces a line
/// naming it in its clause wording — except `Validation` with
/// `findings`, which is `None` because `validate` already states that
/// count on the line above it.
#[test]
fn a_partial_success_total_is_never_hidden_for_any_summary_shape() {
    /// Counts with one total set to the given value, and the clause
    /// wording that names that total.
    type Case = (fn(usize) -> Counts, &'static str);

    let variants = [
        Summary::Renaming,
        Summary::Resolution,
        Summary::Validation,
        Summary::Silent,
    ];
    let cases: [Case; 3] = [
        (
            |n| Counts {
                skipped: n,
                ..Counts::default()
            },
            "skipped",
        ),
        (
            |n| Counts {
                unreached: n,
                ..Counts::default()
            },
            "not reached",
        ),
        (
            |n| Counts {
                findings: n,
                ..Counts::default()
            },
            "findings",
        ),
    ];

    for summary in variants {
        for (make_counts, clause) in cases {
            let counts = make_counts(7);
            let line = human_summary(summary, &counts, 0);

            if summary == Summary::Validation && clause == "findings" {
                assert_eq!(
                    line, None,
                    "Validation must not repeat findings: got {line:?}"
                );
                continue;
            }

            let line = line.unwrap_or_else(|| {
                panic!("{summary:?} hid a nonzero {clause} total: counts {counts:?}")
            });
            assert!(
                line.contains(&format!("7 {clause}")),
                "{summary:?} did not name its {clause} total: got {line:?}"
            );
        }
    }
}

/// design D3a, the exact wording: `Silent` with a nonzero `skipped` and
/// `unreached`, and nothing else, is exactly the two clauses joined by
/// `, `, in that order.
#[test]
fn human_summary_silent_with_skipped_and_unreached_is_exactly_the_two_clauses() {
    let counts = Counts {
        skipped: 2,
        unreached: 1,
        ..Counts::default()
    };

    assert_eq!(
        human_summary(Summary::Silent, &counts, 0),
        Some("2 skipped, 1 not reached".to_string())
    );
}

// --- the tables a run read, on Event::RunStarted ---

/// Spec scenario: "The run log identifies the table".
#[test]
fn json_line_of_run_started_names_each_table_by_path_and_digest() {
    let value: Value = serde_json::from_str(&json_line(&run_started_with_a_table())).unwrap();

    assert_eq!(
        value["tables"],
        serde_json::json!([{
            "name": "jcode",
            "path": "/collection/journals.tsv",
            "digest": "sha256-abc123",
        }])
    );
}

#[test]
fn json_line_of_run_started_carries_an_empty_table_list_when_none_was_read() {
    let value: Value = serde_json::from_str(&json_line(&run_started())).unwrap();

    assert_eq!(value["tables"], serde_json::json!([]));
}

#[test]
fn human_line_of_run_started_is_silent_even_with_tables() {
    assert_eq!(human_line(&run_started_with_a_table()), None);
}

// --- Diagnostic Display and Level ordering ---

#[test]
fn diagnostic_display_of_a_warning_is_prefixed_with_warning() {
    let diagnostic = Diagnostic {
        level: Level::Warning,
        message: "cache directory is unwritable".to_string(),
    };
    assert_eq!(
        diagnostic.to_string(),
        "warning: cache directory is unwritable"
    );
}

#[test]
fn diagnostic_display_of_an_error_is_prefixed_with_error() {
    let diagnostic = Diagnostic {
        level: Level::Error,
        message: "config file is malformed".to_string(),
    };
    assert_eq!(diagnostic.to_string(), "error: config file is malformed");
}

#[test]
fn level_warning_orders_below_error() {
    assert!(Level::Warning < Level::Error);
}

// --- spec scenario: "Machine-readable run" ---
//
// `borax rename --json` on a batch: stdout is only well-formed JSON
// Lines (per-file events plus a summary event); this pins that a
// plausible run's JSON rendering, joined with newlines, is exactly that.

#[test]
fn a_plausible_run_renders_as_json_lines_ending_in_the_summary() {
    let run: Vec<Event> = vec![
        run_started(),
        Event::Resolved {
            path: PathBuf::from("a.pdf"),
            identifier: "10.1000/aaa".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            source: "crossref".to_string(),
            found: "10.1000/aaa".to_string(),

            claims: Vec::new(),

            tier: Some("first-page".to_string()),
            cached: false,
            overrode: None,
            library: None,
        },
        Event::Resolved {
            path: PathBuf::from("b.pdf"),
            identifier: "10.1000/bbb".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            source: "arxiv".to_string(),
            found: "10.1000/bbb".to_string(),

            claims: Vec::new(),

            tier: None,
            cached: true,
            overrode: None,
            library: None,
        },
        Event::Renamed {
            path: PathBuf::from("a.pdf"),
            target: PathBuf::from("smith2024_a.pdf"),
            hash: hash_of("a.pdf"),
        },
        Event::Skipped {
            path: PathBuf::from("c.pdf"),
            reason: SkipReason::NoIdentifier,
            library: None,
        },
        Event::RunFinished {
            counts: Counts {
                resolved: 2,
                renamed: 1,
                skipped: 1,
                named: 0,
                unmatched: 0,
                unreached: 0,
                findings: 0,
            },
        },
    ];

    let lines: Vec<String> = run
        .iter()
        .map(|event| render(Format::Json, event).unwrap())
        .collect();
    let stdout = lines.join("\n");

    for line in stdout.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        assert!(value.is_object(), "not a JSON object: {line}");
    }

    let last: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(last["event"], Value::from("run-finished"));
}

// ---------------------------------------------------------------------
// A move recorded and then refused
// ---------------------------------------------------------------------

/// A move is written to the run log before it is attempted, but the
/// stream reports what happened: a move the filesystem refused reaches
/// the stream as a skip alone, and the totals count it as one.
#[test]
fn a_rename_the_filesystem_refused_counts_as_a_skip_and_not_as_a_rename() {
    let mut counts = Counts::default();

    counts.observe(&Event::Skipped {
        path: PathBuf::from("/lib/a.pdf"),
        reason: SkipReason::RenameFailed {
            message: "permission denied".to_string(),
        },
        library: None,
    });

    assert_eq!(
        counts,
        Counts {
            skipped: 1,
            named: 0,
            ..Counts::default()
        },
        "got {counts:?}"
    );
}

/// The skip recording a refused move never cancels a move that
/// succeeded: a batch whose first file moved and whose second was
/// refused renamed one file, and says so.
#[test]
fn a_refused_move_leaves_an_earlier_successful_one_counted() {
    let mut counts = Counts::default();

    counts.observe(&Event::Renamed {
        path: PathBuf::from("/lib/a.pdf"),
        target: PathBuf::from("/lib/smith2024.pdf"),
        hash: hash_bytes(b"a"),
    });
    counts.observe(&Event::Skipped {
        path: PathBuf::from("/lib/b.pdf"),
        reason: SkipReason::RenameFailed {
            message: "permission denied".to_string(),
        },
        library: None,
    });

    assert_eq!(
        counts,
        Counts {
            renamed: 1,
            skipped: 1,
            named: 0,
            ..Counts::default()
        },
        "got {counts:?}"
    );
}

// ---------------------------------------------------------------------
// Tasks 1.2/1.2a: `resolved`'s `claims`, `found`, and provenance-derived
// `source` (design D2a, D3, D4)
// ---------------------------------------------------------------------

/// design D3: `claims` serializes each title with its origin, in the
/// order they were read.
#[test]
fn resolved_serializes_claims_as_design_d3_shows() {
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        source: "crossref".to_string(),
        found: "doi:10.1000/xyz123".to_string(),
        claims: vec![
            Claim {
                from: ClaimOrigin::Xmp,
                title: "Applications of chiral sulfinyl compounds".to_string(),
            },
            Claim {
                from: ClaimOrigin::Info,
                title: "Microsoft Word - manuscript.docx".to_string(),
            },
        ],
        tier: Some("text-layer".to_string()),
        cached: false,
        overrode: None,
        library: None,
    };

    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();

    assert_eq!(
        value["claims"],
        serde_json::json!([
            {"from": "xmp", "title": "Applications of chiral sulfinyl compounds"},
            {"from": "info", "title": "Microsoft Word - manuscript.docx"}
        ]),
        "got {value:#}"
    );
}

/// design D2a: a file resolved from an arXiv identifier found in the
/// text layer, whose record also carries a DOI, reports the arXiv
/// identifier as `found` and the DOI as the record's own `identifier`.
#[test]
fn an_arxiv_found_identifier_survives_a_doi_carrying_record() {
    let mut record = Record::new(EntryType::Preprint);
    record.doi = Some(Doi::parse("10.1000/from-the-record").unwrap());
    record.borax.arxiv = Some(ArxivId::parse("2401.01234").unwrap());
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_lookup(
            SourceName::Arxiv,
            Identifier::Arxiv(ArxivId::parse("2401.01234").unwrap()),
            Tier::TextLayer,
        ),
        overrode: None,
    });

    let event = event_for(&path, &outcome);

    match event {
        Event::Resolved {
            identifier, found, ..
        } => {
            assert_eq!(
                found, "arXiv:2401.01234",
                "found must be what was looked up"
            );
            assert_eq!(
                identifier, "doi:10.1000/from-the-record",
                "the record's own identifier is unaffected"
            );
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D4: a content-index answer whose provenance names Crossref
/// reports `source: "crossref"` and keeps `cached: true`.
#[test]
fn a_content_index_answer_whose_provenance_names_crossref_reports_crossref() {
    let mut record = Record::new(EntryType::Article);
    record.borax = BoraxExt {
        provenance: [("title".to_string(), Source::Crossref)]
            .into_iter()
            .collect(),
        ..BoraxExt::default()
    };
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_content_index_hit(),
        overrode: None,
    });

    let event = event_for(&path, &outcome);

    match event {
        Event::Resolved { source, cached, .. } => {
            assert_eq!(source, "crossref");
            assert!(cached);
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D4/D1: a record whose provenance names two services is
/// reported with both, in the fixed order Crossref, OpenAlex, arXiv,
/// DataCite, PubMed, sidecar — not the order the fields happen to be
/// keyed in.
#[test]
fn a_record_naming_two_services_orders_them_crossref_then_openalex() {
    let mut record = Record::new(EntryType::Article);
    record.borax = BoraxExt {
        provenance: [
            ("author".to_string(), Source::OpenAlex),
            ("title".to_string(), Source::Crossref),
        ]
        .into_iter()
        .collect(),
        ..BoraxExt::default()
    };
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_content_index_hit(),
        overrode: None,
    });

    let event = event_for(&path, &outcome);

    match event {
        Event::Resolved { source, .. } => {
            assert_eq!(source, "crossref, openalex", "got {source:?}");
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D4: a record whose provenance names no service at all — only
/// extraction, or nothing — keeps reporting the content index itself as
/// `"cache"`.
#[test]
fn a_record_whose_provenance_names_no_service_keeps_cache() {
    let mut record = Record::new(EntryType::Article);
    record.borax = BoraxExt {
        provenance: [("title".to_string(), Source::Extraction)]
            .into_iter()
            .collect(),
        ..BoraxExt::default()
    };
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_content_index_hit(),
        overrode: None,
    });

    let event = event_for(&path, &outcome);

    match event {
        Event::Resolved { source, .. } => {
            assert_eq!(source, "cache", "got {source:?}");
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// consult-library-first, task 2.1: the `library` field (design D3, D5)
// ---------------------------------------------------------------------

/// `resolved()` with `library` set to `answer` and `tier`/`cached` as
/// given, following the JSON shape a library answer or a library
/// problem actually carries (design D3's table).
fn resolved_with_library(tier: Option<&str>, cached: bool, answer: LibraryAnswer) -> Event {
    let Event::Resolved {
        path,
        identifier,
        record,
        source,
        found,
        claims,
        overrode,
        ..
    } = resolved()
    else {
        unreachable!("resolved() builds a resolved event")
    };
    Event::Resolved {
        path,
        identifier,
        record,
        source,
        found,
        claims,
        tier: tier.map(str::to_string),
        overrode,
        cached,
        library: Some(answer),
    }
}

/// `skipped(reason)` with `library` set to `answer`.
fn skipped_with_library(reason: SkipReason, answer: LibraryAnswer) -> Event {
    let Event::Skipped { path, reason, .. } = skipped(reason) else {
        unreachable!("skipped() builds a skipped event")
    };
    Event::Skipped {
        path,
        reason,
        library: Some(answer),
    }
}

fn tracked_answer() -> LibraryAnswer {
    LibraryAnswer::Tracked {
        artifact: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string(),
        item: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70".to_string(),
    }
}

/// design D3: every `LibraryAnswer` variant round-trips through the
/// JSON line, tagged by `kind` in kebab-case, on a `resolved` event.
#[test]
fn json_line_of_resolved_carries_every_library_answer_variant_tagged_by_kind() {
    let cases: Vec<(LibraryAnswer, serde_json::Value)> = vec![
        (
            tracked_answer(),
            serde_json::json!({
                "kind": "tracked",
                "artifact": "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f",
                "item": "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70"
            }),
        ),
        (
            LibraryAnswer::Untracked,
            serde_json::json!({"kind": "untracked"}),
        ),
        (
            LibraryAnswer::UnrecognisedContent {
                artifacts: vec!["a".to_string()],
            },
            serde_json::json!({"kind": "unrecognised-content", "artifacts": ["a"]}),
        ),
        (
            LibraryAnswer::Ambiguous {
                artifacts: vec!["a".to_string(), "b".to_string()],
            },
            serde_json::json!({"kind": "ambiguous", "artifacts": ["a", "b"]}),
        ),
        (
            LibraryAnswer::NoItem {
                artifact: "a".to_string(),
            },
            serde_json::json!({"kind": "no-item", "artifact": "a"}),
        ),
        (
            LibraryAnswer::DanglingItem {
                artifact: "a".to_string(),
                item: "b".to_string(),
            },
            serde_json::json!({"kind": "dangling-item", "artifact": "a", "item": "b"}),
        ),
        (
            LibraryAnswer::UnreadableItem {
                artifact: "a".to_string(),
                item: "b".to_string(),
                path: PathBuf::from("/lib/items/key.b.toml"),
                message: "bad toml".to_string(),
            },
            serde_json::json!({
                "kind": "unreadable-item",
                "artifact": "a",
                "item": "b",
                "path": "/lib/items/key.b.toml",
                "message": "bad toml"
            }),
        ),
        (
            LibraryAnswer::AmbiguousItem {
                artifact: "a".to_string(),
                item: "b".to_string(),
                files: vec![
                    PathBuf::from("/lib/items/x.b.toml"),
                    PathBuf::from("/lib/items/y.b.toml"),
                ],
            },
            serde_json::json!({
                "kind": "ambiguous-item",
                "artifact": "a",
                "item": "b",
                "files": ["/lib/items/x.b.toml", "/lib/items/y.b.toml"]
            }),
        ),
        (
            LibraryAnswer::Unhashable {
                artifacts: vec!["a".to_string()],
            },
            serde_json::json!({"kind": "unhashable", "artifacts": ["a"]}),
        ),
        (
            LibraryAnswer::UnreadableRecords {
                listed: true,
                unreadable: 2,
            },
            serde_json::json!({"kind": "unreadable-records", "listed": true, "unreadable": 2}),
        ),
        (
            LibraryAnswer::UnreadableRecords {
                listed: false,
                unreadable: 0,
            },
            serde_json::json!({"kind": "unreadable-records", "listed": false, "unreadable": 0}),
        ),
    ];

    for (answer, expected) in cases {
        let event = resolved_with_library(Some("library"), false, answer.clone());
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        assert_eq!(value["schema"], Value::from(3));
        assert_eq!(value["library"], expected, "for {answer:?}");
    }
}

/// The same shapes on a `skipped` event.
#[test]
fn json_line_of_skipped_carries_every_library_answer_variant_tagged_by_kind() {
    let event = skipped_with_library(SkipReason::NoIdentifier, LibraryAnswer::Untracked);
    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(value["schema"], Value::from(3));
    assert_eq!(value["library"], serde_json::json!({"kind": "untracked"}));

    let event = skipped_with_library(SkipReason::NoIdentifier, tracked_answer());
    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(
        value["library"],
        serde_json::json!({
            "kind": "tracked",
            "artifact": "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f",
            "item": "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70"
        })
    );
}

/// `library` is `"library":null` for `None`, on both event kinds, and
/// carries the schema unchanged at 3 (design D3, D4).
#[test]
fn json_line_writes_library_null_when_the_library_was_not_consulted() {
    let value: Value = serde_json::from_str(&json_line(&resolved())).unwrap();
    assert_eq!(value["library"], Value::Null);
    assert_eq!(value["schema"], Value::from(3));

    let value: Value =
        serde_json::from_str(&json_line(&skipped(SkipReason::NoIdentifier))).unwrap();
    assert_eq!(value["library"], Value::Null);
    assert_eq!(value["schema"], Value::from(3));
}

/// A `resolved` JSON line written before this change — no `library` key
/// at all — still deserializes, with `library: None` (design D10:
/// `#[serde(default)]`).
#[test]
fn a_resolved_line_with_no_library_key_deserializes_with_library_none() {
    let without_library = r#"{"schema":3,"event":"resolved","path":"paper.pdf",
        "identifier":"doi:10.1000/xyz","record":{"type":"article-journal"},"source":"crossref",
        "found":"doi:10.1000/xyz","claims":[],"tier":null,"overrode":null,"cached":false}"#;

    let value: serde_json::Value = serde_json::from_str(without_library).unwrap();
    let event: Event = serde_json::from_value(value).unwrap();

    match event {
        Event::Resolved { library, .. } => assert_eq!(library, None),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// The same for a `skipped` line with no `library` key.
#[test]
fn a_skipped_line_with_no_library_key_deserializes_with_library_none() {
    let without_library = r#"{"schema":3,"event":"skipped","path":"mystery.pdf",
        "reason":{"kind":"no-identifier"}}"#;

    let value: serde_json::Value = serde_json::from_str(without_library).unwrap();
    let event: Event = serde_json::from_value(value).unwrap();

    match event {
        Event::Skipped { library, .. } => assert_eq!(library, None),
        other => panic!("expected Event::Skipped, got {other:?}"),
    }
}

// --- human_line: design D5's exact strings ---

/// design D5: `tier: Some("library")` gains ` (from the library)`.
#[test]
fn human_line_of_a_library_answer_gains_the_from_the_library_suffix() {
    let event = resolved_with_library(Some("library"), false, tracked_answer());

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved 10.1000/xyz123 via crossref (from the library)"
    );
}

/// A provenance-less item reports `source: "library"`, giving
/// `… via library (from the library)` (design D5).
#[test]
fn human_line_of_a_provenance_less_library_answer_names_library_as_the_source() {
    let Event::Resolved {
        path,
        identifier,
        record,
        found,
        claims,
        overrode,
        ..
    } = resolved_with_library(Some("library"), false, tracked_answer())
    else {
        unreachable!()
    };
    let event = Event::Resolved {
        path,
        identifier,
        record,
        source: "library".to_string(),
        found,
        claims,
        tier: Some("library".to_string()),
        overrode,
        cached: false,
        library: Some(tracked_answer()),
    };

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved 10.1000/xyz123 via library (from the library)"
    );
}

/// design D5: a resolution whose `library.kind` is a problem appends
/// `; the library could not answer: <what>` after the line it would
/// otherwise have had — including ` (cached)`.
#[test]
fn human_line_appends_the_library_problem_clause_to_a_cached_resolved_line() {
    let event = resolved_with_library(
        None,
        true,
        LibraryAnswer::DanglingItem {
            artifact: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string(),
            item: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70".to_string(),
        },
    );

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved 10.1000/xyz123 via crossref (cached); the library could not \
         answer: artifact 0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f links to item \
         0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70, which the library does not hold"
    );
}

/// The same clause on a `skipped` line.
#[test]
fn human_line_appends_the_library_problem_clause_to_a_skipped_line() {
    let event = skipped_with_library(
        SkipReason::NoIdentifier,
        LibraryAnswer::UnrecognisedContent {
            artifacts: vec!["0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string()],
        },
    );

    assert_eq!(
        human_line(&event).unwrap(),
        "mystery.pdf: skipped, no identifier found; the library could not answer: the \
         library records artifact 0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f at this path, but \
         not these bytes"
    );
}

/// design D5's `<what>` with several artifacts, on `Ambiguous`.
#[test]
fn human_line_names_every_artifact_of_an_ambiguous_library_problem() {
    let event = skipped_with_library(
        SkipReason::NoIdentifier,
        LibraryAnswer::Ambiguous {
            artifacts: vec!["a-id".to_string(), "b-id".to_string()],
        },
    );

    assert_eq!(
        human_line(&event).unwrap(),
        "mystery.pdf: skipped, no identifier found; the library could not answer: 2 \
         artifact records claim this file: a-id, b-id"
    );
}

/// design D5's `<what>` for `unreadable-records`, singular and plural,
/// and the `listed: false` wording.
#[test]
fn human_line_names_the_unreadable_records_wording_for_each_shape() {
    let one = skipped_with_library(
        SkipReason::NoIdentifier,
        LibraryAnswer::UnreadableRecords {
            listed: true,
            unreadable: 1,
        },
    );
    assert_eq!(
        human_line(&one).unwrap(),
        "mystery.pdf: skipped, no identifier found; the library could not answer: 1 \
         artifact record file could not be read, so the library cannot say whether it \
         tracks this file"
    );

    let several = skipped_with_library(
        SkipReason::NoIdentifier,
        LibraryAnswer::UnreadableRecords {
            listed: true,
            unreadable: 3,
        },
    );
    assert_eq!(
        human_line(&several).unwrap(),
        "mystery.pdf: skipped, no identifier found; the library could not answer: 3 \
         artifact record files could not be read, so the library cannot say whether it \
         tracks this file"
    );

    let unlistable = skipped_with_library(
        SkipReason::NoIdentifier,
        LibraryAnswer::UnreadableRecords {
            listed: false,
            unreadable: 0,
        },
    );
    assert_eq!(
        human_line(&unlistable).unwrap(),
        "mystery.pdf: skipped, no identifier found; the library could not answer: the \
         library's artifact records could not be listed, so it cannot say whether it \
         tracks this file"
    );
}

/// `Tracked` with `tier: Some("supplied")` (an operator's
/// re-identification of a tracked file) leaves the line byte-identical
/// to today's: a library answer with no problem adds nothing when
/// `tier` is not `"library"` (design D3, D5).
#[test]
fn human_line_of_a_supplied_tracked_file_is_unchanged() {
    let event = resolved_with_library(Some("supplied"), false, tracked_answer());

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved 10.1000/xyz123 via crossref"
    );
}

/// `Untracked` leaves the line unchanged too.
#[test]
fn human_line_of_an_untracked_file_is_unchanged() {
    let event = resolved_with_library(Some("first-page"), false, LibraryAnswer::Untracked);

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved 10.1000/xyz123 via crossref"
    );
}

/// `library: None` — the library was not consulted at all — leaves the
/// line unchanged, exactly as before this change.
#[test]
fn human_line_with_no_library_consultation_is_unchanged() {
    assert_eq!(
        human_line(&resolved()).unwrap(),
        "paper.pdf: resolved 10.1000/xyz123 via crossref"
    );
}

// ---------------------------------------------------------------------
// Event::LibraryExtraction — report-extraction-per-file, task 1.1
// ---------------------------------------------------------------------

fn library_extraction(path: &str, extraction: Extraction) -> Event {
    Event::LibraryExtraction {
        path: path.to_string(),
        extraction,
    }
}

/// One instance of every [`Extraction`] variant, paired with the
/// `extraction` object design D4 shows for it.
fn all_extractions() -> Vec<(Extraction, serde_json::Value)> {
    vec![
        (
            Extraction::Found {
                identifier: "doi:10.1234/x".to_string(),
                tier: "text-layer".to_string(),
            },
            serde_json::json!({
                "kind": "found",
                "identifier": "doi:10.1234/x",
                "tier": "text-layer",
            }),
        ),
        (
            Extraction::NoTextLayer,
            serde_json::json!({"kind": "no-text-layer"}),
        ),
        (
            Extraction::TextWithoutIdentifier,
            serde_json::json!({"kind": "text-without-identifier"}),
        ),
        (
            Extraction::Encrypted,
            serde_json::json!({"kind": "encrypted"}),
        ),
        (
            Extraction::Unreadable {
                message: "truncated stream".to_string(),
            },
            serde_json::json!({"kind": "unreadable", "message": "truncated stream"}),
        ),
    ]
}

/// design D4: `json_line` of `Event::LibraryExtraction` carries
/// `schema`, `event`, `path` and `extraction`, the last tagged by
/// `kind` in kebab-case, for every result kind.
#[test]
fn json_line_of_library_extraction_matches_design_d4_for_every_kind() {
    for (extraction, expected_extraction) in all_extractions() {
        let event = library_extraction("sub/a.pdf", extraction.clone());
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "schema": 3,
                "event": "library-extraction",
                "path": "sub/a.pdf",
                "extraction": expected_extraction,
            }),
            "extraction {extraction:?} produced {value}"
        );
    }
}

/// Every `library-extraction` line deserializes back to the event it
/// was rendered from.
#[test]
fn json_line_of_library_extraction_round_trips_for_every_kind() {
    for (extraction, _) in all_extractions() {
        let event = library_extraction("sub/a.pdf", extraction);
        let line = json_line(&event);
        let parsed: Event = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed, event);
    }
}

/// `Extraction::is_found` is true for `Found` alone.
#[test]
fn extraction_is_found_is_true_for_found_alone() {
    for (extraction, _) in all_extractions() {
        let expected = matches!(extraction, Extraction::Found { .. });
        assert_eq!(
            extraction.is_found(),
            expected,
            "is_found() disagreed for {extraction:?}"
        );
    }
}

/// design D8: a `library-extraction` event is neither a skip nor a
/// finding, for every result kind — `Counts::observe` leaves every
/// counter at zero.
#[test]
fn counts_observe_of_library_extraction_counts_nothing_for_every_kind() {
    for (extraction, _) in all_extractions() {
        let mut counts = Counts::default();
        counts.observe(&library_extraction("sub/a.pdf", extraction.clone()));
        assert_eq!(
            counts,
            Counts::default(),
            "extraction {extraction:?} changed the totals"
        );
    }
}

// ---------------------------------------------------------------------
// Event::LibraryExtraction — human rendering, task 2.1
// ---------------------------------------------------------------------

/// design D7: the exact human line for every result kind.
#[test]
fn human_line_of_library_extraction_matches_design_d7_for_every_kind() {
    let cases: Vec<(Extraction, &str)> = vec![
        (
            Extraction::Found {
                identifier: "doi:10.1234/x".to_string(),
                tier: "embedded-metadata".to_string(),
            },
            "sub/a.pdf: identifier doi:10.1234/x from embedded metadata",
        ),
        (
            Extraction::Found {
                identifier: "doi:10.1234/x".to_string(),
                tier: "text-layer".to_string(),
            },
            "sub/a.pdf: identifier doi:10.1234/x from the text layer",
        ),
        (
            Extraction::Found {
                identifier: "doi:10.1234/x".to_string(),
                tier: "supplied".to_string(),
            },
            "sub/a.pdf: identifier doi:10.1234/x from the file",
        ),
        (
            Extraction::NoTextLayer,
            "sub/a.pdf: no identifier found; the pages read hold no text",
        ),
        (
            Extraction::TextWithoutIdentifier,
            "sub/a.pdf: no identifier found in its metadata or the pages read",
        ),
        (
            Extraction::Encrypted,
            "sub/a.pdf: encrypted, so no identifier could be read",
        ),
        (
            Extraction::Unreadable {
                message: "truncated stream".to_string(),
            },
            "sub/a.pdf: unreadable (truncated stream)",
        ),
    ];

    for (extraction, expected) in cases {
        let event = library_extraction("sub/a.pdf", extraction.clone());
        assert_eq!(
            human_line(&event).unwrap(),
            expected,
            "extraction {extraction:?}"
        );
    }
}

/// design D7: a control character in the path is written as `\xNN` on
/// the human line, and carried raw by the JSON line — JSON is escaped
/// by its own encoding.
#[test]
fn human_line_of_library_extraction_escapes_control_characters_in_the_path() {
    let path = "\u{1b}[2Jpaper.pdf";
    let event = library_extraction(path, Extraction::NoTextLayer);

    assert_eq!(
        human_line(&event).unwrap(),
        "\\x1b[2Jpaper.pdf: no identifier found; the pages read hold no text"
    );

    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(value["path"], Value::from(path));
}

/// The same escaping applies to the identifier on a `found` line.
#[test]
fn human_line_of_library_extraction_escapes_control_characters_in_the_identifier() {
    let event = library_extraction(
        "paper.pdf",
        Extraction::Found {
            identifier: "doi:10.1234/\u{1b}[2J".to_string(),
            tier: "text-layer".to_string(),
        },
    );

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: identifier doi:10.1234/\\x1b[2J from the text layer"
    );

    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(
        value["extraction"]["identifier"],
        Value::from("doi:10.1234/\u{1b}[2J")
    );
}

/// design D7's own example: a message holding `"\u{1b}[2J"` renders
/// `\x1b[2J` on an `unreadable` line, while `json_line` of the same
/// event carries the message unchanged.
#[test]
fn human_line_of_library_extraction_escapes_control_characters_in_the_message() {
    let event = library_extraction(
        "paper.pdf",
        Extraction::Unreadable {
            message: "\u{1b}[2J".to_string(),
        },
    );

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: unreadable (\\x1b[2J)"
    );

    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(value["extraction"]["message"], Value::from("\u{1b}[2J"));
}

/// `render(Format::Human, e)` equals `human_line(e)` for every kind.
#[test]
fn render_human_equals_human_line_for_every_library_extraction_kind() {
    for (extraction, _) in all_extractions() {
        let event = library_extraction("sub/a.pdf", extraction);
        assert_eq!(render(Format::Human, &event), human_line(&event));
    }
}

// ---------------------------------------------------------------------
// Event::LibraryCondition and Adoption::Unindexed — the vocabulary,
// task 1.1
// ---------------------------------------------------------------------

/// design D3: `json_line` of `Event::LibraryCondition` gives exactly
/// the documented line for each kind, field for field and in the order
/// design D3 shows: `missing` carries `id` before `record`, `unlinked`
/// carries `id` alone after `kind`.
#[test]
fn json_line_of_library_condition_matches_design_d3_for_every_kind() {
    let cases: Vec<(Event, String)> = vec![
        (
            library_condition("sub/new.pdf", Condition::Orphan),
            r#"{"schema":3,"event":"library-condition","path":"sub/new.pdf","condition":{"kind":"orphan"}}"#
                .to_string(),
        ),
        (
            library_condition(
                "gone.pdf",
                Condition::Missing {
                    id: COND_UUID_MISSING.to_string(),
                    record: format!(".borax/artifacts/{COND_UUID_MISSING}.toml"),
                },
            ),
            format!(
                r#"{{"schema":3,"event":"library-condition","path":"gone.pdf","condition":{{"kind":"missing","id":"{COND_UUID_MISSING}","record":".borax/artifacts/{COND_UUID_MISSING}.toml"}}}}"#
            ),
        ),
        (
            library_condition(
                "items/milner1978.unlinked.toml",
                Condition::Unlinked {
                    id: COND_UUID_UNLINKED.to_string(),
                },
            ),
            format!(
                r#"{{"schema":3,"event":"library-condition","path":"items/milner1978.unlinked.toml","condition":{{"kind":"unlinked","id":"{COND_UUID_UNLINKED}"}}}}"#
            ),
        ),
    ];

    for (event, expected) in cases {
        assert_eq!(json_line(&event), expected, "got {event:?}");
    }
}

/// design D6: `json_line` of `Event::LibraryAdoption` with
/// `Adoption::Unindexed` is exactly the documented line.
#[test]
fn json_line_of_library_adoption_unindexed_matches_design_d6() {
    let event = library_adoption("new.pdf", Adoption::Unindexed);

    assert_eq!(
        json_line(&event),
        r#"{"schema":3,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"unindexed"}}"#
    );
}

/// Every `library-condition` line deserializes back to the event it was
/// rendered from.
#[test]
fn json_line_of_library_condition_round_trips_for_every_kind() {
    for (path, condition) in all_conditions() {
        let event = library_condition(path, condition);
        let line = json_line(&event);
        let parsed: Event = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed, event);
    }
}

/// The `library-adoption` line for `Adoption::Unindexed` deserializes
/// back to the event it was rendered from.
#[test]
fn json_line_of_library_adoption_unindexed_round_trips() {
    let event = library_adoption("new.pdf", Adoption::Unindexed);
    let line = json_line(&event);
    let parsed: Event = serde_json::from_str(&line).unwrap();
    assert_eq!(parsed, event);
}

/// design D8: a `library-condition` event is neither a skip nor a
/// finding, for every kind — `Counts::observe` leaves every counter at
/// zero.
#[test]
fn counts_observe_of_library_condition_counts_nothing_for_every_kind() {
    for (path, condition) in all_conditions() {
        let mut counts = Counts::default();
        counts.observe(&library_condition(path, condition.clone()));
        assert_eq!(
            counts,
            Counts::default(),
            "condition {condition:?} changed the totals"
        );
    }
}

/// design D8: the same holds for the new `unindexed` adoption kind, as
/// it already does for every other `Adoption` variant.
#[test]
fn counts_observe_of_library_adoption_unindexed_counts_nothing() {
    let mut counts = Counts::default();
    counts.observe(&library_adoption("new.pdf", Adoption::Unindexed));
    assert_eq!(counts, Counts::default(), "got {counts:?}");
}

// ---------------------------------------------------------------------
// Event::LibraryCondition and Adoption::Unindexed — human rendering,
// task 2.1
// ---------------------------------------------------------------------

/// design D7: the exact human line for every condition kind.
#[test]
fn human_line_of_library_condition_matches_design_d7_for_every_kind() {
    let cases: Vec<(Event, String)> = vec![
        (
            library_condition("new.pdf", Condition::Orphan),
            "new.pdf: orphan; no artifact record names it".to_string(),
        ),
        (
            library_condition(
                "gone.pdf",
                Condition::Missing {
                    id: COND_UUID_MISSING.to_string(),
                    record: format!(".borax/artifacts/{COND_UUID_MISSING}.toml"),
                },
            ),
            format!(
                "gone.pdf: missing; artifact record {COND_UUID_MISSING} \
                 (.borax/artifacts/{COND_UUID_MISSING}.toml) names this path \
                 and the library has no artifact here"
            ),
        ),
        (
            library_condition(
                "items/milner1978.unlinked.toml",
                Condition::Unlinked {
                    id: COND_UUID_UNLINKED.to_string(),
                },
            ),
            format!(
                "items/milner1978.unlinked.toml: unlinked; no artifact record \
                 links item {COND_UUID_UNLINKED}"
            ),
        ),
    ];

    for (event, expected) in cases {
        assert_eq!(human_line(&event).unwrap(), expected, "got {event:?}");
    }
}

/// design D7: the exact human line for the new `unindexed` adoption
/// kind — the same closing clause the existing `unreadable` and
/// `unwritten` adoption lines use.
#[test]
fn human_line_of_library_adoption_unindexed_matches_design_d7() {
    let event = library_adoption("new.pdf", Adoption::Unindexed);

    assert_eq!(
        human_line(&event).unwrap(),
        "new.pdf: the content index holds no record of its bytes, so it is still an orphan"
    );
}

/// design D7: a control character in a condition's `path` is written as
/// `\xNN` on the human line, for every kind, while `json_line` of the
/// same event carries the path unchanged.
#[test]
fn human_line_of_library_condition_escapes_control_characters_in_the_path() {
    let path = "\u{1b}[2Jpaper.pdf";
    for (_, condition) in all_conditions() {
        let event = library_condition(path, condition.clone());
        let line = human_line(&event).unwrap();
        assert!(
            line.starts_with("\\x1b[2Jpaper.pdf:"),
            "condition {condition:?} line {line:?}"
        );

        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        assert_eq!(value["path"], Value::from(path));
    }
}

/// design D7: a missing record's `record` is escaped too, on the human
/// line alone — `json_line` carries it unchanged.
#[test]
fn human_line_of_library_condition_escapes_control_characters_in_the_missing_record() {
    let record = "\u{1b}[2J.toml";
    let event = library_condition(
        "gone.pdf",
        Condition::Missing {
            id: COND_UUID_MISSING.to_string(),
            record: record.to_string(),
        },
    );

    let line = human_line(&event).unwrap();
    assert!(line.contains("(\\x1b[2J.toml)"), "got {line:?}");

    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(value["condition"]["record"], Value::from(record));
}

/// design D7: the `LibraryAdoption` arm renders the path for every
/// kind, so a control character in the path is escaped on all five —
/// `recorded`, `held`, `unreadable`, `unwritten` and `unindexed` alike.
#[test]
fn human_line_of_library_adoption_escapes_control_characters_in_the_path_for_every_kind() {
    let path = "\u{1b}[2Jpaper.pdf";
    let adoptions = vec![
        Adoption::Recorded {
            id: COND_UUID_MISSING.to_string(),
            item: COND_UUID_UNLINKED.to_string(),
        },
        Adoption::Held {
            id: COND_UUID_MISSING.to_string(),
        },
        Adoption::Unreadable {
            message: "could not read".to_string(),
        },
        Adoption::Unwritten {
            message: "disk full".to_string(),
        },
        Adoption::Unindexed,
    ];

    for adoption in adoptions {
        let event = library_adoption(path, adoption.clone());
        let line = human_line(&event).unwrap();
        assert!(
            line.starts_with("\\x1b[2Jpaper.pdf:"),
            "adoption {adoption:?} line {line:?}"
        );

        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        assert_eq!(value["path"], Value::from(path));
    }
}

/// design D7: for a path with no control character, the existing
/// `library-adoption` lines are unchanged byte for byte, one per
/// existing kind.
#[test]
fn human_line_of_library_adoption_is_unchanged_for_a_path_with_no_control_character() {
    let cases: Vec<(Adoption, &str)> = vec![
        (
            Adoption::Recorded {
                id: "art-1".to_string(),
                item: "item-1".to_string(),
            },
            "paper.pdf: adopted as artifact art-1 of item item-1",
        ),
        (
            Adoption::Held {
                id: "art-1".to_string(),
            },
            "paper.pdf: holds bytes artifact art-1 already records, so it was left \
             an orphan; run borax reconcile if the file was moved",
        ),
        (
            Adoption::Unreadable {
                message: "truncated".to_string(),
            },
            "paper.pdf: could not be read (truncated), so it is still an orphan",
        ),
        (
            Adoption::Unwritten {
                message: "disk full".to_string(),
            },
            "paper.pdf: could not be recorded (disk full), so it is still an orphan",
        ),
    ];

    for (adoption, expected) in cases {
        let event = library_adoption("paper.pdf", adoption.clone());
        assert_eq!(
            human_line(&event).unwrap(),
            expected,
            "adoption {adoption:?}"
        );
    }
}

/// `render(Format::Human, e)` equals `human_line(e)` for every
/// condition kind and for the new `unindexed` adoption kind.
#[test]
fn render_human_equals_human_line_for_every_library_condition_and_unindexed() {
    for (path, condition) in all_conditions() {
        let event = library_condition(path, condition);
        assert_eq!(render(Format::Human, &event), human_line(&event));
    }
    let event = library_adoption("new.pdf", Adoption::Unindexed);
    assert_eq!(render(Format::Human, &event), human_line(&event));
}
