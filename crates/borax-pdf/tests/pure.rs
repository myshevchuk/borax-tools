#![allow(clippy::unwrap_used)]

use std::io::Write;

use borax_core::identifier::Doi;
use borax_pdf::pure::PurePdf;
use borax_pdf::scan::FoundIdentifier;
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_pdf::tiered::{ExtractionConfig, Tier, extract};

use lopdf::content::{Content, Operation};
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    StringFormat, dictionary,
};

// ---------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------

/// A base-14 Helvetica font resource, so `pdf-extract` can decode the
/// glyphs a content stream shows without an embedded font program.
fn helvetica_font(doc: &mut Document) -> lopdf::ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    })
}

/// A page showing `text` in Helvetica, added under `pages_id`.
fn page_with_text(
    doc: &mut Document,
    pages_id: lopdf::ObjectId,
    resources_id: lopdf::ObjectId,
    text: &str,
) -> lopdf::ObjectId {
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Resources" => resources_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    })
}

/// A page with an empty content stream: no text-showing operators at
/// all, the shape a scan without OCR produces.
fn blank_page(doc: &mut Document, pages_id: lopdf::ObjectId) -> lopdf::ObjectId {
    let content = Content { operations: vec![] };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    })
}

/// A minimal PDF with one page per entry in `page_texts`, an optional
/// Info dictionary, and an optional XMP `/Metadata` stream on the
/// catalog.
fn build_pdf(page_texts: &[&str], info: Option<Dictionary>, xmp: Option<&str>) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = helvetica_font(&mut doc);
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let page_ids: Vec<lopdf::ObjectId> = page_texts
        .iter()
        .map(|text| page_with_text(&mut doc, pages_id, resources_id, text))
        .collect();

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|&id| id.into()).collect::<Vec<_>>(),
        "Count" => page_ids.len() as i64,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    };
    if let Some(xmp) = xmp {
        let meta_id = doc.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            xmp.as_bytes().to_vec(),
        ));
        catalog.set("Metadata", meta_id);
    }
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", catalog_id);

    if let Some(info) = info {
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", info_id);
    }

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// A single-page PDF whose page has no text-showing operators at all.
fn build_blank_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = blank_page(&mut doc, pages_id);
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// A single-page PDF whose only font is a Type0 (composite) font
/// declaring `/Encoding /UCS-2` — a name `pdf-extract` 0.12 does not
/// implement (it only handles `Identity-H`/`Identity-V` by name, or an
/// embedded CMap stream). Resolving this font panics inside the
/// third-party parser rather than returning an error, which is exactly
/// the case `PurePdf`'s panic guard exists for: one malformed file
/// must not unwind past its own extraction and abort a whole batch.
fn build_pdf_with_unsupported_type0_encoding() -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let descendant_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "Broken",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
    });
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "Broken",
        "Encoding" => "UCS-2",
        "DescendantFonts" => vec![Object::Reference(descendant_id)],
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("Tj", vec![Object::string_literal("hi")]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Resources" => resources_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// An Info-dictionary string value encoded the way PDF text strings
/// carrying non-Latin1 text are: a UTF-16BE byte-order mark followed by
/// UTF-16BE code units.
fn utf16be_bom(s: &str) -> Object {
    let mut bytes = vec![0xFEu8, 0xFF];
    bytes.extend(s.encode_utf16().flat_map(u16::to_be_bytes));
    Object::String(bytes, StringFormat::Literal)
}

/// An Info dictionary carrying the four named fields, the five
/// "produced by" fields the model excludes from `custom`, and two
/// non-standard keys the way publishers stash identifiers in them.
fn full_info_dict() -> Dictionary {
    let mut info = Dictionary::new();
    info.set("Title", Object::string_literal("Full Metadata Fixture"));
    info.set("Author", Object::string_literal("Ada Lovelace"));
    info.set("Subject", Object::string_literal("Testing PurePdf"));
    info.set("Keywords", Object::string_literal("doi, pdf, fixture"));
    info.set("Producer", Object::string_literal("scratch-writer 1.0"));
    info.set("Creator", Object::string_literal("scratch-writer"));
    info.set("CreationDate", Object::string_literal("D:20240101000000Z"));
    info.set("ModDate", Object::string_literal("D:20240101000000Z"));
    info.set("Trapped", Object::Name(b"False".to_vec()));
    info.set("doi", Object::string_literal("10.1234/example.doi"));
    info.set("WPS-ARTICLEDOI", Object::string_literal("10.5678/wps.doi"));
    info
}

/// Encrypt `bytes` (a document produced by [`build_pdf`]) under RC4
/// with the given owner and user passwords, permissions-only
/// otherwise. `lopdf`'s encryption machinery needs a file `/ID`, which
/// [`build_pdf`] does not set, so this adds one before encrypting.
fn encrypt(bytes: &[u8], owner_password: &str, user_password: &str) -> Vec<u8> {
    let mut doc = Document::load_mem(bytes).unwrap();
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal(b"0123456789ABCDEF".to_vec()),
            Object::string_literal(b"0123456789ABCDEF".to_vec()),
        ],
    );
    let state: EncryptionState = EncryptionVersion::V2 {
        document: &doc,
        owner_password,
        user_password,
        key_length: 128,
        permissions: Permissions::empty(),
    }
    .try_into()
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut encrypted = Vec::new();
    doc.save_to(&mut encrypted).unwrap();
    encrypted
}

/// Write `bytes` to a fresh temp file and return its path, so
/// [`PurePdf::open`] can be exercised against a real file. The file is
/// removed when the returned `TempPath` drops.
fn write_temp_pdf(bytes: &[u8]) -> tempfile::TempPath {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file.into_temp_path()
}

fn doi(s: &str) -> Doi {
    Doi::parse(s).unwrap()
}

// ---------------------------------------------------------------------
// Page count and page text
// ---------------------------------------------------------------------

#[test]
fn page_count_reflects_the_number_of_pages_in_the_document() {
    let bytes = build_pdf(&["one", "two", "three"], None, None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    assert_eq!(pdf.page_count(), 3);
}

#[test]
fn page_text_is_zero_based_and_distinguishes_pages() {
    let bytes = build_pdf(
        &[
            "first page has unique-marker-alpha in it",
            "second page has unique-marker-beta in it",
        ],
        None,
        None,
    );
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    let first = pdf.page_text(0).unwrap();
    let second = pdf.page_text(1).unwrap();

    assert!(first.contains("unique-marker-alpha"), "got {first:?}");
    assert!(second.contains("unique-marker-beta"), "got {second:?}");
    assert_ne!(first, second);
}

// ---------------------------------------------------------------------
// Info metadata: the four named fields
// ---------------------------------------------------------------------

#[test]
fn info_metadata_maps_the_four_named_fields() {
    let bytes = build_pdf(&["page"], Some(full_info_dict()), None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();
    let info = pdf.info_metadata();

    assert_eq!(info.title.as_deref(), Some("Full Metadata Fixture"));
    assert_eq!(info.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(info.subject.as_deref(), Some("Testing PurePdf"));
    assert_eq!(info.keywords.as_deref(), Some("doi, pdf, fixture"));
}

#[test]
fn utf16be_with_bom_info_string_decodes_correctly() {
    let mut info = Dictionary::new();
    info.set("Title", utf16be_bom("Café résumé — 10.0/naïve"));
    let bytes = build_pdf(&["page"], Some(info), None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    assert_eq!(
        pdf.info_metadata().title.as_deref(),
        Some("Café résumé — 10.0/naïve")
    );
}

#[test]
fn missing_info_dictionary_yields_default_metadata() {
    let bytes = build_pdf(&["page"], None, None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    assert_eq!(pdf.info_metadata(), &InfoMetadata::default());
}

// ---------------------------------------------------------------------
// Info metadata: custom keys
// ---------------------------------------------------------------------

#[test]
fn nonstandard_info_keys_land_in_custom_by_their_name_in_the_file() {
    let bytes = build_pdf(&["page"], Some(full_info_dict()), None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();
    let info = pdf.info_metadata();

    assert_eq!(
        info.custom.get("doi").map(String::as_str),
        Some("10.1234/example.doi")
    );
    assert_eq!(
        info.custom.get("WPS-ARTICLEDOI").map(String::as_str),
        Some("10.5678/wps.doi")
    );
}

#[test]
fn produced_by_info_keys_do_not_leak_into_custom() {
    let bytes = build_pdf(&["page"], Some(full_info_dict()), None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();
    let info = pdf.info_metadata();

    for key in ["Producer", "Creator", "CreationDate", "ModDate", "Trapped"] {
        assert!(!info.custom.contains_key(key), "{key} leaked into custom");
    }
    assert!(
        info.custom.contains_key("doi"),
        "doi should still land in custom"
    );
}

// ---------------------------------------------------------------------
// XMP
// ---------------------------------------------------------------------

#[test]
fn xmp_returns_the_catalog_metadata_stream_content() {
    let xmp = "<x:xmpmeta><rdf:RDF><prism:doi>10.9999/xmp-doi</prism:doi></rdf:RDF></x:xmpmeta>";
    let bytes = build_pdf(&["page"], None, Some(xmp));
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    assert_eq!(pdf.xmp(), Some(xmp));
}

#[test]
fn xmp_is_none_without_a_metadata_stream() {
    let bytes = build_pdf(&["page"], None, None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    assert_eq!(pdf.xmp(), None);
}

// ---------------------------------------------------------------------
// Unreadable input
// ---------------------------------------------------------------------

#[test]
fn from_bytes_on_non_pdf_data_is_unreadable() {
    let result = PurePdf::from_bytes(b"not a pdf file at all, just plain bytes");

    assert!(matches!(result, Err(ExtractionError::Unreadable { .. })));
}

#[test]
fn open_on_a_path_that_does_not_exist_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.pdf");

    let result = PurePdf::open(&missing);

    assert!(matches!(result, Err(ExtractionError::Unreadable { .. })));
}

// ---------------------------------------------------------------------
// open() and from_bytes() agreement
// ---------------------------------------------------------------------

#[test]
fn open_and_from_bytes_agree_on_the_same_document() {
    let bytes = build_pdf(
        &["first page 10.1111/a", "second page 10.2222/b"],
        Some(full_info_dict()),
        Some("<prism:doi>10.9999/xmp-doi</prism:doi>"),
    );
    let path = write_temp_pdf(&bytes);

    let from_path = PurePdf::open(&path).unwrap();
    let from_mem = PurePdf::from_bytes(&bytes).unwrap();

    assert_eq!(from_path.page_count(), from_mem.page_count());
    assert_eq!(from_path.info_metadata(), from_mem.info_metadata());
    assert_eq!(from_path.xmp(), from_mem.xmp());
    for index in 0..from_path.page_count() {
        assert_eq!(
            from_path.page_text(index).unwrap(),
            from_mem.page_text(index).unwrap()
        );
    }
}

// ---------------------------------------------------------------------
// Encryption
// ---------------------------------------------------------------------

#[test]
fn encrypted_with_a_nonempty_user_password_is_reported_encrypted() {
    let bytes = build_pdf(&["secret content 10.3333/enc-doi"], None, None);
    let encrypted = encrypt(&bytes, "owner-pw", "user-pw");

    let result = PurePdf::from_bytes(&encrypted);

    assert_eq!(result.err(), Some(ExtractionError::Encrypted));
}

#[test]
fn encrypted_with_an_empty_user_password_opens_transparently() {
    let bytes = build_pdf(
        &["visible content 10.4444/perm-doi"],
        Some(full_info_dict()),
        None,
    );
    let encrypted = encrypt(&bytes, "owner-pw", "");

    let pdf = PurePdf::from_bytes(&encrypted).unwrap();

    assert_eq!(pdf.page_count(), 1);
    assert!(pdf.page_text(0).unwrap().contains("10.4444/perm-doi"));
    assert_eq!(pdf.info_metadata().author.as_deref(), Some("Ada Lovelace"));
}

// ---------------------------------------------------------------------
// Tiered pipeline integration
// ---------------------------------------------------------------------

#[test]
fn tiered_extract_finds_a_doi_on_the_text_layer_of_a_pure_pdf() {
    let bytes = build_pdf(
        &["cover page mentions 10.5555/text-layer-doi here"],
        None,
        None,
    );
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    let result = extract(&pdf, &ExtractionConfig::default()).unwrap();

    assert_eq!(result.tier, Tier::TextLayer);
    assert_eq!(
        result.identifier,
        FoundIdentifier::Doi(doi("10.5555/text-layer-doi"))
    );
}

#[test]
fn tiered_extract_finds_a_doi_in_the_info_dictionary_of_a_pure_pdf() {
    let mut info = Dictionary::new();
    info.set("doi", Object::string_literal("10.6666/info-doi"));
    let bytes = build_pdf(&["no identifier mentioned on this page"], Some(info), None);
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    let result = extract(&pdf, &ExtractionConfig::default()).unwrap();

    assert_eq!(result.tier, Tier::EmbeddedMetadata);
    assert_eq!(
        result.identifier,
        FoundIdentifier::Doi(doi("10.6666/info-doi"))
    );
}

// ---------------------------------------------------------------------
// Pages without a text layer
// ---------------------------------------------------------------------

#[test]
fn page_with_no_text_operators_yields_blank_text() {
    let bytes = build_blank_pdf();
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    let text = pdf.page_text(0).unwrap();

    assert!(
        text.chars().all(|c| c.is_ascii_whitespace()),
        "expected blank text, got {text:?}"
    );
}

#[test]
fn tiered_extract_reports_no_text_layer_for_a_blank_pure_pdf() {
    let bytes = build_blank_pdf();
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoTextLayer));
}

// ---------------------------------------------------------------------
// Panic containment
// ---------------------------------------------------------------------

#[test]
fn a_font_the_text_extractor_panics_on_is_reported_unreadable_not_a_process_abort() {
    let bytes = build_pdf_with_unsupported_type0_encoding();
    let pdf = PurePdf::from_bytes(&bytes).unwrap();

    let result = pdf.page_text(0);

    assert!(matches!(result, Err(ExtractionError::Unreadable { .. })));
}
