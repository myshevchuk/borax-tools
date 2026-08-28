# Design: remove-undo

## Context

`borax undo` is a shipped command with 230 production lines, 774 lines
of tests, and a set of invariants reaching well outside its own module:
a mandatory pre-flushed apply log, a content hash on every `Renamed`
event, a refusal to apply a rename whose hash is unknown, and a
latest-apply-log selector in `runlog.rs`.

The discussion that produced this change established three things about
it. The pipeline is content-addressed, so re-running a corrected
template already reaches the wanted state from wherever the previous run
left the files — `inputs()` expands directories recursively, so even a
template that created subdirectories does not put files out of reach.
The information a re-run cannot reconstruct is the old-to-new mapping,
which the run log already records in full. And undo is the *less*
consistent of the two recoveries, because `undo_events` moves the file
and emits an event and does nothing else: no ledger correction, no
sidecar move.

The constraint this change works under is that the run log must come out
of it at least as trustworthy as it went in. Removing undo makes the log
the only record of what an apply run moved, so every property that makes
the log dependable is load-bearing afterwards in a way it was not
before.

## Goals / Non-Goals

**Goals:**

- Remove `borax undo` and every construct that exists only to serve it,
  leaving no dead vocabulary in the event schema.
- Keep the run log exactly as strong as it is today, and re-ground in
  the specs the requirements whose stated reason was undo.
- Leave the collection's accounting no worse: nothing this change
  removes was keeping the ledger or sidecars consistent.
- Replace the removed affordance with documentation, so "how do I get
  the old names back" still has an answer.

**Non-Goals:**

- Reworking the sidecar orphan defect. Removing undo closes one of its
  two entry points; the other (renaming an already-renamed file)
  remains, and fixing it properly means deciding what a sidecar's
  identity is, which `STATE.md` already records as a change of its own.
- Building a replacement recovery command. If a use case for one
  appears, it can be proposed then, with the log — unchanged by this
  change — as its input.
- Relitigating the hash gate on applied renames. See Decision 3.
- Adding a machine-readable recovery path (a `--from-log` mode, a
  reverse-plan command). Out of scope by the same reasoning.

## Decisions

### 1. The apply-run log stays mandatory

`runlog::mandatory` returns true for `Rename { apply: true }` and
nothing else, and `rename --apply` refuses to run when it has nowhere
to write. Its docstring justifies this by undo: "`borax undo` reads this
log to reverse the run".

The rule stays; only the reason is restated. With undo gone the log is
not a convenience that undo happens to consume — it is the *whole* of
what a user has after an apply run reorganises a collection. A
best-effort log would mean an apply run could move four hundred files
and record nothing, which is exactly the unrecoverable situation this
change is claiming is survivable.

*Alternative considered*: demote apply logs to best-effort now that
nothing reads them programmatically. Rejected — it removes the property
the removal argument depends on.

### 2. Rename events keep their content hash

The hash is what lets a person or a script confirm that the file now at
a recorded path is the one the run moved there. It is also already
required by the ledger, which stores a hash per admitted entry, and by
the `sha1` template field, so it is not an undo-specific cost in the
first place.

### 3. `SkipReason::Unrecordable` stays

`Applying::carry_out` refuses to apply a rename when the file's hash is
unknown, reporting `Unrecordable`. Its justification is undo-shaped —
an unverifiable move is one undo cannot check — but the property it
preserves outlives undo: every `renamed` line in an apply log is
verifiable, with no silent exceptions. A log with unverifiable entries
would be a weaker recovery record.

This is the one judgement in the change that could reasonably go the
other way, and it is deliberately left alone: this change removes a
command, and loosening the gate that guards the log is the opposite of
what removing the command relies on. If it should be revisited, it is
its own proposal.

*Alternative considered*: drop the gate and let hashless renames apply
with a hashless log line. Rejected for the above.

### 4. `latest_apply_log` is deleted rather than kept as a utility

`latest_apply_log`, `latest_apply_in` and `is_apply_log` exist to find
the log undo replays, and undo is their only production consumer. They
could be kept as a convenience for scripting, but a person recovering by
hand uses `ls`, and keeping them preserves a latent defect: `is_apply_log`
matches any filename ending `-apply.jsonl`, while `applying()` reports
`bib`, `undo` and `ledger rebuild` as applying runs too. A `bib` run
after a `rename --apply` therefore produced `…-bib-apply.jsonl`, which
sorts later and shadows the rename log — undo would report nothing to
revert with the rename's log intact one file earlier. No test covers
this; the existing cases only ever write `-rename-apply` names.

Deleting the selector deletes the bug with it, which is a better outcome
than fixing a function with no remaining caller.

`crates/borax/tests/end_to_end.rs` calls `latest_apply_log` in three
places purely to locate a log to assert on. Those get a small local
helper in the test file, so the assertions keep their coverage without
the production surface carrying a function only tests use.

### 5. The `<dry|apply>` suffix in log names stays

It exists so a preview sorts directly above the run that applied it, a
property of the directory listing a person reads. The `run-logs` spec
asserts it in its own scenario, independent of undo. Only the *selection*
built on the suffix goes.

### 6. The XDG fallback keeps its rule and loses its rationale

The spec's stated reason for writing apply logs under the state
directory outside a collection is "so undo works everywhere". The
behaviour is right for a different reason — an applied rename in a
downloads directory needs a record as much as one in a collection — so
the requirement is restated rather than removed, and its scenario stops
asserting that a subsequent `borax undo` reverts the run.

### 7. The subcommand list in the `cli` spec is corrected while it is open

The list drops `undo`, and gains `ledger`, which ships today and was
never added to it. This widens the edit by one clause in a sentence the
change is already rewriting, in a requirement whose "at minimum" makes
the list non-normative. Noted rather than done silently.

## Risks / Trade-offs

- **A user relies on `undo` today and upgrades into its absence.** →
  Pre-`1.0.0`, and `CLAUDE.md` prefers clean removal over shims. The
  CHANGELOG states the removal as breaking and the README documents
  recovery from the log, which is the mapping undo used and is still
  written. Nothing that was recorded stops being recorded.
- **Recovery becomes manual, and a manual `mv` has none of undo's
  checks** — no hash verification, no reverse ordering, no refusal to
  overwrite an occupied original. → Real, and the honest trade. The
  mitigation is that the situation calling for it is now much rarer than
  it appeared: re-running handles the naming cases, which were the
  common ones. The README section explains what each field is for rather
  than offering a one-liner that would encourage skipping the checks.
- **The removal turns out to be wrong and undo must come back.** → The
  log format is untouched by this change, so a future undo — or a
  better-shaped successor that re-plans against the recorded originals —
  reads exactly the same lines. The change is reversible in the only
  place that matters: the recorded data.
- **A MODIFIED delta silently reverts a sibling change's spec edit.** →
  `scripts/check-spec-deltas.py` runs in CI and passes on this change;
  the three intentional drops carry `drops:` markers.

## Migration Plan

No data migration: no on-disk format changes, and no file borax has
written becomes unreadable. Existing run logs stay valid and keep being
written in the same schema at the same paths.

Deployment is the version bump. `0.3.0` — removing a subcommand is
incompatible, and `CLAUDE.md` treats `0.y.z` increments as bookkeeping.

Rollback, if it is ever wanted, is reverting the change's commits; the
logs written in the meantime are in the schema undo already read.

## Open Questions

None blocking. Decision 3 (`Unrecordable`) is deliberately deferred
rather than open: it is settled for this change and may be revisited on
its own.
