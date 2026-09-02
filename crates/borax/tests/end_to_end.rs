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

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use borax::bib::{RealBibFiles, sidecar_path};
use borax::cli::{Cli, Command, LedgerAction};
use borax::config::{BibLayer, Effective, Layer, Origin, resolve};
use borax::ledger::{FileLedger, Ledger};
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
/// once against the same collection — priming a ledger, rebuilding it —
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
/// `collection_root` is also what a ledger is read from and appended to:
/// `Some` gets a real [`FileLedger`] rooted there, `None` is a run
/// outside any collection. [`run_the_batch`] is this with a fixed
/// command and no collection; every test that exercises the ledger —
/// priming it, rebuilding it — calls this directly, more than once,
/// over directories it keeps alive itself.
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
    let ledger = collection_root.map(FileLedger::at_collection_root);
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
            library: &RealLibrary,
            sources: &sources,
            index: &index,
            filesystem: &RealFilesystem,
            bib_files: &RealBibFiles,
            cache_root: None,
            now: || "e2e-run".to_string(),
            ledger: ledger.as_ref().map(|ledger| ledger as &dyn Ledger),
            collection_root: collection_root.map(Path::to_path_buf),
            // An apply run's mandatory run log needs somewhere to land
            // when there is no collection root; `state` is the stand-in
            // for the XDG state directory.
            state_root: Some(state.to_path_buf()),
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
// The ledger — duplicate detection and rebuild over a real collection
//
// Every test above runs through `run_the_batch`, which is deliberately
// outside any collection, so nothing above exercises the ledger. These
// call `invoke` directly against a `collection_root` a real
// `FileLedger` reads and writes, so the duplicate checks, the
// stale-entry rule, and `borax ledger rebuild` are all proven over real
// PDFs and real sidecars rather than a fake standing in for one.
// ---------------------------------------------------------------------

/// A byte-identical copy of `name`'s corpus fixture, saved as `as_name`
/// under `library` — what a content duplicate is made of: bytes the
/// ledger already has an entry for, under a different name.
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

/// The ledger under `collection`'s accounting directory, as text.
fn ledger_text(collection: &Path) -> String {
    fs::read_to_string(collection.join(".borax/ledger.jsonl")).unwrap()
}

/// ledger spec scenarios "Re-downloaded identical file" and "Second PDF
/// of an archived paper": a content duplicate is caught before a
/// request is made for it, a work duplicate only after resolution
/// reveals the identifier it shares, and neither disturbs the entry it
/// duplicates, the file it names, or its own source file.
#[test]
fn a_batch_with_a_content_duplicate_and_a_work_duplicate_skips_both_and_leaves_the_ledger_alone() {
    let collection = tempdir().unwrap();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");

    // Prime the ledger with one admission under DOI …001.
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
    let after_priming = ledger_text(collection.path());
    assert_eq!(
        after_priming.lines().count(),
        1,
        "expected one seeded entry"
    );

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
    // of the network; a work duplicate shares no bytes with the entry
    // it duplicates, so the run cannot know what it is until
    // resolution has answered, and by then the request is spent.
    //
    // Every request this batch made, in call order — the transport is
    // fresh per `invoke`, so the seeded entry's own request belongs to
    // the priming run and is not counted here, and the two DOIs differ
    // so nothing is served from the response cache either. Two
    // requests for three files, and neither is for `content-dup.pdf`.
    assert_eq!(
        batch.urls,
        vec![
            // `work-dup.pdf`, resolving the very identifier the ledger
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

    // Ledger unchanged by the duplicates: one more entry than priming
    // left, for the one file that was genuinely new.
    let after_batch = ledger_text(collection.path());
    assert!(
        after_batch.starts_with(&after_priming),
        "the seeded entry must be untouched"
    );
    assert_eq!(
        after_batch.lines().count(),
        2,
        "duplicates must not add ledger entries"
    );
}

// ---------------------------------------------------------------------
// `borax ledger rebuild` — determinism over a real fixture collection
// ---------------------------------------------------------------------

/// ledger spec scenarios "Rebuild is idempotent" and "Rebuild after
/// manual deletions", proven end to end: a real apply run writes real
/// PDFs and real sidecars, and `borax ledger rebuild` regenerates the
/// ledger from exactly those files rather than from a fake standing in
/// for one.
#[test]
fn rebuilding_a_real_collection_twice_is_byte_identical_and_compacts_after_deletion() {
    let collection = tempdir().unwrap();
    let state = tempdir().unwrap();
    let master = state.path().join("refs.bib");
    let names = [
        "publisher-info-doi.pdf",
        "publisher-xmp-doi.pdf",
        "arxiv-new-id.pdf",
    ];
    let paths: Vec<PathBuf> = names
        .iter()
        .map(|name| duplicate_of(collection.path(), name, name))
        .collect();

    let applied = invoke(
        Command::rename(paths, true),
        &master,
        state.path(),
        Some(collection.path()),
    );
    assert_eq!(
        applied.outcome,
        Outcome::Success,
        "apply run: {}",
        applied.stderr
    );

    let rebuild = || {
        invoke(
            Command::Ledger {
                action: LedgerAction::rebuild(),
            },
            &master,
            state.path(),
            Some(collection.path()),
        )
    };

    let first = rebuild();
    assert_eq!(
        first.outcome,
        Outcome::Success,
        "first rebuild: {}",
        first.stderr
    );
    assert_eq!(first.tagged("ledger-rebuilt")[0]["entries"], Value::from(3));
    let first_bytes = fs::read(collection.path().join(".borax/ledger.jsonl")).unwrap();

    let second = rebuild();
    assert_eq!(
        second.outcome,
        Outcome::Success,
        "second rebuild: {}",
        second.stderr
    );
    let second_bytes = fs::read(collection.path().join(".borax/ledger.jsonl")).unwrap();

    assert_eq!(
        first_bytes, second_bytes,
        "two rebuilds of an unchanged collection must be byte identical"
    );
    assert_eq!(
        String::from_utf8(first_bytes).unwrap().lines().count(),
        3,
        "one entry per admitted file"
    );

    // Delete one renamed file and its sidecar; the rebuild must compact
    // its entry away rather than carry it forward stale.
    let deleted = collection.path().join("ashby2024.pdf");
    fs::remove_file(&deleted).unwrap();
    fs::remove_file(sidecar_path(&deleted)).unwrap();

    let third = rebuild();
    assert_eq!(
        third.outcome,
        Outcome::Success,
        "compacting rebuild: {}",
        third.stderr
    );
    assert_eq!(third.tagged("ledger-rebuilt")[0]["entries"], Value::from(2));
    let compacted = ledger_text(collection.path());
    assert_eq!(compacted.lines().count(), 2);
    assert!(
        !compacted.contains("ashby2024.pdf"),
        "the deleted file's entry survived rebuild:\n{compacted}"
    );
}
