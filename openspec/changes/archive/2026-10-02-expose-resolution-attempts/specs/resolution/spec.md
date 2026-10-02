## ADDED Requirements

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
SHALL replace the lookup retained for the file, whether it finds a
record or not. A lookup made from a supplied identifier belongs to the
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

#### Scenario: Asking again replaces the file's own lookup
- **WHEN** a file's DOI meets an outage, the operator asks the services
  again, and every service now says it does not hold the DOI
- **THEN** the file's retained lookup is the second one, with every
  attempt as not found, so the lookup is conclusive

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

### Requirement: A resolution says where its record came from
Every record a resolution reaches SHALL be retained with where it was retrieved from: the file's library item, naming the artifact and the item; the content index; a service's response cache, naming the service; or the network, naming the service.

A service's answer served from the response cache SHALL be
distinguishable from one the service sent over the network. This
holds for the attempt that found the record as well as for the record.
A record the library or the content index answered with involves no
service attempt.

Where a record was retrieved from is a separate fact from which
services its fields came from, which is the record's per-field
provenance. A record retrieved from a cache or from the library keeps
the provenance it was stored with.

#### Scenario: A network answer
- **WHEN** a file's DOI is resolved and Crossref's answer is not in the
  response cache
- **THEN** the record is retained as retrieved from the network from
  Crossref, and so is the attempt that found it

#### Scenario: A response-cache answer
- **WHEN** the same DOI is resolved again for another file, and
  Crossref's earlier answer is held in the response cache
- **THEN** no request is sent, and both the record and the attempt that
  found it are retained as served by Crossref's response cache

#### Scenario: A content-index answer
- **WHEN** a file is resolved from the content index
- **THEN** its record is retained as retrieved from the content index,
  with no service attempt

#### Scenario: A library answer
- **WHEN** a file its library tracks is resolved from its item
- **THEN** its record is retained as retrieved from the library, naming
  the artifact record and the item, with no service attempt

#### Scenario: A record an operator reached comes from the service
- **WHEN** the content index answered for a file, or its library
  tracks it, and an operator supplies an identifier that Crossref holds
- **THEN** the record the operator reached is retained as retrieved
  from Crossref, not from the content index or the library, while what
  the content index and the library said is retained as they said it

### Requirement: A resolution says what the content index did
Every resolution of a file SHALL retain what the content index did for it: it answered; it held nothing for the file; it was bypassed because the run turned the cache off; it could not be asked because the file could not be hashed, with the reason hashing gave; or it was not asked because the resolution was already decided, with the reason.

A file that could not be hashed SHALL be retained as such even when the
run bypassed the cache. The missing hash also decides that nothing can
be remembered for the file.

A resolution SHALL also retain what became of writing the record it
reached to the content index under the file's hash. The write was
made; or it failed, with the store's message; or it was not attempted,
with the reason. The reason is the earliest one in the order the
resolution runs:

- the library, the content index, or a content duplicate settled the
  file first;
- extraction found no identifier;
- no service held the identifier;
- the title check refused the record;
- the record waits for an operator to accept it, and is written when
  the file is moved;
- the file has no content hash.

#### Scenario: A hit and a miss
- **WHEN** one file is resolved from the content index, and another,
  whose hash the index does not hold, is extracted and resolved from a
  service
- **THEN** the first retains the content index as having answered, and
  the second retains it as having held nothing and retains the write of
  its record as made

#### Scenario: An unhashable file
- **WHEN** a file cannot be hashed, with the cache on, and its DOI
  resolves from Crossref
- **THEN** its resolution retains the content index as unavailable with
  the hashing error, extraction and the lookup run, and the write is
  retained as not attempted because the file has no content hash

#### Scenario: Unhashable outranks bypassed
- **WHEN** the same file is resolved with `--no-cache`
- **THEN** its resolution still retains the content index as
  unavailable with the hashing error

#### Scenario: The cache bypassed
- **WHEN** a hashable file is resolved with `--no-cache` and its DOI
  resolves
- **THEN** its resolution retains the content index as bypassed, and
  the write of its record as made

### Requirement: A failed cache write is evidence, not a failure
A write to the response cache or to the content index that fails SHALL be retained, with the store's message, as evidence about the resolution or the move it belongs to, and SHALL NOT fail the resolution, change its verdict, fail the rename it followed, or end the run.

Every write borax makes to either store SHALL report whether it was
made. A record a service sent over the network, with a response cache
in front of that service, SHALL be retained with whether it was written
to the response cache. A record sent with no response cache in front,
because the run turned the cache off, SHALL be retained as not written
there.

Remembering a record an operator accepted, when the file is moved,
SHALL yield what became of that write: made; failed, with the store's
message; or not attempted because the file has no content hash. That
result belongs to the move. It is not part of the verdict reported
before the move, which retains the write as waiting for acceptance.

#### Scenario: The response cache cannot be written
- **WHEN** a file's DOI is resolved from Crossref over the network, and
  the response cache's directory cannot be created
- **THEN** the file resolves from Crossref, and the attempt that found
  the record retains the failed write with its message

#### Scenario: The content index cannot be written
- **WHEN** a file is resolved from a service and the content index
  cannot be written
- **THEN** the file resolves exactly as it would otherwise have, and
  its resolution retains the content-index write as failed with the
  store's message

#### Scenario: An accepted answer that could not be kept
- **WHEN** a record an operator accepted is remembered when the file is
  moved, and the content index cannot be written
- **THEN** remembering yields a failed write with the store's message,
  and the move stands

#### Scenario: No response cache in front
- **WHEN** a file's DOI is resolved from Crossref with `--no-cache`
- **THEN** the attempt that found the record retains that the response
  cache was not written

### Requirement: A refused record is kept as evidence and never remembered
When the title check refuses a record, resolution SHALL retain that record as the file's candidate, together with the evidence it was reached by and the conflict that refused it.

The candidate's evidence is everything a resolved record's would be:

- the identifier looked up and where it came from;
- every service attempt, in order;
- where the record was retrieved from;
- what extraction found;
- the titles the file claims.

A refused record SHALL NOT be written to the content index under the
file's content hash, and nothing the content index already held for
that hash SHALL change. A later run therefore extracts and checks the
file again rather than being answered for it from the index. The
response cache keeps the service's answer under the identifier, as it
keeps every answer. That entry says what the service holds and nothing
about the file.

When an operator accepts the refused record, the record's
content-index write SHALL be retained as waiting for the move, no
longer as refused. The title check's conflict SHALL still be retained
beside the acceptance. Accepting the record is the decision the
conflict was put to the operator for, and it does not change what the
check concluded.

Retaining a candidate SHALL NOT offer the candidate to anyone, search
for other records, or accept it. Accepting a record over a conflict
remains an operator's decision, as the requirement "Ambiguity is
skipped, never guessed" states.

#### Scenario: A conflict keeps its candidate
- **WHEN** a batch run resolves a file whose DOI is read from its text
  layer and answered by Crossref over the network, and whose embedded
  title disagrees with the record's
- **THEN** the file is skipped as a conflict, and the refused record is
  retained with the DOI and its text-layer origin, Crossref's attempt
  as found over the network, the extraction result, the file's titles,
  and the conflict with its similarity

#### Scenario: A refused record is not remembered
- **WHEN** the same file is skipped as a conflict
- **THEN** the content index holds no record for the file's hash, the
  refused record retains the content-index write as not attempted
  because it was refused, and resolving the file again checks its
  titles again and skips it again

#### Scenario: An accepted conflict waits for the move
- **WHEN** an operator renames a file over its own title conflict
- **THEN** the accepted record retains the conflict, retains the
  content-index write as waiting for the move rather than as refused,
  and the record is written to the content index only once the file
  has moved

#### Scenario: An earlier entry survives a refusal
- **WHEN** the content index holds a record for a file's hash, and a
  run with `--no-cache` resolves the file to a different record that
  the title check refuses
- **THEN** the content index still holds the earlier record for that
  hash

### Requirement: A resolution says what the title check concluded
Every resolution SHALL retain what the title check concluded about the record it reached: the file's titles agreed with it; they conflicted, with the field, both titles and their similarity; there was not enough evidence to judge, with why; or the check was not made, with the reason.

There SHALL be three reasons for too little evidence, kept apart: the
file claims no title; none of the titles it claims counts as evidence;
or the record has no title to compare. Agreement and too little
evidence SHALL NOT be retained as the same conclusion. Neither refuses
a record, but only agreement is evidence that the record is the
file's.

The rules for which titles count as evidence and for when two titles
agree are those of the requirement "Ambiguity is skipped, never
guessed", and they are unchanged.

#### Scenario: Agreement
- **WHEN** a file's XMP title matches the title of the record its DOI
  resolves to
- **THEN** the resolution retains the title check as agreed

#### Scenario: A placeholder is not agreement
- **WHEN** the only title a file claims is a producer's placeholder,
  and its DOI resolves to a titled record
- **THEN** the file resolves, and the resolution retains the title
  check as having too little evidence because none of the file's titles
  counts, not as agreed

#### Scenario: A file that claims no title
- **WHEN** a file whose metadata carries no title resolves from its DOI
- **THEN** the resolution retains the title check as having too little
  evidence because the file claims no title

#### Scenario: A record with no title
- **WHEN** a file claiming a title resolves to a record that carries no
  title
- **THEN** the resolution retains the title check as having too little
  evidence because the record has no title

#### Scenario: The check was not made
- **WHEN** a file is resolved from the content index
- **THEN** the resolution retains the title check as not made because
  the content index answered

### Requirement: A resolution says why a step was not taken
For every step a resolution did not take, the resolution SHALL retain that the step was not taken and why, and SHALL NOT retain it as a step that ran and found nothing.

The steps are:

- asking the run's library;
- asking the content index;
- reading the file's titles;
- extraction;
- the lookup;
- the title check;
- writing the record to the content index.

The reasons are:

- the run has no library;
- the file lies outside the run's library or in a subtree it excludes;
- the file's bytes duplicate an artifact the library holds;
- the library answered;
- the content index answered;
- extraction found no identifier;
- no service held the identifier, or none could be asked;
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
  content index, the titles, extraction, the lookup, the title check
  and the content-index write are each retained as not taken because
  the file duplicates content the library holds
