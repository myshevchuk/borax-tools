## Why

Change 8 (`expose-resolution-attempts`, archived) made the engine keep
what a resolution found out about a file, step by step, in
`crates/borax/src/evidence.rs`. Nothing renders it. The stream still
speaks schema 3, whose six flat fields on `resolved` cannot say what
the engine now knows:

- `cached` is true only for a content-index hit. A response-cache hit
  reports `false`, so the stream cannot tell a service's cache from the
  network.
- `tier` mixes three questions in one value: which extraction pass read
  the identifier, whether an operator supplied it (`supplied`), and
  whether the library supplied the whole record (`library`). Its `null`
  means "the content index answered".
- `source` names services, except when it names a store (`cache`,
  `library`).
- An empty `claims` list means "not opened", "opened and claims no
  title" or "could not be opened".
- `overrode` repeats the conflict's details on the record that was
  accepted over it.
- Failed attempts before a success, the outcome of each attempt as a
  value rather than a string, and every cache write's result reach no
  event.

Resolution skips are worse off. `pipeline::skipped_for` reports both
`NoTextLayer` and `NoIdentifierFound` as `no-identifier`, and
`Encrypted` as `unreadable`. That is the known defect "Resolution skip
reasons merge distinct extraction failures" in `openspec/STATE.md`: it
breaks the `extraction` requirement "Extraction failures are typed and
non-fatal". A conflict skip carries the two titles and nothing else:
not the identifier looked up, where it came from, which service
answered, or the refused record.

The Phase 3 resolution-schema design discussion (maintainer's notes,
outside the repository) settled the shape that replaces all of this:

- one schema bump, from 3 to 4, made here;
- explicit sections on `resolved` and on every resolution skip;
- a slim skip reason;
- structured service outcomes;
- separate sections for what the library said and for where the
  record came from.

This is change 9 of the roadmap's Phase 3. It also delivers change 3,
the human `resolve` line that names the work it resolved to.

The change alters output that living requirements describe. It
therefore needs a proposal, and the requirements that name the removed
fields are modified here rather than contradicted.

## What Changes

- **BREAKING: event schema 4.** `SCHEMA` moves from 3 to 4, once.
  `resolved` loses `found`, `cached`, `source`, `tier`, `claims` and
  `overrode`, with no aliases and no dual output. It keeps `path`,
  `identifier` and `record`, and gains seven sections in pipeline
  order: `library`, `content_index`, `extraction`, `lookup`,
  `record_retrieval`, `match_check` and `acceptance` (design D1, D3).
- **One shape for a step that did not run.** Every step that was not
  taken is `{"status": "not-attempted", "reason": "<kebab-case>"}`.
  The reasons are change 8's `Unattempted` names, plus `cache-bypassed`
  for a service answer that no response cache stood in front of
  (design D3).
- **BREAKING: resolution skips carry the same sections.** A `skipped`
  event that is a file's resolution verdict keeps `reason.kind` as a
  dispatch key with no detail fields. The kinds are `no-text-layer`,
  `text-without-identifier`, `encrypted`, `unreadable` (which keeps its
  `message`), `unresolvable` and `conflict`. The facts the old reasons
  carried move into the sections. A conflict skip carries the refused
  record as `candidate`. This restores the typed extraction failures in
  the stream and closes the merged-failures defect (design D4).
- **Duplicates are resolution verdicts.** A `duplicate` skip, by content
  or by work, is a verdict `standing` reached, so it carries the
  sections too, with its reason's `reason` and `existing_path` unchanged.
  A content duplicate reports every step not attempted with reason
  `content-duplicate`. A work duplicate reports the evidence of the
  record the file resolved to, so the library answer schema 3 carried
  on it is kept (design D4).
- **BREAKING: other skips lose `library`.** A skip that is not a
  resolution verdict carries `path` and `reason` only. Its reason is
  unchanged. This covers a taken target, a declined move, a failed
  rename and the rest of design D4's list.
- **Structured service outcomes.** Each attempt is `{service, outcome}`.
  The outcome is tagged `found`, `not-found`, `unavailable` with a
  `message`, `rate-limited`, or `malformed` with a `message`. A found
  attempt carries `retrieval` (`service-cache` or `network`). A network
  retrieval also carries `stored`, the response-cache write's result.
  Failed attempts before a success are reported. A lookup that no
  configured service could take is `no-eligible-service` (design D5).
- **Where the record came from, apart from who wrote its fields.**
  `record_retrieval` is `library` (with the artifact and the item),
  `content-index`, `service-cache` or `network` (each with the
  service). Per-field provenance stays inside `record` (design D6).
- **Acceptance.** `acceptance` is `automatic`, `overridden` or
  `not-applicable`. It replaces `overrode`. The overridden conflict's
  details stay in `match_check`, where the title check's conclusion
  already is (design D7).
- **The rename-time content-index write is reported.** A new event,
  `content-index-write`, follows `renamed` (and any
  `library-admission`). It reports what became of writing an operator's
  accepted record to the content index. It cannot travel on `renamed`,
  because the run log records `renamed` before the move, and the log
  must equal the stream (design D8).
- **Change 3: the human `resolve` line names the work.** It reads
  `<path>: resolved <identifier> to "<title>" (<authors>, <year>) via
  <services>, from <where>`. "Via" names whoever supplied the record's
  fields. "From" names where the record was retrieved: the library, the
  content index, the response cache, or the network. The `(cached)` and
  `(from the library)` suffixes go (design D9).
- **Escaping.** Resolution skip lines name the four extraction
  failures. Every value that the `resolved` and `skipped` lines take
  from a record, a file, a library store or a service is escaped.
  `STATE.md`'s defect about unescaped titles is narrowed to what is
  left: adoption's two messages (design D9).
- **The interactive description.** It reads the sections. In place of
  `nothing read`, it says which state the titles are in: read, read
  with none, could not be opened, or not read and why. It shows the
  file's titles on a skip whose extraction or lookup failed, and it
  names the extraction failure (design D10).
- **Engine surface.** The following are removed: `event_for`,
  `unresolvable`, `Attempt`, `Overridden`, `Provenance`, and the
  schema-3 projections `FileRecord::source`, `tier`, `found`, `claims`
  and `cached`. `FileRecord::overrode` becomes `overridden: bool`.
  `Evidence::sections` projects the engine's evidence onto the
  event-side section types, and `Unattempted` gains `CacheBypassed`
  (design D2, D11, D12).

Explicitly out of scope:

- `run-finished` counters. No count is added or changed, and the new
  event counts toward nothing.
- The `identifier_input` section and the `pending`, `accepted` and
  `rejected` acceptance values. These belong to change 10, which adds
  them to schema 4 before 0.9.0 ships. This change leaves room for
  them and designs neither.
- Prompt or answer events, a machine-readable stdin protocol, and title
  search.
- Which services are asked, in what order, what is cached, and when
  anything is written.
- Escaping in lines other than `resolved`, `skipped` and the new event.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED:
    - "Resolution events carry their evidence in sections"
    - "A resolution reports what the content index did and every cache
      write"
    - "Each service asked is reported with its outcome"
    - "A resolution reports where its record was retrieved"
    - "A resolution reports its title check and whether its record was
      accepted"
    - "A resolution skip names its cause and states each fact once"
    - "What became of remembering an operator's answer is reported"
    - "Human resolution lines name the work, where it came from, and
      why a file was skipped"
  - MODIFIED:
    - "A resolution reports the evidence it was checked against"
    - "A resolution says whether its library answered"
    - "A file its library tracks resolves from its library item": one
      scenario named `tier`, `cached` and claims, and another named "no
      library answer"
    - "An operator can supply an identifier": it required `supplied` as
      the pass
- `extraction`: ADDED "A resolution's events report extraction's
  result and the file's titles".
- `cli`: MODIFIED "A question describes whichever verdict it is asking
  about". It required the `unresolvable` reason to carry the
  identifier, the pass and the attempts as schema-3 additions.
- `rename`:
  - ADDED "An interactive description says whether the file's titles
    were read".
  - MODIFIED "An interactive run asks about files it could not settle":
    its scenarios named `tier` `supplied` and the reason
    `no-identifier`.
  - MODIFIED "An interactive question says whether the library
    answered": it read `library` as a `tier`.

Requirements checked and left unchanged:

- `extraction` "Extraction failures are typed and non-fatal". This
  change restores it in the stream, and its text already requires that.
- `extraction` "A resolution keeps extraction's result and the file's
  titles". Its "skipped as before" means skipped rather than resolved.
  The new `extraction` requirement states the kinds the skip now
  carries.
- `resolution`:
  - "Ambiguity is skipped, never guessed". The skip still reports the
    similarity, now in `match_check`. An accepted record's event still
    carries the conflict it overrode, in `match_check` with `acceptance`
    `overridden`.
  - "A library that cannot answer for a file says so". The report
    travels in the `library` section of the same events.
  - "An operator's answer about a file is remembered" and "A failed
    cache write is evidence, not a failure". The new event reports the
    rename-time write without making it a failure of the rename. That
    write result belongs to the move, as change 8 said.
  - "The run summary reports the skip queue".
  - "Sources are queried by identifier type and priority".
  - The seven requirements change 8 added. Each states what a
    resolution retains, and none names a field.
- `cli` "JSON Lines output is first-class". This change removes fields
  and changes reasons, so it bumps the version, as that requirement
  asks.
- `run-logs` "Runs persist their event stream as JSONL run logs" and
  "Apply-run logs are mandatory and flushed before mutation". The new
  event is an ordinary event, written to the log and the stream alike.
  Design D8 keeps the log equal to the stream.
- `rename`:
  - "A file's verdict follows the operator's decision". Its accepted
    record still carries the conflict and is followed by `renamed`.
  - "An interactive question shows what the answer rests on". The new
    `rename` requirement adds the title states without contradicting
    it.

## Impact

- `crates/borax/src/event.rs`:
  - `SCHEMA` becomes 4;
  - `Event::Resolved` and `Event::Skipped` are reshaped, and
    `Event::ContentIndexWrite` is new;
  - the section types of design D12 are added;
  - `SkipReason` gets its new resolution kinds;
  - `Attempt` and `Overridden` are removed;
  - `human_line` and `skipped_because` are rewritten, and
    `services_of` moves here from `pipeline::sources_of`.
- `crates/borax/src/evidence.rs`: `Evidence::sections` and
  `Unattempted::CacheBypassed`.
- `crates/borax/src/pipeline.rs`:
  - `FileRecord::overridden`, `FileRecord::conflict` (retyped) and
    `FileRecord::acceptance`;
  - `skipped_for` maps each failure to its own kind;
  - `verdict_event`, `resolved_event` and the new `resolution_skip`
    build sections;
  - `attempts_of` returns `ServiceAnswer`s;
  - removed: `event_for`, `unresolvable`, `sources_of`, `Provenance`
    and the schema-3 projection methods.
- `crates/borax/src/run.rs`:
  - `Settled::Skip` carries sections and a candidate instead of
    `library`;
  - `situation` matches the new kinds;
  - the retry paths build skips through `resolution_skip`;
  - the move path emits `content-index-write` after `remember`;
  - `reidentified` reads the lookup's origin.
- `crates/borax/src/renaming.rs`: `carry_out`'s skips drop `library`.
- `crates/borax/src/describe.rs`: renders from the sections, with the
  title states, the extraction failures and the titles on failed
  verdicts.
- Tests: the change alters output, so existing expectations about
  schema 3 are rewritten deliberately. Design D14 lists each file and
  the kind of edit, and gives the schema-3 to schema-4 mapping the
  rewrites follow.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`: the `resolve` line, the interactive session's
    description and skip passages, and the Run logs JSONL passages;
  - `README.md`: its two console examples;
  - `CHANGELOG.md`: an Unreleased "Changed" breaking entry naming the
    schema bump.
- `openspec/STATE.md` (implementer):
  - a paragraph for this change;
  - the merged-failures defect is closed;
  - the escaping defect is narrowed.
- No new dependency, configuration key or flag. One new event.

## Deferred

- **Change 10.** It adds `identifier_input` between `extraction` and
  `lookup`, and operator-candidate acceptance (`pending`, `accepted`,
  `rejected`). Until then, a record an operator supplied with no
  conflict reports `acceptance` `automatic`, an interim value the
  maintainer accepted on the condition that change 10 changes it before
  0.9.0 ships (design D7).
- **Escaping adoption's `unreadable` and `unwritten` messages.** This
  is the remainder of the escaping defect, which stays in `STATE.md`.
