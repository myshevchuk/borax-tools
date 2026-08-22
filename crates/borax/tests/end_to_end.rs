#![allow(clippy::unwrap_used)]

//! One whole invocation, over real PDFs, with nothing faked but the
//! socket.
//!
//! Every other test in this workspace exercises a seam against a fake.
//! This one wires the real adapters — the pure-Rust PDF backend reading
//! the committed fixture corpus, the real Crossref and arXiv clients,
//! the real filesystem, the real journal, the real bibliography
//! writer — and asserts what a user would see: the JSON stream, the
//! files on disk afterwards, the journal, the master `.bib`, and the
//! sidecars.
//!
//! What is faked is [`Transport`], which is the one thing a test may
//! not do for real. Requests are routed to the cassettes in
//! `tests/cassettes` by the identifier in their URL, so a wrong URL
//! fails the test rather than quietly reaching the network.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use borax::bib::{RealBibFiles, sidecar_path};
use borax::cli::{Cli, Command, Settings};
use borax::config::{BibLayer, Effective, Layer, Origin, resolve};
use borax::journal::{FileJournal, Journal};
use borax::pipeline::RealLibrary;
use borax::renaming::RealFilesystem;
use borax::run::{Adapters, Configs, Streams, dispatch};
use borax::session::Outcome;
use borax_sources::arxiv::ArxivClient;
use borax_sources::cache::MemoryCache;
use borax_sources::crossref::CrossrefClient;
use borax_sources::http::{HttpRequest, HttpResponse, Politeness, Transport, TransportError};
use borax_sources::source::Source;
use borax_sources::store::ContentIndex;
use serde_json::Value;
use tempfile::{TempDir, tempdir};

// ---------------------------------------------------------------------
// The cassettes, and the transport that serves them
// ---------------------------------------------------------------------

const CROSSREF_001: &str = include_str!("cassettes/crossref-borax-001.json");
const CROSSREF_002: &str = include_str!("cassettes/crossref-borax-002.json");
const ARXIV_NEW: &str = include_str!("cassettes/arxiv-2401.12345.xml");
const ARXIV_OLD: &str = include_str!("cassettes/arxiv-math.GT-0309136.xml");

/// A [`Transport`] that answers from a cassette chosen by what the URL
/// contains, and records every request.
///
/// A URL matching no route is an error rather than an empty response,
/// so a client that builds the wrong URL fails loudly here instead of
/// looking like a service with no record.
struct CassetteTransport {
    routes: Vec<(&'static str, &'static str)>,
    seen: Mutex<Vec<String>>,
}

impl CassetteTransport {
    fn new() -> CassetteTransport {
        CassetteTransport {
            routes: vec![
                ("10.1234/borax.2024.001", CROSSREF_001),
                ("10.1234/borax.2024.002", CROSSREF_002),
                ("2401.12345", ARXIV_NEW),
                ("math.GT/0309136", ARXIV_OLD),
            ],
            seen: Mutex::new(Vec::new()),
        }
    }

    /// The URLs requested, in call order.
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl Transport for &CassetteTransport {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.seen.lock().unwrap().push(request.url.clone());

        // arXiv identifiers reach the URL percent-encoded, so the route
        // is matched against the decoded form.
        let url = request.url.replace("%2F", "/");
        match self.routes.iter().find(|(key, _)| url.contains(key)) {
            Some((_, body)) => Ok(HttpResponse {
                status: 200,
                body: body.to_string(),
            }),
            None => Err(TransportError::Network {
                message: format!("no cassette routes {url}"),
            }),
        }
    }
}

// ---------------------------------------------------------------------
// The fixtures
// ---------------------------------------------------------------------

/// The fixtures the batch runs over, in the order they are given, each
/// with what the run should decide about it.
///
/// Deliberately mixed: both extraction tiers, both identifier kinds,
/// and three distinct ways of failing.
const BATCH: [(&str, Expected); 8] = [
    ("publisher-info-doi.pdf", Expected::Renamed),
    ("publisher-xmp-doi.pdf", Expected::Renamed),
    ("arxiv-new-id.pdf", Expected::Renamed),
    ("arxiv-old-id.pdf", Expected::Renamed),
    ("no-identifier.pdf", Expected::Skipped("no-identifier")),
    (
        "doi-past-page-range.pdf",
        Expected::Skipped("no-identifier"),
    ),
    (
        "encrypted-user-password.pdf",
        Expected::Skipped("unreadable"),
    ),
    ("malformed-truncated.pdf", Expected::Skipped("unreadable")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expected {
    Renamed,
    /// The `kind` the skip reason serializes as.
    Skipped(&'static str),
}

/// The corpus directory, reached from this crate's manifest.
///
/// The fixtures belong to `borax-pdf`, which is where they are
/// generated and where their per-file extraction behaviour is pinned.
/// This test reads the same files rather than keeping a second copy,
/// because two corpora would drift and the point here is that the real
/// backend reads the real files.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../borax-pdf/tests/corpus")
        .canonicalize()
        .expect("the borax-pdf fixture corpus should be in the workspace")
}

/// Copy the batch into a fresh directory, so the run renames copies and
/// the committed corpus is never touched.
fn library_of_copies() -> TempDir {
    let directory = tempdir().unwrap();
    for (name, _) in BATCH {
        fs::copy(corpus().join(name), directory.path().join(name))
            .unwrap_or_else(|error| panic!("copying fixture `{name}`: {error}"));
    }
    directory
}

// ---------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------

/// Everything one invocation left behind.
struct Ran {
    outcome: Outcome,
    /// Each line of stdout, parsed.
    events: Vec<Value>,
    stderr: String,
    library: TempDir,
    state: TempDir,
    master: PathBuf,
    urls: Vec<String>,
}

impl Ran {
    /// The events whose `event` tag is `tag`.
    fn tagged(&self, tag: &str) -> Vec<&Value> {
        self.events
            .iter()
            .filter(|event| event["event"] == tag)
            .collect()
    }

    /// The one `tag` event whose `path` ends in `name`.
    ///
    /// A file can appear more than once — a resolved file is reported
    /// again when it moves — so the tag is what picks out which of its
    /// events is meant.
    fn about(&self, tag: &str, name: &str) -> &Value {
        let matching: Vec<&Value> = self
            .tagged(tag)
            .into_iter()
            .filter(|event| {
                event["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(name))
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one {tag} event about {name}, got {matching:?}"
        );
        matching[0]
    }

    /// The names in the library directory now.
    fn library_names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.library.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

/// Run `borax rename --apply --json` over a fresh copy of the batch,
/// with a master `.bib` and sidecars configured.
fn run_the_batch() -> Ran {
    let library = library_of_copies();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");

    let transport = CassetteTransport::new();
    let politeness = Politeness::default();
    let crossref = CrossrefClient::new(&transport, politeness.clone());
    let arxiv = ArxivClient::new(&transport, politeness);
    let sources: Vec<&dyn Source> = vec![&crossref, &arxiv];

    let index = ContentIndex::new(MemoryCache::new());
    let journal = FileJournal::new(state.path().join("renames.jsonl"));

    let paths: Vec<PathBuf> = BATCH
        .iter()
        .map(|(name, _)| library.path().join(name))
        .collect();

    let effective = effective_with(&master);
    let cli = Cli {
        command: Command::Rename { paths, apply: true },
        settings: Settings::default(),
        json: true,
    };

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let outcome = dispatch(
        &cli,
        &Configs::uniform(effective),
        &Adapters {
            library: &RealLibrary,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            journal: Some(&journal),
            bib_files: &RealBibFiles,
            cache_root: None,
            now: || "e2e-run".to_string(),
            ledger: None,
            collection_root: None,
            // This apply run's mandatory run log needs somewhere to
            // land now that there is no collection root; `state`
            // already stands in for the XDG state directory for the
            // journal above, so it does the same job here.
            state_root: Some(state.path().to_path_buf()),
        },
        &mut Streams {
            out: &mut out,
            err: &mut err,
        },
    );

    let events = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("stdout line is not JSON: {line:?} ({error})"))
        })
        .collect();

    Ran {
        outcome,
        events,
        stderr: String::from_utf8(err).unwrap(),
        library,
        state,
        master,
        urls: transport.seen(),
    }
}

/// The configuration the run uses: a template short enough to read, a
/// master `.bib` at `master`, and sidecars on.
fn effective_with(master: &Path) -> Effective {
    let layer = Layer {
        templates: Some(BTreeMap::from([(
            "default".to_string(),
            "[auth:lower][year]".to_string(),
        )])),
        bib: Some(BibLayer {
            path: Some(master.to_path_buf()),
            duplicates: None,
            sidecars: Some(true),
        }),
        ..Layer::default()
    };
    resolve(vec![(Origin::Flag("test".to_string()), layer)]).unwrap()
}

// ---------------------------------------------------------------------
// The event stream
// ---------------------------------------------------------------------

#[test]
fn every_line_of_stdout_is_a_json_object_carrying_the_schema() {
    let ran = run_the_batch();

    assert!(!ran.events.is_empty());
    for event in &ran.events {
        assert!(event.is_object(), "not an object: {event}");
        assert_eq!(event["schema"], Value::from(1));
        assert!(event["event"].is_string(), "no event tag: {event}");
    }
}

#[test]
fn the_stream_opens_with_run_started_and_closes_with_run_finished() {
    let ran = run_the_batch();

    assert_eq!(ran.events.first().unwrap()["event"], "run-started");
    assert_eq!(ran.events.first().unwrap()["command"], "rename");
    assert_eq!(ran.events.first().unwrap()["applying"], Value::Bool(true));
    assert_eq!(ran.events.last().unwrap()["event"], "run-finished");
}

#[test]
fn each_file_gets_the_verdict_the_batch_expects() {
    let ran = run_the_batch();

    for (name, expected) in BATCH {
        match expected {
            Expected::Renamed => {
                let resolved = ran
                    .tagged("resolved")
                    .into_iter()
                    .find(|event| event["path"].as_str().is_some_and(|p| p.ends_with(name)));
                assert!(resolved.is_some(), "{name} should have resolved");
            }
            Expected::Skipped(kind) => {
                let event = ran.about("skipped", name);
                assert_eq!(event["reason"]["kind"], kind, "{name}: {event}");
            }
        }
    }
}

#[test]
fn the_totals_match_the_batch() {
    let ran = run_the_batch();
    let counts = &ran.events.last().unwrap()["counts"];

    assert_eq!(counts["resolved"], Value::from(4));
    assert_eq!(counts["renamed"], Value::from(4));
    // Four files were skipped at extraction; the four that resolved are
    // renamed, cited, and sidecarred without a skip between them.
    assert_eq!(counts["skipped"], Value::from(4));
}

#[test]
fn a_batch_with_skips_ends_partial_and_says_nothing_on_stderr() {
    let ran = run_the_batch();

    assert_eq!(ran.outcome, Outcome::Partial);
    assert_eq!(ran.stderr, "", "diagnostics: {}", ran.stderr);
}

#[test]
fn a_resolved_event_names_its_tier_and_carries_the_record() {
    let ran = run_the_batch();

    let embedded = ran.about("resolved", "publisher-info-doi.pdf");
    assert_eq!(embedded["tier"], "embedded-metadata");
    assert_eq!(embedded["source"], "crossref");
    assert_eq!(embedded["identifier"], "doi:10.1234/borax.2024.001");
    assert_eq!(embedded["record"]["title"], "A Study of Probe Fixtures");

    let text = ran.about("resolved", "arxiv-new-id.pdf");
    assert_eq!(text["tier"], "text-layer");
    assert_eq!(text["source"], "arxiv");
    assert_eq!(text["record"]["title"], "Preprints and Their Stamps");
}

// Only the four resolvable files cost a request, and each cost one: a
// file with no identifier never reaches a service.
#[test]
fn one_request_per_resolvable_file_and_none_for_the_rest() {
    let ran = run_the_batch();

    assert_eq!(ran.urls.len(), 4, "requested {:?}", ran.urls);
}

// ---------------------------------------------------------------------
// The filesystem afterwards
// ---------------------------------------------------------------------

#[test]
fn the_resolved_files_are_on_disk_under_their_new_names() {
    let ran = run_the_batch();
    let names = ran.library_names();

    for expected in [
        "ashby2024.pdf",
        "brandt2024.pdf",
        "castellan2024.pdf",
        "nowak2003.pdf",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn the_skipped_files_keep_the_names_they_had() {
    let ran = run_the_batch();
    let names = ran.library_names();

    for (name, expected) in BATCH {
        if expected != Expected::Renamed {
            assert!(names.contains(&name.to_string()), "{name} was moved");
        }
    }
}

#[test]
fn a_renamed_file_keeps_its_bytes() {
    let ran = run_the_batch();

    assert_eq!(
        fs::read(ran.library.path().join("ashby2024.pdf")).unwrap(),
        fs::read(corpus().join("publisher-info-doi.pdf")).unwrap(),
        "renaming must not touch a file's contents"
    );
}

#[test]
fn every_renamed_event_names_a_file_that_is_really_there() {
    let ran = run_the_batch();

    for event in ran.tagged("renamed") {
        let target = Path::new(event["target"].as_str().unwrap());
        assert!(target.is_file(), "{} is not on disk", target.display());
        let source = Path::new(event["path"].as_str().unwrap());
        assert!(!source.exists(), "{} was left behind", source.display());
    }
}

// ---------------------------------------------------------------------
// The journal
// ---------------------------------------------------------------------

#[test]
fn the_journal_holds_one_entry_per_move_all_in_one_run() {
    let ran = run_the_batch();
    let journal = FileJournal::new(ran.state.path().join("renames.jsonl"));
    let entries = journal.read();

    assert_eq!(entries.len(), 4, "journalled {entries:?}");
    for entry in &entries {
        assert_eq!(entry.run.as_str(), "e2e-run");
        assert!(entry.to.is_file(), "{} is not there", entry.to.display());
        assert!(!entry.hash.as_str().is_empty());
    }
}

// The journal describes the moves the stream reported, so `undo` and the
// event consumer agree about what happened.
#[test]
fn the_journal_and_the_event_stream_describe_the_same_moves() {
    let ran = run_the_batch();
    let journal = FileJournal::new(ran.state.path().join("renames.jsonl"));

    let mut journalled: Vec<(PathBuf, PathBuf)> = journal
        .read()
        .into_iter()
        .map(|entry| (entry.from, entry.to))
        .collect();
    let mut reported: Vec<(PathBuf, PathBuf)> = ran
        .tagged("renamed")
        .into_iter()
        .map(|event| {
            (
                PathBuf::from(event["path"].as_str().unwrap()),
                PathBuf::from(event["target"].as_str().unwrap()),
            )
        })
        .collect();

    journalled.sort();
    reported.sort();
    assert_eq!(journalled, reported);
}

// ---------------------------------------------------------------------
// The master bibliography
// ---------------------------------------------------------------------

#[test]
fn the_master_bib_holds_one_entry_per_resolved_file() {
    let ran = run_the_batch();
    let content = fs::read_to_string(&ran.master).unwrap();

    assert_eq!(
        content.matches('@').count(),
        4,
        "master bibliography:\n{content}"
    );
    for key in ["ashby2024", "brandt2024", "castellan2024", "nowak2003"] {
        assert!(content.contains(key), "{key} missing from:\n{content}");
    }
}

#[test]
fn every_bib_entry_event_names_a_key_the_master_file_carries() {
    let ran = run_the_batch();
    let content = fs::read_to_string(&ran.master).unwrap();

    let events = ran.tagged("bib-entry");
    assert_eq!(events.len(), 4);
    for event in events {
        let key = event["key"].as_str().unwrap();
        assert_eq!(event["outcome"], "added");
        assert!(content.contains(key), "{key} missing from:\n{content}");
    }
}

#[test]
fn the_master_bib_carries_the_identifiers_that_were_resolved() {
    let ran = run_the_batch();
    let content = fs::read_to_string(&ran.master).unwrap();

    assert!(content.contains("10.1234/borax.2024.001"));
    assert!(content.contains("10.1234/borax.2024.002"));
    assert!(content.contains("2401.12345"));
}

// ---------------------------------------------------------------------
// The sidecars
// ---------------------------------------------------------------------

#[test]
fn a_sidecar_sits_beside_each_renamed_file_under_its_new_name() {
    let ran = run_the_batch();

    for event in ran.tagged("sidecar") {
        let target = Path::new(event["target"].as_str().unwrap());
        assert!(target.is_file(), "{} is not on disk", target.display());
    }
    assert_eq!(ran.tagged("sidecar").len(), 4);

    // The sidecar follows the rename rather than the original name.
    let renamed = ran.library.path().join("ashby2024.pdf");
    assert!(sidecar_path(&renamed).is_file());
}

#[test]
fn a_sidecar_carries_both_the_bibtex_entry_and_the_whole_record() {
    let ran = run_the_batch();
    let sidecar = sidecar_path(&ran.library.path().join("ashby2024.pdf"));
    let content = fs::read_to_string(&sidecar).unwrap();

    assert!(content.contains("@article{ashby2024"), "{content}");
    assert!(content.contains("A Study of Probe Fixtures"), "{content}");

    let record = borax_core::bib_output::parse_sidecar_record(&content)
        .unwrap_or_else(|| panic!("no record recoverable from:\n{content}"));
    assert_eq!(record.title.as_deref(), Some("A Study of Probe Fixtures"));
    assert_eq!(
        record.doi.map(|doi| doi.as_str().to_string()),
        Some("10.1234/borax.2024.001".to_string())
    );
}

// ---------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------

// The same inputs produce the same stream, which is what makes `--json`
// output diffable between runs.
#[test]
fn two_runs_over_the_same_batch_produce_the_same_events() {
    let first = run_the_batch();
    let second = run_the_batch();

    // Paths differ because each run gets its own temporary directory;
    // everything else about the stream has to match.
    let strip = |ran: &Ran| -> Vec<String> {
        ran.events
            .iter()
            .map(|event| {
                let mut event = event.clone();
                for field in ["path", "target", "root"] {
                    if let Some(value) = event.get_mut(field) {
                        *value = Value::from(
                            Path::new(value.as_str().unwrap_or_default())
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        );
                    }
                }
                event.to_string()
            })
            .collect()
    };

    assert_eq!(strip(&first), strip(&second));
}
