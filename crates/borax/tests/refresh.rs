#![allow(clippy::unwrap_used)]

//! `borax::refresh` (refresh-as-candidate, design D2, D3, D7, D8, D9):
//! the pure comparison of an item's record with a fresh one, the
//! replacement record, the lookup, the step a refresh reports, and whose
//! item a reference answer is.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use borax::event::{
    DifferenceStep, FetchedFrom, RefreshAcceptance, RefreshStep, ServiceAnswer, ServiceOutcome,
    WriteStep,
};
use borax::evidence::{ServiceAttempt, Unattempted};
use borax::library::{STATE_DIR, Stores, identity_file};
use borax::refresh::{
    Decision, Difference, Field, Fresh, Refresh, Whose, differences, refresh, replaced, whose,
};
use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_core::index::LibraryReference;
use borax_core::library::{ArtifactId, ItemId, LibraryId};
use borax_core::record::{DateParts, EntryType, Name, Record, Source as FieldSource};
use borax_sources::cache::CacheWrite;
use borax_sources::source::{Fetched, Retrieval, Source, SourceError, SourceName};
use serde_json::{Value, json};
use tempfile::tempdir;

const UUID_A: &str = "018f2b36-7f21-7abc-8def-0123456789ab";
const UUID_B: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef0";
const UUID_C: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef1";
const UUID_D: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef2";

const DOI: &str = "10.1039/c5ay00042d";

// ---------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------

fn doi_identifier() -> Identifier {
    Identifier::Doi(Doi::parse(DOI).unwrap())
}

fn author(family: &str, given: &str) -> Name {
    Name {
        family: family.to_string(),
        given: Some(given.to_string()),
    }
}

/// A journal article with a title, an author, a year and a DOI, whose
/// title and DOI provenance name Crossref.
fn item_record() -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Determination of things".to_string());
    record.authors = vec![author("Klykov", "S.")];
    record.issued = Some(DateParts {
        year: 2015,
        month: None,
        day: None,
    });
    record.doi = Some(Doi::parse(DOI).unwrap());
    record
        .borax
        .provenance
        .insert("title".to_string(), FieldSource::Crossref);
    record
        .borax
        .provenance
        .insert("DOI".to_string(), FieldSource::Crossref);
    record
}

/// A record with every compared field set.
fn full_record() -> Record {
    let mut record = item_record();
    record.container_title = Some("Analytical Methods".to_string());
    record.volume = Some("7".to_string());
    record.issue = Some("3".to_string());
    record.pages = Some("100-110".to_string());
    record.publisher = Some("Royal Society of Chemistry".to_string());
    record.pmid = Some(Pmid::parse("12345678").unwrap());
    record.isbn = Some(Isbn::parse("9780306406157").unwrap());
    record.borax.arxiv = Some(ArxivId::parse("1706.03762v1").unwrap());
    record
}

fn titled(title: &str) -> Record {
    Record {
        title: Some(title.to_string()),
        ..item_record()
    }
}

/// A [`Source`] answering as scripted, recording every identifier it is
/// asked about.
struct Scripted {
    name: SourceName,
    supports: fn(&Identifier) -> bool,
    response: Result<Record, SourceError>,
    asked: Mutex<Vec<String>>,
}

fn any_identifier(_: &Identifier) -> bool {
    true
}

fn arxiv_only(identifier: &Identifier) -> bool {
    matches!(identifier, Identifier::Arxiv(_))
}

impl Scripted {
    fn new(name: SourceName, response: Result<Record, SourceError>) -> Scripted {
        Scripted {
            name,
            supports: any_identifier,
            response,
            asked: Mutex::new(Vec::new()),
        }
    }

    fn supporting(mut self, supports: fn(&Identifier) -> bool) -> Scripted {
        self.supports = supports;
        self
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Source for Scripted {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, identifier: &Identifier) -> bool {
        (self.supports)(identifier)
    }

    fn fetch(&self, identifier: &Identifier) -> Result<Fetched, SourceError> {
        self.asked.lock().unwrap().push(identifier.to_string());
        self.response.clone().map(Fetched::network)
    }
}

/// A source that must never be asked.
struct PanicSource;

impl Source for PanicSource {
    fn name(&self) -> SourceName {
        SourceName::Crossref
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, identifier: &Identifier) -> Result<Fetched, SourceError> {
        panic!("a refresh asked for {identifier}")
    }
}

fn unavailable() -> SourceError {
    SourceError::Unavailable {
        message: "timed out".to_string(),
    }
}

fn found() -> ServiceAttempt {
    found_by(SourceName::Crossref)
}

fn found_by(service: SourceName) -> ServiceAttempt {
    ServiceAttempt {
        service,
        outcome: Ok(Retrieval::Network { stored: None }),
    }
}

fn failed(service: SourceName, error: SourceError) -> ServiceAttempt {
    ServiceAttempt {
        service,
        outcome: Err(error),
    }
}

fn serialized(record: &Record) -> Value {
    serde_json::to_value(record).unwrap()
}

// ---------------------------------------------------------------------
// the comparison (D7)
// ---------------------------------------------------------------------

#[test]
fn fields_are_the_records_own_keys_in_record_order() {
    let keys: Vec<&str> = Field::ALL.iter().map(|field| field.as_str()).collect();

    assert_eq!(
        keys,
        vec![
            "type",
            "title",
            "author",
            "issued",
            "container-title",
            "volume",
            "issue",
            "page",
            "publisher",
            "DOI",
            "PMID",
            "ISBN",
            "arxiv",
        ]
    );
    assert_eq!(Field::ALL[0], Field::Type);
    assert_eq!(Field::ALL[12], Field::Arxiv);
}

#[test]
fn field_value_is_the_fields_json_as_the_record_serializes_it() {
    let record = full_record();
    let whole = serialized(&record);

    for field in Field::ALL {
        let expected = match field {
            Field::Arxiv => whole["borax"]["arxiv"].clone(),
            other => whole[other.as_str()].clone(),
        };
        assert!(!expected.is_null(), "{field:?} is unset in the fixture");
        assert_eq!(field.value(&record), Some(expected), "{field:?}");
    }

    let bare = Record::new(EntryType::Article);
    for field in Field::ALL {
        let expected = (field == Field::Type).then(|| json!("article-journal"));
        assert_eq!(field.value(&bare), expected, "{field:?}");
    }
}

#[test]
fn differences_of_equal_records_are_none() {
    assert_eq!(differences(&full_record(), &full_record()), Vec::new());
}

#[test]
fn differences_ignore_provenance_confidence_and_source_fields() {
    let item = full_record();
    let mut candidate = full_record();
    candidate.borax.provenance.clear();
    candidate
        .borax
        .provenance
        .insert("title".to_string(), FieldSource::OpenAlex);
    candidate.borax.confidence = Some(0.25);
    candidate
        .borax
        .source_fields
        .insert("subject".to_string(), json!(["Chemistry"]));

    assert_eq!(differences(&item, &candidate), Vec::new());
}

#[test]
fn differences_report_a_changed_title_with_both_values() {
    let item = titled("REVIEW CORRECTION: Determination of things");
    let candidate = titled("Determination of things");

    assert_eq!(
        differences(&item, &candidate),
        vec![Difference {
            field: Field::Title,
            item: Some(json!("REVIEW CORRECTION: Determination of things")),
            candidate: Some(json!("Determination of things")),
        }]
    );
}

#[test]
fn differences_report_a_field_the_item_lacks_with_no_item_value() {
    let item = item_record();
    let mut candidate = item_record();
    candidate.volume = Some("7".to_string());

    assert_eq!(
        differences(&item, &candidate),
        vec![Difference {
            field: Field::Volume,
            item: None,
            candidate: Some(json!("7")),
        }]
    );
}

#[test]
fn differences_report_a_field_the_candidate_lacks_with_no_candidate_value() {
    let mut item = item_record();
    item.publisher = Some("Royal Society of Chemistry".to_string());
    let candidate = item_record();

    assert_eq!(
        differences(&item, &candidate),
        vec![Difference {
            field: Field::Publisher,
            item: Some(json!("Royal Society of Chemistry")),
            candidate: None,
        }]
    );
}

#[test]
fn differences_compare_authors_in_order() {
    let mut item = item_record();
    item.authors = vec![author("Klykov", "S."), author("Petrov", "A.")];
    let mut candidate = item_record();
    candidate.authors = vec![author("Petrov", "A."), author("Klykov", "S.")];

    assert_eq!(
        differences(&item, &candidate),
        vec![Difference {
            field: Field::Author,
            item: Field::Author.value(&item),
            candidate: Field::Author.value(&candidate),
        }]
    );
    assert_ne!(Field::Author.value(&item), Field::Author.value(&candidate));
}

#[test]
fn differences_compare_issued_to_its_precision() {
    let item = item_record();
    let mut candidate = item_record();
    candidate.issued = Some(DateParts {
        year: 2015,
        month: Some(3),
        day: None,
    });

    assert_eq!(
        differences(&item, &candidate),
        vec![Difference {
            field: Field::Issued,
            item: Some(json!({"date-parts": [[2015]]})),
            candidate: Some(json!({"date-parts": [[2015, 3]]})),
        }]
    );
}

#[test]
fn differences_compare_the_arxiv_identifier_with_its_version() {
    let mut item = item_record();
    item.borax.arxiv = Some(ArxivId::parse("1706.03762v1").unwrap());
    let mut candidate = item_record();
    candidate.borax.arxiv = Some(ArxivId::parse("1706.03762v2").unwrap());

    let found = differences(&item, &candidate);

    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].field, Field::Arxiv);
    assert_eq!(found[0].item, Field::Arxiv.value(&item));
    assert_eq!(found[0].candidate, Field::Arxiv.value(&candidate));
    assert_ne!(found[0].item, found[0].candidate);
}

#[test]
fn differences_do_not_normalise_whitespace_or_case() {
    let item = titled("Determination of things");

    for variant in [
        "Determination of things ",
        "Determination  of things",
        "determination of things",
    ] {
        let found = differences(&item, &titled(variant));

        assert_eq!(found.len(), 1, "for {variant:?}: {found:?}");
        assert_eq!(found[0].field, Field::Title);
        assert_eq!(found[0].candidate, Some(json!(variant)));
    }
}

#[test]
fn differences_come_in_field_order() {
    let item = item_record();
    let mut candidate = item_record();
    candidate.publisher = Some("Elsevier".to_string());
    candidate.entry_type = EntryType::Preprint;
    candidate.title = Some("Another title".to_string());
    candidate.pages = Some("1-2".to_string());

    let fields: Vec<Field> = differences(&item, &candidate)
        .into_iter()
        .map(|difference| difference.field)
        .collect();

    assert_eq!(
        fields,
        vec![Field::Type, Field::Title, Field::Page, Field::Publisher]
    );
}

// ---------------------------------------------------------------------
// the replacement record (D9)
// ---------------------------------------------------------------------

#[test]
fn replaced_takes_each_differing_field_from_the_candidate() {
    let mut item = item_record();
    item.title = Some("REVIEW CORRECTION: Determination of things".to_string());
    item.volume = Some("1".to_string());
    item.publisher = Some("Kept".to_string());
    let mut candidate = item_record();
    candidate.volume = Some("2".to_string());
    candidate.publisher = Some("Kept".to_string());
    candidate.authors = vec![author("Klykov", "S."), author("Petrov", "A.")];

    let result = replaced(&item, &candidate);

    assert_eq!(result.title, candidate.title);
    assert_eq!(result.volume, Some("2".to_string()));
    assert_eq!(result.authors, candidate.authors);
    assert_eq!(result.publisher, Some("Kept".to_string()));
    assert_eq!(result.doi, item.doi);
    assert_eq!(result.issued, item.issued);
}

#[test]
fn replaced_removes_a_field_the_candidate_lacks() {
    let mut item = item_record();
    item.volume = Some("7".to_string());
    item.borax.arxiv = Some(ArxivId::parse("1706.03762v1").unwrap());
    let candidate = item_record();

    let result = replaced(&item, &candidate);

    assert_eq!(result.volume, None);
    assert_eq!(result.borax.arxiv, None);
    assert_eq!(result.title, item.title);
}

#[test]
fn replaced_takes_the_provenance_entries_of_the_fields_it_changes() {
    let mut item = titled("Corrected");
    item.volume = Some("1".to_string());
    item.borax
        .provenance
        .insert("volume".to_string(), FieldSource::Crossref);
    let mut candidate = titled("Determination of things");
    candidate
        .borax
        .provenance
        .insert("title".to_string(), FieldSource::OpenAlex);
    candidate.volume = None;

    let result = replaced(&item, &candidate);

    assert_eq!(
        result.borax.provenance.get("title"),
        Some(&FieldSource::OpenAlex)
    );
    assert_eq!(result.borax.provenance.get("volume"), None);
}

#[test]
fn replaced_keeps_source_fields_confidence_and_every_other_provenance_entry() {
    let mut item = titled("Corrected");
    item.borax.confidence = Some(0.9);
    item.borax
        .source_fields
        .insert("subject".to_string(), json!(["Item subject"]));
    item.borax
        .provenance
        .insert("author".to_string(), FieldSource::Sidecar);
    let mut candidate = titled("Determination of things");
    candidate.borax.confidence = Some(0.1);
    candidate
        .borax
        .source_fields
        .insert("subject".to_string(), json!(["Candidate subject"]));
    candidate
        .borax
        .provenance
        .insert("author".to_string(), FieldSource::OpenAlex);

    let result = replaced(&item, &candidate);

    assert_eq!(result.borax.confidence, Some(0.9));
    assert_eq!(result.borax.source_fields, item.borax.source_fields);
    assert_eq!(
        result.borax.provenance.get("author"),
        Some(&FieldSource::Sidecar)
    );
    assert_eq!(
        result.borax.provenance.get("DOI"),
        Some(&FieldSource::Crossref)
    );
    assert_eq!(result.title, candidate.title);
}

#[test]
fn replaced_has_no_difference_from_the_candidate() {
    let item = full_record();
    let mut candidate = item_record();
    candidate.title = Some("Another".to_string());
    candidate.entry_type = EntryType::Preprint;
    candidate.pmid = None;
    candidate.borax.arxiv = Some(ArxivId::parse("2401.12345").unwrap());

    let result = replaced(&item, &candidate);

    assert_eq!(differences(&result, &candidate), Vec::new());
    assert!(!differences(&item, &candidate).is_empty());
}

// ---------------------------------------------------------------------
// the lookup (D2)
// ---------------------------------------------------------------------

#[test]
fn refresh_of_a_record_with_no_identifier_asks_nothing_and_is_no_identifier() {
    let record = Record::new(EntryType::Article);
    let sources: Vec<&dyn Source> = vec![&PanicSource];

    assert_eq!(refresh(&record, &sources), Fresh::NoIdentifier);
}

#[test]
fn refresh_looks_up_the_preferred_identifier() {
    let mut record = item_record();
    record.borax.arxiv = Some(ArxivId::parse("1706.03762").unwrap());
    let crossref = Scripted::new(SourceName::Crossref, Ok(record.clone()));
    let arxiv = Scripted::new(SourceName::Arxiv, Ok(record.clone()));
    let sources: Vec<&dyn Source> = vec![&arxiv, &crossref];

    refresh(&record, &sources);

    assert_eq!(crossref.asked(), vec![doi_identifier().to_string()]);
    assert_eq!(arxiv.asked(), Vec::<String>::new());
}

#[test]
fn refresh_with_no_service_supporting_the_identifier_is_no_eligible_service() {
    let arxiv = Scripted::new(SourceName::Arxiv, Ok(item_record())).supporting(arxiv_only);
    let sources: Vec<&dyn Source> = vec![&arxiv];

    assert_eq!(
        refresh(&item_record(), &sources),
        Fresh::NoEligibleService {
            identifier: doi_identifier()
        }
    );
    assert_eq!(arxiv.asked(), Vec::<String>::new());
}

#[test]
fn refresh_no_service_answers_is_unresolved_with_every_attempt_in_order() {
    let crossref = Scripted::new(SourceName::Crossref, Err(unavailable()));
    let openalex = Scripted::new(SourceName::OpenAlex, Err(SourceError::RateLimited));
    let sources: Vec<&dyn Source> = vec![&openalex, &crossref];

    assert_eq!(
        refresh(&item_record(), &sources),
        Fresh::Unresolved {
            identifier: doi_identifier(),
            attempts: vec![
                failed(SourceName::Crossref, unavailable()),
                failed(SourceName::OpenAlex, SourceError::RateLimited),
            ],
        }
    );
}

#[test]
fn refresh_of_an_identifier_no_service_holds_is_unresolved_with_not_found_attempts() {
    let crossref = Scripted::new(SourceName::Crossref, Err(SourceError::NotFound));
    let openalex = Scripted::new(SourceName::OpenAlex, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];

    assert_eq!(
        refresh(&item_record(), &sources),
        Fresh::Unresolved {
            identifier: doi_identifier(),
            attempts: vec![
                failed(SourceName::Crossref, SourceError::NotFound),
                failed(SourceName::OpenAlex, SourceError::NotFound),
            ],
        }
    );
}

#[test]
fn refresh_equal_in_every_compared_field_is_current() {
    let item = full_record();
    let crossref = Scripted::new(SourceName::Crossref, Ok(item.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];

    assert_eq!(
        refresh(&item, &sources),
        Fresh::Current {
            identifier: doi_identifier(),
            attempts: vec![found()],
        }
    );
}

#[test]
fn refresh_differing_in_provenance_alone_is_current() {
    let item = item_record();
    let mut fetched = item_record();
    fetched.borax.provenance.clear();
    fetched.borax.confidence = Some(0.5);
    let crossref = Scripted::new(SourceName::Crossref, Ok(fetched));
    let sources: Vec<&dyn Source> = vec![&crossref];

    assert_eq!(
        refresh(&item, &sources),
        Fresh::Current {
            identifier: doi_identifier(),
            attempts: vec![found()],
        }
    );
}

#[test]
fn refresh_differing_is_differs_with_the_candidate_and_its_differences() {
    let item = titled("REVIEW CORRECTION: Determination of things");
    let fetched = titled("Determination of things");
    let crossref = Scripted::new(SourceName::Crossref, Ok(fetched.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref];

    assert_eq!(
        refresh(&item, &sources),
        Fresh::Differs {
            identifier: doi_identifier(),
            attempts: vec![found()],
            differences: differences(&item, &fetched),
            candidate: fetched,
        }
    );
}

#[test]
fn refresh_keeps_a_failed_service_ahead_of_the_one_that_answered() {
    let item = titled("Corrected");
    let fetched = titled("Determination of things");
    let crossref = Scripted::new(SourceName::Crossref, Err(unavailable()));
    let openalex = Scripted::new(SourceName::OpenAlex, Ok(fetched.clone()));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];

    assert_eq!(
        refresh(&item, &sources),
        Fresh::Differs {
            identifier: doi_identifier(),
            attempts: vec![
                failed(SourceName::Crossref, unavailable()),
                found_by(SourceName::OpenAlex),
            ],
            differences: differences(&item, &fetched),
            candidate: fetched,
        }
    );
}

// ---------------------------------------------------------------------
// the step (D8)
// ---------------------------------------------------------------------

fn item_of_the_step() -> ItemId {
    ItemId::parse(UUID_A).unwrap()
}

fn refresh_of(fresh: Fresh, decision: Decision) -> Refresh {
    Refresh {
        item: item_of_the_step(),
        whose: Whose::Own,
        fresh,
        decision,
    }
}

fn written_answer(service: &str) -> ServiceAnswer {
    ServiceAnswer {
        service: service.to_string(),
        outcome: ServiceOutcome::Found {
            retrieval: FetchedFrom::Network,
            stored: Some(WriteStep::Written),
        },
    }
}

fn written_attempt() -> ServiceAttempt {
    ServiceAttempt {
        service: SourceName::Crossref,
        outcome: Ok(Retrieval::Network {
            stored: Some(CacheWrite::Written),
        }),
    }
}

#[test]
fn refresh_step_of_differs_carries_its_decision_as_acceptance() {
    let item = titled("Corrected");
    let candidate = titled("Determination of things");
    let fresh = Fresh::Differs {
        identifier: doi_identifier(),
        attempts: vec![written_attempt()],
        differences: differences(&item, &candidate),
        candidate: candidate.clone(),
    };
    let cases = [
        (
            Decision::NotAttempted(Unattempted::NotAsked),
            RefreshAcceptance::NotAttempted {
                reason: "not-asked".to_string(),
            },
        ),
        (
            Decision::NotAttempted(Unattempted::AnotherLibrary),
            RefreshAcceptance::NotAttempted {
                reason: "another-library".to_string(),
            },
        ),
        (Decision::Pending, RefreshAcceptance::Pending),
        (Decision::Accepted, RefreshAcceptance::Accepted),
        (
            Decision::Unwritten {
                message: "changed after this run read it".to_string(),
            },
            RefreshAcceptance::Unwritten {
                message: "changed after this run read it".to_string(),
            },
        ),
        (Decision::Declined, RefreshAcceptance::Declined),
    ];

    for (decision, acceptance) in cases {
        let step = refresh_of(fresh.clone(), decision.clone()).step();

        assert_eq!(
            step,
            RefreshStep::Differs {
                identifier: doi_identifier().to_string(),
                attempts: vec![written_answer("crossref")],
                differences: vec![DifferenceStep {
                    field: "title".to_string(),
                    item: Some(json!("Corrected")),
                    candidate: Some(json!("Determination of things")),
                }],
                candidate: Box::new(candidate.clone()),
                acceptance,
            },
            "for {decision:?}"
        );
    }
}

#[test]
fn refresh_step_of_current_carries_no_acceptance() {
    let fresh = Fresh::Current {
        identifier: doi_identifier(),
        attempts: vec![written_attempt()],
    };

    for decision in [
        Decision::NotAttempted(Unattempted::NotAsked),
        Decision::Declined,
    ] {
        assert_eq!(
            refresh_of(fresh.clone(), decision).step(),
            RefreshStep::Current {
                identifier: doi_identifier().to_string(),
                attempts: vec![written_answer("crossref")],
            }
        );
    }
}

#[test]
fn refresh_step_of_no_identifier_is_not_attempted_no_identifier() {
    assert_eq!(
        refresh_of(
            Fresh::NoIdentifier,
            Decision::NotAttempted(Unattempted::NotAsked)
        )
        .step(),
        RefreshStep::NotAttempted {
            reason: "no-identifier".to_string()
        }
    );
}

// ---------------------------------------------------------------------
// whose item it is (D3)
// ---------------------------------------------------------------------

fn write_identity(root: &Path, text: &str) {
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), text).unwrap();
}

fn reference(library: &str, root: &Path) -> LibraryReference {
    LibraryReference {
        library: LibraryId::parse(library).unwrap(),
        root: root.to_path_buf(),
        artifact: ArtifactId::parse(UUID_C).unwrap(),
        item: ItemId::parse(UUID_D).unwrap(),
    }
}

fn identified(root: &Path, library: &str) -> Stores {
    write_identity(root, &format!("id = \"{library}\"\n"));
    let stores = Stores::read(root);
    assert_eq!(
        stores.identity(),
        Some(&LibraryId::parse(library).unwrap()),
        "the fixture's identity must be Present"
    );
    stores
}

#[test]
fn whose_of_a_reference_to_the_runs_identity_at_its_root_is_own() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let stores = identified(root, UUID_A);

    assert_eq!(whose(&reference(UUID_A, root), Some(&stores)), Whose::Own);
}

#[test]
fn whose_of_a_reference_to_the_runs_identity_at_another_root_is_referenced() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("library");
    let copy = directory.path().join("copy");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&copy).unwrap();
    let stores = identified(&root, UUID_A);

    assert_eq!(
        whose(&reference(UUID_A, &copy), Some(&stores)),
        Whose::Referenced
    );
}

#[test]
fn whose_of_a_reference_to_another_identity_at_the_runs_root_is_referenced() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let stores = identified(root, UUID_A);

    assert_eq!(
        whose(&reference(UUID_B, root), Some(&stores)),
        Whose::Referenced
    );
}

#[test]
fn whose_compares_roots_normalised_lexically() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join("sub")).unwrap();
    let spelled = root.join(".").join("sub").join("..");
    write_identity(root, &format!("id = \"{UUID_A}\"\n"));
    let stores = Stores::read(&spelled);
    assert_eq!(stores.identity(), Some(&LibraryId::parse(UUID_A).unwrap()));

    assert_eq!(whose(&reference(UUID_A, root), Some(&stores)), Whose::Own);
    assert_eq!(
        whose(&reference(UUID_A, &spelled), Some(&stores)),
        Whose::Own
    );
}

#[test]
fn whose_in_a_run_whose_library_has_no_identity_is_referenced() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let stores = Stores::read(root);
    assert_eq!(stores.identity(), None, "the fixture has no identity");

    assert_eq!(
        whose(&reference(UUID_A, root), Some(&stores)),
        Whose::Referenced
    );
}

#[test]
fn whose_in_a_run_whose_identity_is_faulty_is_referenced() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    write_identity(root, "id = 7\n");
    let stores = Stores::read(root);
    assert_eq!(stores.identity(), None, "the fixture's identity is faulty");

    assert_eq!(
        whose(&reference(UUID_A, root), Some(&stores)),
        Whose::Referenced
    );
}

#[test]
fn whose_in_a_run_with_no_library_is_referenced() {
    let root = PathBuf::from("/somewhere/library");

    assert_eq!(whose(&reference(UUID_A, &root), None), Whose::Referenced);
}
