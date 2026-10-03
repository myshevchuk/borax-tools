#![allow(clippy::unwrap_used)]

//! Task 2.1: exact expected lines for [`describe::describe`], design D1.
//!
//! Every scenario asserts equality against the whole `Vec<String>`
//! `describe` returns, not a substring: D1 fixes a label column, a
//! hanging indent, and an omission rule for absent fields, and a test
//! that only checked `contains` would let any of those slip.
//!
//! The rule line's own width is not literally pinned to design D1's
//! worked example (see the doc comment on `rule` below) — every other
//! line is copied from, or built in the same shape as, that example.

use std::path::PathBuf;

use borax::describe::{DEFAULT_WIDTH, Position, Proposal, describe};
use borax::event::{
    Acceptance, Claim, ClaimOrigin, ContentIndexSection, Event, ExtractionResultStep,
    ExtractionSection, FetchedFrom, IdentifierOrigin, IndexReadStep, LibraryAnswer, LibraryStep,
    LookupStep, MatchCheckStep, RetrievedFrom, Sections, ServiceAnswer, ServiceOutcome, SkipReason,
    TitlesStep, WriteStep,
};
use borax_core::identifier::{ArxivId, Doi};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record, Source};

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

fn name(given: &str, family: &str) -> Name {
    Name {
        family: family.to_string(),
        given: Some(given.to_string()),
    }
}

/// A resolved event, defaulted to a live text-layer DOI resolution
/// through Crossref. Callers override only what a scenario needs.
struct Fixture {
    path: PathBuf,
    record: Record,
    source: String,
    found: String,
    claims: Vec<Claim>,
    tier: Option<String>,
    cached: bool,
}

impl Fixture {
    fn new(record: Record, found: &str) -> Fixture {
        Fixture {
            path: PathBuf::from("paper.pdf"),
            record,
            source: "crossref".to_string(),
            found: found.to_string(),
            claims: Vec::new(),
            tier: Some("text-layer".to_string()),
            cached: false,
        }
    }

    /// Design D14's mapping, read backwards: the schema-3 shape this
    /// fixture holds, translated into schema-4 sections.
    ///
    /// - `tier: Some(t)` where `t` is `"embedded-metadata"` or
    ///   `"text-layer"` becomes an extracted-origin lookup, found over
    ///   the network by `source`.
    /// - `tier: Some("supplied")` becomes an operator-origin lookup,
    ///   otherwise the same.
    /// - `tier: None, cached: true` becomes a content-index hit, which
    ///   names no service of its own; `source` is injected into the
    ///   record's own provenance so `services_of` still names it,
    ///   unless the record already carries provenance (a test that set
    ///   it directly keeps what it set) or `source` is the schema-3
    ///   `"library"` stand-in for naming none.
    fn sections(&self) -> Sections {
        match (self.tier.as_deref(), self.cached) {
            (None, true) => Sections {
                library: LibraryStep::NotAttempted {
                    reason: "no-library".to_string(),
                },
                content_index: ContentIndexSection {
                    read: IndexReadStep::Hit,
                    write: WriteStep::NotAttempted {
                        reason: "content-index-hit".to_string(),
                    },
                },
                extraction: ExtractionSection {
                    result: ExtractionResultStep::NotAttempted {
                        reason: "content-index-hit".to_string(),
                    },
                    titles: TitlesStep::NotAttempted {
                        reason: "content-index-hit".to_string(),
                    },
                },
                lookup: LookupStep::NotAttempted {
                    reason: "content-index-hit".to_string(),
                },
                record_retrieval: Some(RetrievedFrom::ContentIndex),
                match_check: MatchCheckStep::NotAttempted {
                    reason: "content-index-hit".to_string(),
                },
                acceptance: Acceptance::Automatic,
            },
            (Some(tier), _) => {
                let origin = if tier == "supplied" {
                    IdentifierOrigin::Operator
                } else {
                    IdentifierOrigin::Extracted
                };
                let result_tier = if tier == "supplied" {
                    "text-layer".to_string()
                } else {
                    tier.to_string()
                };
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
                            identifier: self.found.clone(),
                            tier: result_tier,
                        },
                        titles: TitlesStep::Read {
                            claims: self.claims.clone(),
                        },
                    },
                    lookup: LookupStep::Attempted {
                        identifier: self.found.clone(),
                        origin,
                        attempts: vec![ServiceAnswer {
                            service: self.source.clone(),
                            outcome: ServiceOutcome::Found {
                                retrieval: FetchedFrom::Network,
                                stored: Some(WriteStep::Written),
                            },
                        }],
                    },
                    record_retrieval: Some(RetrievedFrom::Network {
                        service: self.source.clone(),
                    }),
                    match_check: MatchCheckStep::Agreed,
                    acceptance: Acceptance::Automatic,
                }
            }
            (None, false) => unreachable!("every fixture sets a tier or a content-index hit"),
        }
    }

    /// `self.record`, with `source` injected as its provenance when
    /// nothing already names one — the schema-3 `source` field's fact,
    /// carried by `record.borax.provenance` wherever the retrieval
    /// itself (content index, library) does not name a service.
    fn record_with_provenance(&self) -> Record {
        let is_content_index_hit = self.tier.is_none() && self.cached;
        match is_content_index_hit {
            true => record_with_source_provenance(&self.record, &self.source),
            false => self.record.clone(),
        }
    }

    fn event(&self) -> Event {
        Event::Resolved {
            path: self.path.clone(),
            identifier: self.found.clone(),
            record: Box::new(self.record_with_provenance()),
            sections: Box::new(self.sections()),
        }
    }
}

/// The [`Source`] a schema-3 `source` string named, or `None` for the
/// schema-3 stand-ins (`"cache"`, `"library"`) that named no service.
fn source_enum(source: &str) -> Option<Source> {
    match source {
        "crossref" => Some(Source::Crossref),
        "openalex" => Some(Source::OpenAlex),
        "arxiv" => Some(Source::Arxiv),
        "datacite" => Some(Source::DataCite),
        "pubmed" => Some(Source::PubMed),
        _ => None,
    }
}

/// `record`, with `source` injected as its provenance when nothing
/// already names one — the schema-3 `source` field's fact, carried by
/// `record.borax.provenance` wherever the retrieval itself (content
/// index, library) does not name a service (D14's mapping: `source ==
/// S` on an index or library answer becomes `record.borax.provenance`).
fn record_with_source_provenance(record: &Record, source: &str) -> Record {
    let mut record = record.clone();
    if record.borax.provenance.is_empty() && source != "library" {
        if let Some(source) = source_enum(source) {
            record.borax.provenance.insert("title".to_string(), source);
        }
    }
    record
}

fn not_found(service: &str) -> ServiceAnswer {
    ServiceAnswer {
        service: service.to_string(),
        outcome: ServiceOutcome::NotFound,
    }
}

fn unavailable(service: &str, message: &str) -> ServiceAnswer {
    ServiceAnswer {
        service: service.to_string(),
        outcome: ServiceOutcome::Unavailable {
            message: message.to_string(),
        },
    }
}

/// A resolution-verdict `skipped` event for an identifier `extraction`
/// found through `tier`, that `attempts` names in lookup order and no
/// service answered. Design D4's mapping for schema-3's
/// `Unresolvable { found, tier, attempts }`.
fn unresolvable_skip(
    path: &str,
    identifier: &str,
    tier: &str,
    attempts: Vec<ServiceAnswer>,
) -> Event {
    let not_attempted = || "no-record".to_string();
    Event::Skipped {
        path: PathBuf::from(path),
        reason: SkipReason::Unresolvable,
        sections: Some(Box::new(Sections {
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
                result: ExtractionResultStep::Found {
                    identifier: identifier.to_string(),
                    tier: tier.to_string(),
                },
                titles: TitlesStep::Read { claims: Vec::new() },
            },
            lookup: LookupStep::Attempted {
                identifier: identifier.to_string(),
                origin: IdentifierOrigin::Extracted,
                attempts,
            },
            record_retrieval: None,
            match_check: MatchCheckStep::NotAttempted {
                reason: not_attempted(),
            },
            acceptance: Acceptance::NotApplicable,
        })),
        candidate: None,
    }
}

/// A resolution-verdict `skipped` event whose extraction itself failed
/// with `result`, carrying no titles (the file opened with none
/// claimed). Design D4's mapping for the four extraction failure kinds.
fn extraction_failure_skip(path: &str, reason: SkipReason, result: ExtractionResultStep) -> Event {
    let not_attempted = || "extraction-failed".to_string();
    Event::Skipped {
        path: PathBuf::from(path),
        reason,
        sections: Some(Box::new(Sections {
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
                titles: TitlesStep::Read { claims: Vec::new() },
            },
            lookup: LookupStep::NotAttempted {
                reason: not_attempted(),
            },
            record_retrieval: None,
            match_check: MatchCheckStep::NotAttempted {
                reason: not_attempted(),
            },
            acceptance: Acceptance::NotApplicable,
        })),
        candidate: None,
    }
}

/// The rule line `describe` opens a description with.
///
/// Design D1's own worked example reads
/// `── 3 of 17 ──────────────────────────────────────────────────────────`,
/// 69 characters at what the surrounding prose implies is the default
/// width of 80 — it does not fill to `width`. That is very likely the
/// design doc's ASCII art being typed by hand rather than a second,
/// narrower rule width nothing else in D1 names. This suite instead
/// pins a rule that fills to exactly `width`, the reading consistent
/// with `width` governing every other line; the discrepancy is called
/// out in the test-writer's report so the orchestrator can confirm it
/// against the implementer's choice.
fn rule(of_this: usize, total: usize, width: usize) -> String {
    let prefix = format!("── {of_this} of {total} ");
    let dashes = width - prefix.chars().count();
    format!("{prefix}{}", "─".repeat(dashes))
}

fn label_line(label: &str, value: &str) -> String {
    format!("{label}{}{value}", " ".repeat(12 - label.chars().count()))
}

fn continuation(value: &str) -> String {
    format!("{}{value}", " ".repeat(12))
}

// ---------------------------------------------------------------------
// A full journal article, resolved live from a text-layer DOI
// ---------------------------------------------------------------------

#[test]
fn a_full_journal_article_from_a_text_layer_doi() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Concise Study of Something Specific".to_string());
    record.authors = vec![name("John", "Smith"), name("Jane", "Doe")];
    record.issued = Some(DateParts {
        year: 2024,
        month: None,
        day: None,
    });
    record.container_title = Some("Journal of Testing".to_string());
    record.volume = Some("12".to_string());
    record.issue = Some("3".to_string());
    record.pages = Some("45-67".to_string());
    record.doi = Some(Doi::parse("10.1234/smith.test.2024").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.1234/smith.test.2024");
    fixture.path = PathBuf::from("/collection/library/smith2024_raw.pdf");
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Xmp,
        title: "A Concise Study of Something Specific".to_string(),
    }];

    let lines = describe(
        &fixture.event(),
        "smith2024_raw.pdf",
        Some(&Proposal {
            target: "smith2024_TestingStudy.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "smith2024_raw.pdf"),
            label_line(
                "identifier",
                "doi:10.1234/smith.test.2024, from the text layer"
            ),
            label_line("record", "Crossref"),
            label_line("type", "journal article"),
            label_line("title", "A Concise Study of Something Specific"),
            label_line("authors", "John Smith, Jane Doe"),
            label_line("issued", "2024"),
            label_line("in", "Journal of Testing 12(3), 45-67"),
            label_line("file says", "A Concise Study of Something Specific (XMP)"),
            label_line("new name", "smith2024_TestingStudy.pdf"),
        ],
        "got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// A content-index answer: design D1's worked example
// ---------------------------------------------------------------------

#[test]
fn a_content_index_answer_is_d1s_worked_example() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some(
        "Applications of chiral sulfinyl auxiliaries in the asymmetric \
         synthesis of fluorinated amines and amino acids"
            .to_string(),
    );
    record.authors = vec![
        name("Nataliya V.", "Lyutenko"),
        name("Alexander E.", "Sorochinsky"),
        name("Vadim A.", "Soloshonok"),
    ];
    record.issued = Some(DateParts {
        year: 2023,
        month: None,
        day: None,
    });
    record.container_title = Some("Ukrainica Bioorganica Acta".to_string());
    record.volume = Some("18".to_string());
    record.issue = Some("1".to_string());
    record.pages = Some("10\u{2013}21".to_string());
    record.doi = Some(Doi::parse("10.15407/bioorganica2023.01.010").unwrap());
    record.borax = BoraxExt {
        provenance: [
            ("title".to_string(), Source::Crossref),
            ("author".to_string(), Source::Crossref),
        ]
        .into_iter()
        .collect(),
        ..BoraxExt::default()
    };

    let mut fixture = Fixture::new(record, "doi:10.15407/bioorganica2023.01.010");
    fixture.path = PathBuf::from("50-Article Text-95-2-10-20240507.pdf");
    fixture.tier = None;
    fixture.cached = true;
    fixture.claims = Vec::new();

    let lines = describe(
        &fixture.event(),
        "50-Article Text-95-2-10-20240507.pdf",
        Some(&Proposal {
            target: "lyutenko2023_ApplicationsChiralSulfinyl.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 3,
            total: 17,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(3, 17, DEFAULT_WIDTH),
            label_line("file", "50-Article Text-95-2-10-20240507.pdf"),
            // Nothing was looked up, so nothing is said about where
            // the identifier came from; the record is what came from
            // an earlier run.
            label_line("identifier", "doi:10.15407/bioorganica2023.01.010"),
            label_line("record", "Crossref, from an earlier run"),
            label_line("type", "journal article"),
            "title       Applications of chiral sulfinyl auxiliaries in the asymmetric".to_string(),
            continuation("synthesis of fluorinated amines and amino acids"),
            "authors     Nataliya V. Lyutenko, Alexander E. Sorochinsky, Vadim A.".to_string(),
            continuation("Soloshonok"),
            label_line("issued", "2023"),
            label_line("in", "Ukrainica Bioorganica Acta 18(1), 10\u{2013}21"),
            label_line("file says", "not read; an earlier run answered"),
            label_line("new name", "lyutenko2023_ApplicationsChiralSulfinyl.pdf"),
        ],
        "got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// A preprint with no container
// ---------------------------------------------------------------------

#[test]
fn a_preprint_with_no_container() {
    let mut record = Record::new(EntryType::Preprint);
    record.title = Some("A Preliminary Report on Something".to_string());
    record.authors = vec![name("Ann", "Preprint"), name("Bob", "Draft")];
    record.issued = Some(DateParts {
        year: 2024,
        month: None,
        day: None,
    });
    record.borax.arxiv = Some(ArxivId::parse("2401.01234").unwrap());

    let mut fixture = Fixture::new(record, "arXiv:2401.01234");
    fixture.path = PathBuf::from("/papers/draft.pdf");
    fixture.source = "arxiv".to_string();
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Info,
        title: "Untitled".to_string(),
    }];

    let lines = describe(
        &fixture.event(),
        "draft.pdf",
        Some(&Proposal {
            target: "preprint2024_PreliminaryReport.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 2,
            total: 5,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(2, 5, DEFAULT_WIDTH),
            label_line("file", "draft.pdf"),
            label_line("identifier", "arXiv:2401.01234, from the text layer"),
            label_line("record", "arXiv"),
            label_line("type", "preprint"),
            label_line("title", "A Preliminary Report on Something"),
            label_line("authors", "Ann Preprint, Bob Draft"),
            label_line("issued", "2024"),
            label_line("file says", "Untitled (document info)"),
            label_line("new name", "preprint2024_PreliminaryReport.pdf"),
        ],
        "a preprint with no container must carry no \"in\" line: got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// Five authors: three names, then a count
// ---------------------------------------------------------------------

#[test]
fn five_authors_show_three_names_and_a_count() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Collaborative Findings in Applied Science".to_string());
    record.authors = vec![
        name("Alice", "Adams"),
        name("Ben", "Brown"),
        name("Cara", "Clark"),
        name("Dan", "Davis"),
        name("Eve", "Evans"),
    ];
    record.issued = Some(DateParts {
        year: 2024,
        month: None,
        day: None,
    });
    record.container_title = Some("Journal of Collaboration".to_string());
    record.volume = Some("5".to_string());
    record.issue = Some("2".to_string());
    record.pages = Some("100-110".to_string());
    record.doi = Some(Doi::parse("10.9999/many.authors.2024").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.9999/many.authors.2024");
    fixture.path = PathBuf::from("/papers/many_authors.pdf");
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Xmp,
        title: "Collaborative Findings in Applied Science".to_string(),
    }];

    let lines = describe(
        &fixture.event(),
        "many_authors.pdf",
        Some(&Proposal {
            target: "adams2024_CollaborativeFindings.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "many_authors.pdf"),
            label_line(
                "identifier",
                "doi:10.9999/many.authors.2024, from the text layer"
            ),
            label_line("record", "Crossref"),
            label_line("type", "journal article"),
            label_line("title", "Collaborative Findings in Applied Science"),
            label_line("authors", "Alice Adams, Ben Brown, Cara Clark, and 2 more"),
            label_line("issued", "2024"),
            label_line("in", "Journal of Collaboration 5(2), 100-110"),
            label_line(
                "file says",
                "Collaborative Findings in Applied Science (XMP)"
            ),
            label_line("new name", "adams2024_CollaborativeFindings.pdf"),
        ],
        "a title is never truncated, but five authors are: got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// A suffixed proposal
// ---------------------------------------------------------------------

#[test]
fn a_suffixed_proposal_notes_the_rendered_name_that_was_taken() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Concise Study of Something Specific".to_string());
    record.authors = vec![name("John", "Smith"), name("Jane", "Doe")];
    record.issued = Some(DateParts {
        year: 2024,
        month: None,
        day: None,
    });
    record.container_title = Some("Journal of Testing".to_string());
    record.volume = Some("12".to_string());
    record.issue = Some("3".to_string());
    record.pages = Some("45-67".to_string());
    record.doi = Some(Doi::parse("10.1234/smith.test.2024").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.1234/smith.test.2024");
    fixture.path = PathBuf::from("/collection/library/smith2024_raw.pdf");
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Xmp,
        title: "A Concise Study of Something Specific".to_string(),
    }];

    let lines = describe(
        &fixture.event(),
        "smith2024_raw.pdf",
        Some(&Proposal {
            target: "smith2024a.pdf".to_string(),
            rendered: Some("smith2024.pdf".to_string()),
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "smith2024_raw.pdf"),
            label_line(
                "identifier",
                "doi:10.1234/smith.test.2024, from the text layer"
            ),
            label_line("record", "Crossref"),
            label_line("type", "journal article"),
            label_line("title", "A Concise Study of Something Specific"),
            label_line("authors", "John Smith, Jane Doe"),
            label_line("issued", "2024"),
            label_line("in", "Journal of Testing 12(3), 45-67"),
            label_line("file says", "A Concise Study of Something Specific (XMP)"),
            label_line("new name", "smith2024a.pdf"),
            continuation("(smith2024.pdf is taken)"),
        ],
        "got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// A title wrapped at width 60, hanging indent, nothing truncated
// ---------------------------------------------------------------------

#[test]
fn a_title_wraps_at_width_60_with_a_hanging_indent_and_nothing_truncated() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some(
        "A Systematic and Comprehensive Review of Crystallization \
         Behavior in Complex Polymer Blends"
            .to_string(),
    );
    record.doi = Some(Doi::parse("10.5555/wide.title.test").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.5555/wide.title.test");
    fixture.path = PathBuf::from("/papers/widetitle.pdf");
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Xmp,
        title: "Untitled draft".to_string(),
    }];

    let width = 60;
    let lines = describe(
        &fixture.event(),
        "widetitle.pdf",
        Some(&Proposal {
            target: "widetitle_2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        width,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, width),
            label_line("file", "widetitle.pdf"),
            label_line(
                "identifier",
                "doi:10.5555/wide.title.test, from the text layer"
            ),
            label_line("record", "Crossref"),
            label_line("type", "journal article"),
            label_line("title", "A Systematic and Comprehensive Review of"),
            continuation("Crystallization Behavior in Complex Polymer"),
            continuation("Blends"),
            label_line("file says", "Untitled draft (XMP)"),
            label_line("new name", "widetitle_2024.pdf"),
        ],
        "no word of the title may be cut short: got {lines:#?}"
    );

    let title_text: String = lines
        .iter()
        .skip_while(|line| !line.starts_with("title"))
        .take_while(|line| line.starts_with("title") || line.starts_with("            "))
        .flat_map(|line| line.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ");
    for word in [
        "Systematic",
        "Comprehensive",
        "Review",
        "Crystallization",
        "Behavior",
        "Complex",
        "Polymer",
        "Blends",
    ] {
        assert!(
            title_text.contains(word),
            "{word:?} must appear whole in the wrapped title, got {title_text:?}"
        );
    }
}

// ---------------------------------------------------------------------
// Fields the record does not hold are left out, not shown empty
// ---------------------------------------------------------------------

#[test]
fn missing_fields_are_left_out_rather_than_shown_empty() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Sparse Record With Few Fields".to_string());
    record.doi = Some(Doi::parse("10.1111/sparse.2024").unwrap());
    // No authors, no issued date, no container/volume/issue/pages, no
    // publisher: everything this test exists to check is left unset.

    let mut fixture = Fixture::new(record, "doi:10.1111/sparse.2024");
    fixture.path = PathBuf::from("/papers/sparse.pdf");
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Xmp,
        title: "Sparse Record With Few Fields".to_string(),
    }];

    let lines = describe(
        &fixture.event(),
        "sparse.pdf",
        Some(&Proposal {
            target: "sparse_SparseRecord.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "sparse.pdf"),
            label_line("identifier", "doi:10.1111/sparse.2024, from the text layer"),
            label_line("record", "Crossref"),
            label_line("type", "journal article"),
            label_line("title", "Sparse Record With Few Fields"),
            label_line("file says", "Sparse Record With Few Fields (XMP)"),
            label_line("new name", "sparse_SparseRecord.pdf"),
        ],
        "authors, issued and in must be left out (not shown empty) when \
         the record does not hold them: got {lines:#?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("authors")),
        "got {lines:#?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("issued")),
        "got {lines:#?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("in ") || line == "in"),
        "got {lines:#?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("publisher")),
        "publisher is never shown at all, per D1: got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// A producer's placeholder claim is shown, not filtered
// ---------------------------------------------------------------------

#[test]
fn a_producers_placeholder_claim_is_shown_not_filtered() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Genuine Title Of The Work".to_string());
    record.authors = vec![name("Pat", "Author")];
    record.issued = Some(DateParts {
        year: 2024,
        month: None,
        day: None,
    });
    record.container_title = Some("Journal X".to_string());
    record.volume = Some("1".to_string());
    record.issue = Some("1".to_string());
    record.pages = Some("1-10".to_string());
    record.doi = Some(Doi::parse("10.2222/placeholder.2024").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.2222/placeholder.2024");
    fixture.path = PathBuf::from("/papers/placeholder.pdf");
    // The check that would dismiss this as evidence runs after
    // extraction and never touches what `describe` is given: both
    // claims reach the description exactly as they were read.
    fixture.claims = vec![
        Claim {
            from: ClaimOrigin::Xmp,
            title: "Genuine Title".to_string(),
        },
        Claim {
            from: ClaimOrigin::Info,
            title: "Placeholder Doc".to_string(),
        },
    ];

    let lines = describe(
        &fixture.event(),
        "placeholder.pdf",
        Some(&Proposal {
            target: "author2024_GenuineTitle.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "placeholder.pdf"),
            label_line(
                "identifier",
                "doi:10.2222/placeholder.2024, from the text layer"
            ),
            label_line("record", "Crossref"),
            label_line("type", "journal article"),
            label_line("title", "Genuine Title Of The Work"),
            label_line("authors", "Pat Author"),
            label_line("issued", "2024"),
            label_line("in", "Journal X 1(1), 1-10"),
            label_line("file says", "Genuine Title (XMP)"),
            // A line each: two titles run together wrap into each
            // other, and telling them apart is why both are shown.
            continuation("Placeholder Doc (document info)"),
            label_line("new name", "author2024_GenuineTitle.pdf"),
        ],
        "the placeholder the conflict check would dismiss must still \
         appear: got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// What the reviewer gate found: evidence that would have been wrong,
// and a description a document could redraw
// ---------------------------------------------------------------------

/// The content index keeps records, not the identifiers they were
/// reached by. A record found last time from an arXiv identifier and
/// carrying a DOI must not be described as a DOI found in an earlier
/// run: nothing is known about where its identifier came from, and the
/// description says nothing rather than something false.
#[test]
fn a_cached_answer_names_no_origin_for_its_identifier() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Preprint That Was Published".to_string());
    record.authors = vec![name("Ada", "Byron")];
    record.doi = Some(Doi::parse("10.1234/published.2024").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.1234/published.2024");
    // What a content-index answer looks like: no pass ran, no service
    // was asked, and the claims were never read.
    fixture.tier = None;
    fixture.cached = true;
    fixture.source = "crossref".to_string();

    let lines = describe(
        &fixture.event(),
        "paper.pdf",
        Some(&Proposal {
            target: "byron2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("identifier", "doi:10.1234/published.2024")),
        "a cached answer must name the identifier without claiming where \
         it was found: got {lines:#?}"
    );
    assert!(
        lines.contains(&label_line("record", "Crossref, from an earlier run")),
        "the record is what came from an earlier run: got {lines:#?}"
    );
}

/// A PDF's title is written by whoever made the file. An escape
/// sequence in one, written to the terminal as it stands, could erase
/// the lines above it and redraw a different file and target over a
/// menu whose first choice is Rename.
#[test]
fn control_characters_in_a_title_are_shown_rather_than_acted_on() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Innocent Title".to_string());
    record.doi = Some(Doi::parse("10.1234/escape.2024").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.1234/escape.2024");
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Xmp,
        title: "\u{1b}[2J\u{1b}[Hfile        innocent.pdf".to_string(),
    }];

    let lines = describe(
        &fixture.event(),
        "paper.pdf",
        Some(&Proposal {
            target: "author2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    let text = lines.join("\n");
    assert!(
        !text.contains('\u{1b}'),
        "no escape may reach the terminal: got {text:?}"
    );
    assert!(
        text.contains("\\x1b[2J"),
        "the sequence is shown as text instead: got {text:?}"
    );
}

/// A record can hold a volume and pages and name no container. Every
/// field the run knows is shown; a container it does not have is the
/// only thing left out.
#[test]
fn a_volume_with_no_container_is_still_reported() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Published Somewhere Unnamed".to_string());
    record.volume = Some("12".to_string());
    record.pages = Some("45-67".to_string());
    record.doi = Some(Doi::parse("10.1234/nocontainer.2024").unwrap());

    let lines = describe(
        &Fixture::new(record, "doi:10.1234/nocontainer.2024").event(),
        "paper.pdf",
        Some(&Proposal {
            target: "author2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("in", "12, 45-67")),
        "a volume and pages are known even with no container to hold \
         them: got {lines:#?}"
    );
}

/// A record can hold text that is empty or only spaces. It is absent,
/// not a field to print a label for.
#[test]
fn a_blank_field_is_absent_rather_than_an_empty_label() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("   ".to_string());
    record.container_title = Some(String::new());
    record.authors = vec![name("Ada", "Byron")];
    record.doi = Some(Doi::parse("10.1234/blank.2024").unwrap());

    let lines = describe(
        &Fixture::new(record, "doi:10.1234/blank.2024").event(),
        "paper.pdf",
        Some(&Proposal {
            target: "byron.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        !lines.iter().any(|line| line.starts_with("title")),
        "a title of spaces is no title: got {lines:#?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("in")),
        "an empty container is no container: got {lines:#?}"
    );
}

/// Only the identifier is exempt from wrapping. A word longer than the
/// room — a long filename, typically — is broken rather than allowed to
/// run past the width and break the layout.
#[test]
fn a_word_longer_than_the_width_is_broken_rather_than_overrunning() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A".repeat(120));
    record.doi = Some(Doi::parse("10.1234/long.2024").unwrap());

    let lines = describe(
        &Fixture::new(record, "doi:10.1234/long.2024").event(),
        "paper.pdf",
        Some(&Proposal {
            target: "author2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    let overrunning: Vec<&String> = lines
        .iter()
        .filter(|line| line.chars().count() > DEFAULT_WIDTH)
        .filter(|line| !line.starts_with("identifier"))
        .collect();
    assert!(
        overrunning.is_empty(),
        "nothing but an identifier may pass the width: got {overrunning:#?}"
    );
}

// ---------------------------------------------------------------------
// Task 3.6a, design D8: the description renders whichever verdict the
// driver is holding, not only a resolution.
//
// These are red until `describe` learns `Event::Skipped`, which it
// currently falls through to `Vec::new()` for.
// ---------------------------------------------------------------------

/// A file whose identifier no service holds: the identifier that was
/// looked up, then what each service answered, one to a line under a
/// `no record` label that replaces the `record` line.
#[test]
fn a_file_no_service_holds_a_record_for_names_what_each_one_said() {
    let skipped = unresolvable_skip(
        "preprint-v2.pdf",
        "arXiv:2401.12345",
        "text-layer",
        vec![not_found("crossref"), not_found("openalex")],
    );

    let lines = describe(
        &skipped,
        "preprint-v2.pdf",
        None,
        Position {
            of_this: 4,
            total: 17,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(4, 17, DEFAULT_WIDTH),
            label_line("file", "preprint-v2.pdf"),
            label_line("identifier", "arXiv:2401.12345, from the text layer"),
            label_line("no record", "crossref: not found"),
            continuation("openalex: not found"),
            label_line("file says", "no title in its metadata"),
        ],
        "design D8's worked example, and no `new name` line: there is \
         no proposal to make for a file with no record"
    );
}

/// The same file when the services could not be reached rather than
/// answering: what each said is still what is shown, since that is the
/// difference the operator is being asked to judge.
#[test]
fn an_unreachable_service_is_shown_saying_what_it_said() {
    let skipped = unresolvable_skip(
        "preprint-v2.pdf",
        "arXiv:2401.12345",
        "embedded-metadata",
        vec![unavailable("arxiv", "timed out")],
    );

    let lines = describe(
        &skipped,
        "preprint-v2.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "preprint-v2.pdf"),
            label_line("identifier", "arXiv:2401.12345, from embedded metadata"),
            label_line("no record", "arxiv: unavailable: timed out"),
            label_line("file says", "no title in its metadata"),
        ]
    );
}

/// A file with no identifier at all names the extraction failure and
/// what the file's titles say (design D10's table), and nothing else.
#[test]
fn a_file_with_no_identifier_describes_only_itself() {
    let skipped = extraction_failure_skip(
        "scanned.pdf",
        SkipReason::TextWithoutIdentifier,
        ExtractionResultStep::TextWithoutIdentifier,
    );

    let lines = describe(
        &skipped,
        "scanned.pdf",
        None,
        Position {
            of_this: 2,
            total: 9,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(2, 9, DEFAULT_WIDTH),
            label_line("file", "scanned.pdf"),
            label_line("identifier", "none found in its metadata or the pages read"),
            label_line("file says", "no title in its metadata"),
        ]
    );
}

/// A conflict being asked about: both titles are already on the
/// layout, as the record's `title` and the file's `file says`, so the
/// `conflict` line carries only how close they were.
#[test]
fn a_conflict_asked_about_shows_how_close_the_two_titles_were() {
    let not_attempted = || "refused".to_string();
    let mut candidate = Record::new(EntryType::Article);
    candidate.title = Some("Asymmetric Synthesis of Fluorinated Amines".to_string());
    let skipped = Event::Skipped {
        path: PathBuf::from("paper.pdf"),
        reason: SkipReason::Conflict,
        sections: Some(Box::new(Sections {
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
                result: ExtractionResultStep::Found {
                    identifier: "doi:10.1000/conflict".to_string(),
                    tier: "text-layer".to_string(),
                },
                titles: TitlesStep::Read {
                    claims: vec![Claim {
                        from: ClaimOrigin::Xmp,
                        title: "Preliminary Notes on Solvent Effects".to_string(),
                    }],
                },
            },
            lookup: LookupStep::Attempted {
                identifier: "doi:10.1000/conflict".to_string(),
                origin: IdentifierOrigin::Extracted,
                attempts: vec![ServiceAnswer {
                    service: "crossref".to_string(),
                    outcome: ServiceOutcome::Found {
                        retrieval: FetchedFrom::Network,
                        stored: Some(WriteStep::Written),
                    },
                }],
            },
            record_retrieval: Some(RetrievedFrom::Network {
                service: "crossref".to_string(),
            }),
            match_check: MatchCheckStep::Conflict {
                field: "title".to_string(),
                extracted: "Preliminary Notes on Solvent Effects".to_string(),
                resolved: "Asymmetric Synthesis of Fluorinated Amines".to_string(),
                similarity: 0.08,
            },
            acceptance: Acceptance::NotApplicable,
        })),
        candidate: Some(Box::new(candidate)),
    };

    let lines = describe(
        &skipped,
        "paper.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "paper.pdf"),
            label_line("title", "Asymmetric Synthesis of Fluorinated Amines"),
            label_line("file says", "Preliminary Notes on Solvent Effects"),
            label_line("conflict", "titles 8% alike"),
        ],
        "a skipped conflict carries the two titles and the similarity \
         and nothing else — there is no record on the event to draw the \
         rest of the layout from"
    );
}

/// A conflict the operator accepted, reported on the record that
/// accepted it: the same `conflict` line, on a full resolved layout.
#[test]
fn an_overridden_conflict_shows_the_same_line_on_the_record_it_accepted() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("Asymmetric Synthesis of Fluorinated Amines".to_string());
    record.doi = Some(Doi::parse("10.1021/jacs.4c01234").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.1021/jacs.4c01234");
    fixture.tier = Some("embedded-metadata".to_string());
    fixture.claims = vec![Claim {
        from: ClaimOrigin::Info,
        title: "Preliminary Notes on Solvent Effects".to_string(),
    }];
    let Event::Resolved {
        path,
        identifier,
        record,
        mut sections,
    } = fixture.event()
    else {
        unreachable!("the fixture builds a resolved event")
    };
    sections.match_check = MatchCheckStep::Conflict {
        field: "title".to_string(),
        extracted: "Preliminary Notes on Solvent Effects".to_string(),
        resolved: "Asymmetric Synthesis of Fluorinated Amines".to_string(),
        similarity: 0.08,
    };
    sections.acceptance = Acceptance::Overridden;
    let resolved = Event::Resolved {
        path,
        identifier,
        record,
        sections,
    };

    let lines = describe(
        &resolved,
        "paper.pdf",
        Some(&Proposal {
            target: "jacs2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("conflict", "titles 8% alike")),
        "one renderer serves the question and the record that accepted \
         it: got {lines:#?}"
    );
}

/// An identifier the operator supplied says so where a pass's name
/// would go — a bare `supplied`, since the other values in that slot
/// name where the identifier was read and this one names that it was
/// not read at all.
#[test]
fn a_supplied_identifier_says_supplied_where_a_pass_would_be_named() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Published Version".to_string());
    record.doi = Some(Doi::parse("10.1021/jacs.4c01234").unwrap());

    let mut fixture = Fixture::new(record, "doi:10.1021/jacs.4c01234");
    fixture.tier = Some("supplied".to_string());

    let lines = describe(
        &fixture.event(),
        "paper.pdf",
        Some(&Proposal {
            target: "jacs2024.pdf".to_string(),
            rendered: None,
        }),
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line(
            "identifier",
            "doi:10.1021/jacs.4c01234, supplied"
        )),
        "got {lines:#?}"
    );
}

// ---------------------------------------------------------------------
// consult-library-first, task 5.2: the description names a library
// answer, and states a library problem (design D5)
// ---------------------------------------------------------------------

fn tracked_answer() -> LibraryAnswer {
    LibraryAnswer::Tracked {
        artifact: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e6f".to_string(),
        item: "0192a1b2-c3d4-75e6-8f70-1a2b3c4d5e70".to_string(),
    }
}

/// `fixture.event()` with `tier` and `library` overridden, following
/// the shape of the resolved event a library answer actually produces:
/// `tier: "library"`, `cached: false`, `claims: []`.
fn library_resolved_event(fixture: &Fixture, answer: LibraryAnswer) -> Event {
    let LibraryAnswer::Tracked { artifact, item } = &answer else {
        unreachable!("library_resolved_event is only called with a Tracked answer")
    };
    let not_attempted = || "library-answered".to_string();
    let record = record_with_source_provenance(&fixture.record, &fixture.source);
    Event::Resolved {
        path: fixture.path.clone(),
        identifier: fixture.found.clone(),
        record: Box::new(record),
        sections: Box::new(Sections {
            library: LibraryStep::Consulted {
                answer: answer.clone(),
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
            lookup: LookupStep::NotAttempted {
                reason: not_attempted(),
            },
            record_retrieval: Some(RetrievedFrom::Library {
                artifact: artifact.clone(),
                item: item.clone(),
            }),
            match_check: MatchCheckStep::NotAttempted {
                reason: not_attempted(),
            },
            acceptance: Acceptance::Automatic,
        }),
    }
}

/// design D5: `identifier` for a library answer names `found` with no
/// origin clause — `whence("library")` is `None`, since nothing was
/// looked up.
#[test]
fn identifier_line_for_a_library_answer_has_no_origin_clause() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Corrected Title".to_string());
    record.doi = Some(Doi::parse("10.1000/library-answer").unwrap());
    let fixture = Fixture::new(record, "doi:10.1000/library-answer");
    let event = library_resolved_event(&fixture, tracked_answer());

    let lines = describe(
        &event,
        "paper.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("identifier", "doi:10.1000/library-answer")),
        "got {lines:#?}"
    );
}

/// design D5: `record` reads `<services>, from the library`.
#[test]
fn record_line_for_a_library_answer_names_the_service_and_the_library() {
    let record = Record::new(EntryType::Article);
    let mut fixture = Fixture::new(record, "doi:10.1000/library-answer-2");
    fixture.source = "crossref".to_string();
    let event = library_resolved_event(&fixture, tracked_answer());

    let lines = describe(
        &event,
        "paper.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("record", "Crossref, from the library")),
        "got {lines:#?}"
    );
}

/// design D5: when `source` is `library` alone (a provenance-less
/// item), `record` reads just `the library`.
#[test]
fn record_line_for_a_provenance_less_library_answer_names_only_the_library() {
    let record = Record::new(EntryType::Article);
    let mut fixture = Fixture::new(record, "doi:10.1000/library-answer-3");
    fixture.source = "library".to_string();
    let event = library_resolved_event(&fixture, tracked_answer());

    let lines = describe(
        &event,
        "paper.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("record", "the library")),
        "got {lines:#?}"
    );
}

/// design D10: `file says` reads `not read; the library answered` — the
/// file was not opened, and the state names why (D14's mapping: the
/// schema-3 `nothing read` line becomes D10's state line).
#[test]
fn file_says_line_for_a_library_answer_reads_not_read_the_library_answered() {
    let record = Record::new(EntryType::Article);
    let fixture = Fixture::new(record, "doi:10.1000/library-answer-4");
    let event = library_resolved_event(&fixture, tracked_answer());

    let lines = describe(
        &event,
        "paper.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert!(
        lines.contains(&label_line("file says", "not read; the library answered")),
        "got {lines:#?}"
    );
}

/// design D5: a problem kind adds a `library` line, following `record`,
/// carrying D5's `<what>` wording.
#[test]
fn a_library_problem_on_a_resolved_event_adds_a_library_line_after_record() {
    let record = Record::new(EntryType::Article);
    let mut fixture = Fixture::new(record, "doi:10.1000/library-problem");
    fixture.tier = Some("text-layer".to_string());
    fixture.cached = false;
    let Event::Resolved {
        path,
        identifier,
        record,
        mut sections,
    } = fixture.event()
    else {
        unreachable!()
    };
    sections.library = LibraryStep::Consulted {
        answer: LibraryAnswer::DanglingItem {
            artifact: "a".to_string(),
            item: "b".to_string(),
        },
    };
    let event = Event::Resolved {
        path,
        identifier,
        record,
        sections,
    };

    let lines = describe(
        &event,
        "paper.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    let record_index = lines
        .iter()
        .position(|line| line.starts_with("record"))
        .unwrap_or_else(|| panic!("no record line: {lines:#?}"));
    assert_eq!(
        lines.get(record_index + 1),
        Some(&label_line(
            "library",
            "artifact a links to item b, which the library does not hold"
        )),
        "got {lines:#?}"
    );
}

/// design D5: on a failed file, the `library` line follows the
/// reason's own lines.
#[test]
fn a_library_problem_on_a_skipped_event_follows_the_reasons_lines() {
    let Event::Skipped {
        path,
        reason,
        mut sections,
        candidate,
    } = extraction_failure_skip(
        "scanned.pdf",
        SkipReason::TextWithoutIdentifier,
        ExtractionResultStep::TextWithoutIdentifier,
    )
    else {
        unreachable!()
    };
    sections.as_mut().unwrap().library = LibraryStep::Consulted {
        answer: LibraryAnswer::UnrecognisedContent {
            artifacts: vec!["a".to_string()],
        },
    };
    let event = Event::Skipped {
        path,
        reason,
        sections,
        candidate,
    };

    let lines = describe(
        &event,
        "scanned.pdf",
        None,
        Position {
            of_this: 1,
            total: 1,
        },
        DEFAULT_WIDTH,
    );

    assert_eq!(
        lines,
        vec![
            rule(1, 1, DEFAULT_WIDTH),
            label_line("file", "scanned.pdf"),
            label_line("identifier", "none found in its metadata or the pages read"),
            label_line("file says", "no title in its metadata"),
            label_line(
                "library",
                "the library records artifact a at this path, but not these bytes"
            ),
        ],
        "got {lines:#?}"
    );
}
