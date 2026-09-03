# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The README is now an introduction rather than a reference: what borax
  is for, a few worked examples, how to install it, how to build it,
  and where everything else is. The reference material it used to
  carry — the template language, the fields and filters, external
  lookup tables, configuration, and run logs — moved to
  `docs/manual.org`, which also documents each subcommand and every
  setting with its flag and its default. Nothing about the tool's
  behaviour changed. Release archives now carry the manual beside the
  README.
- borax describes itself as "fast, configurable" rather than "fast,
  error-free", in the README and in the crate description. The old
  phrase promised what no tool can guarantee; what it was reaching for —
  previews by default, never overwrites, never guesses — is stated as
  those three things instead.

## [0.4.0] - 2026-09-03

### Added

- Templates can look a value up in a table you maintain outside borax.
  A table is a tab-separated file with a header row, declared in the
  new `[tables]` section of a configuration file, which names the
  column supplying its keys and the column supplying its values rather
  than fixing their positions — so one curated file can serve borax and
  whatever else already reads it, and borax asks it for no column and
  no column order it does not already have. A `lookup("<table>")`
  filter substitutes what the named table holds for its input, so
  `[journal:lookup("jcode")]` names a paper by the journal code you
  decided on rather than by anything a source reports. Both sides of
  the comparison are folded by the four steps the `slug` filter already
  performs — transliterate, lowercase, every run outside `a-z0-9` to a
  single `-`, trim the leading and trailing `-` — so `J Am Chem Soc`
  reaches a row keyed `J. Am. Chem. Soc.` and the punctuation the
  sources disagree about stops deciding anything. The README states
  those four steps normatively, because they are the contract any other
  tool reading the same file has to implement to resolve it the same
  way.

  A table's values are literal text unless its declaration says
  `values = "template"`, in which case each is compiled as a template
  fragment when the table loads. That is how a row varies the shape of
  what it contributes and not only its content: a journal cited with
  its volume holds `ABB[volume:prefix("-")]` where one cited without
  holds `AA`, and `[year]-[journal:lookup("jcode")]-[firstpage]` names
  both without knowing which is which. A fragment may not itself
  contain a `lookup`, so indirection stops at one level and recursion
  is impossible by construction.

  Nothing in the mechanism executes, and rendering stays total: a
  lookup that finds no row renders the empty string, so a `||`
  alternative supplies the fallback. It is never silent, though — a
  miss is reported once per distinct table and value however many files
  hit it, and counted in the run summary, which is what tells you which
  row to add to your file. Everything that is a property of the
  declaration or of the file ends the run before the first file is
  touched: a path that cannot be read, a header without a declared
  column, two rows folding to one key with different values, a fragment
  that will not compile, and a template naming a table nothing
  declared. A relative path resolves against the directory of the
  configuration file that declared it rather than the working
  directory, so a `.borax.toml` can ship a table beside itself; `~` is
  expanded here no more than anywhere else in borax. Each run's opening
  event names every table it read by path and by a digest of its
  contents, because a table is a file that changes and the run log has
  to be able to say why a file got the name it did afterwards. Editing
  one changes names — and citation keys already typed into documents —
  for every work it now answers for.

- Five fields join the template vocabulary: `volume`, `issue`, `pages`,
  `publisher`, and `firstpage`, which renders `pages` up to its first
  dash of any width, trimmed, and the whole value when it holds none.
  A page range therefore yields its first page and an article number
  such as `e0123456` or `045301` comes through whole. All five were
  already on the record, populated wherever the source supplies them;
  only the template engine could not reach them.

- `prefix("<text>")` and `suffix("<text>")` put text before or after a
  value and leave the empty string alone, which is how an optional
  segment takes its separator with it:
  `[year]-[journal:abbr][volume:prefix("-")]-[firstpage]` renders
  `2024-JACS-146-1234` for a paper with a volume and `2024-JACS-1234`
  for one without, rather than leaving `2024-JACS--1234` behind.

### Changed

- **BREAKING:** A setting flag now follows the subcommand that reads
  it. `borax rename --mailto you@example.org papers/` is the accepted
  form, and `borax --mailto you@example.org rename papers/` is a usage
  error naming the flag. `--json` is the exception and still goes on
  either side, because it chooses how the event stream is rendered and
  every subcommand honours it.

  The position changed because the surface did. Each subcommand now
  declares only the settings that can change what it reports, writes or
  moves, and a flag declared on a subcommand is recognized only after
  that subcommand's name — an argument propagates down from the parent
  and never up from a child, so a per-command surface and a
  position-independent flag cannot both hold. What the position buys is
  a `--help` that describes the command in front of you, and a refusal
  where there used to be a silent no-op: `borax cache --no-cache`,
  `borax cache --mailto …`, `borax rename --concurrency 8`,
  `borax bib --no-ledger`, `borax bib --collision skip` and
  `borax ledger rebuild --no-ledger` all named settings those commands
  never read, and all of them were accepted and dropped. They are now
  unknown arguments.

  `borax config` still accepts every setting there is, since passing an
  override to it is not a no-op but the question it answers.
  Configuration is untouched: a configuration file and an environment
  variable still set every key whatever command runs, so
  `network.concurrency` in a `.borax.toml` still resolves and is still
  reported under a `rename` that will not read it. Nothing about
  layering, precedence, origins or `borax config` output changed.

  There is no deprecation period and no shim. The error names the flag,
  and the subcommand's `--help` now lists it in the right place.

- A command compiles only the template tables it renders from.
  `borax rename` names files and cites records, so it compiles both
  `[templates]` and `[citation-keys]`; `borax bib` renders no file
  name, so it compiles the citation keys alone and a `templates.default`
  that will not compile no longer ends it. You hear about that template
  from `rename`, where it is what you were asking for. `borax config`
  compiles nothing, as before, and keeps reporting a template as the
  source text a configuration file gave it. Lookup tables are still
  loaded for both commands whichever templates are compiled, since
  loading one is what checks the `lookup` tokens in those that are.

### Removed

- **BREAKING:** The `--template` flag is gone, with no replacement.
  Set `templates.default` in a configuration file — the global
  `borax/config.toml`, or a `.borax.toml` in the directory of the files
  you are renaming, which is also how you set a per-entry-type template
  and how `[citation-keys]` and `[tables]` have always been set.

  The flag could reach only the `default` key, so it was a partial door
  into a structure the command line is otherwise not allowed to open,
  and the README has always said templates are settable in
  configuration files alone. It was also the sharpest case of a flag
  ending a run it could not affect: preflight compiled the filename
  templates for every command, so an unparseable `--template` aborted a
  `borax bib` run that renders no file name.

## [0.3.0] - 2026-08-29

### Removed

- **BREAKING:** `borax undo` is gone. It restored the exact names a run
  replaced, which is rarely the state you want: if a run named your
  files badly, what you want is a better name, not the old one. borax
  identifies files by their contents rather than their names, so the
  ordinary fix is to correct your template or the metadata and run
  `rename --apply` again — it finds the files wherever the previous run
  left them, including in subdirectories a template created, and you do
  not have to put anything back first.

  Reverting was also the less tidy of the two recoveries. It never
  corrected the ledger, so every file it moved back left an entry
  pointing at a path that no longer existed until the next
  `borax ledger rebuild`; and it never moved the sidecar, which had been
  written beside the new name, so it left a stray `.bib` next to the
  restored file. Re-running has neither problem: it re-admits to the
  ledger and writes the sidecar beside the name the file now carries.

  What undo read is unchanged and still written. Run logs still record
  every applied rename with the original path, the new path, and the
  content hash the file had when it moved, so the old-to-new mapping for
  any run is still there to read. The apply-run log is still mandatory,
  still flushed before the first file moves, and still cannot be
  suppressed with `--no-run-log`; `rename --apply` still refuses to run
  when it has nowhere to write one. The README now explains where the
  logs are and how to read a mapping back out of one.

  There is no migration and no shim: logs written by 0.2.0 are in the
  same format and stay readable. If you were relying on `undo`, the
  replacement is a corrected re-run for a naming mistake, and the run
  log for anything the names themselves were carrying.

## [0.2.0] - 2026-08-24

### Added

- `rename` now checks each file against the collection's ledger and
  skips the ones already admitted, so re-downloading a paper you
  already filed no longer files it twice. A file is a duplicate by
  content when its bytes are already recorded — caught after hashing,
  before any network request — and by work when a different file
  resolves to an identifier already recorded; both are reported with
  the path of the file they duplicate and leave the source untouched.
  The ledger lives at `.borax/ledger.jsonl` under the directory holding
  the nearest `.borax.toml`, so it travels with the files it accounts
  for. Only an applied run records anything; a preview records nothing.
  Disk decides: an entry whose file has been moved or deleted never
  keeps a file out, and a run that meets one warns that
  `borax ledger rebuild` is due. A missing or unreadable ledger costs
  the run its duplicate detection and nothing else, and `--no-ledger`
  turns it off outright.
- `borax ledger rebuild` regenerates the ledger from the collection
  itself, scanning its files and the sidecars beside them, so the
  accounting is derived rather than authoritative: delete
  `.borax/ledger.jsonl` and a rebuild restores it. Entries come out
  sorted by path, which makes two rebuilds of an unchanged collection
  byte-identical and a ledger something you can keep under version
  control and read a diff of. A file the scan cannot account for —
  no sidecar, no record borax wrote, or no readable bytes — yields no
  entry, so a rebuild also compacts away whatever has since been
  deleted or moved. The new ledger replaces the old one in a single
  atomic write, leaving the previous one intact if the rebuild cannot
  finish.
- Every run can now leave a record of itself. A run log is the same
  versioned event stream `--json` prints, written to
  `.borax/runs/<UTC-timestamp>-<command>-<dry|apply>.jsonl`, so a
  preview and the run that applied it sit next to each other in a
  listing and a directory of them sorts by time. The log is written
  whichever output format the terminal got, since the format chooses
  what you read, not what is recorded. `--no-run-log` suppresses it,
  and a log that cannot be written warns without failing the run —
  except for `borax rename --apply`, whose log is created before the
  first file moves and whose failure stops the run while every file
  still has its original name. An applying rename outside any
  collection writes that log under the XDG state directory instead, so
  a one-off rename in a downloads directory stays recoverable.

### Changed

- The `renamed` event carries the content hash of the file it moved,
  which is what lets the run log stand in for the journal undo used to
  read. The `unjournalable` skip reason is now `unrecordable`, naming
  what it always meant rather than the component that has been
  removed.
- `apply = true` in a configuration file is refused by name. It was
  already an error, as every key borax does not know is, but the
  message listed the keys it expected instead — which reads as a
  misspelling when the key is spelled correctly and simply may not be
  there. The refusal now says the flag must be passed on the command
  line, and says it wherever the key appears, including under
  `[rename]` where a reader who found `collision` would look for it
  next. The value makes no difference: `apply = false` is refused too,
  because what is refused is the file having an opinion about the
  apply gate at all.
- Citation keys now come from their own `[citation-keys]` table
  instead of the `[templates]` table that names files, and their
  built-in default is `[auth:lower][year]`, so a 2024 paper by Smith is
  cited as `smith2024`. Keys written by 0.1.0 were the rendered file
  name with whitespace and `,{}%` removed, so this release cites the
  same works under different keys; setting `citation-keys.default` to
  your file-name template restores the old keys exactly. Changing how
  files are named no longer changes how works are cited.
- A run reports as it goes. Each event is written when it happens
  rather than after the run has ended, so a network-bound run shows
  progress while it is bound and a slow run is distinguishable from a
  stopped one. A run ended by a configuration or usage error still
  emits neither `run-started` nor `run-finished`.
- `rename` and `bib` report one file at a time. A file's resolution,
  the rename planned or applied for it, and any sidecar written beside
  it now appear together and in the order the files were given,
  instead of every file's resolution followed by every file's rename —
  two lists that could not be paired, since the second omitted the
  files that were skipped. Output written to a shared master `.bib`
  is unchanged and still follows the files of its directory. No file
  receives a different name for this: collision suffixes stay
  deterministic and a preview stays identical to what `--apply` does.

### Removed

- The rename journal is gone. `borax undo` now reads the most recent
  apply-run log instead, which records the same moves with the same
  content hashes and is verified the same way, so undo behaves as it
  did — it simply reads the record every run already writes rather
  than a second file kept only for it. One consequence is worth
  stating plainly: an undo spanning the upgrade will not find its run.
  If you applied a rename with 0.1.0 and upgrade before undoing it,
  that run is not recoverable through borax. A `renames.jsonl` written
  by 0.1.0 is left exactly where it lies — not read, not converted,
  and not deleted behind your back.

## [0.1.0] - 2026-08-17

The first release. Nothing was published before it, so the "Changed"
and "Fixed" entries below record work done during development rather
than changes to a version anyone could have installed; "Added" opens
with the capability set as a whole and then lists what arrived late in
that development.

### Added

- The `borax` binary, with `resolve` (report what each file resolves
  to), `rename` (rename each file after its record), `bib` (write
  bibliography output and rename nothing), `undo` (move back everything
  the last applied run moved), `config` (print every setting and the
  layer it came from), and `cache` (report or clear the response
  cache).
- Renaming previews by default and moves only under `--apply`, never
  overwrites a file it did not plan for, and journals each move before
  making it so `borax undo` can reverse an interrupted run.
- Metadata resolution against Crossref, OpenAlex and arXiv, on a
  bounded thread pool, paced per service and cached on disk.
  Identifiers are extracted from the PDF in tiers — embedded metadata
  first, then a bounded scan of the text layer — and the pure-Rust
  extractor needs no native toolchain on any platform.
- A template language for filenames: bracket tokens over a record's
  fields, chained filters, `||` alternatives, `/` to file into
  subdirectories, and per-entry-type templates. Rendered names go
  through a sanitization pass that cannot be disabled, so a name is
  valid on Linux, macOS and Windows at once. The README documents the
  language.
- BibTeX output, either merged into one master `.bib` or written as a
  sidecar beside each file.
- Configuration in layers — built-in defaults, a global `config.toml`,
  a per-directory `.borax.toml`, environment variables, then flags —
  with `borax config` reporting the origin of every effective value.
- A JSON Lines event stream behind `--json` on every subcommand, as the
  integration contract for editor and watcher tooling.
- `borax resolve` emits the whole canonical record on each `resolved`
  event, not just the identifier it looked up — the JSON stream is the
  composable interface, and a caller should not have to go back to the
  network for what borax already resolved.
- Skip events for a metadata conflict report how similar the two titles
  were, so a near miss can be told from two unrelated works.

### Changed

- Files are resolved on a bounded pool of threads rather than one at a
  time, so a directory of PDFs is processed several times faster.
  `network.concurrency` sets the width. The event stream is unaffected:
  results are written back at their input position, so the JSON output
  is identical to a sequential run's and stays diffable. Requests stay
  paced per service, so more threads do not mean a faster rate of
  asking.

- A template containing `/` files a document into a subdirectory, which
  the templates specification always described and which failed for
  every file: the subdirectory was never created, and a name already
  taken inside it was invisible to the collision planner.

### Fixed

- A run spanning two directory trees applies each tree's own
  `.borax.toml` to its own files. Configuration was resolved once from
  the first input path, so the whole run took the first tree's
  overrides and swapping the arguments changed the result.
- `borax rename <dir>` finds `<dir>/.borax.toml` again. The override
  search took the parent of whatever path it was given, which is right
  for a file and one level too high for a directory, so naming a
  directory silently ran on the configuration above it while naming a
  file inside it did not.
- Title-conflict detection no longer skips a file over typographic
  differences it cannot control. Titles are compared as content words
  with both sides folded toward the lossiest encoding either could
  carry, and judged by similarity rather than exact equality, so a
  character the PDF's producer could not encode (a Greek letter, a
  Unicode hyphen) no longer reads as a different work.
- A PDF's title claims are no longer trusted blindly. Placeholders
  (`untitled`, `PowerPoint-Präsentation`), filenames, and bare
  identifiers no longer count as evidence of disagreement, and a
  document's XMP and Info titles are both considered rather than one
  preferred — agreement by either clears the file.

- The master `.bib` is replaced atomically instead of being truncated
  and rewritten, so a run that dies mid-write leaves the previous
  bibliography intact rather than a truncated one.
- Sidecars are named `paper.pdf.bib` rather than `paper.bib`, and a file
  borax did not write is never overwritten — the old name is the one a
  person keeping notes on `paper.pdf` by hand would have chosen.
- Applied renames are journaled before the file moves, not after the
  batch finishes. A run interrupted partway through no longer leaves
  moves that `borax undo` cannot see, and a move that cannot be
  journaled is no longer made.
- A named input path that does not exist is reported as a skipped file.
  It used to contribute nothing, so a mistyped filename produced an
  empty batch that exited 0, indistinguishable from a clean run.
- A `.borax.toml` that exists but cannot be read now ends the run
  instead of being silently ignored as though it were absent.
- Sources borax has no client for (DataCite, PubMed) are no longer in
  the default set and are refused by configuration, naming the sources
  that do work. Selecting one used to resolve nothing and report every
  file unresolvable.

- Requests to a service are now spaced out. `--min-interval-ms` and
  `network.min-interval-ms` were accepted, validated and reported while
  changing nothing, so borax asked for polite-pool access and
  rate-limited none of its traffic.
- Successful source responses are now cached on disk, not just the
  content-hash index. Two different files carrying the same DOI, or a
  re-run after the index is cleared, no longer re-query the service.

- arXiv identifiers issued before April 2007 (`math.GT/0309136`)
  resolve again. The reader took the abstract URL's last path segment,
  which drops the archive that is part of such an identifier, so every
  pre-2007 preprint failed with a malformed-response error.

[Unreleased]: https://github.com/myshevchuk/borax-tools/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/myshevchuk/borax-tools/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/myshevchuk/borax-tools/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/myshevchuk/borax-tools/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/myshevchuk/borax-tools/releases/tag/v0.1.0
