//! The canonical bibliographic record: a CSL-JSON superset.
//!
//! A [`Record`] serializes to a JSON object that is a valid CSL-JSON
//! item — standard fields carry CSL names and value shapes — plus one
//! extension key, `"borax"`, holding what CSL cannot: the arXiv id,
//! per-field provenance, resolution confidence, and unmapped source
//! fields. Serialization is lossless: a record serialized and parsed
//! back compares equal to the original.

use std::collections::BTreeMap;

use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::identifier::{ArxivId, Doi, Isbn, Pmid};

/// The document types borax represents, each mapped to a fixed CSL-JSON
/// `type` value (the serialized form). `Article` is a published journal
/// article (`article-journal`); `Preprint` uses CSL's generic
/// `article`, which keeps the two distinguishable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryType {
    #[serde(rename = "article-journal")]
    Article,
    #[serde(rename = "article")]
    Preprint,
    #[serde(rename = "book")]
    Book,
    #[serde(rename = "chapter")]
    Chapter,
    #[serde(rename = "thesis")]
    Thesis,
    #[serde(rename = "report")]
    Report,
    #[serde(rename = "patent")]
    Patent,
    #[serde(rename = "standard")]
    Standard,
}

impl EntryType {
    /// The CSL-JSON `type` string this variant serializes to.
    pub fn csl(&self) -> &'static str {
        match self {
            EntryType::Article => "article-journal",
            EntryType::Preprint => "article",
            EntryType::Book => "book",
            EntryType::Chapter => "chapter",
            EntryType::Thesis => "thesis",
            EntryType::Report => "report",
            EntryType::Patent => "patent",
            EntryType::Standard => "standard",
        }
    }
}

/// Where a field's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Extracted from the file itself (embedded metadata or text layer).
    Extraction,
    Crossref,
    #[serde(rename = "openalex")]
    OpenAlex,
    Arxiv,
    #[serde(rename = "datacite")]
    DataCite,
    #[serde(rename = "pubmed")]
    PubMed,
    /// Restated from a borax sidecar file.
    Sidecar,
}

/// One agent in a CSL name field: `family` is required, `given`
/// optional (initials or full). Institutional ("literal") names are not
/// yet modeled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name {
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given: Option<String>,
}

/// A (partial) calendar date. Serializes in the CSL-JSON date-variable
/// shape: `{"date-parts": [[year, month, day]]}` with the inner array
/// truncated after the last known part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateParts {
    pub year: i32,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

impl Serialize for DateParts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let _ = serializer;
        todo!("serialize as CSL {{\"date-parts\": [[y, m, d]]}}")
    }
}

impl<'de> Deserialize<'de> for DateParts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _ = deserializer;
        todo!("parse the CSL date-parts shape; a day without a month is invalid")
    }
}

/// The `"borax"` extension object: everything the record carries that
/// has no CSL-JSON slot.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BoraxExt {
    /// The arXiv identifier (CSL-JSON has no field for it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arxiv: Option<ArxivId>,
    /// Resolution confidence in `[0.0, 1.0]`; absent when the record was
    /// never scored (e.g. built from an explicit identifier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Which source supplied each field, keyed by the field's serialized
    /// (CSL) name — `"title"`, `"author"`, `"DOI"`, ….
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provenance: BTreeMap<String, Source>,
    /// Source fields with no CSL-JSON equivalent, preserved verbatim
    /// under the supplying source's key names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_fields: BTreeMap<String, serde_json::Value>,
}

impl BoraxExt {
    fn is_empty(&self) -> bool {
        self.arxiv.is_none()
            && self.confidence.is_none()
            && self.provenance.is_empty()
            && self.source_fields.is_empty()
    }
}

/// The canonical bibliographic record. Field names and shapes follow
/// CSL-JSON; `borax` is the extension area (see [`BoraxExt`]).
///
/// Values are stored as the source supplied them — the record does not
/// reformat page ranges, titles, or names; presentation belongs to the
/// emitters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "author", default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Name>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued: Option<DateParts>,
    /// The containing work: journal title for articles, book title for
    /// chapters.
    #[serde(
        rename = "container-title",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub container_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
    #[serde(rename = "page", default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(rename = "DOI", default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<Doi>,
    #[serde(rename = "PMID", default, skip_serializing_if = "Option::is_none")]
    pub pmid: Option<Pmid>,
    #[serde(rename = "ISBN", default, skip_serializing_if = "Option::is_none")]
    pub isbn: Option<Isbn>,
    #[serde(default, skip_serializing_if = "BoraxExt::is_empty")]
    pub borax: BoraxExt,
}

impl Record {
    /// An empty record of the given type: no fields set, empty
    /// extension.
    pub fn new(entry_type: EntryType) -> Record {
        Record {
            entry_type,
            title: None,
            authors: Vec::new(),
            issued: None,
            container_title: None,
            volume: None,
            issue: None,
            pages: None,
            publisher: None,
            doi: None,
            pmid: None,
            isbn: None,
            borax: BoraxExt::default(),
        }
    }
}
