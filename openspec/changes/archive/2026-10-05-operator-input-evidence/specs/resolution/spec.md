## ADDED Requirements

### Requirement: A resolution reports the identifiers an operator supplied
Every `resolved` event and every resolution `skipped` event SHALL carry an `identifier_input` section, between `extraction` and `lookup`, reporting every text an operator submitted at the identifier prompt for the file, or why none was submitted.

When at least one text was submitted, the section SHALL be `supplied`,
with `submissions`, `used` and `displaced`. `submissions` SHALL list
every text the prompt returned for the file, in the order typed,
whether it parsed or not. Each entry SHALL carry:

- `submission`: its number, counting from 1 within the file, with no
  gaps;
- `raw`: its text, as the prompt returned it;
- `syntax`: `parsed` with the normalised identifier, or `rejected` with
  why, as the requirement "Supplied text that names no identifier says
  why" states.

A prompt the operator abandons submits nothing and SHALL add no entry.

When nothing was submitted, the section SHALL be not attempted, with
one of these reasons:

- `content-duplicate`, for a content duplicate;
- `not-asked`, where no question was put about the file. That is every
  verdict of a batch run, of `resolve` and of `bib`, and every file an
  interactive run settled or reported without a question;
- `not-supplied`, where a question was put about the file and nothing
  was submitted.

A run that takes no operator input SHALL still carry the section, not
attempted, and SHALL NOT leave it out.

#### Scenario: A batch run takes no operator input
- **WHEN** `borax resolve` resolves one file and skips another because
  no identifier was found in it
- **THEN** both events carry `identifier_input` not attempted with
  reason `not-asked`

#### Scenario: Asked, and nothing supplied
- **WHEN** an interactive run asks about a file with no identifier, and
  the operator chooses to supply one, enters an empty line, and then
  answers skip
- **THEN** the file's `skipped` event carries `identifier_input` not
  attempted with reason `not-supplied`

#### Scenario: Every text is a submission
- **WHEN** the operator types `see email from Anna` and then
  `10.1000/xyz` at the identifier prompt for one file
- **THEN** that file's `identifier_input` is `supplied`, with submission
  1 whose `raw` is `see email from Anna` and whose `syntax` is
  `rejected`, then submission 2 whose `raw` is `10.1000/xyz` and whose
  `syntax` is `parsed` with identifier `doi:10.1000/xyz`

#### Scenario: A passed-over file is reported as a batch run reports it
- **WHEN** an interactive run with `rename.skip-named` on passes over a
  file already carrying its name
- **THEN** its `resolved` event carries `identifier_input` not attempted
  with reason `not-asked`, as a batch run's would

### Requirement: Each submission reports what it came to
Each submission an operator made for a file SHALL carry its own outcome — its `lookup`, its `record_retrieval`, its `match_check`, its `acceptance`, and the `record` it reached where it reached one — except the one submission whose record the event itself reports, which `used` SHALL name and whose outcome SHALL be the event's own sections.

A submission's outcome SHALL be:

- **For a text that did not parse:** its `lookup`, its `match_check`
  and its `acceptance` not attempted with reason `unparsed`, and its
  `record_retrieval` `null`. Nothing was sent to any service.
- **For an identifier no service held, or none could be asked about:**
  its `lookup` as any lookup is reported, with origin `operator`. Its
  `match_check` and its `acceptance` not attempted with reason
  `no-record`, and its `record_retrieval` `null`.
- **For an identifier that reached a record:**
  - its `lookup`;
  - its `record_retrieval`, naming the service and whether its response
    cache or the network answered;
  - its `match_check`;
  - the `record`, in full;
  - its `acceptance`: `rejected` when the record was on offer and
    stopped being, because the operator answered skip or a later
    submission reached a record that took its place on offer; or not
    attempted with reason `no-move` when the record led to no move and
    so was never put to the operator.

A record on offer SHALL NOT be rejected by a later text that is
refused, by a later identifier that reaches no record or a record with
no move, or by the run's saying the library already holds a file of its
work. It stays on offer, as the `rename` requirement "A file's verdict
keeps every identifier its operator supplied" states.

`used` SHALL be a submission number exactly when the event's `lookup`
names the operator as the identifier's origin. When it is:

- that submission's `syntax` SHALL be `parsed`, and its identifier
  SHALL equal the event's `lookup.identifier`;
- its entry SHALL carry only `submission`, `raw` and `syntax`;
- the event's `lookup`, `record_retrieval`, `match_check`, `acceptance`
  and `record` SHALL be its outcome, stated once.

Otherwise `used` SHALL be `null`.

When `used` is a number, `displaced` SHALL carry the file's own
`lookup`, `record_retrieval` and `match_check`. Where the file's own
resolution reached a record, resolved or refused, it SHALL carry that
`record` too. These are the facts the operator's record took the place
of in the event's sections, and a correction SHALL NOT erase them.
Otherwise `displaced` SHALL be `null`.

#### Scenario: A refused text
- **WHEN** the operator types `not-an-identifier` at the identifier
  prompt
- **THEN** its submission's `syntax` is `rejected` with reason
  `unrecognised`, its `lookup`, `match_check` and `acceptance` are not
  attempted with reason `unparsed`, and its `record_retrieval` is
  `null`

#### Scenario: A supplied identifier nobody holds
- **WHEN** the operator supplies a DOI that Crossref and OpenAlex both
  say they do not hold
- **THEN** its submission's `lookup` is attempted with origin
  `operator` and both services as `not-found`, its `match_check` and
  `acceptance` are not attempted with reason `no-record`, and its
  `record_retrieval` is `null`

#### Scenario: A candidate skipped
- **WHEN** the operator supplies a DOI for which Crossref returns a
  record over the network, and then answers skip
- **THEN** its submission carries that record, `record_retrieval`
  `network` naming Crossref, the title check's conclusion, and
  `acceptance` `rejected`, and `used` is `null`

#### Scenario: A candidate set aside for another identifier
- **WHEN** a supplied DOI's record is on offer and the operator supplies
  a second DOI whose record is offered in its place
- **THEN** the first submission's `acceptance` is `rejected`

#### Scenario: A candidate outlasts a supply nobody holds
- **WHEN** a supplied DOI's record is on offer, the operator supplies a
  second DOI that no service holds, and then renames the file from the
  first record
- **THEN** `used` names the first submission, and the second follows it
  with `acceptance` not attempted for `no-record`

#### Scenario: A candidate that leads to no move
- **WHEN** the record a supplied DOI reaches renders the name the file
  already carries
- **THEN** its submission's `acceptance` is not attempted with reason
  `no-move`, and no question is put about it

#### Scenario: The accepted submission is stated once
- **WHEN** the operator supplies a DOI and renames the file from the
  record it reaches
- **THEN** `used` is that submission's number, its entry carries only
  `submission`, `raw` and `syntax`, and the event's `lookup` names that
  DOI with origin `operator`

#### Scenario: A correction keeps the file's own failed lookup
- **WHEN** a file's text layer carries a DOI that no service holds, and
  the operator supplies the published DOI and renames the file from its
  record
- **THEN** `displaced.lookup` carries the file's own DOI with origin
  `extracted` and every service as `not-found`, `displaced.record_retrieval`
  is `null`, and the event's `lookup` is the operator's

#### Scenario: A refused text after a candidate
- **WHEN** a supplied DOI's record is on offer, the operator chooses to
  supply again, types a text that is refused, abandons the prompt, and
  renames the file from the record on offer
- **THEN** `used` names the DOI's submission, and the refused text
  follows it in `submissions` with the next number

### Requirement: Supplied text that names no identifier says why
When a text an operator submits names no identifier, its `syntax` SHALL be `rejected` with a `reason` of `unrecognised`, `invalid` or `checksum`, and, for `invalid` and `checksum`, an `expected` naming the form the text named: `doi`, `arxiv`, `pmid` or `isbn`.

A text names a form by the first of these it carries:

1. a `doi:` prefix or a DOI resolver address;
2. an `arXiv:` prefix;
3. a `pmid:` prefix;
4. an `isbn:`, `isbn-10:` or `isbn-13:` prefix;
5. a body that begins with `10.`, as every DOI does.

Prefixes are compared without regard to case. The reasons mean:

- `invalid`: the text names a form and does not satisfy that form's
  syntax;
- `checksum`: an ISBN that is well formed and fails its check digit;
- `unrecognised`: the text names no form and is not a bare arXiv
  identifier. That covers prose, blank text, and a bare run of digits,
  which could be a PMID or an ISBN.

Which texts are accepted SHALL NOT change: a text accepted as an
identifier is accepted as the same identifier, and a text refused is
refused. Each reason SHALL be a distinction the identifier's own syntax
draws, and none SHALL guess what a text was meant to be.

#### Scenario: Prose
- **WHEN** the operator types `see email from Anna`
- **THEN** its `syntax` is `rejected` with reason `unrecognised` and no
  `expected`

#### Scenario: Bare digits
- **WHEN** the operator types `9781593278281` with no prefix
- **THEN** its `syntax` is `rejected` with reason `unrecognised`

#### Scenario: A malformed DOI
- **WHEN** the operator types `https://doi.org/abc`, and on another
  occasion `10.1234`
- **THEN** each `syntax` is `rejected` with reason `invalid` and
  `expected` `doi`

#### Scenario: An ISBN with a wrong check digit
- **WHEN** the operator types `isbn:9781593278282`
- **THEN** its `syntax` is `rejected` with reason `checksum` and
  `expected` `isbn`

#### Scenario: A malformed PMID
- **WHEN** the operator types `pmid:0`
- **THEN** its `syntax` is `rejected` with reason `invalid` and
  `expected` `pmid`

## MODIFIED Requirements

### Requirement: An operator can supply an identifier
In an interactive rename run, the operator SHALL be able to supply an identifier for a file, and resolution SHALL treat a supplied identifier as it treats an extracted one: parsed and normalised by the same rules, dispatched to the same services in the same priority order, and checked against the titles the file claims.

A supplied identifier SHALL be accepted as a DOI in any form extraction
accepts, as an arXiv identifier, or, with a `pmid:` or `isbn:` prefix,
as a PMID or ISBN. Input that is none of these SHALL be refused with the
forms accepted and SHALL NOT be sent to any service. A refused input is
still a submission, and SHALL be reported with why it was refused, as
the requirement "Supplied text that names no identifier says why"
states.

A supplied identifier SHALL be looked up exactly as it was parsed and
normalised. Nothing SHALL complete, extend or otherwise repair it. A
DOI cut short in copying is still a well-formed DOI: it is looked up as
typed and reported with whatever the services answer about it.

A record resolved from a supplied identifier SHALL be described to the
operator and used only on the operator's further answer. A disagreement
between it and the file's claimed titles SHALL be shown and SHALL NOT
by itself prevent that answer. Agreement SHALL NOT stand in for the
answer either: until the operator answers, the record is pending,
whatever its title check concluded.

A `resolved` event for a record found from a supplied identifier SHALL
report the operator, and not an extraction pass, as the origin of the
identifier its `lookup` section names.

#### Scenario: A preprint without a DOI
- **WHEN** an interactive run finds no identifier in a file and the
  operator supplies `arXiv:2401.12345`
- **THEN** the arXiv record is resolved, described, and proposed as the
  file's name, and nothing moves until the operator answers

#### Scenario: A pasted DOI link
- **WHEN** the operator supplies `https://doi.org/10.1021/JACS.4C01234`
- **THEN** the identifier resolved is the DOI `10.1021/jacs.4c01234`

#### Scenario: Input that names nothing
- **WHEN** the operator supplies `see email from Anna`
- **THEN** it is refused with the accepted forms, no service is queried,
  and the operator is asked again

#### Scenario: An unknown supplied identifier
- **WHEN** the operator supplies a well-formed DOI that no service holds
- **THEN** the services' answers are shown and the file's question is put
  again

#### Scenario: A truncated DOI is looked up as typed
- **WHEN** the operator supplies `10.1039/c9cc02492`, which is the DOI
  `10.1039/c9cc02492a` with its last character lost, and no service
  holds it
- **THEN** the identifier looked up is `doi:10.1039/c9cc02492`, each
  service's answer about that DOI is reported, and no other DOI is
  looked up or proposed

#### Scenario: Agreeing titles do not accept a supplied record
- **WHEN** the operator supplies a DOI whose record's title matches the
  title the file claims
- **THEN** the record is described as pending, and nothing moves and
  nothing is written to the content index until the operator answers

### Requirement: Resolution events carry their evidence in sections
Every `resolved` event, and every `skipped` event that is a file's resolution verdict, SHALL carry the evidence of the file's resolution as eight sections, in the order the resolution runs: `library`, `content_index`, `extraction`, `identifier_input`, `lookup`, `record_retrieval`, `match_check` and `acceptance`.

Each section that is one step, and each of the two steps `content_index`
and `extraction` hold under their own keys (`read` and `write`, `result`
and `titles`), SHALL be an object tagged by `status`, naming what the
step concluded. `record_retrieval` names a place rather than a step: it
SHALL be tagged by `kind`, and SHALL be `null` on a verdict that reached
no record.

A step the resolution did not take SHALL be reported as exactly
`{"status": "not-attempted", "reason": <reason>}`, where the reason is
the kebab-case name of why it was not taken: `no-library`,
`outside-library`, `content-duplicate`, `library-answered`,
`content-index-hit`, `extraction-failed`, `no-record`, `refused`,
`unhashable`, `awaiting-acceptance`, `cache-bypassed`, `not-asked`,
`not-supplied`, `unparsed`, or `no-move`. A step not taken SHALL NOT be
left out, and SHALL NOT be reported as a step that ran and found
nothing.

The event schema version SHALL be 4. A `resolved` event SHALL NOT carry
the schema-3 top-level fields `found`, `cached`, `source`, `tier`,
`claims` or `overrode`, and the `reason` of a resolution `skipped` event
SHALL NOT carry the schema-3 reason fields `found`, `tier`, `attempts`,
`field`, `extracted`, `resolved` or `similarity`, whether under those
names or under other names at those places. A name schema 4 uses inside
a section — `found` as a status, `tier` in extraction's result, `claims`
in the titles — is schema 4's own and is not one of these. No event
SHALL carry a schema-3 rendering of a fact beside its schema-4 one. A
`resolved` event keeps `path`, `identifier`, the record's preferred
identifier, and `record`, the whole record.

The `identifier_input` section, the `earlier` rounds of a lookup, and
the `acceptance` value `pending` are additions to schema 4. A consumer
that ignores what it does not know reads them unchanged.

One change is not an addition. A record reached from a supplied
identifier and renamed from with no conflict is `accepted`, so
`automatic` no longer covers it, which narrows that value's meaning.
No release has carried schema 4. The requirement "JSON Lines output is
first-class" lets a schema that no release has carried change without
a further bump. So none of these SHALL change the version, and it SHALL
remain 4.

#### Scenario: A content-index answer in sections
- **WHEN** a run with no library resolves a file from the content index
- **THEN** its `resolved` event carries schema version 4; `library` not
  attempted with reason `no-library`; `content_index` with `read` `hit`
  and `write` not attempted with reason `content-index-hit`;
  `extraction`'s `result` and `titles`, `lookup` and `match_check` each
  not attempted with reason `content-index-hit`; `identifier_input` not
  attempted with reason `not-asked`; `record_retrieval` of kind
  `content-index`; and `acceptance` `automatic`

#### Scenario: No schema-3 field survives
- **WHEN** any run emits a `resolved` event or a resolution `skipped`
  event
- **THEN** the `resolved` event carries none of the keys `found`,
  `cached`, `source`, `tier`, `claims` or `overrode` at its top level,
  and the `skipped` event's `reason` carries none of the keys `found`,
  `tier`, `attempts`, `field`, `extracted`, `resolved` or `similarity`

#### Scenario: A resolution skip carries every section
- **WHEN** a file is skipped because no service holds its identifier
- **THEN** its `skipped` event carries all eight sections, with
  `record_retrieval` `null`

#### Scenario: The sections in pipeline order
- **WHEN** any run emits a `resolved` event
- **THEN** its line carries `identifier_input` after `extraction` and
  before `lookup`

### Requirement: A resolution says why a step was not taken
For every step a resolution did not take, the resolution SHALL retain that the step was not taken and why, and SHALL NOT retain it as a step that ran and found nothing.

The steps are:

- asking the run's library;
- asking the content index;
- reading the file's titles;
- extraction;
- taking an operator's identifier input;
- the lookup;
- the title check;
- writing the record to the content index;
- and, for each identifier an operator submitted, its lookup, its title
  check, and the operator's acceptance of what it reached.

The reasons are:

- the run has no library;
- the file lies outside the run's library or in a subtree it excludes;
- the file's bytes duplicate an artifact the library holds;
- the library answered;
- the content index answered;
- extraction found no identifier;
- no service held the identifier, or none could be asked;
- no question was put to an operator about the file;
- a question was put and no identifier was submitted;
- the text submitted named no identifier;
- the record a submission reached led to no move, so it was never put
  to the operator;
- and, for the content-index write alone, those the requirement "A
  resolution says what the content index did" names.

A lookup that was never made is not one that found nothing. Titles
that were never read are not a file claiming none.

Evidence SHALL be retained for every verdict a resolution reaches, a
resolved record and a skip alike. A skip that is a file's resolution
verdict retains the evidence of every step that ran before it.

#### Scenario: A library answer
- **WHEN** a file its library tracks is resolved from its item
- **THEN** the content index, the titles, extraction, the lookup, the
  title check and the content-index write are each retained as not
  taken because the library answered

#### Scenario: A content-index answer
- **WHEN** a file is resolved from the content index
- **THEN** the titles, extraction, the lookup, the title check and the
  content-index write are each retained as not taken because the
  content index answered

#### Scenario: Extraction found no identifier
- **WHEN** a file carrying a title yields no identifier
- **THEN** it is skipped, its titles are retained as read, and the
  lookup, the title check and the content-index write are retained as
  not taken because extraction found no identifier

#### Scenario: No service held the identifier
- **WHEN** a file's DOI is held by no service
- **THEN** it is skipped with every attempt retained, and the title
  check and the content-index write are retained as not taken because
  no service held the identifier

#### Scenario: No library, and a file outside it
- **WHEN** one file is resolved by a run with no library, and another by
  a run whose library does not contain it
- **THEN** the first retains the library as not asked because the run
  has none, and the second as not asked because the file lies outside
  it

#### Scenario: A content duplicate
- **WHEN** a rename run reaches a file whose bytes an artifact of its
  library already holds
- **THEN** it is skipped as a content duplicate, and the library, the
  content index, the titles, extraction, the operator's identifier
  input, the lookup, the title check and the content-index write are
  each retained as not taken because the file duplicates content the
  library holds

#### Scenario: A submission that names nothing
- **WHEN** the text an operator submits names no identifier
- **THEN** that submission's lookup, title check and acceptance are
  retained as not taken because the text named no identifier

### Requirement: A resolution reports its title check and whether its record was accepted
The `match_check` section SHALL report the title check's conclusion as `agreed`, `conflict` with the `field`, the `extracted` and `resolved` values and their `similarity`, `insufficient-evidence` with a `reason` of `record-untitled`, `no-titles` or `no-evidence`, or not attempted, and the `acceptance` section SHALL report `automatic`, `overridden`, `accepted`, `pending` or `not-applicable`.

`acceptance` SHALL be `overridden` exactly when an operator accepted a
record over a conflict the title check concluded, whether the record
was the file's own or one reached from an identifier they supplied. The
conflict stays in `match_check` on the same event, and `acceptance`
SHALL carry nothing beside its status. It SHALL be:

- `accepted` when an operator accepted a record reached from an
  identifier they supplied, and no conflict was overridden to use it;
- `pending` while such a record is on offer and the operator has not
  answered, whatever its title check concluded;
- `automatic` when any other record was used and no conflict was
  overridden to use it;
- `not-applicable` on every resolution skip, a conflict skip included.

A file's reported verdict SHALL NOT carry `pending`. An interactive run
reports a file only once an answer has settled the record on offer.
A rename the run meets with a notice that the library already holds a
file of the record's work settles nothing, and the record stays
`pending`. `pending` is carried by the event a
question's description is rendered from. What the operator decided
about a record that did not become the file's verdict is reported on
the submission that reached it, and never in this section.

Agreement and too little evidence SHALL NOT be reported alike.

#### Scenario: A placeholder is not agreement
- **WHEN** the only title a file claims is a producer's placeholder and
  its DOI resolves to a titled record
- **THEN** the file resolves, and its `match_check` is
  `insufficient-evidence` with reason `no-evidence`

#### Scenario: An operator overrides a conflict
- **WHEN** an operator renames a file over its own title conflict
- **THEN** its `resolved` event carries `match_check` `conflict` with
  the field, both titles and the similarity, and `acceptance`
  `overridden`

#### Scenario: A batch run's conflict
- **WHEN** a batch run skips a file as a conflict
- **THEN** its `skipped` event carries `match_check` `conflict` and
  `acceptance` `not-applicable`

#### Scenario: A supplied record renamed
- **WHEN** an operator supplies a DOI whose record's title agrees with
  the file's, and renames the file from that record
- **THEN** its `resolved` event carries `match_check` `agreed` and
  `acceptance` `accepted`

#### Scenario: A supplied record renamed over its conflict
- **WHEN** an operator supplies a DOI whose record's title conflicts
  with the file's, and renames the file from that record anyway
- **THEN** its `resolved` event carries `match_check` `conflict` and
  `acceptance` `overridden`

#### Scenario: A supplied record awaiting an answer
- **WHEN** a record reached from a supplied DOI, whose title agrees with
  the file's, is described before the operator answers
- **THEN** the event the description is rendered from carries
  `acceptance` `pending`

#### Scenario: A retried record renamed
- **WHEN** an operator asks the services again after an outage, and
  renames the file from the record its own DOI now reaches, whose title
  agrees with the file's
- **THEN** its `resolved` event carries `acceptance` `automatic`

#### Scenario: A retried record renamed over its conflict
- **WHEN** an operator asks the services again after an outage, the
  record its own DOI now reaches has a title that conflicts with the
  file's, and the operator renames the file anyway
- **THEN** its `resolved` event carries `match_check` `conflict`,
  `acceptance` `overridden`, and `lookup.origin` `extracted`

#### Scenario: A collision notice keeps a supplied record pending
- **WHEN** an operator answers rename on a record a supplied DOI
  reached, and the run says the library already holds a file of its
  work
- **THEN** the event the next question's description is rendered from
  still carries `acceptance` `pending`

### Requirement: A resolution skip names its cause and states each fact once
A `skipped` event that is a file's resolution verdict SHALL carry a `reason` whose `kind` is one of `no-text-layer`, `text-without-identifier`, `encrypted`, `unreadable`, `unresolvable`, `conflict` or `duplicate`, and SHALL carry no field in its reason other than the reader's `message` on `unreadable` and, on `duplicate`, the duplicate's `reason` and `existing_path` as before.

What the schema-3 reasons carried SHALL be reported in the sections
instead: the identifier looked up, its origin and the services' answers
in `lookup`; the pass that read the identifier in `extraction`; the
conflict's field, both values and similarity in `match_check`.

A skip of kind `conflict` SHALL carry the record the title check refused
as `candidate`, in full, as a `resolved` event carries its `record`. No
other skip SHALL carry a `candidate`.

A duplicate, by content or by work, is a resolution verdict: the
library's duplicate checks reach it while the file is being resolved,
and no later step produces one. A content duplicate is reached before
any step runs, and SHALL report every section not attempted with
reason `content-duplicate`, with `record_retrieval` `null`. A work
duplicate is reached once the file has resolved to a record, and SHALL
report the evidence of the resolution that reached that record —
`library` included — with `record_retrieval` naming where that record
came from. Both SHALL report `acceptance` `not-applicable`, and neither
SHALL carry a `candidate`.

Every other skip is not a resolution verdict: a taken target, a name
that renders empty, a declined move, a failed rename, a bibliography
that could not be written, a record that renders no citation key, a
taken sidecar, an unrecordable move, and a stranding move. Such a skip
SHALL carry its `path` and its `reason`, spelled as before, and no
section.

#### Scenario: An unresolvable skip
- **WHEN** a batch run skips a file because no service holds the DOI in
  its text layer
- **THEN** the `skipped` event's reason is `{"kind": "unresolvable"}`,
  and its `lookup` carries the DOI, `origin` `extracted` and each
  service's answer, and its `extraction` result carries `text-layer` as
  the tier

#### Scenario: A conflict skip keeps its candidate
- **WHEN** a batch run skips a file whose embedded title disagrees with
  the record Crossref holds for its DOI
- **THEN** the `skipped` event's reason is `{"kind": "conflict"}`, its
  `candidate` is that record, its `match_check` carries the conflict,
  its `content_index.write` is not attempted with reason `refused`, and
  its `record_retrieval` names Crossref

#### Scenario: A taken target carries no sections
- **WHEN** a batch rename skips a resolved file because its target is
  taken
- **THEN** the `skipped` event carries its path and the reason
  `target-taken` with the target, and none of the sections

#### Scenario: A content duplicate carries its sections
- **WHEN** a rename run skips a file whose bytes an artifact of its
  library already holds
- **THEN** the `skipped` event carries the `duplicate` reason of kind
  `content` with the existing path, every section not attempted with
  reason `content-duplicate`, `record_retrieval` `null` and
  `acceptance` `not-applicable`

#### Scenario: A work duplicate carries its record's evidence
- **WHEN** a batch rename skips a file whose DOI, answered by Crossref,
  names a work the library already holds a file of, and the library has
  no record of this file
- **THEN** the `skipped` event carries the `duplicate` reason of kind
  `work` with the existing path, a `library` section whose answer is
  `untracked`, a `lookup` with Crossref's attempt as `found`,
  `record_retrieval` naming Crossref, and `acceptance`
  `not-applicable`, and no `candidate`

### Requirement: Human resolution lines name the work, where it came from, and why a file was skipped
The human rendering of a `resolved` event SHALL be `<path>: resolved <identifier>`, then ` to ` and the record's title in double quotes followed by its authors and year of issue in parentheses, then ` via ` and the services that supplied the record, then `, from ` and where the record was retrieved, with each part the record does not hold left out.

The authors SHALL be named by family name: one alone, two joined by
` and `, and three or more as the first followed by ` et al.`. The
parenthesis SHALL hold whichever of the authors and the year the record
holds, joined by `, `, and SHALL be left out when it holds neither. The
services SHALL be those the requirement "A resolution reports the
evidence it was checked against" names; ` via ` and the services SHALL
be left out when there are none. Where the record was retrieved SHALL be
written `the library`, `the content index`, `the response cache` or `the
network`.

The human rendering of a resolution skip SHALL be `<path>: skipped, `
followed by, for each kind:

- `no-text-layer`: `no identifier found; the pages read hold no text`;
- `text-without-identifier`: `no identifier found in its metadata or
  the pages read`;
- `encrypted`: `encrypted, so no identifier could be read`;
- `unreadable`: `unreadable (<message>)`;
- `unresolvable`: `no source had a record for <identifier>` followed
  by each service's answer in parentheses, `; `-separated, or `no
  configured service could be asked about <identifier>` when no service
  could be asked;
- `conflict`: `<field> disagrees <N>% (file says <extracted>, record
  says <resolved>)`.

A `resolved` line or a resolution skip line whose `identifier_input`
holds a submission the operator rejected SHALL end with one more
clause, after everything above and after any clause saying the library
could not answer:

- one such submission: `; candidate rejected: ` and its identifier;
- more than one: `; candidates rejected: ` and every such identifier, in
  submission order, joined by `, `.

A line with no rejected submission SHALL carry no such clause. A
submission that reached no record, or whose record led to no move,
SHALL NOT be named in it.

In the human rendering of a `resolved`, `skipped` or
`content-index-write` event, the text after the leading `<path>: ` SHALL
be escaped as the interactive description escapes it, so that a control
character in any value that text takes from a record, a file's contents,
a library store or a service is shown rather than acted on. The leading
path SHALL be printed as every other human line prints it; escaping file
names is outside this requirement.
A `content-index-write` event SHALL render no line when its write was
made, and a line saying the content index could not keep the answer,
with the store's message, when it failed.

#### Scenario: A network answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from Crossref over the
  network to a record with DOI `10.1039/c5ay00042d`, title
  `Determination of things`, one author whose family name is `Smith`,
  and issued in 2015
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the network`

#### Scenario: A content-index answer read by a person
- **WHEN** the same file is resolved from the content index, its
  record's provenance naming Crossref
- **THEN** the line ends `via crossref, from the content index`

#### Scenario: A title carrying terminal escapes
- **WHEN** a file is skipped as a conflict and its embedded title holds
  an escape sequence that would clear the screen
- **THEN** its skip line shows the sequence as text

#### Scenario: A blank scan read by a person
- **WHEN** `borax resolve` skips `scan.pdf`, whose pages hold no text
- **THEN** the line is
  `scan.pdf: skipped, no identifier found; the pages read hold no text`

#### Scenario: A skip after a rejected candidate read by a person
- **WHEN** an interactive run skips `paper.pdf`, in which no identifier
  was found, after its operator saw and skipped the record
  `doi:10.1039/c9cc02492a` reached
- **THEN** the line is
  `paper.pdf: skipped, no identifier found in its metadata or the pages read; candidate rejected: doi:10.1039/c9cc02492a`

<!-- drops: the scenario "Asking again replaces the file's own lookup", renamed "Asking again keeps every round of the file's own lookup" because a retry no longer replaces the earlier lookup -->

### Requirement: A resolution keeps every service it asked, in order
Resolution SHALL retain, for each identifier it looks up, every service it asked about that identifier, in the order it asked them, each with the outcome that service gave.

The outcome SHALL be exactly one of: found; not found; unavailable,
with the message the failure carried; rate limited; or malformed, with
the message the failure carried. No two of these SHALL be retained as
the same outcome, since only "not found" is an answer about the
identifier and the others say the service could not give one.

A service that failed before another service supplied the record SHALL
be retained ahead of the service that supplied it. A record found on a
second attempt is not evidence that the first service was never asked.

The identifier looked up SHALL be retained with its attempts, together
with where that identifier came from: the extraction pass that read it
from the file, or an operator who supplied it. A lookup of an
identifier that no configured service supports SHALL be retained with
the identifier and no attempts. That keeps a lookup no service could
take apart from one that every service declined.

This applies to every lookup made for a file: the one made from the
file's own identifier, one an operator asks for again after an outage,
which keeps the extraction pass as its origin, and one made from an
identifier an operator supplied.

A lookup the operator asks for again is the file's own lookup, and it
SHALL become the file's current lookup, whether it finds a record or
not. Every lookup of the file's own identifier before it SHALL be
retained with it, in the order made, each with its attempts, so that an
outage the operator asked past is not lost. Whether the file's lookup
is conclusive, which decides whether asking again is offered, SHALL be
judged on the current lookup alone. A lookup made from a supplied identifier belongs to the
candidate it reached. It SHALL NOT replace or alter the file's own
lookup, even when no service holds the supplied identifier.

Retaining attempts SHALL NOT change which services are asked or in
what order. The requirement "Sources are queried by identifier type
and priority" fixes both.

#### Scenario: A failure before a success
- **WHEN** a file's DOI is looked up while Crossref is unavailable and
  OpenAlex holds the record
- **THEN** the file resolves from OpenAlex, and its resolution retains
  two attempts in order: Crossref as unavailable with its message,
  then OpenAlex as found

#### Scenario: Ways of failing are told apart
- **WHEN** a file's DOI is looked up while Crossref rate-limits the
  request and OpenAlex answers with a body that cannot be read as a
  record
- **THEN** the file is not resolved, and its resolution retains
  Crossref as rate limited and OpenAlex as malformed with its message,
  and retains neither as not found

#### Scenario: Not found everywhere is conclusive
- **WHEN** a file's DOI is looked up and every service asked says it
  does not hold it
- **THEN** each attempt is retained as not found, and the lookup is
  distinguishable from one in which any service failed to answer

#### Scenario: No service could be asked
- **WHEN** a run whose services are Crossref and OpenAlex looks up an
  arXiv identifier, which neither of them supports
- **THEN** no service is asked, and the resolution retains the arXiv
  identifier and its extraction pass with no attempts

#### Scenario: A supplied identifier's lookup names its origin
- **WHEN** an operator supplies a DOI for a file in an interactive run
  and Crossref holds it
- **THEN** the lookup retained for that record names the operator as
  the origin of the DOI and Crossref as found

#### Scenario: Asking again keeps every round of the file's own lookup
- **WHEN** a file's DOI meets an outage, the operator asks the services
  again, and every service now says it does not hold the DOI
- **THEN** the file's current lookup is the second one, with every
  attempt as not found, so the lookup is conclusive and asking again is
  not offered; the first lookup, with the outage, is retained before it

#### Scenario: A supplied identifier nobody holds leaves the file's lookup alone
- **WHEN** a file's DOI meets an outage, and the operator then supplies
  another DOI that no service holds
- **THEN** the file's retained lookup is still the one that met the
  outage, and the supplied DOI's attempts are not part of it

#### Scenario: Asking again keeps the file's own origin
- **WHEN** an operator asks the services again about a file's own
  extracted DOI after an outage, and the services now answer
- **THEN** the lookup retained for the record names the extraction pass
  that read the DOI as its origin, not the operator

#### Scenario: Asking again until the services answer
- **WHEN** a file's DOI meets an outage, the operator asks the services
  again, and Crossref now holds the DOI
- **THEN** the record's current lookup names Crossref as found, and the
  lookup that met the outage is retained before it with its attempts

### Requirement: Each service asked is reported with its outcome
The `lookup` section SHALL report the identifier the resolution looked up, its `origin` as `extracted` or `operator`, and every service asked about it, in the order asked, each as an object with the `service` and an `outcome` tagged `found`, `not-found`, `unavailable` with its `message`, `rate-limited`, or `malformed` with its `message`.

A `found` outcome SHALL carry `retrieval`: `service-cache` when the
service's response cache answered and no request was sent, or `network`
when the service answered over the network. A service that failed
before another found the record SHALL be reported ahead of it, and the
found attempt SHALL be the last.

A lookup in which at least one service was asked SHALL have `status`
`attempted`. A lookup of an identifier no configured service supports
SHALL have `status` `no-eligible-service`, with the identifier and its
origin and no attempts. A resolution that looked nothing up SHALL report
`lookup` not attempted, with the reason.

`origin` `extracted` SHALL be reported for the file's own identifier,
including when an operator asked for its lookup to be made again;
`operator` for an identifier an operator supplied. The pass that read an
extracted identifier is `extraction`'s result's `tier`, and SHALL NOT be
repeated in `lookup`.

A lookup of the file's own identifier that an operator asked for again
SHALL report the lookups before it as `earlier`. That is an ordered
list, oldest first, of rounds, each `attempted` with its `attempts` or
`no-eligible-service`. A round SHALL NOT repeat the identifier or the
origin, which are the lookup's own. The lookup's `status`, its
`attempts` and its outcome are the current round's. `earlier` SHALL be
absent from a lookup made once, and from every lookup of an identifier
an operator supplied, and its absence is not a step that did not run.
The human lines and the interactive description SHALL continue to show
the current round only.

#### Scenario: A failure before a success
- **WHEN** a file's DOI is looked up while Crossref is unavailable and
  OpenAlex answers over the network
- **THEN** `lookup.attempts` holds Crossref as `unavailable` with its
  message, then OpenAlex as `found` with `retrieval` `network`, and
  `record_retrieval` is `network` naming OpenAlex

#### Scenario: Not found everywhere
- **WHEN** every service asked about a file's DOI says it does not hold
  it
- **THEN** the file's `skipped` event has reason `unresolvable` and
  every attempt's outcome is `not-found`

#### Scenario: Ways of failing are told apart
- **WHEN** Crossref rate-limits the request and OpenAlex answers with a
  body that is not a record
- **THEN** the attempts are `rate-limited` and `malformed` with its
  message, in that order, and neither is `not-found`

#### Scenario: No service could be asked
- **WHEN** a run whose services are Crossref and OpenAlex reaches a file
  carrying only an arXiv identifier
- **THEN** its `lookup` has `status` `no-eligible-service`, the arXiv
  identifier and `origin` `extracted`, and no attempts

#### Scenario: A supplied identifier's origin
- **WHEN** an operator supplies a DOI for a file and renames it from the
  record Crossref holds
- **THEN** the file's `resolved` event reports `lookup.origin`
  `operator`

#### Scenario: An outage, a retry, and an answer
- **WHEN** Crossref and OpenAlex are both unavailable for a file's DOI,
  the operator asks the services again, and OpenAlex now answers over
  the network
- **THEN** the file's `resolved` event's `lookup.attempts` holds the
  second round, ending with OpenAlex as `found`, and `lookup.earlier`
  holds one `attempted` round with both services as `unavailable` and
  their messages

#### Scenario: An outage, a retry, and not found everywhere
- **WHEN** Crossref is unavailable for a file's DOI, the operator asks
  the services again, and every service now says it does not hold it
- **THEN** the file's `skipped` event has reason `unresolvable`,
  `lookup.attempts` every outcome `not-found`, and `lookup.earlier` one
  round with Crossref as `unavailable`; the question put again offers
  no retry, and the human line names only the second round's answers

#### Scenario: A lookup made once has no history
- **WHEN** a batch run looks up a file's DOI
- **THEN** its `lookup` carries no `earlier`
