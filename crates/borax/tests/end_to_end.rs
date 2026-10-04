#![allow(clippy::unwrap_used)]

//! One whole invocation, over real PDFs, with nothing faked but the
//! socket.
//!
//! Every other test in this workspace exercises a seam against a fake.
//! This one wires the real adapters — the pure-Rust PDF backend reading
//! the committed fixture corpus, the real Crossref and arXiv clients,
//! the real filesystem, the real bibliography writer — and asserts what
//! a user would see: the JSON stream, the files on disk afterwards, the
//! run log, the master `.bib`, and the sidecars.
//!
//! What is faked is [`Transport`], which is the one thing a test may
//! not do for real. Requests are routed to the cassettes in
//! `tests/cassettes` by the identifier in their URL, so a wrong URL
//! fails the test rather than quietly reaching the network.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use borax::bib::{RealBibFiles, sidecar_path};
use borax::cli::{Cli, Command};
use borax::config::{BibLayer, Effective, Layer, Origin, resolve};
use borax::event::SCHEMA;
use borax::pipeline::RealDocuments;
use borax::renaming::RealFilesystem;
use borax::run::{Adapters, Configs, Streams, dispatch};
use borax::session::{Answer, Asker, Outcome, Question, Session, TextPrompt};
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
    (
        "no-identifier.pdf",
        Expected::Skipped("text-without-identifier"),
    ),
    (
        "doi-past-page-range.pdf",
        Expected::Skipped("text-without-identifier"),
    ),
    (
        "encrypted-user-password.pdf",
        Expected::Skipped("encrypted"),
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

/// A fresh directory holding copies of exactly `names`, under their own
/// names, so a run renames copies and the committed corpus is never
/// touched.
fn library_of(names: &[&str]) -> TempDir {
    let directory = tempdir().unwrap();
    for name in names {
        fs::copy(corpus().join(name), directory.path().join(name))
            .unwrap_or_else(|error| panic!("copying fixture `{name}`: {error}"));
    }
    directory
}

/// [`library_of`] over every fixture in [`BATCH`].
fn library_of_copies() -> TempDir {
    library_of(&BATCH.map(|(name, _)| name))
}

// ---------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------

/// Query helpers shared by every value that carries a parsed event
/// stream, so [`Ran`] and [`Invocation`] answer the same questions the
/// same way.
trait EventStream {
    fn events(&self) -> &[Value];

    /// The events whose `event` tag is `tag`.
    fn tagged(&self, tag: &str) -> Vec<&Value> {
        self.events()
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
}

/// Everything one invocation left behind, owning the temporary
/// directories it ran over.
///
/// What [`run_the_batch`] hands back: a run with nothing before it and
/// nothing after, over its own fresh copies. A test that runs more than
/// once against the same library — priming its store, then running
/// against what that left —
/// keeps its own directories alive across several calls to
/// [`invoke`] instead of asking for a second `Ran`, which could not own
/// them without double-owning what the first already does.
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

impl EventStream for Ran {
    fn events(&self) -> &[Value] {
        &self.events
    }
}

impl Ran {
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

/// One invocation's outcome, without the directories a caller already
/// owns.
///
/// [`invoke`]'s return value: everything [`Ran`] carries except the
/// temporary directories, for a caller running more than once against
/// the same collection.
struct Invocation {
    outcome: Outcome,
    events: Vec<Value>,
    stderr: String,
    urls: Vec<String>,
}

impl EventStream for Invocation {
    fn events(&self) -> &[Value] {
        &self.events
    }
}

/// Run `command` against the real adapters — the cassette transport, the
/// pure-Rust PDF backend, the real filesystem — with bibliography output
/// going to `master` and a run log to `collection_root`'s `.borax/runs`
/// or, absent one, to `state`.
///
/// `collection_root` is also the library the run records in and checks
/// its files against, `None` being a run outside any library.
/// [`run_the_batch`] is this with a fixed command and no library; every
/// test that exercises the store — priming it, then checking against
/// what it holds — calls this directly, more than once, over
/// directories it keeps alive itself.
fn invoke(
    command: Command,
    master: &Path,
    state: &Path,
    collection_root: Option<&Path>,
) -> Invocation {
    let transport = CassetteTransport::new();
    let politeness = Politeness::default();
    let crossref = CrossrefClient::new(&transport, politeness.clone());
    let arxiv = ArxivClient::new(&transport, politeness);
    let sources: Vec<&dyn Source> = vec![&crossref, &arxiv];

    let index = ContentIndex::new(MemoryCache::new());
    let effective = effective_with(master);
    let cli = Cli {
        command,
        json: true,
    };

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let outcome = dispatch(
        &cli,
        &Configs::uniform(effective),
        &Adapters {
            documents: &RealDocuments,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            bib_files: &RealBibFiles,
            cache_root: None,
            now: || "e2e-run".to_string(),
            collection_root: collection_root.map(Path::to_path_buf),
            // An apply run's mandatory run log needs somewhere to land
            // when there is no collection root; `state` is the stand-in
            // for the XDG state directory.
            state_root: Some(state.to_path_buf()),
        },
        &mut Session::batch(),
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

    Invocation {
        outcome,
        events,
        stderr: String::from_utf8(err).unwrap(),
        urls: transport.seen(),
    }
}

/// Run `borax rename --apply --json` over a fresh copy of the batch,
/// with a master `.bib` and sidecars configured, outside any collection.
fn run_the_batch() -> Ran {
    let library = library_of_copies();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");
    let paths: Vec<PathBuf> = BATCH
        .iter()
        .map(|(name, _)| library.path().join(name))
        .collect();

    let invocation = invoke(Command::rename(paths, true), &master, state.path(), None);

    Ran {
        outcome: invocation.outcome,
        events: invocation.events,
        stderr: invocation.stderr,
        library,
        state,
        master,
        urls: invocation.urls,
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
        assert_eq!(event["schema"], Value::from(SCHEMA));
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
    assert_eq!(
        embedded["extraction"]["result"]["tier"],
        "embedded-metadata"
    );
    assert_eq!(
        embedded["record_retrieval"],
        serde_json::json!({"kind": "network", "service": "crossref"})
    );
    assert_eq!(embedded["identifier"], "doi:10.1234/borax.2024.001");
    assert_eq!(embedded["record"]["title"], "A Study of Probe Fixtures");

    let text = ran.about("resolved", "arxiv-new-id.pdf");
    assert_eq!(text["extraction"]["result"]["tier"], "text-layer");
    assert_eq!(
        text["record_retrieval"],
        serde_json::json!({"kind": "network", "service": "arxiv"})
    );
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
// The run log — the record an applied run leaves behind
// ---------------------------------------------------------------------

/// The apply run's log under `state`, whose `runs` directory holds one
/// per run of a test that applies exactly once.
///
/// The name ends `-apply.jsonl` ([`borax::runlog::log_name`]) and leads
/// with a timestamp, so the greatest name is the most recent run.
fn apply_log_under(state: &Path) -> Option<PathBuf> {
    fs::read_dir(state.join(borax::runlog::RUNS_DIR))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with("-apply.jsonl"))
        })
        .max()
}

// The run log describes the moves the stream reported, so a reader of
// the log and a reader of the stream agree about what happened.
#[test]
fn the_run_log_and_the_event_stream_describe_the_same_moves() {
    let ran = run_the_batch();
    let log_path = apply_log_under(ran.state.path())
        .expect("an apply run outside a collection must leave a run log under the state root");
    let log_text = fs::read_to_string(&log_path).unwrap();

    let mut logged: Vec<(PathBuf, PathBuf)> = log_text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|event| event["event"] == "renamed")
        .map(|event| {
            (
                PathBuf::from(event["path"].as_str().unwrap()),
                PathBuf::from(event["target"].as_str().unwrap()),
            )
        })
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

    logged.sort();
    reported.sort();
    assert_eq!(logged, reported);
}

/// The record part B will replay from: each `renamed` line in the run
/// log round-trips the same hash the stdout stream reported for that
/// move, and it is never empty.
#[test]
fn the_run_logs_renamed_lines_carry_the_same_hash_the_stream_reported() {
    let ran = run_the_batch();
    let log_path = apply_log_under(ran.state.path())
        .expect("an apply run outside a collection must leave a run log under the state root");
    let log_text = fs::read_to_string(&log_path).unwrap();

    let stream_hashes: BTreeMap<String, Value> = ran
        .tagged("renamed")
        .into_iter()
        .map(|event| {
            (
                event["path"].as_str().unwrap().to_string(),
                event["hash"].clone(),
            )
        })
        .collect();

    let mut checked = 0;
    for logged in log_text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|event: &Value| event["event"] == "renamed")
    {
        let path = logged["path"].as_str().unwrap().to_string();
        let hash = logged["hash"].as_str().unwrap();
        assert!(!hash.is_empty(), "empty hash for {path}");
        assert_eq!(
            Some(&logged["hash"]),
            stream_hashes.get(&path),
            "the run log and the stream must agree on {path}'s hash"
        );
        checked += 1;
    }
    assert_eq!(checked, 4, "expected to check all four renamed files");
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

// ---------------------------------------------------------------------
// The library store — duplicate detection over a real library
//
// Every test above runs through `run_the_batch`, which is deliberately
// outside any library, so nothing above exercises the store. These call
// `invoke` directly against a `collection_root` the run reads and
// writes, so the two duplicate checks are proven over real PDFs and
// real records rather than a fake standing in for either.
// ---------------------------------------------------------------------

/// A byte-identical copy of `name`'s corpus fixture, saved as `as_name`
/// under `library` — what a content duplicate is made of: bytes the
/// library already has a record for, under a different name.
fn duplicate_of(library: &Path, name: &str, as_name: &str) -> PathBuf {
    let target = library.join(as_name);
    fs::copy(corpus().join(name), &target)
        .unwrap_or_else(|error| panic!("copying fixture `{name}` as `{as_name}`: {error}"));
    target
}

/// `name`'s corpus fixture, copied under `as_name` with a comment line
/// appended after its own `%%EOF` — what a work duplicate is made of:
/// bytes that hash differently but still carry the same identifier.
///
/// Verified empirically before this test was built on it: `PurePdf`
/// (`crates/borax-pdf/src/pure.rs`, backed by `lopdf`) locates the
/// trailer by scanning backward for `startxref`, which trailing bytes
/// after the file's own `%%EOF` do not move, so the Info-dictionary DOI
/// `publisher-info-doi.pdf` carries is still found.
fn work_duplicate_of(library: &Path, name: &str, as_name: &str) -> PathBuf {
    let mut bytes = fs::read(corpus().join(name))
        .unwrap_or_else(|error| panic!("reading fixture `{name}`: {error}"));
    bytes.extend_from_slice(b"\n%borax-duplicate\n");
    let target = library.join(as_name);
    fs::write(&target, &bytes).unwrap_or_else(|error| panic!("writing `{as_name}`: {error}"));
    target
}

/// Every artifact record the library at `collection` holds, as the
/// bytes of the files they were read from, keyed by file name — so what
/// a run wrote can be compared against what the run before it left,
/// record for record and byte for byte.
fn records_of(collection: &Path) -> BTreeMap<String, Vec<u8>> {
    let dir = collection.join(".borax").join("artifacts");
    let Ok(entries) = fs::read_dir(&dir) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

/// ledger spec scenarios "Re-downloaded identical file" and "Second PDF
/// of an archived paper": a content duplicate is caught before a
/// request is made for it, a work duplicate only after resolution
/// reveals the identifier it shares, and neither disturbs the record it
/// duplicates, the file that record names, or its own source file.
#[test]
fn a_batch_with_a_content_duplicate_and_a_work_duplicate_skips_both_and_leaves_the_store_alone() {
    let collection = tempdir().unwrap();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");

    // Prime the library with one admission under DOI …001.
    let seed = duplicate_of(
        collection.path(),
        "publisher-info-doi.pdf",
        "publisher-info-doi.pdf",
    );
    let priming = invoke(
        Command::rename(vec![seed], true),
        &master,
        state.path(),
        Some(collection.path()),
    );
    assert_eq!(
        priming.outcome,
        Outcome::Success,
        "priming run: {}",
        priming.stderr
    );
    let after_priming = records_of(collection.path());
    assert_eq!(after_priming.len(), 1, "expected one seeded record");

    let content_dup = duplicate_of(
        collection.path(),
        "publisher-info-doi.pdf",
        "content-dup.pdf",
    );
    let work_dup = work_duplicate_of(collection.path(), "publisher-info-doi.pdf", "work-dup.pdf");
    let fresh = duplicate_of(
        collection.path(),
        "publisher-xmp-doi.pdf",
        "publisher-xmp-doi.pdf",
    );

    let batch = invoke(
        Command::rename(vec![content_dup.clone(), work_dup.clone(), fresh], true),
        &master,
        state.path(),
        Some(collection.path()),
    );

    let content_reason = &batch.about("skipped", "content-dup.pdf")["reason"];
    assert_eq!(content_reason["kind"], "duplicate");
    assert_eq!(content_reason["reason"], "content");
    assert!(
        content_reason["existing_path"]
            .as_str()
            .unwrap()
            .ends_with("ashby2024.pdf"),
        "got {content_reason:?}"
    );

    let work_reason = &batch.about("skipped", "work-dup.pdf")["reason"];
    assert_eq!(work_reason["kind"], "duplicate");
    assert_eq!(work_reason["reason"], "work");
    assert!(
        work_reason["existing_path"]
            .as_str()
            .unwrap()
            .ends_with("ashby2024.pdf"),
        "got {work_reason:?}"
    );

    assert_eq!(
        batch.about("resolved", "publisher-xmp-doi.pdf")["identifier"],
        "doi:10.1234/borax.2024.002"
    );

    // What makes the two duplicate kinds different rather than merely
    // differently labelled is where each is caught. A content
    // duplicate is recognised from its hash, before anything is asked
    // of the network; a work duplicate shares no bytes with the record
    // it duplicates, so the run cannot know what it is until
    // resolution has answered, and by then the request is spent.
    //
    // Every request this batch made, in call order — the transport is
    // fresh per `invoke`, so the seeded file's own request belongs to
    // the priming run and is not counted here, and the two DOIs differ
    // so nothing is served from the response cache either. Two
    // requests for three files, and neither is for `content-dup.pdf`.
    assert_eq!(
        batch.urls,
        vec![
            // `work-dup.pdf`, resolving the very identifier the library
            // already holds — the cost of finding that out.
            "https://api.crossref.org/works/10.1234/borax.2024.001".to_string(),
            // `publisher-xmp-doi.pdf`, genuinely new.
            "https://api.crossref.org/works/10.1234/borax.2024.002".to_string(),
        ],
        "a content duplicate must cost no request, a work duplicate exactly one"
    );

    // Untouched sources: neither duplicate's file was deleted,
    // overwritten, or moved.
    assert_eq!(
        fs::read(&content_dup).unwrap(),
        fs::read(corpus().join("publisher-info-doi.pdf")).unwrap(),
        "content duplicate's bytes changed"
    );
    assert!(work_dup.is_file(), "work duplicate's source was removed");

    // The store is unchanged by the duplicates: the seeded record is
    // byte-identical, and there is one more record than priming left,
    // for the one file that was genuinely new.
    let after_batch = records_of(collection.path());
    for (name, bytes) in &after_priming {
        assert_eq!(
            after_batch.get(name),
            Some(bytes),
            "the seeded record must be untouched: {name}"
        );
    }
    assert_eq!(
        after_batch.len(),
        2,
        "duplicates must write no artifact record"
    );
    assert_eq!(
        fs::read_dir(collection.path().join("items"))
            .unwrap()
            .count(),
        2,
        "and must mint no item either"
    );
}

/// design D1/D3, end to end: a run applied over a real collection,
/// pointed at the files it just renamed, finds every one of them
/// already named — not a duplicate of itself, not a skip — and exits
/// 0, the distinction the partial-success code exists to draw.
#[test]
fn a_second_run_over_a_renamed_collection_finds_every_file_already_named() {
    let collection = tempdir().unwrap();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");

    let names = [
        "publisher-info-doi.pdf",
        "publisher-xmp-doi.pdf",
        "arxiv-new-id.pdf",
        "arxiv-old-id.pdf",
    ];
    let paths: Vec<PathBuf> = names
        .iter()
        .map(|name| duplicate_of(collection.path(), name, name))
        .collect();

    let first = invoke(
        Command::rename(paths, true),
        &master,
        state.path(),
        Some(collection.path()),
    );
    assert_eq!(
        first.outcome,
        Outcome::Success,
        "first run: {}",
        first.stderr
    );

    let renamed: Vec<PathBuf> = fs::read_dir(collection.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "pdf"))
        .collect();
    assert_eq!(
        renamed.len(),
        names.len(),
        "expected every fixture to have been renamed to a distinct file"
    );

    let second = invoke(
        Command::rename(renamed, true),
        &master,
        state.path(),
        Some(collection.path()),
    );

    assert_eq!(
        second.outcome,
        Outcome::Success,
        "a re-run over an entirely named collection must exit 0: {}",
        second.stderr
    );
    assert_eq!(
        second.tagged("already-named").len(),
        names.len(),
        "every file must be reported already named: {:?}",
        second.events
    );
    assert!(
        second.tagged("skipped").is_empty(),
        "no file must be reported a duplicate of itself: {:?}",
        second.events
    );
    assert!(
        second.tagged("renamed").is_empty(),
        "nothing should move on the re-run: {:?}",
        second.events
    );
}

// ---------------------------------------------------------------------
// task 4.1: a rename from a supplied identifier is recognised offline
// by a later run, batch included ("asked once")
// ---------------------------------------------------------------------

/// A scripted [`Asker`], local to this file since the real terminal
/// adapter (`TerminalAsker`) is not under test here: answers a fixed
/// list of choices and a fixed list of text answers, each in order, and
/// panics if asked for more of either than it was given.
struct ScriptedAsker {
    answers: RefCell<std::vec::IntoIter<Answer>>,
    texts: RefCell<std::vec::IntoIter<Option<String>>>,
}

impl ScriptedAsker {
    fn new(answers: Vec<Answer>, texts: Vec<Option<String>>) -> ScriptedAsker {
        ScriptedAsker {
            answers: RefCell::new(answers.into_iter()),
            texts: RefCell::new(texts.into_iter()),
        }
    }
}

impl Asker for ScriptedAsker {
    fn choose(&mut self, question: &Question) -> Answer {
        self.answers
            .borrow_mut()
            .next()
            .unwrap_or_else(|| panic!("asked more questions than were scripted: {question:?}"))
    }

    fn text(&mut self, prompt: &TextPrompt) -> Option<String> {
        self.texts
            .borrow_mut()
            .next()
            .unwrap_or_else(|| panic!("asked for more text than was scripted: {prompt:?}"))
    }
}

/// design "Asked once" (spec `resolution`, task 4.1): a rename made in
/// an interactive run from a supplied identifier is written to the
/// content index, so a later run — batch included — resolves the file
/// from there under its new name: no extraction, and no source queried
/// at all, checked against the transport's own call count rather than
/// inferred from the event stream.
#[test]
fn a_rename_from_a_supplied_identifier_is_recognised_offline_by_a_later_batch_run() {
    let library = library_of(&["no-identifier.pdf"]);
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");
    let path = library.path().join("no-identifier.pdf");

    let transport = CassetteTransport::new();
    let politeness = Politeness::default();
    let crossref = CrossrefClient::new(&transport, politeness.clone());
    let arxiv = ArxivClient::new(&transport, politeness);
    let sources: Vec<&dyn Source> = vec![&crossref, &arxiv];
    let index = ContentIndex::new(MemoryCache::new());

    // The first, interactive run: the file has no identifier of its
    // own, so the operator supplies one of the cassette's own DOIs.
    let mut asker = ScriptedAsker::new(
        vec![Answer::Supply, Answer::Rename],
        vec![Some("10.1234/borax.2024.001".to_string())],
    );
    let mut out = Vec::new();
    let mut err = Vec::new();
    let first_outcome = dispatch(
        &Cli {
            command: Command::rename(vec![path.clone()], false),
            json: true,
        },
        &Configs::uniform(effective_with(&master)),
        &Adapters {
            documents: &RealDocuments,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            bib_files: &RealBibFiles,
            cache_root: None,
            now: || "e2e-supply-first".to_string(),
            collection_root: None,
            state_root: Some(state.path().to_path_buf()),
        },
        &mut Session::interactive(&mut asker),
        &mut Streams {
            out: &mut out,
            err: &mut err,
        },
    );

    let renamed_path = library.path().join("ashby2024.pdf");
    assert!(
        renamed_path.exists(),
        "the file must have been renamed from the supplied identifier's record ({:?}); \
         directory now holds {:?}, stderr was {:?}",
        first_outcome,
        fs::read_dir(library.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        String::from_utf8(err).unwrap(),
    );
    assert_eq!(
        first_outcome,
        Outcome::Success,
        "the first, interactive run should succeed"
    );
    let calls_after_first_run = transport.seen().len();
    assert!(
        calls_after_first_run > 0,
        "the first run must have queried the supplied identifier for this to be a real test"
    );

    // The second, batch run, over the file's new name.
    let mut out2 = Vec::new();
    let mut err2 = Vec::new();
    let second_outcome = dispatch(
        &Cli {
            command: Command::rename(vec![renamed_path.clone()], true),
            json: true,
        },
        &Configs::uniform(effective_with(&master)),
        &Adapters {
            documents: &RealDocuments,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            bib_files: &RealBibFiles,
            cache_root: None,
            now: || "e2e-supply-second".to_string(),
            collection_root: None,
            state_root: Some(state.path().to_path_buf()),
        },
        &mut Session::batch(),
        &mut Streams {
            out: &mut out2,
            err: &mut err2,
        },
    );

    let events: Vec<Value> = String::from_utf8(out2)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(second_outcome, Outcome::Success, "got {events:?}");
    assert!(
        events.iter().any(|event| event["event"] == "resolved"
            && event["record_retrieval"]["kind"] == "content-index"
            && event["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("ashby2024.pdf"))),
        "the second run must have resolved the file from the content index: got {events:?}"
    );
    assert!(
        events.iter().any(|event| event["event"] == "already-named"
            && event["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("ashby2024.pdf"))),
        "resolved from the content index, the file must be reported already named: got {events:?}"
    );
    assert_eq!(
        transport.seen().len(),
        calls_after_first_run,
        "the second, batch run must not have queried any source at all: got {:?}",
        transport.seen()
    );
}

// ---------------------------------------------------------------------
// report-extraction-per-file, task 3.3: `status --identify` over the
// real backend
// ---------------------------------------------------------------------

/// Six fixtures covering every `extraction` result kind, run through
/// the pure-Rust PDF backend rather than a fake. Avoids
/// `publisher-text-doi.pdf` and `doi-on-third-page.pdf`, which the
/// corpus README records as red on the pure backend.
const IDENTIFY_FIXTURES: [&str; 6] = [
    "publisher-info-doi.pdf",
    "arxiv-new-id.pdf",
    "no-text-layer.pdf",
    "no-identifier.pdf",
    "encrypted-user-password.pdf",
    "malformed-truncated.pdf",
];

#[test]
fn status_identify_over_the_real_backend_reports_every_fixture() {
    let library = library_of(&IDENTIFY_FIXTURES);
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");

    let ran = invoke(
        Command::status(Some(library.path().to_path_buf()), true),
        &master,
        state.path(),
        Some(library.path()),
    );

    assert_eq!(ran.outcome, Outcome::Success, "got {:?}", ran.outcome);
    assert_eq!(ran.urls, Vec::<String>::new(), "got {:?}", ran.urls);

    let extractions = ran.tagged("library-extraction");
    let mut by_path: BTreeMap<String, Value> = BTreeMap::new();
    for event in &extractions {
        by_path.insert(
            event["path"].as_str().unwrap().to_string(),
            event["extraction"].clone(),
        );
    }

    let found = &by_path["publisher-info-doi.pdf"];
    assert_eq!(found["kind"], Value::from("found"));
    assert_eq!(
        found["identifier"],
        Value::from("doi:10.1234/borax.2024.001")
    );
    assert_eq!(found["tier"], Value::from("embedded-metadata"));

    let arxiv = &by_path["arxiv-new-id.pdf"];
    assert_eq!(arxiv["kind"], Value::from("found"));
    assert_eq!(arxiv["identifier"], Value::from("arXiv:2401.12345v2"));
    assert_eq!(arxiv["tier"], Value::from("text-layer"));

    assert_eq!(
        by_path["no-text-layer.pdf"]["kind"],
        Value::from("no-text-layer")
    );
    assert_eq!(
        by_path["no-identifier.pdf"]["kind"],
        Value::from("text-without-identifier")
    );
    assert_eq!(
        by_path["encrypted-user-password.pdf"]["kind"],
        Value::from("encrypted")
    );

    let unreadable = &by_path["malformed-truncated.pdf"];
    assert_eq!(unreadable["kind"], Value::from("unreadable"));
    assert!(
        unreadable["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "got {unreadable:?}"
    );

    assert_eq!(
        extractions.len(),
        IDENTIFY_FIXTURES.len(),
        "got {:?}",
        ran.events
    );

    let statuses = ran.tagged("library-status");
    assert_eq!(statuses.len(), 1, "got {:?}", ran.events);
    assert_eq!(statuses[0]["identifiable"], Value::from(2));

    assert_eq!(
        ran.events
            .iter()
            .position(|event| event["event"] == "library-status"),
        Some(ran.events.len() - 2),
        "library-status must be the last event before run-finished: got {:?}",
        ran.events
    );
    assert_eq!(
        ran.events.last().unwrap()["event"],
        Value::from("run-finished")
    );
    assert_eq!(
        ran.events.last().unwrap()["counts"],
        serde_json::json!({
            "resolved": 0, "renamed": 0, "skipped": 0, "named": 0,
            "unmatched": 0, "unreached": 0, "findings": 0,
        }),
        "got {:?}",
        ran.events.last()
    );
}

// ---------------------------------------------------------------------
// sectioned-resolved-event, tasks 8.1 and 9.3: the real binary's
// `resolved` and `skipped` events carry schema 4's key set and none of
// the six removed fields.
// ---------------------------------------------------------------------

/// design D3: over the real backend, every `resolved` event carries
/// exactly the schema-4 key set, in pipeline order, and none of
/// `found`, `cached`, `source`, `tier`, `claims` or `overrode` at the
/// top level; every resolution `skipped` event's `reason` object
/// carries none of `found`, `tier`, `attempts`, `field`, `extracted`,
/// `resolved` or `similarity` — the facts D4 moved into the sections.
#[test]
fn resolve_and_rename_over_the_real_backend_carry_no_removed_field() {
    let ran = run_the_batch();

    let mut saw_resolved = false;
    let mut saw_skipped = false;
    for event in &ran.events {
        match event["event"].as_str() {
            Some("resolved") => {
                saw_resolved = true;
                let object = event.as_object().unwrap();
                for removed in ["found", "cached", "source", "tier", "claims", "overrode"] {
                    assert!(
                        !object.contains_key(removed),
                        "resolved event still carries {removed:?}: {event}"
                    );
                }
                for section in [
                    "library",
                    "content_index",
                    "extraction",
                    "identifier_input",
                    "lookup",
                    "record_retrieval",
                    "match_check",
                    "acceptance",
                ] {
                    assert!(
                        object.contains_key(section),
                        "resolved event is missing section {section:?}: {event}"
                    );
                }
                // design D1: a batch run asks no question, so
                // `identifier_input` is `not-attempted` with reason
                // `not-asked`, or `content-duplicate` on a content
                // duplicate.
                let reason = object["identifier_input"]["reason"].as_str();
                assert!(
                    matches!(reason, Some("not-asked") | Some("content-duplicate")),
                    "resolved event's identifier_input has an unexpected reason: {event}"
                );
                let line = event.to_string();
                let extraction_at = line.find("\"extraction\"").unwrap();
                let input_at = line.find("\"identifier_input\"").unwrap();
                let lookup_at = line.find("\"lookup\"").unwrap();
                assert!(
                    extraction_at < input_at && input_at < lookup_at,
                    "identifier_input is out of pipeline order: {line}"
                );
            }
            Some("skipped") => {
                saw_skipped = true;
                let reason = event["reason"].as_object().unwrap();
                for removed in [
                    "found",
                    "tier",
                    "attempts",
                    "field",
                    "extracted",
                    "resolved",
                    "similarity",
                ] {
                    assert!(
                        !reason.contains_key(removed),
                        "skipped event's reason still carries {removed:?}: {event}"
                    );
                }
                // design D1: every resolution-verdict skip (one that
                // carries sections) carries identifier_input too.
                if let Some(sections) = event.get("sections").filter(|v| !v.is_null()) {
                    assert!(
                        sections.get("identifier_input").is_some(),
                        "resolution skip is missing identifier_input: {event}"
                    );
                    let input_reason = sections["identifier_input"]["reason"].as_str();
                    assert!(
                        matches!(input_reason, Some("not-asked") | Some("content-duplicate")),
                        "skip's identifier_input has an unexpected reason: {event}"
                    );
                }
            }
            _ => {}
        }
    }
    assert!(saw_resolved, "the batch must resolve at least one file");
    assert!(saw_skipped, "the batch must skip at least one file");
}

/// The same audit over `borax resolve --json`, which never renames and
/// so exercises a different code path to the same events.
#[test]
fn resolve_over_the_real_backend_carries_no_removed_field() {
    let library = library_of_copies();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");
    let paths: Vec<PathBuf> = BATCH
        .iter()
        .map(|(name, _)| library.path().join(name))
        .collect();

    let ran = invoke(Command::resolve(paths), &master, state.path(), None);

    let resolved = ran.tagged("resolved");
    assert!(
        !resolved.is_empty(),
        "the batch must resolve at least one file"
    );
    for event in resolved {
        let object = event.as_object().unwrap();
        for removed in ["found", "cached", "source", "tier", "claims", "overrode"] {
            assert!(
                !object.contains_key(removed),
                "resolved event still carries {removed:?}: {event}"
            );
        }
    }
}
