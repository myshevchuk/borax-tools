# Tasks: supply-identifiers-interactively

Work on branch `change/supply-identifiers-interactively`, stacked on
`change/show-record-before-asking`. Implement after all three changes
below it are archived: it uses the interactive session, the
`rename.skip-named` setting, and the description.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it. Group 1 is a refactor with the suite green throughout.

## 1. Resolution in parts, batch unchanged

- [ ] 1.1 Split `resolve_file` into steps a caller can drive — content
      index, extraction (with claims), resolving an `Identifier`, the
      conflict check — and re-express `resolve_file` and
      `resolve_file_checking_ledger` as their composition; every
      existing pipeline test passes unchanged. A commit of its own.

## 2. Supplied identifiers

- [ ] 2.1 Red: `crates/borax-core/tests/identifier.rs` — the supplied
      input parser: DOI forms, arXiv forms, `pmid:` and `isbn:` prefixes,
      bare digits refused, prose refused with the accepted forms named
- [ ] 2.2 Green: the parser, built from the existing `parse` functions
      (design D2)
- [ ] 2.3 Red: `crates/borax/tests/pipeline.rs` — resolving a supplied
      identifier for a file: the record, `tier` `supplied`, the conflict
      check's result reported rather than enforced; an unresolvable one
      returns the attempts

## 3. The questions

- [ ] 3.1 Extend `Answer` with `Supply`, `Override` and `Keep`, and the
      `Asker` trait with `text`; extend the scripted asker to script
      text input
- [ ] 3.2 Red: `crates/borax/tests/run.rs` — with a scripted asker, one
      test per row of design D1's table, asserting the choices offered,
      the default, and the events emitted per design D5
- [ ] 3.3 Red: the reference-DOI case — supply a different identifier on
      a move question; the first proposal's name stays unclaimed
- [ ] 3.4 Red: refused input is asked again; Esc returns to the menu; an
      unresolvable supplied identifier puts the menu again
- [ ] 3.5 Red: `--no-skip-named` — keep emits `already-named`; supplying
      and renaming re-identifies the file
- [ ] 3.6 Green: the driver situations, the held verdict, `overrode` and
      `supplied` on `resolved`
- [ ] 3.7 Green: `describe` renders attempts, a conflict's two titles and
      similarity, and a supplied identifier

## 4. Remembering

- [ ] 4.1 Red: `crates/borax/tests/end_to_end.rs` — rename from a
      supplied identifier, then a batch run over the new name: resolved
      from the content index, already named, no source queried
- [ ] 4.2 Red: supply then skip, supply then quit, override then skip —
      no content-index entry for the file
- [ ] 4.3 Green: the content-index write on rename only (design D7)

## 5. The terminal

- [ ] 5.1 The `inquire::Text` adapter for `Asker::text`; Esc is `None`
- [ ] 5.2 Verify by hand on a copy of the real-PDF corpus: supply an
      arXiv identifier for a preprint with none, accept a conflict,
      catch a reference DOI, re-run in batch and see nothing asked

## 6. Documents

- [ ] 6.1 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, a section on supplying identifiers and what the content
      index remembers), `README.md` and `CHANGELOG.md`, with this
      proposal as source; check the diff
- [ ] 6.2 Record the built state in `openspec/STATE.md`, including the
      deferred title search as the next step for the hit rate
- [ ] 6.3 `openspec validate supply-identifiers-interactively --strict`
      and `scripts/check-spec-deltas.py` pass; full suite green
