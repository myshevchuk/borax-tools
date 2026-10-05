#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use borax::event::{
    Acceptance, Adoption, Claim, ClaimOrigin, Condition, ContentIndexSection, Counts, Diagnostic,
    Displaced, Event, Extraction, ExtractionResultStep, ExtractionSection, FetchedFrom, Format,
    IdentifierInputStep, IdentifierOrigin, IndexReadStep, Level, LibraryAnswer, LibraryStep,
    LookupRound, LookupStep, MatchCheckStep, RetrievedFrom, SCHEMA, Sections, ServiceAnswer,
    ServiceOutcome, SkipReason, Submission, SubmissionAcceptance, SubmissionOutcome, Summary,
    SyntaxStep, TableUsed, TitlesStep, WriteStep, human_line, human_summary, json_line, render,
};
use borax::evidence::{
    Consultation, Evidence, ExtractionEvidence, ExtractionStep, IdentifierInput, IndexEvidence,
    IndexRead, IndexWrite, LookupEvidence, MatchCheck, Origin, ServiceAttempt, Titles, Unattempted,
};
use borax::pipeline::{FileRecord, resolved_event};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_core::record::{BoraxExt, EntryType, Record, Source};
use borax_pdf::tiered::Tier;
use borax_sources::cache::CacheWrite;
use borax_sources::source::{Retrieval, SourceName};
use serde_json::Value;

/// The evidence of a record reached through a lookup: the lookup
/// attempted `identifier` through `service`, found it over the network,
/// and wrote it to the content index.
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
        identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
        lookup: LookupEvidence::Attempted {
            identifier,
            origin: Origin::Extracted(tier),
            attempts: vec![ServiceAttempt {
                service,
                outcome: Ok(Retrieval::Network { stored: None }),
            }],
            earlier: vec![],
        },
        match_check: MatchCheck::Agreed,
    }
}

/// The evidence of a record the content index answered for.
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
        identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
        lookup: LookupEvidence::NotAttempted(Unattempted::ContentIndexHit),
        match_check: MatchCheck::NotAttempted(Unattempted::ContentIndexHit),
    }
}

// --- Sections fixtures (design D3, D12) ---

/// Every section a file resolved by a fresh network lookup carries: no
/// library, a content-index write, an identifier found in the text
/// layer, titles read empty, a lookup naming `service`, a record
/// retrieved over the network from that service, titles agreeing, and
/// automatic acceptance.
fn sections_via_network(service: &str, identifier: &str) -> Sections {
    Sections {
        library: LibraryStep::NotAttempted {
            reason: "no-library".to_string(),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::Miss,
            write: WriteStep::Written,
        },
        extraction: ExtractionSection {
            result: ExtractionResultStep::Found {
                identifier: identifier.to_string(),
                tier: "text-layer".to_string(),
            },
            titles: TitlesStep::Read { claims: Vec::new() },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: "not-asked".to_string(),
        },
        lookup: LookupStep::Attempted {
            identifier: identifier.to_string(),
            origin: IdentifierOrigin::Extracted,
            attempts: vec![ServiceAnswer {
                service: service.to_string(),
                outcome: ServiceOutcome::Found {
                    retrieval: FetchedFrom::Network,
                    stored: Some(WriteStep::Written),
                },
            }],
            earlier: vec![],
        },
        record_retrieval: Some(RetrievedFrom::Network {
            service: service.to_string(),
        }),
        match_check: MatchCheckStep::Agreed,
        acceptance: Acceptance::Automatic,
    }
}

/// Every section as a content-index hit reports them: every step not
/// attempted for `content-index-hit`, the record retrieved from the
/// content index.
fn sections_via_content_index() -> Sections {
    let not_attempted = || "content-index-hit".to_string();
    Sections {
        library: LibraryStep::NotAttempted {
            reason: "no-library".to_string(),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::Hit,
            write: WriteStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        extraction: ExtractionSection {
            result: ExtractionResultStep::NotAttempted {
                reason: not_attempted(),
            },
            titles: TitlesStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: "not-asked".to_string(),
        },
        lookup: LookupStep::NotAttempted {
            reason: not_attempted(),
        },
        record_retrieval: Some(RetrievedFrom::ContentIndex),
        match_check: MatchCheckStep::NotAttempted {
            reason: not_attempted(),
        },
        acceptance: Acceptance::Automatic,
    }
}

/// Every section as a resolution reached through the library's own item
/// reports them: every step not attempted for `library-answered`.
fn sections_via_library(artifact: &str, item: &str) -> Sections {
    let not_attempted = || "library-answered".to_string();
    Sections {
        library: LibraryStep::Consulted {
            answer: tracked_answer_with(artifact, item),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::NotAttempted {
                reason: not_attempted(),
            },
            write: WriteStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        extraction: ExtractionSection {
            result: ExtractionResultStep::NotAttempted {
                reason: not_attempted(),
            },
            titles: TitlesStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: "not-asked".to_string(),
        },
        lookup: LookupStep::NotAttempted {
            reason: not_attempted(),
        },
        record_retrieval: Some(RetrievedFrom::Library {
            artifact: artifact.to_string(),
            item: item.to_string(),
        }),
        match_check: MatchCheckStep::NotAttempted {
            reason: not_attempted(),
        },
        acceptance: Acceptance::Automatic,
    }
}

/// Every section as a verdict reached before any step ran: every step
/// not attempted for `content-duplicate`, no record retrieved.
fn sections_content_duplicate() -> Sections {
    let not_attempted = || "content-duplicate".to_string();
    Sections {
        library: LibraryStep::NotAttempted {
            reason: not_attempted(),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::NotAttempted {
                reason: not_attempted(),
            },
            write: WriteStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        extraction: ExtractionSection {
            result: ExtractionResultStep::NotAttempted {
                reason: not_attempted(),
            },
            titles: TitlesStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: not_attempted(),
        },
        lookup: LookupStep::NotAttempted {
            reason: not_attempted(),
        },
        record_retrieval: None,
        match_check: MatchCheckStep::NotAttempted {
            reason: not_attempted(),
        },
        acceptance: Acceptance::NotApplicable,
    }
}

fn tracked_answer() -> LibraryAnswer {
    tracked_answer_with(
        "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f",
        "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70",
    )
}

fn tracked_answer_with(artifact: &str, item: &str) -> LibraryAnswer {
    LibraryAnswer::Tracked {
        artifact: artifact.to_string(),
        item: item.to_string(),
    }
}

/// A minimal record with the given title, authors and year, for the
/// human-line tests that need a `<work>` clause.
fn record_with(title: Option<&str>, authors: &[&str], year: Option<i32>) -> Record {
    use borax_core::record::{DateParts, Name};

    let mut record = Record::new(EntryType::Article);
    record.title = title.map(str::to_string);
    record.authors = authors
        .iter()
        .map(|family| Name {
            family: family.to_string(),
            given: None,
        })
        .collect();
    record.issued = year.map(|year| DateParts {
        year,
        month: None,
        day: None,
    });
    record
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

/// design D3: a resolved event through a fresh network lookup, carrying
/// no title (so the human line's `<work>` clause is absent).
fn resolved() -> Event {
    Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz123")),
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

/// design D12: a non-resolution skip, carrying `path` and `reason` and
/// nothing else.
fn skipped(reason: SkipReason) -> Event {
    Event::Skipped {
        path: PathBuf::from("mystery.pdf"),
        reason,
        sections: None,
        candidate: None,
    }
}

/// design D4, D12: a resolution-verdict skip, carrying every section.
fn skipped_verdict(reason: SkipReason, sections: Sections) -> Event {
    assert!(
        reason.is_resolution_verdict(),
        "skipped_verdict is for a resolution verdict; got {reason:?}"
    );
    Event::Skipped {
        path: PathBuf::from("mystery.pdf"),
        reason,
        sections: Some(Box::new(sections)),
        candidate: None,
    }
}

/// design D4: a conflict skip, which alone carries `candidate`.
fn skipped_conflict(sections: Sections, candidate: Record) -> Event {
    Event::Skipped {
        path: PathBuf::from("mystery.pdf"),
        reason: SkipReason::Conflict,
        sections: Some(Box::new(sections)),
        candidate: Some(Box::new(candidate)),
    }
}

fn content_index_write(write: WriteStep) -> Event {
    Event::ContentIndexWrite {
        path: PathBuf::from("smith2024_borax.pdf"),
        write,
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

/// Every resolution-verdict `SkipReason`, one instance each, paired with
/// sections that satisfy it (design D4).
fn all_resolution_skips() -> Vec<Event> {
    vec![
        skipped_verdict(SkipReason::NoTextLayer, no_text_layer_sections()),
        skipped_verdict(
            SkipReason::TextWithoutIdentifier,
            text_without_identifier_sections(),
        ),
        skipped_verdict(SkipReason::Encrypted, encrypted_sections()),
        skipped_verdict(
            SkipReason::Unreadable {
                message: "not a PDF".to_string(),
            },
            unreadable_extraction_sections("not a PDF"),
        ),
        skipped_verdict(SkipReason::Unresolvable, unresolvable_sections()),
        skipped_conflict(conflict_sections(), conflict_candidate()),
        skipped_verdict(
            SkipReason::Duplicate {
                reason: borax_core::library::DuplicateReason::Content,
                existing_path: PathBuf::from("/lib/smith2015.pdf"),
            },
            sections_content_duplicate(),
        ),
        skipped_verdict(
            SkipReason::Duplicate {
                reason: borax_core::library::DuplicateReason::Work,
                existing_path: PathBuf::from("/lib/smith2015.pdf"),
            },
            sections_via_content_index(),
        ),
    ]
}

/// Every non-resolution `SkipReason`, one instance each.
fn all_plain_skips() -> Vec<Event> {
    vec![
        skipped(SkipReason::TargetTaken {
            target: PathBuf::from("smith2024_borax.pdf"),
        }),
        skipped(SkipReason::Unnameable),
        skipped(SkipReason::Declined),
        skipped(SkipReason::RenameFailed {
            message: "permission denied".to_string(),
        }),
        skipped(SkipReason::BibWriteFailed {
            message: "disk full".to_string(),
        }),
        skipped(SkipReason::Unciteable),
        skipped(SkipReason::SidecarTaken {
            target: PathBuf::from("paper.bib"),
        }),
        skipped(SkipReason::Unrecordable {
            message: "the file's content hash is unknown".to_string(),
        }),
        skipped(SkipReason::Stranding {
            id: "0190c3a2-aaaa-7000-8000-000000000003".to_string(),
        }),
    ]
}

/// Every `Event` variant, so coverage-oriented tests can iterate once.
fn all_events() -> Vec<Event> {
    let mut events = vec![
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
        content_index_write(WriteStep::Written),
        content_index_write(WriteStep::Failed {
            message: "disk full".to_string(),
        }),
    ];
    events.extend(all_resolution_skips());
    events.extend(all_plain_skips());
    events.extend([
        bib_entry(),
        sidecar(),
        config_setting(),
        cache_status(),
        cache_cleared(),
        lookup_missed(),
        run_finished(),
    ]);
    events
}

// ---------------------------------------------------------------------
// json_line() renders every variant as one well-formed JSON object
// ---------------------------------------------------------------------

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
fn schema_is_four() {
    assert_eq!(SCHEMA, 4);
}

#[test]
fn json_line_event_tag_is_the_variant_name_in_kebab_case() {
    let cases: Vec<(Event, &str)> = vec![
        (run_started(), "run-started"),
        (resolved(), "resolved"),
        (planned(), "planned"),
        (renamed(), "renamed"),
        (skipped(SkipReason::Declined), "skipped"),
        (
            content_index_write(WriteStep::Written),
            "content-index-write",
        ),
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

/// design D1, D3, D12: a `resolved` event serializes to exactly
/// `schema`, `event`, `path`, `identifier`, `record`, then the eight
/// section keys in pipeline order (`identifier_input` between
/// `extraction` and `lookup`), and none of the six removed schema-3
/// fields.
#[test]
fn json_line_of_resolved_has_exactly_the_documented_field_set() {
    let value: Value = serde_json::from_str(&json_line(&resolved())).unwrap();
    let object = value.as_object().unwrap();

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "acceptance",
            "content_index",
            "event",
            "extraction",
            "identifier",
            "identifier_input",
            "library",
            "lookup",
            "match_check",
            "path",
            "record",
            "record_retrieval",
            "schema",
        ]
    );

    for removed in ["found", "cached", "source", "tier", "claims", "overrode"] {
        assert!(
            !object.contains_key(removed),
            "resolved still carries removed field {removed:?}: {object:?}"
        );
    }

    assert_eq!(object["path"], Value::from("paper.pdf"));
    assert_eq!(object["identifier"], Value::from("doi:10.1000/xyz123"));
}

/// design D1: the section keys appear in pipeline order, since a
/// consumer that reads the raw text (rather than a parsed map) depends
/// on it. `identifier_input` sits between `extraction` and `lookup`.
#[test]
fn json_line_of_resolved_orders_its_keys_in_pipeline_order() {
    let line = json_line(&resolved());
    let order = [
        "\"schema\"",
        "\"event\"",
        "\"path\"",
        "\"identifier\"",
        "\"record\"",
        "\"library\"",
        "\"content_index\"",
        "\"extraction\"",
        "\"identifier_input\"",
        "\"lookup\"",
        "\"record_retrieval\"",
        "\"match_check\"",
        "\"acceptance\"",
    ];
    let mut last = 0;
    for key in order {
        let at = line
            .find(key)
            .unwrap_or_else(|| panic!("{key} missing from {line}"));
        assert!(at >= last, "{key} appears out of pipeline order in {line}");
        last = at;
    }
}

/// design D3: a resolution `skipped` event serializes to `schema`,
/// `event`, `path`, `reason`, the eight sections, and `candidate` only
/// on the conflict kind.
#[test]
fn json_line_of_a_resolution_skip_carries_sections_and_no_candidate_except_conflict() {
    for event in all_resolution_skips() {
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        let object = value.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();

        let is_conflict = matches!(
            &event,
            Event::Skipped {
                reason: SkipReason::Conflict,
                ..
            }
        );
        let mut expected = vec![
            "acceptance",
            "content_index",
            "event",
            "extraction",
            "identifier_input",
            "library",
            "lookup",
            "match_check",
            "path",
            "reason",
            "record_retrieval",
            "schema",
        ];
        if is_conflict {
            expected.push("candidate");
            expected.sort_unstable();
        }
        assert_eq!(keys, expected, "event {event:?}");
    }
}

/// design D4: a non-resolution `skipped` event serializes to `schema`,
/// `event`, `path` and `reason` and nothing else.
#[test]
fn json_line_of_a_non_resolution_skip_carries_path_and_reason_only() {
    for event in all_plain_skips() {
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        let object = value.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["event", "path", "reason", "schema"],
            "event {event:?}"
        );
    }
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

/// design D8: `content-index-write` serializes to `schema`, `event`,
/// `path` and `write`, with `write` either `written` or `failed` with
/// `message`.
#[test]
fn json_line_of_content_index_write_has_exactly_the_documented_field_set() {
    let value: Value =
        serde_json::from_str(&json_line(&content_index_write(WriteStep::Written))).unwrap();
    let object = value.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["event", "path", "schema", "write"]);
    assert_eq!(object["write"], serde_json::json!({"status": "written"}));

    let value: Value = serde_json::from_str(&json_line(&content_index_write(WriteStep::Failed {
        message: "disk full".to_string(),
    })))
    .unwrap();
    assert_eq!(
        value["write"],
        serde_json::json!({"status": "failed", "message": "disk full"})
    );
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

// ---------------------------------------------------------------------
// SkipReason nesting under Skipped.reason, tagged by kind (design D12)
// ---------------------------------------------------------------------

#[test]
fn skipped_nests_the_reason_under_reason_with_a_kebab_case_kind_tag() {
    let cases: Vec<(Event, &str)> = vec![
        (
            skipped_verdict(SkipReason::NoTextLayer, no_text_layer_sections()),
            "no-text-layer",
        ),
        (
            skipped_verdict(
                SkipReason::TextWithoutIdentifier,
                text_without_identifier_sections(),
            ),
            "text-without-identifier",
        ),
        (
            skipped_verdict(SkipReason::Encrypted, encrypted_sections()),
            "encrypted",
        ),
        (
            skipped_verdict(SkipReason::Unresolvable, unresolvable_sections()),
            "unresolvable",
        ),
        (
            skipped_conflict(conflict_sections(), conflict_candidate()),
            "conflict",
        ),
        (
            skipped(SkipReason::TargetTaken {
                target: PathBuf::from("x.pdf"),
            }),
            "target-taken",
        ),
        (
            skipped(SkipReason::Unreadable {
                message: "bad".to_string(),
            }),
            "unreadable",
        ),
        (
            skipped(SkipReason::BibWriteFailed {
                message: "disk full".to_string(),
            }),
            "bib-write-failed",
        ),
        (skipped(SkipReason::Unciteable), "unciteable"),
        (
            skipped(SkipReason::Unrecordable {
                message: "the file's content hash is unknown".to_string(),
            }),
            "unrecordable",
        ),
    ];

    for (event, expected_kind) in cases {
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        assert_eq!(value["reason"]["kind"], Value::from(expected_kind));
    }
}

/// design D4: `no-text-layer` and `text-without-identifier` carry
/// nothing but their kind — the fact schema 3's `no-identifier` carried,
/// now split in two (D14's mapping).
#[test]
fn no_text_layer_reason_carries_nothing_but_its_kind() {
    let value: Value = serde_json::from_str(&json_line(&skipped_verdict(
        SkipReason::NoTextLayer,
        no_text_layer_sections(),
    )))
    .unwrap();
    let reason = value["reason"].as_object().unwrap();
    assert_eq!(reason.keys().collect::<Vec<_>>(), vec!["kind"]);
}

#[test]
fn text_without_identifier_reason_carries_nothing_but_its_kind() {
    let value: Value = serde_json::from_str(&json_line(&skipped_verdict(
        SkipReason::TextWithoutIdentifier,
        text_without_identifier_sections(),
    )))
    .unwrap();
    let reason = value["reason"].as_object().unwrap();
    assert_eq!(reason.keys().collect::<Vec<_>>(), vec!["kind"]);
}

/// design D4: `encrypted` and `unresolvable` and `conflict` also carry
/// nothing but their kind — every fact that used to sit on `reason`
/// moved into the sections (D4's table).
#[test]
fn encrypted_unresolvable_and_conflict_reasons_carry_nothing_but_their_kind() {
    for event in [
        skipped_verdict(SkipReason::Encrypted, encrypted_sections()),
        skipped_verdict(SkipReason::Unresolvable, unresolvable_sections()),
        skipped_conflict(conflict_sections(), conflict_candidate()),
    ] {
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        let reason = value["reason"].as_object().unwrap();
        assert_eq!(
            reason.keys().collect::<Vec<_>>(),
            vec!["kind"],
            "event {event:?}"
        );
    }
}

/// `unreadable` alone keeps its `message` on the reason, stated a
/// second time in `extraction.result` (D4: "the maintainer kept it on
/// the reason").
#[test]
fn unreadable_reason_keeps_its_message() {
    let value: Value = serde_json::from_str(&json_line(&skipped_verdict(
        SkipReason::Unreadable {
            message: "truncated stream".to_string(),
        },
        unreadable_extraction_sections("truncated stream"),
    )))
    .unwrap();
    let mut keys: Vec<&str> = value["reason"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["kind", "message"]);
    assert_eq!(value["reason"]["message"], Value::from("truncated stream"));
    assert_eq!(
        value["extraction"]["result"]["message"],
        Value::from("truncated stream")
    );
}

#[test]
fn unciteable_reason_carries_nothing_but_its_kind() {
    let value: Value = serde_json::from_str(&json_line(&skipped(SkipReason::Unciteable))).unwrap();
    let reason = value["reason"].as_object().unwrap();
    assert_eq!(reason.keys().collect::<Vec<_>>(), vec!["kind"]);
}

/// design D4, D5: the facts schema 3's `unresolvable {found, tier,
/// attempts}` carried now live in `lookup` (`identifier`, `origin`,
/// `attempts`) and `extraction.result.tier`; `reason` itself carries
/// only `kind` (pinned above). `Attempt {source, error}` becomes
/// `ServiceAnswer {service, outcome}`.
#[test]
fn unresolvable_sections_carry_the_lookup_identifier_origin_and_attempts() {
    let event = skipped_verdict(SkipReason::Unresolvable, unresolvable_sections());
    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();

    assert_eq!(value["lookup"]["status"], Value::from("attempted"));
    assert_eq!(
        value["lookup"]["identifier"],
        Value::from("doi:10.1000/xyz123")
    );
    assert_eq!(value["lookup"]["origin"], Value::from("extracted"));
    assert_eq!(
        value["extraction"]["result"]["tier"],
        Value::from("text-layer")
    );

    let attempts = value["lookup"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["service"], Value::from("crossref"));
    assert_eq!(attempts[0]["outcome"]["status"], Value::from("not-found"));
    assert_eq!(attempts[1]["service"], Value::from("arxiv"));
    assert_eq!(attempts[1]["outcome"]["status"], Value::from("unavailable"));
    assert_eq!(attempts[1]["outcome"]["message"], Value::from("timed out"));
}

fn no_text_layer_sections() -> Sections {
    extraction_failure_sections(ExtractionResultStep::NoTextLayer)
}

fn text_without_identifier_sections() -> Sections {
    extraction_failure_sections(ExtractionResultStep::TextWithoutIdentifier)
}

fn encrypted_sections() -> Sections {
    extraction_failure_sections(ExtractionResultStep::Encrypted)
}

fn unreadable_extraction_sections(message: &str) -> Sections {
    extraction_failure_sections(ExtractionResultStep::Unreadable {
        message: message.to_string(),
    })
}

/// Sections for a resolution that failed at extraction: `library` not
/// attempted for `no-library`, the content index missed, extraction
/// failed with `result`, titles failed too, and every later step not
/// attempted for `extraction-failed`.
fn extraction_failure_sections(result: ExtractionResultStep) -> Sections {
    let not_attempted = || "extraction-failed".to_string();
    Sections {
        library: LibraryStep::NotAttempted {
            reason: "no-library".to_string(),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::Miss,
            write: WriteStep::NotAttempted {
                reason: not_attempted(),
            },
        },
        extraction: ExtractionSection {
            result,
            titles: TitlesStep::Failed {
                message: "could not open".to_string(),
            },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: not_attempted(),
        },
        lookup: LookupStep::NotAttempted {
            reason: not_attempted(),
        },
        record_retrieval: None,
        match_check: MatchCheckStep::NotAttempted {
            reason: not_attempted(),
        },
        acceptance: Acceptance::NotApplicable,
    }
}

/// Sections for an identifier no source held.
fn unresolvable_sections() -> Sections {
    Sections {
        library: LibraryStep::NotAttempted {
            reason: "no-library".to_string(),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::Miss,
            write: WriteStep::NotAttempted {
                reason: "no-record".to_string(),
            },
        },
        extraction: ExtractionSection {
            result: ExtractionResultStep::Found {
                identifier: "doi:10.1000/xyz123".to_string(),
                tier: "text-layer".to_string(),
            },
            titles: TitlesStep::Read { claims: Vec::new() },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: "not-asked".to_string(),
        },
        lookup: LookupStep::Attempted {
            identifier: "doi:10.1000/xyz123".to_string(),
            origin: IdentifierOrigin::Extracted,
            attempts: vec![
                ServiceAnswer {
                    service: "crossref".to_string(),
                    outcome: ServiceOutcome::NotFound,
                },
                ServiceAnswer {
                    service: "arxiv".to_string(),
                    outcome: ServiceOutcome::Unavailable {
                        message: "timed out".to_string(),
                    },
                },
            ],
            earlier: vec![],
        },
        record_retrieval: None,
        match_check: MatchCheckStep::NotAttempted {
            reason: "no-record".to_string(),
        },
        acceptance: Acceptance::NotApplicable,
    }
}

/// Sections for a title conflict, the record found over the network.
fn conflict_sections() -> Sections {
    Sections {
        library: LibraryStep::NotAttempted {
            reason: "no-library".to_string(),
        },
        content_index: ContentIndexSection {
            read: IndexReadStep::Miss,
            write: WriteStep::NotAttempted {
                reason: "refused".to_string(),
            },
        },
        extraction: ExtractionSection {
            result: ExtractionResultStep::Found {
                identifier: "doi:10.1000/ref".to_string(),
                tier: "text-layer".to_string(),
            },
            titles: TitlesStep::Read {
                claims: vec![Claim {
                    from: ClaimOrigin::Xmp,
                    title: "Graphene on copper".to_string(),
                }],
            },
        },
        identifier_input: IdentifierInputStep::NotAttempted {
            reason: "not-asked".to_string(),
        },
        lookup: LookupStep::Attempted {
            identifier: "doi:10.1000/ref".to_string(),
            origin: IdentifierOrigin::Extracted,
            attempts: vec![ServiceAnswer {
                service: "crossref".to_string(),
                outcome: ServiceOutcome::Found {
                    retrieval: FetchedFrom::Network,
                    stored: Some(WriteStep::Written),
                },
            }],
            earlier: vec![],
        },
        record_retrieval: Some(RetrievedFrom::Network {
            service: "crossref".to_string(),
        }),
        match_check: MatchCheckStep::Conflict {
            field: "title".to_string(),
            extracted: "Graphene on copper".to_string(),
            resolved: "A survey of something else".to_string(),
            similarity: 0.08,
        },
        acceptance: Acceptance::NotApplicable,
    }
}

fn conflict_candidate() -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A survey of something else".to_string());
    record.doi = Some(Doi::parse("10.1000/ref").unwrap());
    record
}

#[test]
fn unciteable_reason_has_no_message_field() {
    let value: Value = serde_json::from_str(&json_line(&skipped(SkipReason::Unciteable))).unwrap();
    assert!(value["reason"].get("message").is_none());
}

// ---------------------------------------------------------------------
// round-trip through Event's own (de)serialization
// ---------------------------------------------------------------------
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
        assert_eq!(parsed, event, "line was {line}");
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

// ---------------------------------------------------------------------
// identifier_input: the section between `extraction` and `lookup`
// (design D1-D4, D8; task 2.1)
// ---------------------------------------------------------------------

/// A submission that was refused: it parsed to nothing, so every one of
/// its own outcome steps is not attempted for `unparsed`.
fn refused_submission(number: u32, raw: &str) -> Submission {
    Submission {
        submission: number,
        raw: raw.to_string(),
        syntax: SyntaxStep::Rejected {
            reason: "unrecognised".to_string(),
            expected: None,
        },
        outcome: Some(Box::new(SubmissionOutcome {
            lookup: LookupStep::NotAttempted {
                reason: "unparsed".to_string(),
            },
            record_retrieval: None,
            match_check: MatchCheckStep::NotAttempted {
                reason: "unparsed".to_string(),
            },
            acceptance: SubmissionAcceptance::NotAttempted {
                reason: "unparsed".to_string(),
            },
            record: None,
        })),
    }
}

/// A submission whose identifier no service held: not attempted for
/// `no-record`, with its lookup's attempts kept.
fn no_record_submission(number: u32, raw: &str, identifier: &str) -> Submission {
    Submission {
        submission: number,
        raw: raw.to_string(),
        syntax: SyntaxStep::Parsed {
            identifier: identifier.to_string(),
        },
        outcome: Some(Box::new(SubmissionOutcome {
            lookup: LookupStep::Attempted {
                identifier: identifier.to_string(),
                origin: IdentifierOrigin::Operator,
                attempts: vec![
                    ServiceAnswer {
                        service: "crossref".to_string(),
                        outcome: ServiceOutcome::NotFound,
                    },
                    ServiceAnswer {
                        service: "openalex".to_string(),
                        outcome: ServiceOutcome::NotFound,
                    },
                ],
                earlier: vec![],
            },
            record_retrieval: None,
            match_check: MatchCheckStep::NotAttempted {
                reason: "no-record".to_string(),
            },
            acceptance: SubmissionAcceptance::NotAttempted {
                reason: "no-record".to_string(),
            },
            record: None,
        })),
    }
}

/// A submission whose record was reached and then rejected: the
/// operator answered Skip, or a later submission took its place on
/// offer.
fn rejected_submission(number: u32, raw: &str, identifier: &str) -> Submission {
    Submission {
        submission: number,
        raw: raw.to_string(),
        syntax: SyntaxStep::Parsed {
            identifier: identifier.to_string(),
        },
        outcome: Some(Box::new(SubmissionOutcome {
            lookup: LookupStep::Attempted {
                identifier: identifier.to_string(),
                origin: IdentifierOrigin::Operator,
                attempts: vec![ServiceAnswer {
                    service: "crossref".to_string(),
                    outcome: ServiceOutcome::Found {
                        retrieval: FetchedFrom::Network,
                        stored: Some(WriteStep::Written),
                    },
                }],
                earlier: vec![],
            },
            record_retrieval: Some(RetrievedFrom::Network {
                service: "crossref".to_string(),
            }),
            match_check: MatchCheckStep::InsufficientEvidence {
                reason: "no-titles".to_string(),
            },
            acceptance: SubmissionAcceptance::Rejected,
            record: Some(Box::new(Record::new(EntryType::Article))),
        })),
    }
}

/// The used submission: bare, carrying only `submission`, `raw` and
/// `syntax` (design D2, D9).
fn used_submission(number: u32, raw: &str, identifier: &str) -> Submission {
    Submission {
        submission: number,
        raw: raw.to_string(),
        syntax: SyntaxStep::Parsed {
            identifier: identifier.to_string(),
        },
        outcome: None,
    }
}

#[test]
fn identifier_input_not_attempted_serializes_status_and_reason_only() {
    for reason in ["content-duplicate", "not-asked", "not-supplied"] {
        let step = IdentifierInputStep::NotAttempted {
            reason: reason.to_string(),
        };
        let value = serde_json::to_value(&step).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"status": "not-attempted", "reason": reason})
        );
    }
}

/// `serde_json::Value`'s map has no `preserve_order` here, so it sorts
/// keys alphabetically: an order assertion has to read the raw text
/// instead. True when each of `keys`, in turn, is found no earlier
/// than the one before it.
fn in_wire_order(text: &str, keys: &[&str]) -> bool {
    let mut last = 0;
    for key in keys {
        let needle = format!("\"{key}\":");
        match text.find(&needle) {
            Some(pos) if pos >= last => last = pos,
            _ => return false,
        }
    }
    true
}

#[test]
fn identifier_input_supplied_serializes_status_submissions_used_displaced_in_order() {
    let step = IdentifierInputStep::Supplied {
        submissions: vec![used_submission(
            1,
            "10.1039/c9cc02492a",
            "doi:10.1039/c9cc02492a",
        )],
        used: Some(1),
        displaced: Some(Box::new(Displaced {
            lookup: LookupStep::NotAttempted {
                reason: "extraction-failed".to_string(),
            },
            record_retrieval: None,
            match_check: MatchCheckStep::NotAttempted {
                reason: "extraction-failed".to_string(),
            },
            record: None,
        })),
    };
    let text = serde_json::to_string(&step).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 4, "got {text}");
    assert!(
        in_wire_order(&text, &["status", "submissions", "used", "displaced"]),
        "got {text}"
    );
    assert_eq!(value["used"], Value::from(1));
}

#[test]
fn identifier_input_supplied_serializes_null_used_and_displaced_when_none() {
    let step = IdentifierInputStep::Supplied {
        submissions: vec![refused_submission(1, "not-an-identifier")],
        used: None,
        displaced: None,
    };
    let value = serde_json::to_value(&step).unwrap();
    assert_eq!(value["used"], Value::Null);
    assert_eq!(value["displaced"], Value::Null);
}

#[test]
fn a_submission_with_an_outcome_serializes_its_flattened_keys_and_the_record() {
    let submission = rejected_submission(1, "10.1039/c9cc02492a", "doi:10.1039/c9cc02492a");
    let text = serde_json::to_string(&submission).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 8, "got {text}");
    assert!(
        in_wire_order(
            &text,
            &[
                "submission",
                "raw",
                "syntax",
                "lookup",
                "record_retrieval",
                "match_check",
                "acceptance",
                "record",
            ]
        ),
        "got {text}"
    );
}

#[test]
fn the_used_submission_serializes_submission_raw_and_syntax_only() {
    let submission = used_submission(1, "10.1039/c9cc02492a", "doi:10.1039/c9cc02492a");
    let text = serde_json::to_string(&submission).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3, "got {text}");
    assert!(
        in_wire_order(&text, &["submission", "raw", "syntax"]),
        "got {text}"
    );
}

#[test]
fn syntax_parsed_serializes_status_and_identifier() {
    let step = SyntaxStep::Parsed {
        identifier: "doi:10.1039/c9cc02492".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&step).unwrap(),
        serde_json::json!({"status": "parsed", "identifier": "doi:10.1039/c9cc02492"})
    );
}

#[test]
fn syntax_rejected_omits_expected_when_none() {
    let step = SyntaxStep::Rejected {
        reason: "unrecognised".to_string(),
        expected: None,
    };
    let value = serde_json::to_value(&step).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"status": "rejected", "reason": "unrecognised"})
    );
}

#[test]
fn syntax_rejected_carries_expected_when_some() {
    let step = SyntaxStep::Rejected {
        reason: "invalid".to_string(),
        expected: Some("doi".to_string()),
    };
    let value = serde_json::to_value(&step).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"status": "rejected", "reason": "invalid", "expected": "doi"})
    );
}

#[test]
fn acceptance_pending_and_accepted_serialize_by_status_alone() {
    assert_eq!(
        serde_json::to_value(Acceptance::Pending).unwrap(),
        serde_json::json!({"status": "pending"})
    );
    assert_eq!(
        serde_json::to_value(Acceptance::Accepted).unwrap(),
        serde_json::json!({"status": "accepted"})
    );
}

#[test]
fn submission_acceptance_rejected_serializes_by_status_alone() {
    assert_eq!(
        serde_json::to_value(SubmissionAcceptance::Rejected).unwrap(),
        serde_json::json!({"status": "rejected"})
    );
}

/// design D1: `identifier_input` sits between `extraction` and
/// `lookup` on every `resolved` event and every resolution `skipped`
/// event.
#[test]
fn identifier_input_sits_between_extraction_and_lookup() {
    for event in all_resolution_skips()
        .into_iter()
        .chain(std::iter::once(resolved()))
    {
        let line = json_line(&event);
        let extraction_at = line.find("\"extraction\"").unwrap();
        let input_at = line.find("\"identifier_input\"").unwrap();
        let lookup_at = line.find("\"lookup\"").unwrap();
        assert!(
            extraction_at < input_at && input_at < lookup_at,
            "got {line}"
        );
    }
}

/// The round-two session's final skip (design.md's first illustrative
/// line): a refused text, a truncated DOI no service holds, and the
/// published DOI, rejected after a final Skip.
fn round_two_final_skip_event() -> Event {
    let mut sections = text_without_identifier_sections();
    sections.identifier_input = IdentifierInputStep::Supplied {
        submissions: vec![
            refused_submission(1, "not-an-identifier"),
            no_record_submission(2, "10.1039/c9cc02492", "doi:10.1039/c9cc02492"),
            rejected_submission(3, "10.1039/c9cc02492a", "doi:10.1039/c9cc02492a"),
        ],
        used: None,
        displaced: None,
    };
    Event::Skipped {
        path: PathBuf::from("paper.pdf"),
        reason: SkipReason::TextWithoutIdentifier,
        sections: Some(Box::new(sections)),
        candidate: None,
    }
}

#[test]
fn the_round_two_final_skip_carries_every_submission_in_order() {
    let event = round_two_final_skip_event();
    match &event {
        Event::Skipped {
            sections: Some(sections),
            ..
        } => match &sections.identifier_input {
            IdentifierInputStep::Supplied {
                submissions,
                used,
                displaced,
            } => {
                assert_eq!(submissions.len(), 3);
                assert_eq!(submissions[0].submission, 1);
                assert_eq!(submissions[1].submission, 2);
                assert_eq!(submissions[2].submission, 3);
                assert_eq!(*used, None);
                assert!(displaced.is_none());
            }
            other => panic!("expected Supplied, got {other:?}"),
        },
        other => panic!("expected a resolution skip, got {other:?}"),
    }
}

#[test]
fn the_round_two_final_skip_human_line_names_the_rejected_candidate() {
    let event = round_two_final_skip_event();
    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: skipped, no identifier found in its metadata or the pages read; \
candidate rejected: doi:10.1039/c9cc02492a"
    );
}

/// The event the third question's description rendered, before the
/// Skip: submission 3 is the used one, and the event's own `lookup`,
/// `match_check` and `acceptance` are its outcome, stated once.
fn pending_candidate_event() -> Event {
    let mut sections = conflict_sections();
    sections.extraction = ExtractionSection {
        result: ExtractionResultStep::TextWithoutIdentifier,
        titles: TitlesStep::Read { claims: Vec::new() },
    };
    sections.identifier_input = IdentifierInputStep::Supplied {
        submissions: vec![
            refused_submission(1, "not-an-identifier"),
            no_record_submission(2, "10.1039/c9cc02492", "doi:10.1039/c9cc02492"),
            used_submission(3, "10.1039/c9cc02492a", "doi:10.1039/c9cc02492a"),
        ],
        used: Some(3),
        displaced: Some(Box::new(Displaced {
            lookup: LookupStep::NotAttempted {
                reason: "extraction-failed".to_string(),
            },
            record_retrieval: None,
            match_check: MatchCheckStep::NotAttempted {
                reason: "extraction-failed".to_string(),
            },
            record: None,
        })),
    };
    sections.lookup = LookupStep::Attempted {
        identifier: "doi:10.1039/c9cc02492a".to_string(),
        origin: IdentifierOrigin::Operator,
        attempts: vec![ServiceAnswer {
            service: "crossref".to_string(),
            outcome: ServiceOutcome::Found {
                retrieval: FetchedFrom::Network,
                stored: Some(WriteStep::Written),
            },
        }],
        earlier: vec![],
    };
    sections.record_retrieval = Some(RetrievedFrom::Network {
        service: "crossref".to_string(),
    });
    sections.match_check = MatchCheckStep::InsufficientEvidence {
        reason: "no-titles".to_string(),
    };
    sections.acceptance = Acceptance::Pending;
    Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1039/c9cc02492a".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    }
}

#[test]
fn the_pending_candidate_event_names_the_used_submission_and_its_displaced_facts() {
    let event = pending_candidate_event();
    match &event {
        Event::Resolved { sections, .. } => {
            assert_eq!(sections.acceptance, Acceptance::Pending);
            match &sections.identifier_input {
                IdentifierInputStep::Supplied {
                    used, displaced, ..
                } => {
                    assert_eq!(*used, Some(3));
                    assert!(displaced.is_some());
                }
                other => panic!("expected Supplied, got {other:?}"),
            }
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// invariant 1 (design D2): `used` is a number exactly when the
/// event's `lookup.origin` is `operator`, and that submission's
/// `syntax.identifier` equals `lookup.identifier`.
#[test]
fn used_submission_identifier_matches_the_events_own_lookup_identifier() {
    let event = pending_candidate_event();
    let Event::Resolved { sections, .. } = &event else {
        panic!("expected Event::Resolved");
    };
    let LookupStep::Attempted {
        identifier: looked_up,
        origin,
        ..
    } = &sections.lookup
    else {
        panic!("expected an attempted lookup");
    };
    assert_eq!(*origin, IdentifierOrigin::Operator);
    let IdentifierInputStep::Supplied {
        submissions, used, ..
    } = &sections.identifier_input
    else {
        panic!("expected Supplied");
    };
    let used_entry = submissions
        .iter()
        .find(|s| Some(s.submission) == *used)
        .unwrap();
    let SyntaxStep::Parsed { identifier } = &used_entry.syntax else {
        panic!("expected Parsed syntax");
    };
    assert_eq!(identifier, looked_up);
}

/// A `resolved` event whose `identifier_input` is `supplied`, with a
/// used entry, a rejected entry carrying a record and a conflict
/// `match_check`, and `displaced` holding a record, round-trips through
/// `json_line` and back to an equal `Event`.
#[test]
fn a_supplied_resolved_event_with_displaced_record_round_trips() {
    let mut sections = conflict_sections();
    sections.identifier_input = IdentifierInputStep::Supplied {
        submissions: vec![
            rejected_submission(1, "10.1000/old", "doi:10.1000/old"),
            used_submission(2, "10.1000/ref", "doi:10.1000/ref"),
        ],
        used: Some(2),
        displaced: Some(Box::new(Displaced {
            lookup: LookupStep::Attempted {
                identifier: "doi:10.1000/extracted".to_string(),
                origin: IdentifierOrigin::Extracted,
                attempts: vec![ServiceAnswer {
                    service: "crossref".to_string(),
                    outcome: ServiceOutcome::NotFound,
                }],
                earlier: vec![],
            },
            record_retrieval: Some(RetrievedFrom::Network {
                service: "crossref".to_string(),
            }),
            match_check: MatchCheckStep::Agreed,
            record: Some(Box::new(Record::new(EntryType::Article))),
        })),
    };
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/ref".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };
    let line = json_line(&event);
    let parsed: Event = serde_json::from_str(&line).unwrap();
    assert_eq!(parsed, event, "line was {line}");
}

/// design.md's first illustrative line: the round-two final skip's
/// `identifier_input` matches the published JSON exactly.
#[test]
fn the_round_two_final_skip_identifier_input_matches_the_illustrative_json() {
    let event = round_two_final_skip_event();
    let Event::Skipped {
        sections: Some(sections),
        ..
    } = &event
    else {
        panic!("expected a resolution skip");
    };
    let value = serde_json::to_value(&sections.identifier_input).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "status": "supplied",
            "submissions": [
                {
                    "submission": 1,
                    "raw": "not-an-identifier",
                    "syntax": {"status": "rejected", "reason": "unrecognised"},
                    "lookup": {"status": "not-attempted", "reason": "unparsed"},
                    "record_retrieval": null,
                    "match_check": {"status": "not-attempted", "reason": "unparsed"},
                    "acceptance": {"status": "not-attempted", "reason": "unparsed"},
                },
                {
                    "submission": 2,
                    "raw": "10.1039/c9cc02492",
                    "syntax": {"status": "parsed", "identifier": "doi:10.1039/c9cc02492"},
                    "lookup": {
                        "status": "attempted",
                        "identifier": "doi:10.1039/c9cc02492",
                        "origin": "operator",
                        "attempts": [
                            {"service": "crossref", "outcome": {"status": "not-found"}},
                            {"service": "openalex", "outcome": {"status": "not-found"}},
                        ],
                    },
                    "record_retrieval": null,
                    "match_check": {"status": "not-attempted", "reason": "no-record"},
                    "acceptance": {"status": "not-attempted", "reason": "no-record"},
                },
                {
                    "submission": 3,
                    "raw": "10.1039/c9cc02492a",
                    "syntax": {"status": "parsed", "identifier": "doi:10.1039/c9cc02492a"},
                    "lookup": {
                        "status": "attempted",
                        "identifier": "doi:10.1039/c9cc02492a",
                        "origin": "operator",
                        "attempts": [
                            {"service": "crossref", "outcome": {
                                "status": "found", "retrieval": "network",
                                "stored": {"status": "written"}
                            }},
                        ],
                    },
                    "record_retrieval": {"kind": "network", "service": "crossref"},
                    "match_check": {"status": "insufficient-evidence", "reason": "no-titles"},
                    "acceptance": {"status": "rejected"},
                    "record": serde_json::to_value(Record::new(EntryType::Article)).unwrap(),
                },
            ],
            "used": null,
            "displaced": null,
        })
    );
}

/// A `skipped` event whose `identifier_input` is `supplied` with
/// `used: null` round-trips.
#[test]
fn a_supplied_skipped_event_with_used_null_round_trips() {
    let event = round_two_final_skip_event();
    let line = json_line(&event);
    let parsed: Event = serde_json::from_str(&line).unwrap();
    assert_eq!(parsed, event, "line was {line}");
}

// ---------------------------------------------------------------------
// lookup.earlier: every round of the file's own lookup (design D12)
// ---------------------------------------------------------------------

#[test]
fn attempted_lookup_serializes_earlier_after_attempts_oldest_first() {
    let step = LookupStep::Attempted {
        identifier: "doi:10.1000/xyz".to_string(),
        origin: IdentifierOrigin::Extracted,
        attempts: vec![ServiceAnswer {
            service: "crossref".to_string(),
            outcome: ServiceOutcome::Found {
                retrieval: FetchedFrom::Network,
                stored: Some(WriteStep::Written),
            },
        }],
        earlier: vec![
            LookupRound::Attempted {
                attempts: vec![ServiceAnswer {
                    service: "crossref".to_string(),
                    outcome: ServiceOutcome::Unavailable {
                        message: "HTTP 503".to_string(),
                    },
                }],
            },
            LookupRound::NoEligibleService,
        ],
    };
    let value = serde_json::to_value(&step).unwrap();
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let attempts_pos = keys.iter().position(|k| *k == "attempts").unwrap();
    let earlier_pos = keys.iter().position(|k| *k == "earlier").unwrap();
    assert!(attempts_pos < earlier_pos, "got {keys:?}");
    assert_eq!(
        value["earlier"],
        serde_json::json!([
            {"status": "attempted", "attempts": [
                {"service": "crossref", "outcome": {"status": "unavailable", "message": "HTTP 503"}}
            ]},
            {"status": "no-eligible-service"},
        ])
    );
}

#[test]
fn no_eligible_service_lookup_serializes_earlier_after_origin() {
    let step = LookupStep::NoEligibleService {
        identifier: "pmid:12345678".to_string(),
        origin: IdentifierOrigin::Extracted,
        earlier: vec![LookupRound::NoEligibleService],
    };
    let text = serde_json::to_string(&step).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert!(in_wire_order(&text, &["origin", "earlier"]), "got {text}");
    assert_eq!(
        value["earlier"],
        serde_json::json!([{"status": "no-eligible-service"}])
    );
}

#[test]
fn empty_earlier_is_omitted_and_absent_earlier_deserializes_to_empty() {
    let step = LookupStep::Attempted {
        identifier: "doi:10.1000/xyz".to_string(),
        origin: IdentifierOrigin::Extracted,
        attempts: vec![ServiceAnswer {
            service: "crossref".to_string(),
            outcome: ServiceOutcome::NotFound,
        }],
        earlier: vec![],
    };
    let text = serde_json::to_string(&step).unwrap();
    assert!(!text.contains("earlier"), "got {text}");
    let parsed: LookupStep = serde_json::from_str(&text).unwrap();
    let LookupStep::Attempted { earlier, .. } = parsed else {
        panic!("expected Attempted");
    };
    assert_eq!(earlier, Vec::<LookupRound>::new());
}

#[test]
fn lookup_earlier_round_trips_for_attempted_and_no_eligible_service() {
    let attempted = LookupStep::Attempted {
        identifier: "doi:10.1000/xyz".to_string(),
        origin: IdentifierOrigin::Extracted,
        attempts: vec![ServiceAnswer {
            service: "crossref".to_string(),
            outcome: ServiceOutcome::NotFound,
        }],
        earlier: vec![LookupRound::Attempted {
            attempts: vec![ServiceAnswer {
                service: "crossref".to_string(),
                outcome: ServiceOutcome::Unavailable {
                    message: "HTTP 503".to_string(),
                },
            }],
        }],
    };
    let no_eligible = LookupStep::NoEligibleService {
        identifier: "pmid:1".to_string(),
        origin: IdentifierOrigin::Extracted,
        earlier: vec![LookupRound::NoEligibleService],
    };
    for step in [attempted, no_eligible] {
        let text = serde_json::to_string(&step).unwrap();
        let parsed: LookupStep = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, step, "line was {text}");
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
    for event in all_resolution_skips().into_iter().chain(all_plain_skips()) {
        let line = human_line(&event).unwrap();
        assert!(!line.contains('\n'));
        assert!(
            line.contains("mystery.pdf"),
            "event {event:?} produced line without the path: {line}"
        );
    }
}

/// design D9's resolution skip clauses, one per extraction failure kind
/// plus unresolvable and conflict.
#[test]
fn human_line_of_resolution_skips_matches_design_d9() {
    let cases: Vec<(Event, &str)> = vec![
        (
            skipped_verdict(SkipReason::NoTextLayer, no_text_layer_sections()),
            "mystery.pdf: skipped, no identifier found; the pages read hold no text",
        ),
        (
            skipped_verdict(
                SkipReason::TextWithoutIdentifier,
                text_without_identifier_sections(),
            ),
            "mystery.pdf: skipped, no identifier found in its metadata or the pages read",
        ),
        (
            skipped_verdict(SkipReason::Encrypted, encrypted_sections()),
            "mystery.pdf: skipped, encrypted, so no identifier could be read",
        ),
        (
            skipped_verdict(
                SkipReason::Unreadable {
                    message: "not a PDF".to_string(),
                },
                unreadable_extraction_sections("not a PDF"),
            ),
            "mystery.pdf: skipped, unreadable (not a PDF)",
        ),
        (
            skipped_verdict(SkipReason::Unresolvable, unresolvable_sections()),
            "mystery.pdf: skipped, no source had a record for doi:10.1000/xyz123 \
             (crossref: not found; arxiv: unavailable: timed out)",
        ),
        (
            skipped_conflict(conflict_sections(), conflict_candidate()),
            "mystery.pdf: skipped, title disagrees 8% (file says Graphene on copper, \
             record says A survey of something else)",
        ),
    ];

    for (event, expected) in cases {
        assert_eq!(human_line(&event).unwrap(), expected);
    }
}

/// design D9: `unresolvable` with `lookup` `no-eligible-service` names
/// no services at all.
#[test]
fn human_line_of_unresolvable_with_no_eligible_service() {
    let mut sections = unresolvable_sections();
    sections.lookup = LookupStep::NoEligibleService {
        identifier: "arXiv:2401.12345".to_string(),
        origin: IdentifierOrigin::Extracted,
        earlier: vec![],
    };
    let event = skipped_verdict(SkipReason::Unresolvable, sections);

    assert_eq!(
        human_line(&event).unwrap(),
        "mystery.pdf: skipped, no configured service could be asked about arXiv:2401.12345"
    );
}

// ---------------------------------------------------------------------
// human_line: the rejected-candidate clause (design D6, task 6.1)
// ---------------------------------------------------------------------

/// A submission rejected with its record, for the human-line and
/// description tests: `number` numbers it, `identifier` is its parsed
/// text.
fn rejected_candidate_submission(number: u32, identifier: &str) -> Submission {
    Submission {
        submission: number,
        raw: identifier.to_string(),
        syntax: SyntaxStep::Parsed {
            identifier: identifier.to_string(),
        },
        outcome: Some(Box::new(SubmissionOutcome {
            lookup: LookupStep::Attempted {
                identifier: identifier.to_string(),
                origin: IdentifierOrigin::Operator,
                attempts: vec![ServiceAnswer {
                    service: "crossref".to_string(),
                    outcome: ServiceOutcome::Found {
                        retrieval: FetchedFrom::Network,
                        stored: Some(WriteStep::Written),
                    },
                }],
                earlier: vec![],
            },
            record_retrieval: Some(RetrievedFrom::Network {
                service: "crossref".to_string(),
            }),
            match_check: MatchCheckStep::Agreed,
            acceptance: SubmissionAcceptance::Rejected,
            record: Some(Box::new(Record::new(EntryType::Article))),
        })),
    }
}

/// design D6: a `resolved` line whose file had two candidates rejected
/// ends `; candidates rejected: <a>, <b>`, in submission order.
#[test]
fn human_line_of_resolved_with_two_rejected_candidates() {
    let mut sections = sections_via_network("crossref", "doi:10.1000/xyz123");
    sections.identifier_input = IdentifierInputStep::Supplied {
        submissions: vec![
            rejected_candidate_submission(1, "doi:10.1000/a"),
            rejected_candidate_submission(2, "doi:10.1000/b"),
        ],
        used: None,
        displaced: None,
    };
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };
    assert!(
        human_line(&event)
            .unwrap()
            .ends_with("; candidates rejected: doi:10.1000/a, doi:10.1000/b"),
        "got {:?}",
        human_line(&event)
    );
}

/// design D6: a `resolved` line with one rejected candidate carries the
/// clause after the library clause, when there is one.
#[test]
fn human_line_of_resolved_with_one_rejected_candidate_follows_the_library_clause() {
    let mut sections = sections_via_library("0192a1b2", "item-1");
    sections.identifier_input = IdentifierInputStep::Supplied {
        submissions: vec![rejected_candidate_submission(1, "doi:10.1000/a")],
        used: None,
        displaced: None,
    };
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };
    let line = human_line(&event).unwrap();
    assert!(
        line.ends_with("; candidate rejected: doi:10.1000/a"),
        "got {line:?}"
    );
}

/// design D6: a line whose submissions hold none `rejected` has no
/// clause — `unparsed`, `no-record`, `no-move`, a used submission, and
/// not attempted.
#[test]
fn human_line_has_no_clause_when_nothing_was_rejected() {
    let unparsed = refused_submission(1, "garbage");
    let no_record = no_record_submission(1, "10.1000/x", "doi:10.1000/x");
    let used = used_submission(1, "10.1000/x", "doi:10.1000/x");

    for submissions in [vec![unparsed], vec![no_record], vec![used]] {
        let mut sections = sections_via_network("crossref", "doi:10.1000/xyz123");
        sections.identifier_input = IdentifierInputStep::Supplied {
            submissions,
            used: None,
            displaced: None,
        };
        let event = Event::Resolved {
            path: PathBuf::from("paper.pdf"),
            identifier: "doi:10.1000/xyz123".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            sections: Box::new(sections),
        };
        assert!(
            !human_line(&event).unwrap().contains("rejected"),
            "got {:?}",
            human_line(&event)
        );
    }

    let not_attempted = sections_via_network("crossref", "doi:10.1000/xyz123");
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(not_attempted),
    };
    assert!(!human_line(&event).unwrap().contains("rejected"));
}

/// design D6: a rejected identifier carrying a control character is
/// written escaped, as the rest of the line is.
#[test]
fn human_line_escapes_a_control_character_in_a_rejected_identifier() {
    let mut sections = sections_via_network("crossref", "doi:10.1000/xyz123");
    sections.identifier_input = IdentifierInputStep::Supplied {
        submissions: vec![rejected_candidate_submission(1, "doi:10.1000/a\u{7}b")],
        used: None,
        displaced: None,
    };
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };
    let line = human_line(&event).unwrap();
    assert!(!line.contains('\u{7}'), "got {line:?}");
}

/// design D12: an `unresolvable` skip whose `lookup` has an `earlier`
/// round of `unavailable` answers gives exactly the line it gives
/// without `earlier` — the human line states the current round only.
/// Expected to pass at once; it pins behaviour.
#[test]
fn human_line_of_unresolvable_shows_the_current_round_only() {
    let plain = skipped_verdict(SkipReason::Unresolvable, unresolvable_sections());
    let mut with_earlier_sections = unresolvable_sections();
    let LookupStep::Attempted { earlier, .. } = &mut with_earlier_sections.lookup else {
        panic!("expected an attempted lookup");
    };
    *earlier = vec![LookupRound::Attempted {
        attempts: vec![ServiceAnswer {
            service: "crossref".to_string(),
            outcome: ServiceOutcome::Unavailable {
                message: "HTTP 503".to_string(),
            },
        }],
    }];
    let with_earlier = skipped_verdict(SkipReason::Unresolvable, with_earlier_sections);

    assert_eq!(
        human_line(&with_earlier).unwrap(),
        human_line(&plain).unwrap()
    );
}

#[test]
fn human_line_of_skipped_makes_the_reason_legible_for_every_non_resolution_variant() {
    let cases: Vec<(SkipReason, &str)> = vec![
        (
            SkipReason::TargetTaken {
                target: PathBuf::from("smith2024_borax.pdf"),
            },
            "smith2024_borax.pdf",
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

/// design D8: `Counts::observe` ignores `content-index-write` entirely.
#[test]
fn counts_observe_ignores_content_index_write() {
    let mut counts = Counts::default();
    counts.observe(&content_index_write(WriteStep::Written));
    counts.observe(&content_index_write(WriteStep::Failed {
        message: "disk full".to_string(),
    }));
    assert_eq!(counts, Counts::default());
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
            identifier: "doi:10.1000/aaa".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            sections: Box::new(sections_via_network("crossref", "doi:10.1000/aaa")),
        },
        Event::Resolved {
            path: PathBuf::from("b.pdf"),
            identifier: "doi:10.1000/bbb".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            sections: Box::new(sections_via_content_index()),
        },
        Event::Renamed {
            path: PathBuf::from("a.pdf"),
            target: PathBuf::from("smith2024_a.pdf"),
            hash: hash_of("a.pdf"),
        },
        content_index_write(WriteStep::Written),
        skipped(SkipReason::Declined),
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

    counts.observe(&skipped(SkipReason::RenameFailed {
        message: "permission denied".to_string(),
    }));

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
    counts.observe(&skipped(SkipReason::RenameFailed {
        message: "permission denied".to_string(),
    }));

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
// resolved's identifier, record and sections (design D2a, D3, D6)
// ---------------------------------------------------------------------

/// design D3: `extraction.titles` serializes each claim with its
/// origin, in the order they were read.
#[test]
fn resolved_serializes_claims_as_design_d3_shows() {
    let mut sections = sections_via_network("crossref", "doi:10.1000/xyz123");
    sections.extraction.titles = TitlesStep::Read {
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
    };
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };

    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();

    assert_eq!(
        value["extraction"]["titles"]["claims"],
        serde_json::json!([
            {"from": "xmp", "title": "Applications of chiral sulfinyl compounds"},
            {"from": "info", "title": "Microsoft Word - manuscript.docx"}
        ]),
        "got {value:#}"
    );
}

/// design D14's mapping for `found == X` after a lookup: the top-level
/// `identifier` is the record's own identifier, and `lookup.identifier`
/// is what was looked up — they differ for an arXiv identifier whose
/// record also carries a DOI.
#[test]
fn an_arxiv_found_identifier_survives_a_doi_carrying_record() {
    let mut record = Record::new(EntryType::Preprint);
    record.doi = Some(Doi::parse("10.1000/from-the-record").unwrap());
    record.borax.arxiv = Some(ArxivId::parse("2401.01234").unwrap());
    let path = PathBuf::from("paper.pdf");
    let file = FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_lookup(
            SourceName::Arxiv,
            Identifier::Arxiv(ArxivId::parse("2401.01234").unwrap()),
            Tier::TextLayer,
        ),
        overridden: false,
        accepted: false,
    };

    let event = resolved_event(&path, &file);

    match event {
        Event::Resolved {
            identifier,
            sections,
            ..
        } => {
            let LookupStep::Attempted {
                identifier: looked_up,
                ..
            } = sections.lookup
            else {
                panic!("expected an attempted lookup");
            };
            assert_eq!(
                looked_up, "arXiv:2401.01234",
                "lookup.identifier must be what was looked up"
            );
            assert_eq!(
                identifier, "doi:10.1000/from-the-record",
                "the record's own identifier is unaffected"
            );
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D14: `cached == true` becomes `record_retrieval.kind ==
/// "content-index"` with `content_index.read.status == "hit"`; the
/// human line's `via` names the service `record.borax.provenance` names
/// (D6), here Crossref.
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
    let file = FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_content_index_hit(),
        overridden: false,
        accepted: false,
    };

    let event = resolved_event(&path, &file);

    match &event {
        Event::Resolved { sections, .. } => {
            assert_eq!(sections.record_retrieval, Some(RetrievedFrom::ContentIndex));
            assert_eq!(sections.content_index.read, IndexReadStep::Hit);
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
    assert!(
        human_line(&event).unwrap().contains("via crossref"),
        "got {:?}",
        human_line(&event)
    );
}

/// design D6/D1: a record whose provenance names two services is
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
    let file = FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_content_index_hit(),
        overridden: false,
        accepted: false,
    };

    let event = resolved_event(&path, &file);

    assert!(
        human_line(&event)
            .unwrap()
            .contains("via crossref, openalex"),
        "got {:?}",
        human_line(&event)
    );
}

/// design D6: a record whose provenance names no service at all — only
/// extraction, or nothing — names the content index as `from`, with no
/// `via` clause at all (the schema-3 `"cache"` stand-in is gone).
#[test]
fn a_record_whose_provenance_names_no_service_names_no_via() {
    let mut record = Record::new(EntryType::Article);
    record.borax = BoraxExt {
        provenance: [("title".to_string(), Source::Extraction)]
            .into_iter()
            .collect(),
        ..BoraxExt::default()
    };
    let path = PathBuf::from("paper.pdf");
    let file = FileRecord {
        record,
        hash: Some(hash_bytes(b"paper")),
        evidence: evidence_via_content_index_hit(),
        overridden: false,
        accepted: false,
    };

    let event = resolved_event(&path, &file);
    let line = human_line(&event).unwrap();

    assert!(!line.contains(" via "), "got {line:?}");
    assert!(line.contains("from the content index"), "got {line:?}");
}

// ---------------------------------------------------------------------
// the `library` section (design D3, D12)
// ---------------------------------------------------------------------

/// `resolved()` with its `library` section set to `answer`, retrieved
/// through the library item.
fn resolved_with_library(answer: LibraryAnswer) -> Event {
    let (artifact, item) = match &answer {
        LibraryAnswer::Tracked { artifact, item } => (artifact.clone(), item.clone()),
        _ => (
            "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string(),
            "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70".to_string(),
        ),
    };
    let mut sections = match &answer {
        LibraryAnswer::Tracked { .. } => sections_via_library(&artifact, &item),
        _ => sections_via_network("crossref", "doi:10.1000/xyz123"),
    };
    sections.library = LibraryStep::Consulted { answer };
    Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    }
}

/// `skipped_verdict` with its `library` section set to `answer`,
/// otherwise failing at extraction (so `library` is the only section a
/// library-problem human-line test cares about).
fn skipped_with_library(answer: LibraryAnswer) -> Event {
    let mut sections = no_text_layer_sections();
    sections.library = LibraryStep::Consulted { answer };
    skipped_verdict(SkipReason::NoTextLayer, sections)
}

/// design D3: every `LibraryAnswer` variant round-trips through the
/// JSON line, tagged by `kind` in kebab-case, nested under
/// `library.answer` on a `resolved` event (D14's mapping: `library ==
/// {kind: ..}` becomes `library == {status: consulted, answer: {kind:
/// ..}}`).
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
        let event = resolved_with_library(answer.clone());
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
        assert_eq!(value["schema"], Value::from(4));
        assert_eq!(value["library"]["status"], Value::from("consulted"));
        assert_eq!(value["library"]["answer"], expected, "for {answer:?}");
    }
}

/// The same shapes on a `skipped` event.
#[test]
fn json_line_of_skipped_carries_every_library_answer_variant_tagged_by_kind() {
    let event = skipped_with_library(LibraryAnswer::Untracked);
    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(value["schema"], Value::from(4));
    assert_eq!(
        value["library"]["answer"],
        serde_json::json!({"kind": "untracked"})
    );

    let event = skipped_with_library(tracked_answer());
    let value: Value = serde_json::from_str(&json_line(&event)).unwrap();
    assert_eq!(
        value["library"]["answer"],
        serde_json::json!({
            "kind": "tracked",
            "artifact": "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f",
            "item": "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70"
        })
    );
}

/// design D14's mapping `library == null` becomes `library == {status:
/// not-attempted, reason: no-library / outside-library}` on a
/// `resolved` event, and the `library` key is absent on a non-verdict
/// skip.
#[test]
fn library_not_attempted_replaces_the_schema_3_null() {
    let value: Value = serde_json::from_str(&json_line(&resolved())).unwrap();
    assert_eq!(
        value["library"],
        serde_json::json!({"status": "not-attempted", "reason": "no-library"})
    );
    assert_eq!(value["schema"], Value::from(4));

    let value: Value = serde_json::from_str(&json_line(&skipped(SkipReason::Declined))).unwrap();
    assert!(value.get("library").is_none());
    assert_eq!(value["schema"], Value::from(4));
}

// --- human_line: design D9's exact strings ---

/// design D9: a record retrieved from the library's own item is
/// reported `from the library`.
#[test]
fn human_line_of_a_library_answer_names_the_library_as_where() {
    let event = resolved_with_library(tracked_answer());

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved doi:10.1000/xyz123, from the library"
    );
}

/// design D9: a resolution whose `library` section could not answer
/// appends `; the library could not answer: <what>` after the `from`
/// clause.
#[test]
fn human_line_appends_the_library_problem_clause_after_the_from_clause() {
    let event = skipped_with_library(LibraryAnswer::DanglingItem {
        artifact: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string(),
        item: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70".to_string(),
    });

    assert_eq!(
        human_line(&event).unwrap(),
        "mystery.pdf: skipped, no identifier found; the pages read hold no text; the \
         library could not answer: artifact 0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f links \
         to item 0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70, which the library does not hold"
    );
}

/// The library-problem clause follows the `from` clause on a `resolved`
/// line too, here a content-index answer whose record names no service.
#[test]
fn human_line_appends_the_library_problem_clause_to_a_content_index_resolved_line() {
    let mut sections = sections_via_content_index();
    sections.library = LibraryStep::Consulted {
        answer: LibraryAnswer::DanglingItem {
            artifact: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string(),
            item: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70".to_string(),
        },
    };
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved doi:10.1000/xyz123, from the content index; the library \
         could not answer: artifact 0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f links to item \
         0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70, which the library does not hold"
    );
}

#[test]
fn human_line_names_every_artifact_of_an_ambiguous_library_problem() {
    let event = skipped_with_library(LibraryAnswer::Ambiguous {
        artifacts: vec!["a-id".to_string(), "b-id".to_string()],
    });

    assert_eq!(
        human_line(&event).unwrap(),
        "mystery.pdf: skipped, no identifier found; the pages read hold no text; the \
         library could not answer: 2 artifact records claim this file: a-id, b-id"
    );
}

#[test]
fn human_line_names_the_unreadable_records_wording_for_each_shape() {
    let one = skipped_with_library(LibraryAnswer::UnreadableRecords {
        listed: true,
        unreadable: 1,
    });
    assert_eq!(
        human_line(&one).unwrap(),
        "mystery.pdf: skipped, no identifier found; the pages read hold no text; the \
         library could not answer: 1 artifact record file could not be read, so the \
         library cannot say whether it tracks this file"
    );

    let several = skipped_with_library(LibraryAnswer::UnreadableRecords {
        listed: true,
        unreadable: 3,
    });
    assert_eq!(
        human_line(&several).unwrap(),
        "mystery.pdf: skipped, no identifier found; the pages read hold no text; the \
         library could not answer: 3 artifact record files could not be read, so the \
         library cannot say whether it tracks this file"
    );

    let unlistable = skipped_with_library(LibraryAnswer::UnreadableRecords {
        listed: false,
        unreadable: 0,
    });
    assert_eq!(
        human_line(&unlistable).unwrap(),
        "mystery.pdf: skipped, no identifier found; the pages read hold no text; the \
         library could not answer: the library's artifact records could not be listed, \
         so it cannot say whether it tracks this file"
    );
}

/// A tracked file the operator re-identified is reported from the
/// service that answered, and its library answer adds no clause.
#[test]
fn human_line_of_a_supplied_tracked_file_names_the_service() {
    let mut sections = sections_via_network("crossref", "doi:10.1000/xyz123");
    sections.library = LibraryStep::Consulted {
        answer: tracked_answer(),
    };
    if let LookupStep::Attempted { origin, .. } = &mut sections.lookup {
        *origin = IdentifierOrigin::Operator;
    }
    let event = Event::Resolved {
        path: PathBuf::from("paper.pdf"),
        identifier: "doi:10.1000/xyz123".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved doi:10.1000/xyz123 via crossref, from the network"
    );
}

/// `Untracked` adds nothing: a library answer with no problem leaves
/// the line as the `from` clause alone states it.
#[test]
fn human_line_of_an_untracked_file_adds_nothing() {
    let event = resolved_with_library(LibraryAnswer::Untracked);

    assert_eq!(
        human_line(&event).unwrap(),
        "paper.pdf: resolved doi:10.1000/xyz123 via crossref, from the network"
    );
}

/// `library` not attempted — the run has no library at all — leaves the
/// line unchanged too.
#[test]
fn human_line_with_no_library_consultation_is_unchanged() {
    assert_eq!(
        human_line(&resolved()).unwrap(),
        "paper.pdf: resolved doi:10.1000/xyz123 via crossref, from the network"
    );
}

/// design D9: the content-index examples, with a record carrying a
/// title, authors and year.
#[test]
fn human_line_of_resolved_shows_the_work_and_the_from_clause() {
    let mut record = record_with(Some("Determination of things"), &["Smith"], Some(2015));
    record
        .borax
        .provenance
        .insert("title".to_string(), Source::Crossref);
    let mut sections = sections_via_network("crossref", "doi:10.1039/c5ay00042d");
    sections.record_retrieval = Some(RetrievedFrom::ContentIndex);
    sections.content_index.read = IndexReadStep::Hit;
    let event = Event::Resolved {
        path: PathBuf::from("papers/smith.pdf"),
        identifier: "doi:10.1039/c5ay00042d".to_string(),
        record: Box::new(record),
        sections: Box::new(sections),
    };

    assert_eq!(
        human_line(&event).unwrap(),
        "papers/smith.pdf: resolved doi:10.1039/c5ay00042d to \"Determination of things\" \
         (Smith, 2015) via crossref, from the content index"
    );
}

/// design D9: author rendering for two and for three-or-more authors.
#[test]
fn human_line_of_resolved_renders_author_counts() {
    let two = record_with(Some("A Title"), &["Smith", "Jones"], Some(2020));
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(two),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz")),
    };
    assert!(
        human_line(&event)
            .unwrap()
            .contains("(Smith and Jones, 2020)"),
        "got {:?}",
        human_line(&event)
    );

    let three = record_with(Some("A Title"), &["Smith", "Jones", "Lee"], Some(2020));
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(three),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz")),
    };
    assert!(
        human_line(&event).unwrap().contains("(Smith et al., 2020)"),
        "got {:?}",
        human_line(&event)
    );
}

/// design D9: no title, no authors and no year each drop their part of
/// the `<work>` clause, and all three absent drops the clause whole.
#[test]
fn human_line_of_resolved_drops_absent_parts_of_the_work_clause() {
    let no_title = record_with(None, &["Smith"], Some(2020));
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(no_title),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz")),
    };
    let line = human_line(&event).unwrap();
    assert!(!line.contains('"'), "got {line:?}");
    assert!(line.contains("(Smith, 2020)"), "got {line:?}");

    let no_authors_or_year = record_with(Some("A Title"), &[], None);
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(no_authors_or_year),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz")),
    };
    let line = human_line(&event).unwrap();
    assert!(line.contains("\"A Title\""), "got {line:?}");
    assert!(!line.contains('('), "got {line:?}");

    let nothing = record_with(None, &[], None);
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(nothing),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz")),
    };
    let line = human_line(&event).unwrap();
    assert!(!line.contains(" to "), "got {line:?}");
}

/// design D9: a response-cache answer is `from the response cache`.
#[test]
fn human_line_of_resolved_names_the_response_cache() {
    let mut sections = sections_via_network("crossref", "doi:10.1000/xyz");
    sections.record_retrieval = Some(RetrievedFrom::ServiceCache {
        service: "crossref".to_string(),
    });
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(Record::new(EntryType::Article)),
        sections: Box::new(sections),
    };
    assert_eq!(
        human_line(&event).unwrap(),
        "a.pdf: resolved doi:10.1000/xyz via crossref, from the response cache"
    );
}

/// `(cached)` and `(from the library)` go, replaced wholly by the
/// `from` clause (design D9).
#[test]
fn human_line_never_shows_the_schema_3_cached_or_from_the_library_suffixes() {
    for event in [
        resolved(),
        resolved_with_library(tracked_answer()),
        Event::Resolved {
            path: PathBuf::from("a.pdf"),
            identifier: "doi:10.1000/xyz".to_string(),
            record: Box::new(Record::new(EntryType::Article)),
            sections: Box::new(sections_via_content_index()),
        },
    ] {
        let line = human_line(&event).unwrap();
        assert!(!line.contains("(cached)"), "got {line:?}");
        assert!(!line.contains("(from the library)"), "got {line:?}");
    }
}

// ---------------------------------------------------------------------
// Escaping (design D9)
// ---------------------------------------------------------------------

/// design D9: a title carrying a control character is escaped on the
/// `resolved` human line.
#[test]
fn human_line_of_resolved_escapes_a_control_character_in_the_title() {
    let record = record_with(Some("Evil\u{1b}[2JTitle"), &["Smith"], Some(2020));
    let event = Event::Resolved {
        path: PathBuf::from("a.pdf"),
        identifier: "doi:10.1000/xyz".to_string(),
        record: Box::new(record),
        sections: Box::new(sections_via_network("crossref", "doi:10.1000/xyz")),
    };
    assert!(
        human_line(&event).unwrap().contains("Evil\\x1b[2JTitle"),
        "got {:?}",
        human_line(&event)
    );
}

/// The same holds for a resolution skip's unreadable message and a
/// non-resolution skip's message (`rename-failed`).
#[test]
fn human_line_escapes_control_characters_in_skip_messages() {
    let event = skipped_verdict(
        SkipReason::Unreadable {
            message: "bad\u{1b}[2Jfile".to_string(),
        },
        unreadable_extraction_sections("bad\u{1b}[2Jfile"),
    );
    assert!(
        human_line(&event).unwrap().contains("bad\\x1b[2Jfile"),
        "got {:?}",
        human_line(&event)
    );

    let event = skipped(SkipReason::RenameFailed {
        message: "disk\u{1b}[2Jfull".to_string(),
    });
    assert!(
        human_line(&event).unwrap().contains("disk\\x1b[2Jfull"),
        "got {:?}",
        human_line(&event)
    );
}

/// design D8: `content-index-write` renders nothing for `written`, and
/// the documented line, escaped, for `failed`.
#[test]
fn human_line_of_content_index_write() {
    assert_eq!(human_line(&content_index_write(WriteStep::Written)), None);

    let event = content_index_write(WriteStep::Failed {
        message: "disk\u{1b}[2Jfull".to_string(),
    });
    assert_eq!(
        human_line(&event).unwrap(),
        "smith2024_borax.pdf: the content index could not keep this answer (disk\\x1b[2Jfull)"
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
/// `kind` in kebab-case, for every result kind. This event does not
/// change in this change: the `found` status and `tier` field are
/// `library-extraction`'s own, unaffected by `resolved`'s field
/// removal (D14, "not about the stream").
#[test]
fn json_line_of_library_extraction_matches_design_d4_for_every_kind() {
    for (extraction, expected_extraction) in all_extractions() {
        let event = library_extraction("sub/a.pdf", extraction.clone());
        let value: Value = serde_json::from_str(&json_line(&event)).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "schema": 4,
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
            r#"{"schema":4,"event":"library-condition","path":"sub/new.pdf","condition":{"kind":"orphan"}}"#
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
                r#"{{"schema":4,"event":"library-condition","path":"gone.pdf","condition":{{"kind":"missing","id":"{COND_UUID_MISSING}","record":".borax/artifacts/{COND_UUID_MISSING}.toml"}}}}"#
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
                r#"{{"schema":4,"event":"library-condition","path":"items/milner1978.unlinked.toml","condition":{{"kind":"unlinked","id":"{COND_UUID_UNLINKED}"}}}}"#
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
        r#"{"schema":4,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"unindexed"}}"#
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
