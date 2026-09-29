#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use borax::event::{
    Attempt, Claim, ClaimOrigin, Counts, Diagnostic, Event, Format, Level, SCHEMA, SkipReason,
    Summary, TableUsed, human_line, human_summary, json_line, render,
};
use borax::pipeline::{FileOutcome, FileRecord, event_for};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_core::record::{BoraxExt, EntryType, Record, Source};
use serde_json::Value;

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

/// Every `Event` variant, so coverage-oriented tests can iterate once.
fn all_events() -> Vec<Event> {
    vec![
        run_started(),
        resolved(),
        planned(),
        renamed(),
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
    assert_eq!(keys, vec!["event", "path", "reason", "schema"]);

    assert_eq!(object["path"], Value::from("mystery.pdf"));
    assert!(object["reason"].is_object());
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
    let variants = [
        Summary::Renaming,
        Summary::Resolution,
        Summary::Validation,
        Summary::Silent,
    ];
    let cases: [(fn(usize) -> Counts, &str); 3] = [
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
        },
        Event::Renamed {
            path: PathBuf::from("a.pdf"),
            target: PathBuf::from("smith2024_a.pdf"),
            hash: hash_of("a.pdf"),
        },
        Event::Skipped {
            path: PathBuf::from("c.pdf"),
            reason: SkipReason::NoIdentifier,
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
        source: Some(borax_sources::source::SourceName::Arxiv),
        tier: Some(borax::pipeline::Provenance::Extracted(
            borax_pdf::tiered::Tier::TextLayer,
        )),
        found: Some(Identifier::Arxiv(ArxivId::parse("2401.01234").unwrap())),
        claims: Vec::new(),
        cached: false,
        hash: Some(hash_bytes(b"paper")),
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
        source: None,
        tier: None,
        found: None,
        claims: Vec::new(),
        cached: true,
        hash: Some(hash_bytes(b"paper")),
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
        source: None,
        tier: None,
        found: None,
        claims: Vec::new(),
        cached: true,
        hash: Some(hash_bytes(b"paper")),
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
        source: None,
        tier: None,
        found: None,
        claims: Vec::new(),
        cached: true,
        hash: Some(hash_bytes(b"paper")),
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
