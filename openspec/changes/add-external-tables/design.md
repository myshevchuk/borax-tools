## Context

`zotero-jcode` is a Zotero 7 plugin that writes `jcode: JACS` into an
item's Extra field by looking its `publicationTitle` up in
`journal_titles.tsv` — a hand-curated, tab-separated table with a header
row `abbreviation`, `title`, `shorttitle` and roughly five hundred rows.
That table is the user's own work and the authority for what a journal
is called in short form. It is the same authority borax needs, and the
whole point of this change is that there be exactly one copy of it.

borax's template engine (`crates/borax-core/src/template.rs`) compiles a
template once, validates fields, filters and regexes at compile time,
and renders infallibly and deterministically thereafter. Its field
vocabulary stops at `journal`/`doi`/`arxiv`/`sha1`, its filters are all
pure string transforms, and there is no way for a template to consult
anything outside the record and the file hash. Configuration
(`crates/borax/src/config.rs`) resolves in layers — defaults, global
file, nearest `.borax.toml`, environment, flags — with `templates` and
`citation-keys` already merging per open-ended key.

`openspec/STATE.md` names "the template grammar invites scope creep
toward a general expression language" as a live risk. This change adds
to the vocabulary and the filter set; it deliberately adds no new
grammar production and no control flow.

## Goals / Non-Goals

**Goals:**

- One curated abbreviation file, read unmodified by both `zotero-jcode`
  and borax, producing the same abbreviation for the same journal.
- Render `2024-JACS-146-1234` from a resolved article, and
  `2024-JACS-1234` from one with no volume, with one template.
- A general mechanism, of which journal abbreviations are one case: any
  named map from string to string, applied to any field.
- Every failure that is a property of the configuration — a missing
  file, a missing column, an undeclared table, ambiguous rows — reported
  before the first file is processed.
- A miss is visible. The user learns which journal to add to the table
  from the run, not from a wrong filename discovered later.
- Rendering stays total, infallible and deterministic.

**Non-Goals:**

- Any form of executable extension. No scripting, no connectors, no
  templates that call out. A table substitutes one string for another
  and that is the whole of its power.
- Fuzzy or probabilistic matching. Normalization is a fixed fold, not a
  similarity score; two strings match or they do not.
- Generating abbreviations. borax never derives an abbreviation from a
  title (that is ISO 4's job and the LTWA's); it only looks one up.
- List-shaped external data. The hardcoded `FUNCTION_WORDS` is a
  tempting second case, but a set is not a map and would need its own
  configuration shape and its own semantics.
- Book series, which needs a record field that does not exist. See the
  proposal's Deferred section.

## Decisions

### D1. A filter, not a field

`lookup("<table>")` is a filter taking a string and returning a string,
not a `[jcode]` field that renders the container title's abbreviation.

A filter composes with everything already in the grammar and is not
bound to journals: `[publisher:lookup("pubcodes")]`,
`[auth:lookup("canonical-names")]`, and
`[journal:lookup("jcode") || journal:abbr]` all fall out with no further
work. It also inherits the alternatives mechanism for free, which is how
a miss gets a fallback.

*Rejected:* a dedicated field. It would hard-wire container-title →
abbreviation, need a second field for the next case, and buy nothing
that the filter does not.

### D2. The file is jcode's TSV; configuration names the columns

A table declaration is a path, a key column (or columns), and a value
column:

```toml
[tables.jcode]
path  = "/home/<user>/lib/journal_titles.tsv"
key   = ["title", "shorttitle"]
value = "abbreviation"
```

borax defines no format of its own and requires no column the curated
file does not already have. Naming key and value explicitly, rather than
fixing "first column is the key", is what lets one file serve both
tools: jcode maps title → abbreviation while the file's columns happen
to lead with the abbreviation.

It also makes a second view of the same file free — a table with
`value = "shorttitle"` gives `[journal:lookup("jshort")]` — which is the
clearest evidence the mechanism is general rather than a journal feature
wearing a general name.

TSV is the only format. There is no `format` key: adding one when a
second format exists is not a breaking change, and inventing the
extension point before the second case is speculative.

*Rejected:* a borax-native table format (a TOML map, say). It would be
pleasant to read and would immediately create the second copy this
change exists to prevent.

### D3. Matching keys are `slug`s, and that is a declared standard

Both sides of a lookup — every key a table row supplies, and the value
flowing into the filter — are folded to a canonical form before
comparison. The fold is the one the existing `slug` filter already
performs, in this order:

1. transliterate (borax's table: `ä`→`ae`, `ö`→`oe`, `ü`→`ue`,
   `ß`→`ss`, `æ`→`ae`, `ø`→`o`, `å`→`a`, `đ`→`d`, `ł`→`l`, `ñ`→`n`,
   `ç`→`c`, and the accented vowels lose their accents);
2. lowercase (Unicode default case conversion);
3. every maximal run of characters outside `a-z0-9` becomes a single
   `-`;
4. leading and trailing `-` are trimmed.

So `Journal of the American Chemical Society.` and
`Journal of the American Chemical Society` fold alike, and
`J. Am. Chem. Soc.` and `J Am Chem Soc` fold alike. This matters because
Crossref supplies `container-title[0]` and OpenAlex supplies
`primary_location.source.display_name`, and the two disagree about
punctuation more often than about words.

**There is no industry standard to adopt here.** ISO 4 and the ISSN
Centre's LTWA standardize how an abbreviation is *derived* from a title,
which is the opposite direction and not what a lookup needs. Unicode
defines case folding (`CaseFolding.txt`) and normalization forms, which
cover step 2 only. Steps 1, 3 and 4 are choices — notably, borax expands
`ä` to `ae` in the German manner rather than stripping the diaeresis as
NFKD would.

This fold is therefore **borax's definition, and the intended standard
across the user's own tools**: `zotero-jcode` and anything after it
should implement these four steps exactly so that the same table and the
same title produce the same answer everywhere. It is specified
normatively in the `external-tables` spec and restated in `README.md`
for that reason.

*Rejected:* exact string matching, which is what jcode does today and
what its own design document records as its known weakness ("items whose
`publicationTitle` differs from the canonical title by even a trailing
period or capitalization will be classified `no-match`"). Borax's inputs
vary more than Zotero's field does, so inheriting the weakness would
make the feature miss constantly.

*Rejected:* NFKD plus case folding, i.e. the pure-Unicode route. It is
more standard and less useful: it would fold `Müller` to `Muller` where
every German bibliography writes `Mueller`, and borax already made the
opposite choice in `transliterate`. Having two disagreeing folds in one
program is worse than having one non-standard fold.

A string whose fold is empty — a title written entirely in a script the
transliteration table does not cover — is not a key. A row whose key
folds to empty is dropped with a warning; an input that folds to empty
is a miss. Without this, every such title would match every other.

### D4. One row may supply several keys; ambiguity is a load error

`key` accepts a string or an array of strings, and a row contributes one
key per named column. `AA | Amino Acids | Amino Acids` therefore
contributes the same key twice, which is not an error — the file really
does have rows whose title and shorttitle are identical, and both fold
alike.

Two rows folding to one key with **different** values is a configuration
error, reported at load, naming the table, the key, and both values.
Silently keeping the last row is how a collection acquires a systematic
wrong abbreviation that nobody can explain later; jcode warns and keeps
the last, which is the right call for a tool that logs to a debug
console and the wrong one for a tool that renames files.

### D5. A miss renders empty and is reported

`lookup` on a value with no row renders the empty string. That is what
makes `[journal:lookup("jcode") || journal:abbr]` work: the chain
renders empty, the alternative is tried, and rendering never fails.

Rendering therefore has an observable output beyond the string, so
`Template::render` returns

```rust
pub struct Rendered {
    pub text: String,
    pub misses: Vec<Miss>,          // { table: String, input: String }
}
```

rather than a bare `String`. Making the caller receive the misses is
deliberate: the obligation to report them should be visible at every
call site rather than optional. Misses are collected from every chain
that was evaluated, including chains whose result an alternative
replaced — a table lacking a journal is worth knowing about even when
the fallback covered for it.

The run deduplicates across files and emits one event per distinct
(table, input) pair, with a count in the run summary. This is the borax
form of the single most useful thing jcode does: its
`Unmatched titles:` log block is what tells the user which row to write
next.

Determinism is unaffected: `misses` is in template evaluation order and
is a function of the same inputs as `text`.

*Rejected:* treating a miss as a render failure. The templates spec
requires rendering to be infallible, and a missing abbreviation is
ordinary — half a collection is preprints and theses with no journal at
all.

*Rejected:* a `SkipReason`. Nothing was skipped; the file was renamed,
just not with the name the user hoped for.

### D6. `tables` is per-directory configuration, merged per name

`[tables]` sits beside `templates` and `citation-keys` in the layer
stack and merges the same way: per table name, so a collection's
`.borax.toml` can add or replace one table without disturbing the global
ones. Like them, it cannot be set from the environment or a flag,
because its keys are open-ended. `borax config` reports
`tables.jcode.path`, `tables.jcode.key` and `tables.jcode.value` with
the origin of each, which is how a user answers "which abbreviation file
am I actually using".

A **relative** `path` resolves against the directory of the file that
declared it, not the working directory. A global config at
`~/.config/borax/config.toml` saying `path = "journal_titles.tsv"` means
the file beside it; a `.borax.toml` can ship a table next to itself and
stay portable. This is a new rule — `bib.path` and `collection-root` are
taken as given — and it is confined to `tables.*.path`, where a config
file naming a data file next to itself is the common case rather than
the exception.

Tilde is not expanded, here or anywhere. See the proposal's Deferred
section.

### D7. Tables load in preflight and travel with the templates

Table files are read, parsed and folded in `preflight`, alongside
template compilation and before the first event is written, so a missing
file, a header without the named column, or an ambiguous pair of rows
ends the run the way an uncompilable template does — with no
`run-started`, the reason on stderr, and the fatal exit code.

The loaded tables join `Group` next to `filenames` and `citation_keys`,
because a table is per-directory configuration exactly as a template is:
a run spanning two trees is a run under two sets of tables, and the same
file must not be looked up differently depending on which path was given
first.

A `lookup("jcode")` naming a table the configuration did not declare is
a **compile** error, reported with the same wording as an unknown field
or filter: `templates.article: unknown table "jcode"`. Template
compilation therefore needs the set of declared table names, which is
available at that point because configuration resolves first.

### D8. Affix filters, not optional groups

`prefix("<text>")` and `suffix("<text>")` wrap their input and are the
**identity on the empty string**. The pattern's optional volume is then:

```text
[year]-[journal:lookup("jcode")][volume:prefix("-")]-[firstpage]
```

which renders `2024-JACS-146-1234` with a volume and `2024-JACS-1234`
without one.

*Rejected:* a grammar-level optional group, `{-[volume]}`. It is more
expressive and it is exactly the drift `STATE.md` warns about: a new
production, its own emptiness rule, its own error cases, and a step
toward a template language with control flow. Two string-to-string
filters cost one sentence of specification and no grammar at all.

*Rejected:* a post-render pass collapsing repeated separators. It cannot
know which `-` was meant to survive, and it would silently rewrite a
deliberate `--` that a user typed.

### D9. `firstpage` is a field with a stated rule

`firstpage` is the part of the record's `page` value before the first
`-`, `–` or `—`, trimmed; the whole value when there is none. So
`1234-1245` → `1234`, `1234–45` → `1234`, and an article number such as
`e0123456` or `045301` passes through whole, which is what a journal
citing by article number wants.

A field rather than an idiom. `[pages:regex("^([^-–—]+).*$","$1")]` does
the same job today and is unreadable, unmemorable, and easy to get
subtly wrong; the concept is part of the domain and deserves a name.

The four neighbours — `volume`, `issue`, `pages`, `publisher` — are
added at the same time. They are already on the record and already
populated by both source readers; their absence from the vocabulary is
an oversight rather than a decision, and each is one match arm.

### D10. A run records the tables it read

`RunStarted` names each table a run loaded, by path and by a digest of
its contents. Rendering is deterministic given the table, but the table
is a file that changes, so without this the run log answers "what did
this run do" and not "why". Since the run log is now the only record of
an applied rename, that gap is worth one line per table.

### D11. The mechanism stops at maps

"Externally supplied configuration" could mean many things. It means
exactly one here: a named map from string to string, declared in
configuration, applied by one filter. That covers journal abbreviations,
publisher codes, series codes once a series field exists, and canonical
author names.

Everything else stays out, and for one reason each: list-shaped data
needs different semantics; templates loaded from files are already
configuration; and anything executable would end the property that makes
this design acceptable — that rendering is a total function of the
record, the template and a fixed set of strings.

## Risks / Trade-offs

- **The two tools disagree until jcode adopts the fold.** borax matches
  normalized, jcode matches exactly, so borax will find rows jcode
  misses. → The fold is specified normatively rather than described, so
  jcode can adopt it as a contained change; until then the disagreement
  is in the safe direction (borax matches more, never differently).
- **Editing the table changes citation keys already typed into
  documents.** Adding a row changes the key of every affected work from
  the fallback to the abbreviation. → This is the risk citation keys
  always carried; D10 makes it diagnosable after the fact, and the
  `bib-output` uniqueness suffix does not paper over it. Worth stating
  in `README.md` where the mechanism is documented.
- **Two journals folding to one key.** `J. Chem. Soc.` and
  `J Chem Soc` are the same journal; two genuinely different journals
  could collide too. → D4 makes it a load-time error naming both rows,
  so it is impossible to hit silently. The cost is that a table with a
  latent collision stops working until the user resolves it, which is
  the right trade for a tool that renames files.
- **Vocabulary growth invites more.** Five fields and three filters in
  one change is the largest single expansion the engine has had. → Every
  addition is string-to-string or record-field-to-string, the grammar is
  untouched, and D11 states where the line is. The next request that
  needs a new production is the one to refuse.
- **A large table read per group.** A five-hundred-row TSV folded once
  per directory group is negligible; a hundred-thousand-row table would
  not be. → Tables load once per group and are shared by reference
  across every file and both template tables; no per-file work is added
  beyond a hash lookup.
