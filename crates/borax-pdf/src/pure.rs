//! The default [`PdfSource`] backend: pure Rust, no native engine.
//!
//! "Pure" is about the build, not the parsing: this backend reads PDFs
//! with `lopdf` and `pdf-extract`, so `cargo install borax` needs no C
//! toolchain, no downloaded binaries and no per-platform packaging.
//! Fidelity on malformed publisher PDFs is what that costs, and the
//! optional pdfium backend is where it is bought back.
//!
//! Third-party parsing runs inside a panic guard. The parsers panic on
//! some malformed input, and a batch must survive one bad file, so a
//! panic is caught and reported as [`ExtractionError::Unreadable`]
//! rather than taking the process down.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, decode_text_string};
use pdf_extract::{PlainTextOutput, output_doc_page};

use crate::source::{ExtractionError, InfoMetadata, PdfSource};

/// A PDF read by the pure-Rust engine.
///
/// Metadata is read once at open time; page text is extracted on
/// demand, so a document whose identifier sits in its Info dictionary
/// costs no text extraction at all.
///
/// Reading a page's text rewrites that page's content stream in the
/// in-memory document, which is why the document sits behind a
/// [`RefCell`]. Nothing is written back to the file.
pub struct PurePdf {
    document: RefCell<Document>,
    /// The document's pages in order, each as the number `lopdf` gives
    /// it paired with the object holding it. The index a caller passes
    /// addresses this list, not the numbering: a document whose page
    /// tree skips numbers still has pages `0..page_count`.
    pages: Vec<(u32, ObjectId)>,
    info: InfoMetadata,
    xmp: Option<String>,
}

impl PurePdf {
    /// Read the PDF at `path`.
    ///
    /// Fails with [`ExtractionError::Unreadable`] when the file cannot
    /// be read or does not parse as a PDF, and with
    /// [`ExtractionError::Encrypted`] when it is encrypted under a
    /// password. Encryption that only encodes permissions — an empty
    /// user password, which is what most publisher PDFs carry — is
    /// transparent: the document opens normally.
    pub fn open(path: &Path) -> Result<PurePdf, ExtractionError> {
        PurePdf::from_bytes(&fs::read(path).map_err(unreadable)?)
    }

    /// Read a PDF already held in memory.
    ///
    /// Fails exactly as [`PurePdf::open`] does, minus the I/O.
    pub fn from_bytes(bytes: &[u8]) -> Result<PurePdf, ExtractionError> {
        guard_panic(|| {
            let mut document = Document::load_mem(bytes).map_err(unreadable)?;
            if document.is_encrypted() {
                document
                    .decrypt("")
                    .map_err(|_| ExtractionError::Encrypted)?;
            }
            Ok(PurePdf {
                pages: document.get_pages().into_iter().collect(),
                info: read_info(&document),
                xmp: read_xmp(&document),
                document: RefCell::new(document),
            })
        })
    }
}

impl PdfSource for PurePdf {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn info_metadata(&self) -> &InfoMetadata {
        &self.info
    }

    fn xmp(&self) -> Option<&str> {
        self.xmp.as_deref()
    }

    fn page_text(&self, index: usize) -> Result<String, ExtractionError> {
        let Some(&(number, page)) = self.pages.get(index) else {
            return Ok(String::new());
        };
        guard_panic(|| {
            // The text extractor implements neither `'` nor `"`, so a
            // page is rewritten into the operators it does implement
            // first. Doing it here rather than at open time keeps the
            // cost off documents whose text is never read.
            expand_text_showing_shorthands(&mut self.document.borrow_mut(), page);
            let mut text = String::new();
            let mut output = PlainTextOutput::new(&mut text);
            output_doc_page(&self.document.borrow(), &mut output, number).map_err(unreadable)?;
            Ok(text)
        })
    }
}

/// Rewrite the `'` and `"` text-showing operators on page `page` of
/// `document` into the sequences the PDF specification defines them as:
///
/// - `string '` becomes `T*` then `string Tj`
/// - `aw ac string "` becomes `aw Tw`, `ac Tc`, `T*`, then `string Tj`
///
/// The rewrite is confined to the in-memory document, and repeating it
/// over the same page yields the same content stream.
///
/// A page whose content stream cannot be decoded, re-encoded or written
/// back is left as it stands, as is a `"` carrying other than the three
/// operands the specification gives it.
fn expand_text_showing_shorthands(document: &mut Document, page: ObjectId) {
    let Ok(content) = document.get_and_decode_page_content(page) else {
        return;
    };
    if !content
        .operations
        .iter()
        .any(|operation| matches!(operation.operator.as_str(), "'" | "\""))
    {
        return;
    }

    let mut operations = Vec::with_capacity(content.operations.len());
    for operation in content.operations {
        match operation.operator.as_str() {
            "'" => {
                operations.push(Operation::new("T*", Vec::new()));
                operations.push(Operation::new("Tj", operation.operands));
            }
            "\"" => match <[Object; 3]>::try_from(operation.operands) {
                Ok([word_spacing, character_spacing, string]) => {
                    operations.push(Operation::new("Tw", vec![word_spacing]));
                    operations.push(Operation::new("Tc", vec![character_spacing]));
                    operations.push(Operation::new("T*", Vec::new()));
                    operations.push(Operation::new("Tj", vec![string]));
                }
                Err(operands) => operations.push(Operation::new("\"", operands)),
            },
            _ => operations.push(operation),
        }
    }

    if let Ok(encoded) = (Content { operations }).encode() {
        let _ = document.change_page_content(page, encoded);
    }
}

/// Info-dictionary keys that never become `custom` entries: the four
/// the model names, plus the produced-by fields, which carry authoring
/// tool names rather than anything about the work.
const STANDARD_INFO_KEYS: [&[u8]; 9] = [
    b"Title",
    b"Author",
    b"Subject",
    b"Keywords",
    b"Creator",
    b"Producer",
    b"CreationDate",
    b"ModDate",
    b"Trapped",
];

/// Read the document-information dictionary.
///
/// A document without one, or with one that does not parse, yields the
/// default (empty) metadata: absent metadata is not a failure, it is
/// the next pass's turn. An entry present but empty reads as absent.
fn read_info(document: &Document) -> InfoMetadata {
    let Some(dictionary) = info_dictionary(document) else {
        return InfoMetadata::default();
    };
    InfoMetadata {
        title: info_string(dictionary, b"Title"),
        author: info_string(dictionary, b"Author"),
        subject: info_string(dictionary, b"Subject"),
        keywords: info_string(dictionary, b"Keywords"),
        custom: custom_info_entries(dictionary),
    }
}

/// The Info dictionary, whether the trailer holds it by reference or
/// inline.
fn info_dictionary(document: &Document) -> Option<&Dictionary> {
    let entry = document.trailer.get(b"Info").ok()?;
    document.dereference(entry).ok()?.1.as_dict().ok()
}

/// The value of `key` as text, absent when the key is missing, is not a
/// string, or decodes to nothing.
fn info_string(dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    let value = decode_text_string(dictionary.get(key).ok()?).ok()?;
    (!value.is_empty()).then_some(value)
}

/// Custom Info-dictionary entries, keyed by the name as it appears in
/// the file.
fn custom_info_entries(dictionary: &Dictionary) -> BTreeMap<String, String> {
    dictionary
        .iter()
        .filter(|(key, _)| !STANDARD_INFO_KEYS.contains(&key.as_slice()))
        .filter_map(|(key, value)| {
            let text = decode_text_string(value).ok().filter(|it| !it.is_empty())?;
            Some((String::from_utf8_lossy(key).into_owned(), text))
        })
        .collect()
}

/// Read the XMP packet from the catalog's `/Metadata` stream.
///
/// `None` when the document carries no packet or its stream cannot be
/// decompressed. Bytes are taken as UTF-8, which XMP mandates, with
/// invalid sequences replaced rather than rejected.
fn read_xmp(document: &Document) -> Option<String> {
    let entry = document.catalog().ok()?.get(b"Metadata").ok()?;
    let stream = document.dereference(entry).ok()?.1.as_stream().ok()?;
    Some(String::from_utf8_lossy(&stream.decompressed_content().ok()?).into_owned())
}

/// Run `parse`, converting a panic from the PDF parsers into an
/// [`ExtractionError::Unreadable`].
fn guard_panic<T>(
    parse: impl FnOnce() -> Result<T, ExtractionError>,
) -> Result<T, ExtractionError> {
    match panic::catch_unwind(AssertUnwindSafe(parse)) {
        Ok(result) => result,
        Err(_) => Err(ExtractionError::Unreadable {
            message: "the PDF parser panicked".to_string(),
        }),
    }
}

/// Report an engine or I/O failure as an unreadable file.
fn unreadable(error: impl fmt::Display) -> ExtractionError {
    ExtractionError::Unreadable {
        message: error.to_string(),
    }
}
