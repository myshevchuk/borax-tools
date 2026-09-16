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
use borax::event::{Claim, ClaimOrigin, Event};
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

    fn event(&self) -> Event {
        Event::Resolved {
            path: self.path.clone(),
            identifier: self.found.clone(),
            record: Box::new(self.record.clone()),
            source: self.source.clone(),
            found: self.found.clone(),
            claims: self.claims.clone(),
            tier: self.tier.clone(),
            overrode: None,
            cached: self.cached,
        }
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
        &Proposal {
            target: "smith2024_TestingStudy.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "lyutenko2023_ApplicationsChiralSulfinyl.pdf".to_string(),
            rendered: None,
        },
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
            label_line("file says", "nothing read"),
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
        &Proposal {
            target: "preprint2024_PreliminaryReport.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "adams2024_CollaborativeFindings.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "smith2024a.pdf".to_string(),
            rendered: Some("smith2024.pdf".to_string()),
        },
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
        &Proposal {
            target: "widetitle_2024.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "sparse_SparseRecord.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "author2024_GenuineTitle.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "byron2024.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "author2024.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "author2024.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "byron.pdf".to_string(),
            rendered: None,
        },
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
        &Proposal {
            target: "author2024.pdf".to_string(),
            rendered: None,
        },
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
