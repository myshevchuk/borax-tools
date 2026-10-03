# rename Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
### Requirement: Preview is the default; apply is explicit
A rename run SHALL move no file without an explicit decision to move it.
There are exactly two such decisions: `--apply`, given to a batch run,
which carries out the plan that run would otherwise preview; and a yes,
given in an interactive run to a question naming the file and the target
it would move to, which carries out that one move. No default, no
configuration value and no answer that was never put to a person is a
decision.

A batch rename run SHALL, by default, print the planned old→new mapping
and mutate nothing on disk. Renames SHALL only be executed in a batch
run when `--apply` is given.

An interactive run SHALL show each proposed move before it happens, as
the question that decides it. A batch run SHALL show one when it is
previewing, which is the run an `--apply` run repeats; `--apply` itself
carries out the plan without showing it again.

#### Scenario: Default run is a preview
- **WHEN** `borax rename --batch` runs over a directory without
  `--apply`
- **THEN** the old→new mapping is printed and every file keeps its
  original name

#### Scenario: An unanswered question moves nothing
- **WHEN** an interactive run is proposing a move and the operator has
  not yet answered
- **THEN** the file keeps its original name

### Requirement: Collisions never overwrite
A planned rename whose target path already exists SHALL never overwrite
the existing file. The planner SHALL detect collisions among the batch's
own targets and against existing files, including on case-insensitive
filesystems. Per configuration, a collision is resolved by suffixing
(deterministic `a`, `b`, `c`… suffixes) or by skipping with a reported
reason. A target that already contains a byte-identical file SHALL be
reported as already-named rather than treated as a collision.

#### Scenario: Two records render the same filename
- **WHEN** two different files in one batch render identical target names
- **THEN** with suffixing configured, the second receives a deterministic
  suffix, and with skipping configured, the second is skipped with reason
  "target collision"

#### Scenario: Case-insensitive collision
- **WHEN** a target differs from an existing file's name only by letter
  case
- **THEN** the planner treats it as a collision on all platforms

### Requirement: A template may file a document into a subdirectory
A rendered name containing `/` SHALL be treated as a path relative to
the directory the file belongs in, and any part of it that does not
exist SHALL be created when the rename is applied. Sanitization
prevents such a target from leaving that directory.

The directory a file belongs in is the collection root, where the run
has one. A rendered subdirectory therefore says where in the collection
a file belongs rather than adding a level to wherever it sits now: a
file already filed there is at its target and is already named, one
filed under a subdirectory the template no longer renders moves across
into the new one rather than deeper into the old, and one further down
the tree is filed where it belongs.

A run with no collection root SHALL join a rendered subdirectory to the
file's own directory, having no other base to use.

A rendered name containing no `/` SHALL leave the file where it is,
whatever the run's base: a rendered subdirectory says where in the
collection a file belongs, and a plain name says what to call it where
it already sits.

Collisions SHALL be detected across every file a run files into one
directory, whichever directories those files came from. A run filing
two files from different directories into one journal directory SHALL
suffix the second exactly as it would if both had started in the same
place, and SHALL do so when previewing as well as when applying.

Collisions SHALL be detected where the file is going, not where it came
from: a name already taken in the target subdirectory blocks or suffixes
exactly as one in the file's own directory does, and two files heading
for the same name in different subdirectories do not collide.

#### Scenario: Filing by journal
- **WHEN** a template renders `nature/smith2024` for a file in `~/lib`
- **THEN** the file is moved to `~/lib/nature/smith2024.pdf`, creating
  `~/lib/nature` if it is not there

#### Scenario: The nested name is taken
- **WHEN** a different file already sits at the nested target
- **THEN** it is suffixed or skipped by the collision policy, exactly as
  it would be in the file's own directory

#### Scenario: Re-running a filed collection changes nothing
- **WHEN** a run files `zeng2026.pdf` into `Nature/` under the
  collection root and the same run is repeated over the same collection
- **THEN** the file is reported already named and stays at
  `Nature/zeng2026.pdf`, rather than being proposed
  `Nature/Nature/zeng2026.pdf`

#### Scenario: Filing outside a collection
- **WHEN** a run over a directory with no `.borax.toml` above it renders
  `sub/smith2024` for a file in that directory
- **THEN** the file is filed into `sub/` beneath it, as it is today

#### Scenario: The journal a file is filed under changes
- **WHEN** a file sits at `Nature/zeng2026.pdf` and its template now
  renders `Science/zeng2026`
- **THEN** the file is proposed `Science/zeng2026.pdf` beside the
  `Nature` directory, not inside it

### Requirement: Applied renames are recorded in the run log
Every applied rename SHALL be recorded as a rename event (original path,
new path, file content hash, timestamp, run identifier) in the run's
apply-run log — the mandatory record defined by the `run-logs`
capability. There is no separate journal file.

Each such event SHALL be written and flushed to the log before the move
it records is made, so that a run interrupted at any point has already
recorded every move it may have made. A move whose event cannot be
written SHALL NOT be made.

A move that then fails SHALL be reported as it is today, and its failure
SHALL be written to the log after the rename event it followed, so a
reader of the log sees both the intent and its outcome.

#### Scenario: Rename events written on apply
- **WHEN** a run applies three renames
- **THEN** the apply-run log contains three rename events sharing one
  run identifier, each written before the file it names was moved

#### Scenario: A move recorded and then refused
- **WHEN** the filesystem refuses a move whose rename event was already
  written
- **THEN** the log carries that rename event followed by the skip
  recording the failure

### Requirement: A run reports one file at a time
A run SHALL report each file completely before it begins the next: the
file's resolution, the rename planned or applied for it, and any sidecar
written beside it appear together and in the order the files were given.
A run SHALL NOT report every file's resolution first and every file's
rename afterwards, which leaves the two lists unpairable — the second
omits the files that were skipped, and a reader cannot recover which
verdict belongs to which file.

Bibliography output written to a single shared destination is exempt,
being work about the batch rather than about a file.

Ordering is the only thing this constrains. Which name a file receives
SHALL NOT depend on it: collision suffixes stay deterministic, and a
preview stays identical to what the same run with `--apply` would do.

#### Scenario: A file's verdict and its fate are adjacent
- **WHEN** `borax rename` runs over a directory where some files
  resolve and others are skipped
- **THEN** each resolved file's rename line immediately follows its own
  resolution line, and no file's lines are separated by another file's

#### Scenario: Ordering does not move a suffix
- **WHEN** two files in one directory render the same target name
- **THEN** the first file given receives the unsuffixed name and the
  second the suffix, exactly as when the batch was planned as a whole

### Requirement: A rename run on a terminal is interactive
A rename run SHALL be interactive when its standard input is a terminal, its output is human-readable, `--apply` was not given, and the `rename.batch` setting is off, which is its default. Every other rename run SHALL be a batch run, behaving as a rename run behaved before interactive runs existed.

`--apply` SHALL select a batch run by itself, so that `borax rename
--apply <path>` carries out a plan without asking whatever the setting
says. Giving `--apply` together with `--no-batch` SHALL be a usage error
naming both flags, refused before anything is read or resolved, because
the pair asks both for a plan carried out unasked and for every move to
be asked about.

The mode SHALL be decided before the run's first event and SHALL NOT
change during the run.

#### Scenario: Terminal default
- **WHEN** `borax rename papers/` runs from a terminal with no setting
  or flag selecting batch
- **THEN** the run is interactive and `run-started` reports
  `interactive: true`

#### Scenario: Apply keeps its meaning
- **WHEN** `borax rename --apply papers/` runs from a terminal
- **THEN** the run is a batch run that moves every file its plan names,
  asking nothing

#### Scenario: Configured batch default
- **WHEN** configuration sets `rename.batch = true` and `borax rename
  papers/` runs from a terminal
- **THEN** the run is a batch preview, and `borax rename --no-batch
  papers/` from the same terminal is interactive

#### Scenario: Piped and JSON runs never ask
- **WHEN** `borax rename --no-batch papers/` runs with stdin redirected,
  or `borax rename --json papers/` runs from a terminal
- **THEN** the run is a batch preview and asks nothing

#### Scenario: Apply against no-batch
- **WHEN** `borax rename --apply --no-batch papers/` runs
- **THEN** the invocation is rejected as a usage error naming both flags,
  no event is emitted, and the exit code is the fatal one

### Requirement: An interactive run asks before each move
An interactive run SHALL put a question to the operator for each file whose plan is a move to a free target, before the file is moved, cited or admitted. The question SHALL name the file and the target it would move to, including any collision suffix, and SHALL offer at least: rename, skip, and quit.

A rename answer SHALL carry out the move as an applying batch run would
carry it out, and report it the same way. A skip answer SHALL leave the
file untouched and report it skipped with reason `declined`.

A proposed target SHALL be claimed only when the operator accepts it. A
declined proposal SHALL leave the run's plan as though that file had
proposed nothing, so no later file is suffixed on account of a move that
did not happen.

A file whose plan is not a move — already named, blocked by a collision
under the skip policy, or unnameable — and a file that did not resolve
SHALL be reported as a batch run reports it, without a question, unless
another requirement gives it one.

Files SHALL be asked about in the order a batch run would report them,
and each file's resolution, question, fate and sidecar SHALL remain
adjacent.

#### Scenario: Accepting a proposal
- **WHEN** the operator answers rename to the question for `a.pdf →
  smith2024.pdf`
- **THEN** the file is moved, a `renamed` event names both paths and the
  content hash, and an artifact record names its new library-relative
  path and the item it belongs to, as an applied rename would

#### Scenario: Declining a proposal
- **WHEN** the operator answers skip to the question for `a.pdf`
- **THEN** `a.pdf` keeps its name and is reported skipped with reason
  `declined`

#### Scenario: A declined name stays free
- **WHEN** `a.pdf` and `b.pdf` in one directory both render
  `smith2024.pdf`, and the operator declines `a.pdf`
- **THEN** the question for `b.pdf` proposes `smith2024.pdf` without a
  suffix

#### Scenario: A file with nothing to decide
- **WHEN** an interactive run reaches a file that already carries the
  name its record implies
- **THEN** it is reported as already named and no question is put

### Requirement: Quitting an interactive run leaves the rest untouched
Answering quit, or interrupting a question, SHALL end an interactive run at that file: the file and every file after it SHALL be left untouched, nothing further SHALL be resolved, and the run SHALL close with `run-finished` as a completed run does.

`run-finished` SHALL count as unreached the file the run ended at and
every file after it, since none of them was given a fate. A run that
leaves any file unreached SHALL exit with the partial-success code.

Work already done SHALL stand: files moved before the quit stay moved
and recorded, and bibliography output for the files that were visited is
written as it would have been had the run finished.

#### Scenario: Quit midway
- **WHEN** the operator quits at the third of ten files, having renamed
  the first two
- **THEN** the first two stay renamed, the other eight keep their names,
  `run-finished` counts eight unreached, and the exit code is the
  partial-success code

#### Scenario: Interrupt at a prompt
- **WHEN** the operator presses Ctrl-C while a question is showing
- **THEN** the run ends as it would on quit, closing with `run-finished`

### Requirement: A file already carrying its name is not a skip
A file whose current name is the name its record implies under the configuration in force, or whose target is occupied by a byte-identical file, SHALL be reported as an `already-named` outcome of its own, and SHALL NOT be reported or counted as skipped.

`run-finished` SHALL count already-named files in a total of their own.
Already-named files SHALL NOT make a run exit with the partial-success
code: a run over files that are all already named exits 0.

Whether a file is already named SHALL be decided by rendering its record
through the templates in force for its directory, never by whether the
library holds an artifact record for it, so a file named under an
earlier template is not already named under a changed one.

An applying run SHALL write an artifact record for an already-named file
inside the library, as it does for one it moved: the run holds the
record, agrees with the name, and a library already in good order would
otherwise never be recorded at all. Nothing is moved, and a preview
writes no record.

#### Scenario: Re-run over a folder in order
- **WHEN** `borax rename --batch` runs over a directory in which every
  file already carries the name its record implies
- **THEN** each file is reported `already-named`, `run-finished` counts
  them as named and none as skipped, and the exit code is 0

#### Scenario: Template changed since the last run
- **WHEN** a file was renamed under one filename template and the
  configuration now holds a different one
- **THEN** the file is not already named, and a rename to the new
  template's name is planned for it

#### Scenario: An already-named file is recorded
- **WHEN** `borax rename --apply` runs over a library whose files all
  already carry the names their records imply and none of which has an
  artifact record
- **THEN** nothing is moved, every file is reported `already-named`, the
  exit code is 0, and each file now has an artifact record naming an
  item

### Requirement: An interactive run passes over already-named files
An interactive rename run SHALL, when the `rename.skip-named` setting is on, render nothing to the operator for a file that is already named: no resolution line, no outcome line and no question. The setting SHALL default to on, and `--no-skip-named` SHALL turn it off for a run.

A passed-over file SHALL still be reported in the event stream and the
run log exactly as when the setting is off, and the run's closing
summary SHALL state how many files were passed over.

With the setting off, an interactive run SHALL render already-named
files as a batch run does, and SHALL put the question an already-named
file is owed — keep its name, supply a different identifier, or quit —
so that a file named from the wrong record can be put right. The
setting SHALL NOT affect batch runs.

#### Scenario: New papers among renamed ones
- **WHEN** an interactive run reaches a directory holding three files
  that need renaming and two hundred already named
- **THEN** the operator sees the three questions and no line about the
  other two hundred, and the summary reports two hundred already named

#### Scenario: Showing named files
- **WHEN** the same run is started with `--no-skip-named`
- **THEN** each already-named file is shown as already named, and is
  asked about

#### Scenario: The run log keeps the full account
- **WHEN** an interactive run passes over an already-named file
- **THEN** its run log carries that file's resolution and its
  `already-named` event

### Requirement: A collision suffix never lands on the file's own name
A collision suffix candidate equal to the name the file already carries SHALL be reported as already named rather than planned as a move. A file's own name is exempt from the collision check while its own decision is made, so the suffix ladder can reach it; arriving there means the file already sits where the plan would put it.

#### Scenario: The second of two works, re-run
- **WHEN** two works render `smith2024.pdf`, the second was filed as
  `smith2024a.pdf`, and the directory is run again
- **THEN** `smith2024a.pdf` is reported already named, and no move onto
  its own name is planned or attempted

#### Scenario: A plain name does not move a filed file
- **WHEN** a template rendering no `/` is applied to a file sitting in a
  subdirectory of the collection root
- **THEN** the file is renamed where it is, not hoisted to the root

#### Scenario: Two directories filing into one
- **WHEN** two files in different directories of one collection both
  render `Nature/zeng2026`
- **THEN** the second is suffixed, in a preview and in an applying run
  alike

### Requirement: An interactive question shows what the answer rests on
Before an interactive run puts a question about a resolved file, it SHALL show the operator a description of that file comprising, as far as the run knows them: the file's name and its position among the run's files; the identifier used and where it was found; the services that supplied the record; the record's type, title, authors, date of issue, container title, volume, issue and pages; every title the file claims for itself, with where each was read; and the proposed name.

A field the record does not hold, or holds as text that is empty or
only spaces, SHALL be left out rather than shown empty. A field the
record does hold SHALL be shown whether or not its neighbours are: a
volume and pages are reported when the record names no container to
have them.

The identifier SHALL be named with where it was found only where the
run found it. Where the content index answered and nothing was looked
up, the description SHALL name the record's own identifier without
claiming where it came from. No title SHALL be truncated, and no identifier SHALL be broken
across lines: one that cannot be read back in one piece is no longer
evidence.

The file SHALL be named as the run names it elsewhere, so that two
files sharing a name in different directories are told apart by the
question rather than only by their place in the run. When the proposed name carries a
collision suffix, the description SHALL say which rendered name was
taken.

The description SHALL show nothing about the resolution that the file's
`resolved` event does not carry, so that a reader of the stream and the
operator at the terminal are told the same things about a file. What
belongs to the question rather than to the resolution — the file's
position in the run, the proposed name, and the note saying which
rendered name a collision suffix stepped around — is the question's to
show and is not held to that.

The identifier the description names SHALL be the one the run looked
up, with the pass that found it. A record may carry identifiers that
were not looked up, and naming one of those as found would be evidence
the run does not have.

The description SHALL be written where the question is written, so that
a run whose standard output is redirected still asks its questions with
the evidence attached.

Every value taken from a record or from a file's own metadata SHALL be
escaped before it is written, so that a control character is shown
rather than acted on. A file's metadata is written by whoever made the
file; an escape sequence in a title must not be able to redraw the
question the operator is answering.

No line SHALL exceed the width the description is rendered to, except
an identifier, which is never broken.

The description SHALL appear only in interactive runs, and SHALL NOT
change which questions are put or what their answers do.

#### Scenario: Deciding with the record in view
- **WHEN** an interactive run is about to ask whether to rename a file
  resolved through Crossref from a DOI in its text layer
- **THEN** the operator is shown the DOI, that it came from the text
  layer, that Crossref supplied the record, the record's title and
  authors, the titles the file's metadata claims, and the proposed name,
  before the question

#### Scenario: A suffixed proposal
- **WHEN** the proposed name is `smith2024a.pdf` because `smith2024.pdf`
  is taken
- **THEN** the description says that `smith2024.pdf` is taken

#### Scenario: A record from an earlier run
- **WHEN** the content index answers for the file
- **THEN** the description says the identifier comes from an earlier
  run, names the services the record's provenance records, and says no
  title was read from the file

#### Scenario: The record's identifier is not the one that was found
- **WHEN** a file's arXiv identifier is found in its text layer and the
  record resolved for it carries a DOI as well
- **THEN** the description names the arXiv identifier as the one found
  in the text layer

#### Scenario: Redirected output still asks with evidence
- **WHEN** an interactive run's standard output is redirected to a file
- **THEN** each question and its description appear at the terminal,
  and the file receives the event stream

#### Scenario: Two files of one name
- **WHEN** a run spans two directories that each hold `paper.pdf` and
  both are proposed a move
- **THEN** the two questions name their files distinguishably

#### Scenario: A title carrying terminal escapes
- **WHEN** a file's embedded title contains an escape sequence that
  would clear the screen
- **THEN** the description shows the sequence as text and the question
  above it stays on the screen

#### Scenario: A record with no container
- **WHEN** a record holds a volume and pages but names no container
- **THEN** the description reports the volume and the pages

#### Scenario: A cached answer names no origin
- **WHEN** the content index answers for a file
- **THEN** the description names the record's identifier without saying
  where it was found, and says the record comes from an earlier run

### Requirement: An interactive run asks about files it could not settle
An interactive rename run SHALL put a question to the operator, in addition to the questions for proposed moves, for each file whose content hash is known and which: had no identifier found in it; had an identifier no service holds; could not be read as a PDF or is encrypted; resolved to a record its claimed titles conflict with; or resolved to an identifier an item already in the library carries.

A file with no identifier, an unreadable body, or an identifier no
service holds SHALL be offered: supply an identifier, skip, or quit. A
file whose resolution failed without settling whether any service holds
its identifier — a service unreachable, rate-limited, or answering
unusably — SHALL additionally be offered a retry, first, and its
question SHALL say that the services did not answer rather than that
the identifier is unknown. A file with a
conflict whose proposal is a move SHALL additionally be offered renaming
to the proposed target despite the conflict, and that choice SHALL name
the target and SHALL NOT be the default. Every question about a proposed
move SHALL additionally offer supplying a different identifier.

A file whose own resolution lands on an identifier an item already
carries SHALL be
offered: file it as another artifact of that item, skip, or quit. The
question SHALL name the item and a path already recorded against it
that still holds a file, so that the operator is deciding about a work
they can see they hold, and filing SHALL NOT be the default. An item
with no such path is not a work duplicate at all and SHALL NOT produce
this question: there is no file to compare against, and the incoming
one is that item's first artifact. Accepting SHALL return the file to
planning, where the ordinary question about its move is put; only then
is it moved and recorded. Declining SHALL report it with the
work-duplicate reason a batch run would have given. This is the one
question borax asks about a duplicate, and only an operator can answer
it: a second artifact of a work and an unwanted second copy of it are
one thing in the bytes and two in the intention.

A record the operator reached rather than one the run resolved — an
identifier they supplied, a lookup they retried, a record they accepted
over a conflict — SHALL be checked against the library the same way and
SHALL NOT produce that question. Where it lands on a work the library
already holds a file of, the run SHALL say so, naming the same item and
recorded path the question would have named, and SHALL put the ordinary
question about the file's move again; the answer given after it stands,
and a move carried out then admits the file as another artifact of that
item. The collision SHALL be stated once, so that a second answer is
never asked for twice and the move is never unreachable. Supplying an
identifier is the operator saying what the file is, which is the
statement the filing question exists to obtain, so what is left is not
a decision to ask for but a fact they have not been told.

#### Scenario: A supplied identifier lands on a work already held
- **WHEN** an interactive run's operator supplies the DOI of a work the
  library already holds a file of, and answers rename
- **THEN** the run reports the item and the recorded path it collided
  with, puts the question about the move again, and a second rename
  answer moves the file and records it as another artifact of that
  item, with no second item minted

When `rename.skip-named` is off, an already-named file SHALL be put to
the operator with the choices keep its name, supply a different
identifier, or quit. This is the one question an already-named file is
asked, and it is asked only when that setting is off; a run passing
over named files asks nothing about them.

A file reported as a content duplicate, or whose content hash is
unknown, SHALL NOT be asked about. The same bytes are already in the
library, so there is nothing an operator could add: the question above
is asked about a work the library holds, never about a file it holds.

Skipping any of these files SHALL leave it untouched and report it with
the reason a batch run would have given, not as `declined`. A file that
had a move on offer and was skipped after a supplied identifier led
nowhere SHALL be reported `declined`, since a move is what was
declined.

A supplied identifier that resolves SHALL leave the file in whatever
situation its new record puts it: a move to offer, a target taken, a
name that renders empty, or a file already named. A supplied
identifier that does not resolve, and an input the operator abandons,
SHALL leave the file exactly as it was, with the record and the choices
it already had.

#### Scenario: Supplying an identifier for an unidentified file
- **WHEN** an interactive run finds no identifier in an author manuscript
  and the operator supplies the DOI of its published version and then
  answers rename
- **THEN** the file is renamed from the published record, and the run
  log records a `resolved` event whose `lookup` names the operator as
  the identifier's origin and a `renamed` event for it

#### Scenario: Skipping an unidentified file
- **WHEN** the operator skips a file with no identifier
- **THEN** it keeps its name and is reported skipped with the reason
  its extraction gave: `text-without-identifier` where its pages hold
  text, or `no-text-layer` where they hold none

#### Scenario: A reference's DOI caught
- **WHEN** a proposal comes from a DOI that belongs to a paper the file
  cites, and the operator supplies the file's own DOI instead
- **THEN** a new proposal is made from the file's own record, and the
  name first proposed is not claimed

#### Scenario: Re-identifying a named file
- **WHEN** an interactive run with `--no-skip-named` reaches a file named
  from the wrong record, and the operator supplies the right identifier
  and answers rename
- **THEN** the file is renamed from the right record, and if it has an
  artifact record that record is re-linked to the item for the supplied
  record, so its name and what the library says it is are corrected
  together

#### Scenario: Filing a second artifact of a work already held
- **WHEN** an interactive run reaches a scan of a paper whose published
  PDF the library already holds under the same DOI, and the operator
  answers that it is another artifact of that item
- **THEN** the question names the item and the recorded path of the
  artifact already linked to it, the ordinary move question follows, and
  answering rename files the scan and records it against that same item

#### Scenario: Declining a second copy
- **WHEN** the operator answers skip to that question
- **THEN** the file keeps its name and is reported with the
  work-duplicate reason a batch run would have given, not as `declined`

#### Scenario: A content duplicate is still never asked about
- **WHEN** an interactive run reaches a file whose hash is already in an
  artifact record's history
- **THEN** it is reported a content duplicate and no question is put

### Requirement: A file's verdict follows the operator's decision
In an interactive run, the events reporting a file's resolution and fate SHALL be emitted once the operator's decisions about that file are made, and SHALL describe the outcome of those decisions: a file SHALL NOT be reported both skipped and renamed, and a record the operator abandoned for another identifier SHALL NOT be reported as the file's resolution.

A file renamed from a record accepted over a conflict SHALL be reported
with a `resolved` event carrying the conflict it overrode — the field,
both values and the similarity — followed by its `renamed` event.

#### Scenario: One report per file
- **WHEN** a file's extracted identifier conflicts, the operator supplies
  another identifier, and renames the file from its record
- **THEN** the stream carries one `resolved` event for the file, for the
  supplied identifier's record, followed by its `renamed` event, and no
  `skipped` event for it

#### Scenario: A supplied identifier that leads nowhere
- **WHEN** a file with a record and a move on offer is given another
  identifier, and no service holds it
- **THEN** what the services answered is shown, the file's original
  record is still what is on offer, and the operator may accept it,
  supply another, or skip

#### Scenario: Services that could not answer
- **WHEN** resolution failed because no service could be reached
- **THEN** the question says so and offers a retry before offering to
  supply an identifier

### Requirement: A candidate the operator abandoned leaves nothing behind
A record resolved from a supplied identifier and then abandoned — another identifier supplied, the file skipped, or the run quit — SHALL NOT be reported as the file's resolution, written to the content index, or cited. No sidecar SHALL be written from it and no entry SHALL be merged into the master bibliography from it.

Only the record a file's decision settled on SHALL reach any of those.
A file whose own resolution stands is cited from that record as it
would be in a batch run, whatever candidates were shown along the way.

#### Scenario: A wrong identifier is not written beside the file
- **WHEN** the operator supplies an identifier, sees a record for
  another paper, and supplies a different one instead
- **THEN** no sidecar and no bibliography entry is written from the
  abandoned record, and the file's own sidecar, when it is written,
  carries the record the operator accepted

### Requirement: An interactive question says whether the library answered
The description an interactive run shows before a question SHALL say when the file's record is its library item's, and SHALL state what the library could not answer for when it could not.

Where the file's `resolved` event reports `library` as its
`record_retrieval`, the description SHALL name the record's own
identifier without claiming
where it was found, since nothing was looked up; SHALL name on its
`record` line the services the item's provenance records followed by
`from the library`, or the library alone where the provenance names no
service; and SHALL show that no title was read from the file. It SHALL
NOT describe a library answer as coming from the file or from an
earlier run.

Where the file's resolution event reports a library answer of a kind the
library could not answer with, the description SHALL carry a line
labelled `library` stating the problem as the file's human output line
states it — after the `record` line for a resolved file, and after the
reason for a file that was skipped.

The description SHALL remain a rendering of the file's resolution
event: it shows what that event's `library` and `record_retrieval`
carry, and nothing the event does not. What the questions offer and what their
answers do SHALL NOT change: a file the library answered for is asked
about its move as any resolved file is, and one already named is passed
over under `rename.skip-named` as any already-named file is.

#### Scenario: Deciding on a library answer
- **WHEN** an interactive run is about to ask whether to rename a
  tracked file whose item's provenance names Crossref
- **THEN** the description names the item's DOI without saying where it
  was found, its `record` line reads `Crossref, from the library`, and
  it says nothing was read from the file

#### Scenario: A library that could not answer
- **WHEN** an interactive run is about to ask about a file whose record
  links to an item the library does not hold, and which resolved from
  the content index instead
- **THEN** the description carries a `library` line naming the artifact
  and the missing item, and its `record` line says the record comes from
  an earlier run

#### Scenario: A file nothing identified, whose library could not answer
- **WHEN** an interactive run offers to supply an identifier for a file
  that carries none and whose record links an item the library does not
  hold
- **THEN** the description carries a `library` line naming the artifact
  and the missing item after the reason the file was not identified

#### Scenario: A tracked file already named is passed over
- **WHEN** an interactive run with `rename.skip-named` on reaches a
  tracked file that already carries the name its item's record implies
- **THEN** nothing is shown or asked about it, and its `resolved` and
  `already-named` events are in the run log

### Requirement: An interactive description says whether the file's titles were read
The description an interactive run shows before a question SHALL state which state the file's titles are in: the titles read, each with where it was read; that the file was read and claims no title; that the file could not be opened, with the reason; or that the file was not read, with why.

The description of a file skipped because extraction found no identifier
in it, or because no service held its identifier, SHALL show the file's
titles in the same way, since they are what an operator supplying an
identifier has to go on, and they are carried by the file's `skipped`
event.

The description of a file extraction found no identifier in SHALL say
which way extraction failed: the pages read held no text, or held text
and no identifier. An encrypted file SHALL be described as encrypted,
not as unreadable.

The description SHALL remain a rendering of the file's resolution event,
as the requirement "An interactive question shows what the answer rests
on" states: each of these is read from the event's `extraction`
section, and none is worked out anew.

#### Scenario: A titled blank scan
- **WHEN** an interactive run offers to supply an identifier for a PDF
  whose single page is blank and whose document information carries the
  title `A Title`
- **THEN** the description says no text was found on the pages read,
  and shows `A Title` as the title the file claims, read from the
  document information

#### Scenario: A file that claims no title
- **WHEN** an interactive run asks about a file that was opened, claims
  no title, and resolved from its DOI
- **THEN** the description says the file claims no title in its
  metadata, rather than that nothing was read

#### Scenario: A record from an earlier run
- **WHEN** an interactive run asks about a file the content index
  answered for
- **THEN** the description says the file's titles were not read because
  an earlier run answered

#### Scenario: An encrypted file
- **WHEN** an interactive run offers to supply an identifier for a PDF
  encrypted under a user password
- **THEN** the description says the file is encrypted, and that its
  titles could not be read, with the reason

