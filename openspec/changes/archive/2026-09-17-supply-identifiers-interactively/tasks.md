# Tasks: supply-identifiers-interactively

Work on branch `change/supply-identifiers-interactively`, stacked on
`change/show-record-before-asking`. Implement after all three changes
below it are archived: it uses the interactive session, the
`rename.skip-named` setting, and the description.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it. Group 1 is a refactor with the suite green throughout.

## 0. Reconcile the requirements this change inherits

Before any test. Three requirements from the two changes below this one
say things this change contradicts, and they will be living by the time
it is implemented. Re-copy each from `openspec/specs/` into a MODIFIED
block and amend it, rather than leaving the archive holding both:

- [x] 0.1 `rename`, "An interactive run passes over already-named
      files": with `--no-skip-named` such a file is now asked about
- [x] 0.2 `cli`, "A run reports as it goes": a file's verdict may be
      held until the operator's decisions about it are made, which is
      longer than the hold that change allowed
- [x] 0.3 `resolution`, "Ambiguity is skipped, never guessed": the
      scenarios that skip every conflict unconditionally have to
      distinguish a fresh conflict from one an operator accepted, since
      the index answers for the second without checking again
- [x] 0.4 `openspec validate --strict` and `check-spec-deltas.py` pass
      with the MODIFIED blocks in place

## 1. Resolution in parts, batch unchanged

- [x] 1.1 Split `resolve_file` into steps a caller can drive — content
      index, extraction (with claims), resolving an `Identifier`, the
      conflict check — and re-express `resolve_file` and
      `resolve_file_checking_ledger` as their composition; every
      existing pipeline test passes unchanged. A commit of its own.
- [x] 1.2 Red: the file's claimed titles can be read on their own, for
      a file the content index answered for and for one no identifier
      was found in (design D2a); `extract_from` collects them whether or
      not an identifier turns up
- [x] 1.3 Green: claims are read when a comparison needs them

## 2. Supplied identifiers

- [x] 2.1 Red: `crates/borax-core/tests/identifier.rs` — the supplied
      input parser: DOI forms, arXiv forms, `pmid:` and `isbn:` prefixes,
      bare digits refused, prose refused with the accepted forms named
- [x] 2.2 Green: the parser, built from the existing `parse` functions
      (design D2)
- [x] 2.3 Red: `crates/borax/tests/pipeline.rs` — resolving a supplied
      identifier for a file: the record, `tier` `supplied`, the conflict
      check's result reported rather than enforced; an unresolvable one
      returns the attempts

## 3. The questions

- [x] 3.1 Extend `Answer` with `Supply`, `Override` and `Keep`, and the
      `Asker` trait with `text`; extend the scripted asker to script
      text input
- [x] 3.2 Red: `crates/borax/tests/dispatch.rs` — with a scripted
      asker, one test per row of design D1's two tables, asserting the
      choices offered, the default, and the events emitted per design
      D5; the inconclusive-resolution row offers a retry and the
      conclusive one does not
- [x] 3.2a Red: the transitions — a supplied identifier that does not
      resolve leaves the file's original record on offer; one that
      resolves into a taken target, an empty name, or an already-named
      file reports that outcome and asks again; an abandoned input
      changes nothing
- [x] 3.3 Red: the reference-DOI case — supply a different identifier on
      a move question; the first proposal's name stays unclaimed
- [x] 3.4 Red: refused input is asked again; Esc returns to the menu; an
      unresolvable supplied identifier puts the menu again
- [x] 3.5 Red: `--no-skip-named` — keep emits `already-named`; supplying
      and renaming re-identifies the file
- [x] 3.6 Green: the driver situations, the held verdict, `overrode` and
      `supplied` on `resolved`
- [x] 3.6a Red: `crates/borax/tests/describe.rs` — `describe` renders
      the held verdict whichever it is (design D8): the services'
      answers for an unresolvable file, both titles and the similarity
      for a conflict asked about or overridden, and `supplied` on the
      identifier line. `describe` is pure, so this is the one part of
      the change that can be pinned exactly
- [x] 3.7 Green: `describe` renders attempts, a conflict's two titles and
      similarity, and a supplied identifier

## 4. Remembering

- [x] 4.1 Red: `crates/borax/tests/end_to_end.rs` — rename from a
      supplied identifier, then a batch run over the new name: resolved
      from the content index, already named, no source queried
- [x] 4.2 Red: supply then skip, supply then quit, override then skip —
      the abandoned candidate is not written to the content index, and a
      record the file already had is still there
- [x] 4.2a Red: an abandoned candidate is never cited — no sidecar and
      no master-bibliography entry from it — while a file whose own
      record stands is cited from that record as a batch run cites it
- [x] 4.2b Red: a content-index write that fails leaves the rename
      standing and reported, and the next run asks about the file
      again
- [x] 4.3 Green: the content-index write on rename only (design D7)

## 5. The terminal

- [x] 5.1 The `inquire::Text` adapter for `Asker::text`; Esc is `None`
- [x] 5.2 Verify by hand on a copy of the real-PDF corpus: supply an
      arXiv identifier for a preprint with none, accept a conflict,
      catch a reference DOI, re-run in batch and see nothing asked

## 6. Documents

- [x] 6.1 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, a section on supplying identifiers and what the content
      index remembers), `README.md` and `CHANGELOG.md`, with this
      proposal as source; check the diff
- [x] 6.2 Record the built state in `openspec/STATE.md`, including the
      deferred title search as the next step for the hit rate
- [x] 6.3 `openspec validate supply-identifiers-interactively --strict`
      and `scripts/check-spec-deltas.py` pass; full suite green
