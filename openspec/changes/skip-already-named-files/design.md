## Context

Three facts about the current pipeline produce the behaviour the
proposal describes.

`renaming::Applying::carry_out` maps `PlannedRename::AlreadyNamed` to
`Event::Skipped { reason: SkipReason::AlreadyNamed }`, and
`Counts::observe` counts every `Skipped`. `session::outcome_for` returns
`Partial` whenever `skipped > 0`.

`pipeline::resolve_file_checking_ledger` looks the file's hash up in the
ledger index before resolving, and `Collection::live_path` accepts a
match whenever the recorded path exists. The recorded path of a file
borax admitted is where that file now sits, so it always exists. The
work check after resolution has the same shape and the same hole: a
file annotated after admission has a new hash and its old identifiers,
and matches the entry recording itself.

Observed on a copy of twelve corpus PDFs in a scratch collection, after
one applied run renamed three of them:

```text
lyutenko2023_ApplicationsChiralSulfinyl.pdf: skipped, same bytes
  already archived at lyutenko2023_ApplicationsChiralSulfinyl.pdf
fang2019_Phosphinate-containingRhodolFluorescein.pdf: skipped, already
  carries that name
5 resolved, 0 renamed, 12 skipped
```

Twelve skips, of which eight are files in exactly the state the user
wants.

## Goals / Non-Goals

Goals:

- A re-run over an entirely named folder exits 0.
- No file is reported as a duplicate of itself.
- An interactive session over a mostly named folder shows only the files
  that need a decision, and says how many it passed over.

Non-goals:

- Changing what batch output prints per file, beyond the wording of the
  already-named line.
- Changing when a file counts as already named. The planner's rule —
  the file's name equals its rendered target, or a byte-identical file
  already occupies that target — is unchanged.

## Decisions

### D1. "Already renamed" is the planner's verdict, not the ledger's

There were two candidate definitions of a file borax has already
renamed. The ledger's: an entry records this file's hash at this path.
The planner's: rendering this file's record through the configuration
in force yields the name it already has.

The planner's is the one this change uses. It answers the question the
user is asking — is there anything to do about this file — while the
ledger's answers a different one — did borax once do something about
it. They diverge exactly where the difference matters: after a template
change, every admitted file is in the ledger and none carries the name
the new template renders, and passing them over would make a template
change silently apply to new files only. The planner's definition also
works outside a collection, where there is no ledger to ask.

The cost is resolution, and it is small: a file borax resolved before is
answered by the content index without opening it or asking a service.
A run with `--no-cache` pays for a live query per named file, which is
what `--no-cache` asks for.

### D2. An entry recording the incoming file itself is not a duplicate

Both ledger checks pass over an entry whose recorded path is the
incoming file's own, and go on looking rather than reporting nothing.
The distinction matters: `Index` keeps one entry per hash and one per
identifier, the latest, so a file's own entry is the entry that won.
Discarding what the lookup returned would hide a second copy sitting
elsewhere in the collection behind the file's own row. The lookups
therefore take the incoming path and skip it, which is what keeps
"genuine copies are still detected" true.

Paths are compared as both sides are built: the entry's
collection-relative path joined to the collection root, and the
incoming path made absolute against the working directory, each
normalised lexically — `.` and `..` resolved textually, separators
unified — and compared case-sensitively on Unix and case-insensitively
on Windows, as the planner already compares names. Symlinks are not
resolved: the ledger records the path borax moved the file to, and the
run names the path it was given, so a link and its target are two names
this check does not attempt to unify.

This could have been a special case that reports `already-named`
directly from the ledger match, without resolving. D1 rules that out: a
ledger match says nothing about whether the name is still the one the
template renders.

The check stays where it is — after hashing and before resolution — so
a genuine byte-identical copy elsewhere in the collection still costs no
network access.

### D3. Already named is an outcome of its own

`Event::AlreadyNamed { path }` replaces the skip reason, and `Counts`
gains `named`. The alternative was keeping the `skipped` event and
excluding that one reason from the count. It would leave a stream in
which an event called `skipped` is sometimes not a skip, and every
consumer counting skips from the stream would have to learn the
exception. A separate event makes the stream say what the summary says.

`outcome_for` is unchanged: it still reads `skipped` alone, and a named
file no longer contributes to it.

In batch human output the line reads `<path>: already named`, and the
summary gains `N already named` when N is not zero.

### D4. The setting is named for what the user asked for

`rename.skip-named`, default `true`, with the `--skip-named` /
`--no-skip-named` pair. The polarity follows the request: skipping is
the behaviour that is on by default, and the flag a user types is the
one that turns it off.

"Skip" here means passing over without a word, and it collides with the
stream's `skipped` event, which this change otherwise works to keep
precise. `rename.ask-named` (default `false`) would avoid the collision
at the cost of inverting the polarity the user described; it is the
alternative if the word proves confusing in the manual.

The setting is operative only in interactive runs. It is declared on
`rename` all the same, because the `cli` capability scopes flags by
subcommand, and it can change what an interactive `rename` reports.

### D5a. Filing into a subdirectory is idempotent

`Planning` joins a rendered name to the file's own directory, so a file
already filed at `Nature/Zeng2026.pdf` is proposed
`Nature/Nature/Zeng2026.pdf` on the next run, and a level deeper on
every run after that. Reproduced on a scratch collection with
`default = "[journal]/[auth][year]"`: three filed files, three
proposals to nest them again.

The rendered subdirectory names where the file belongs, not a level to
add to where it is. So the base a target is joined to is the file's
directory with the rendered subdirectory removed when it is already the
tail of it:

```text
file      <base>/Nature/Zeng2026.pdf
rendered  Nature/Zeng2026.pdf
target    <base>/Nature/Zeng2026.pdf     (already named)

file      <base>/Nature/Zeng2026.pdf     (journal now renders Science)
rendered  Science/Zeng2026.pdf
target    <base>/Science/Zeng2026.pdf    (moves across, not deeper)
```

Only a tail that matches the whole rendered prefix is removed, so a
file in `Nature/supplementary/` under a template rendering `Nature/` is
filed into `Nature/` from where it is, and a directory that merely
shares a name with the rendered one is not mistaken for it.

This is a rename-capability change and not a planner one: the planner
compares names in a namespace, and which namespace a file's name is
compared in is `Planning`'s to decide.

### D5b. A suffix never lands on the file's own name

Two works rendering `smith2024.pdf` file as `smith2024.pdf` and
`smith2024a.pdf`. On the next run the second renders `smith2024.pdf`,
finds it taken by the first, and walks the ladder to `smith2024a.pdf` —
which is free, because a file's own name is exempt from the collision
check for the duration of its own decision. The decision is a move onto
the name the file already carries: `hard_link` refuses it, and the run
reports a rename that failed.

A candidate equal to the item's own source is therefore
`AlreadyNamed`. The exemption stays what it is — it is what lets a file
change only the case of its name — and this is the one place where
reaching the exempt name means the ladder has arrived where the file
already is.

### D5. A file's lines are rendered once its fate is known

In an interactive run, the human renderer holds a file's `resolved`
line until the file's planning outcome is known, then renders the file's
lines together — or none of them, for a passed-over already-named file.

The hold ends at planning and never reaches a question. Planning
follows resolution with no network in between, so the wait is
computation and not a person; a hold that lasted until the file's fate
would keep the operator's own resolution line off the screen while they
decided about it, which is the opposite of what reporting as it goes is
for. Quitting releases what is held for the file quit at, as a fate
would.

Bibliography output is not held and is not suppressed. A sidecar
written beside a passed-over file, an entry merged into the master
`.bib`, and any failure in either are reported as they happen: the
setting hides a file that needs no decision, and a failure needs one.
The run log and any `--json` rendering are unaffected, since the
setting governs what is shown to the operator and not what happened,
and the JSON stream is where a complete account is kept.

This makes the interactive human rendering the one place where the
terminal shows less than the stream holds. The summary is what keeps it
honest: `N already named (not shown)`.

## Risks / Trade-offs

- **A file named from a wrong identifier is now quietly passed over.**
  Before, it was at least a line. That is the point of the setting and
  also its cost; `--no-skip-named` shows them, and
  `supply-identifiers-interactively` gives those runs a way to correct
  one.
- **Consumers of `skipped`/`already-named` break.** Pre-`1.0.0` and with
  no known external consumer (`openspec/STATE.md`, live risks), the
  change is recorded in the changelog rather than shimmed.

### D6. The schema version goes to 2

`SkipReason::AlreadyNamed` is removed rather than deprecated, and a
consumer counting skips by reason has no way to notice. The event
schema version is what announces that, so `SCHEMA` becomes 2.

The interactive rename that preceded this change added fields and a
reason without bumping it, which was right: a consumer that ignores
what it does not know reads such a stream unchanged. Removal is the
case the version exists for.

Pre-`1.0.0` this is bookkeeping rather than a compatibility promise —
`openspec/project.md` says the schemas may change in any release before
then — but the number is what a consumer reads, and leaving it at 1
would tell a reader nothing had changed.
