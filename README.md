# borax-tools

A fast, error-free CLI suite for bibliography work. The core job: given a
file (typically a PDF), resolve its bibliographic metadata from online
sources (Crossref, OpenAlex, arXiv) and rename the file by configurable
template rules — as a stateless pipeline that previews by default, never
overwrites, never guesses, and can undo.

Status: pre-release. The architecture and behaviour are specified in
`openspec/` (see the `add-core-pipeline` change); implementation is in
progress.

## The template language

Once borax has resolved a file's metadata, it still has to decide what to
call the file. You are the one who knows what your collection should look
like — one person wants `smith2024_AwesomePaperBorax.pdf`, the next wants
everything sorted into a directory per year. A *template* is how you say
which. It is a short string that borax renders once per file, using the
record it resolved, to produce the new file name.

With no configuration at all, borax uses this template:

```text
[auth:lower][year]_[shorttitle3:camel]
```

Rendered against a 2024 paper by Smith, Doe, and Roe titled "An Awesome
Paper on Borax", it produces:

```text
smith2024_AwesomePaperBorax
```

The extension is not part of the template; borax keeps the original
file's extension.

### Literal text and tokens

A template is literal text with *tokens* embedded in it. Anything
outside a bracket is copied through verbatim — the `_` in the default
template above is literal, and so are spaces, hyphens, and any other
character you type. A token is a field name in square brackets,
optionally followed by filters:

```text
[field]
[field:filter]
[field:filter:filter]
```

The field supplies a value from the record, and each filter transforms
whatever the filter to its left produced. So `[title:slug:trunc10]`
slugifies the title first and then cuts the slug to ten characters,
which on our example record gives `an-awesome`. Reversing the two
filters would truncate first and slugify the shorter string, which is a
different result.

A field whose value the record does not have renders as the empty
string. It is not an error, and it does not stop the run — see
Alternatives below for how to supply a fallback.

Do **not** expect to write a literal `[` in a template. There is no
escape for it: `[` always opens a token. A `]` outside a token is
literal.

### Fields

| Field | Value | Example output |
|---|---|---|
| `auth` | The first author's family name. | `Smith` |
| `authors` | Every author's family name, joined with `-`. | `Smith-Doe-Roe` |
| `authorsN` | The first N family names; `-etal` is appended when that dropped anyone. | `[authors2]` → `Smith-Doe-etal` |
| `year` | The year the work was issued. | `2024` |
| `title` | The title, verbatim. | `An Awesome Paper on Borax` |
| `shorttitle` | The first three words of the title that are not function words. | `Awesome Paper Borax` |
| `shorttitleN` | The same, keeping N words instead of three. | `[shorttitle2]` → `Awesome Paper` |
| `journal` | The container title, verbatim. | `J. Chem. Ed.` |
| `doi` | The normalized DOI. | `10.1021/jacs.4c01234` |
| `arxiv` | The arXiv id, with any version suffix removed. | `2401.12345` |
| `sha1` | The hash of the file's contents, in lowercase hex. | `da39a3ee5e…` |
| `entrytype` | The record's CSL type string. | `article-journal` |

A *function word* is one of the sixteen words that carry no subject
matter of their own — `a`, `an`, `the`, `of`, `on`, `in`, `for`, `and`,
`or`, `to`, `with`, `at`, `by`, `from`, `into`, `upon` — compared
without regard to case. `shorttitle` skips them, which is why
`shorttitle3` of "An Awesome Paper on Borax" is `Awesome Paper Borax`
and not `An Awesome Paper`.

The `entrytype` field renders the CSL-JSON type string, which is not
always the name you use to configure a template for that type. A journal
article renders as `article-journal` here, but is configured under the
key `article`; a preprint renders as `article`. See Configuration below.

### Filters

| Filter | Effect | Example |
|---|---|---|
| `lower` | Lowercases the whole string. | `Smith` → `smith` |
| `upper` | Uppercases the whole string. | `Smith` → `SMITH` |
| `capitalize` | Uppercases the first character, lowercases the rest. | `an AWESOME paper` → `An awesome paper` |
| `titlecase` | Uppercases the first character of each word, lowercases the rest, and keeps the spaces. | `An Awesome Paper On Borax` |
| `camel` | Title-cases, then removes the spaces. | `An Awesome Paper on Borax` → `AnAwesomePaperOnBorax` |
| `slug` | Transliterates, lowercases, and replaces every run of characters outside `a-z0-9` with a single `-`, trimming leading and trailing hyphens. | `An Awesome Paper on Borax` → `an-awesome-paper-on-borax` |
| `abbr` | Keeps the first character of each word, preserving case. | `J. Chem. Ed.` → `JCE` |
| `truncN` | Keeps the first N characters. | `[title:trunc11]` → `An Awesome ` |
| `transliterate` | Folds common Latin letters to ASCII: `ä`→`ae`, `ß`→`ss`, `ø`→`o`, `ñ`→`n`, and the accented vowels lose their accents. | `Müller` → `Mueller` |
| `regex("pattern","replacement")` | Replaces every match of the pattern. `$1` refers to a capture group. | `[title:regex("[Bb]orax","B2O3"):slug]` → `an-awesome-paper-on-b2o3` |

`transliterate` folds the Latin letters it knows and passes every other
character through unchanged, so its result is not guaranteed to be
ASCII; Greek, Cyrillic, and CJK survive it intact. Follow it with `slug`
when you need a name that is certainly ASCII.

`truncN` counts characters, not bytes, so it will not cut a multi-byte
character in half.

### Alternatives

Records are incomplete in different ways: a preprint has no DOI, an old
scan has no arXiv id. Rather than fail, a token can list alternatives
separated by `||`, and the first one that renders something non-empty
wins:

```text
[doi:slug || sha1:trunc8]
```

For a record that has a DOI, that renders `10-1021-jacs-4c01234`.
For one that does not, it falls back to the first eight characters of
the file hash, `da39a3ee`. Each alternative is a full field-and-filters
chain of its own, and the filters run before the emptiness test, so an
alternative that filters down to nothing is passed over.

Whitespace around `||` is ignored. Elsewhere inside a token it is an
error, so write `[title:slug]`, never `[title : slug]`.

### Filing into subdirectories

A `/` in the rendered name is a directory separator, on every platform.
That makes a filing scheme a matter of writing one into the template:

```text
[year]/[auth:lower][year]_[shorttitle2:camel]
```

renders `2024/smith2024_AwesomePaper`, and borax creates the `2024`
directory if it is not already there. The path is relative to the
directory the file is already in.

### Sanitization

Every rendered name goes through a sanitization pass before it reaches
the filesystem. You cannot turn it off, and templates cannot opt out of
it. It applies the strictest rules of any supported platform — Windows's
— everywhere, so that a collection stays valid when it syncs between
machines:

- The characters Windows forbids (`<` `>` `:` `"` `\` `|` `?` `*` and
  the control characters) each become `_`.
- Trailing dots and spaces are trimmed.
- A component whose name is a reserved Windows device (`CON`, `PRN`,
  `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`) gets a `_` appended to its
  stem, so `CON.pdf` becomes `CON_.pdf`.
- A component longer than 255 bytes is truncated, keeping its extension;
  a whole path longer than 1024 bytes is trimmed until it fits.

So a template that renders `CON: a study? of borax.pdf` is written to
disk as:

```text
CON_ a study_ of borax.pdf
```

Sanitization applies per path component, so the `/` separators you wrote
survive it.

### Configuration

Templates live in the `[templates]` table of a configuration file. There
are two: a global `borax/config.toml` under your configuration directory
(`$XDG_CONFIG_HOME`, or `$APPDATA` on Windows), and a `.borax.toml` in a
directory of files, which overrides the global one for the files below
it. Templates can only be set in these files — unlike the other
settings, they cannot be set from an environment variable or a
command-line flag, because their keys are open-ended.

The `default` key is required and is what borax falls back to; every
other key names an entry type and overrides the default for records of
that type:

```toml
[templates]
default = "[auth:lower][year]_[shorttitle3:camel]"
thesis  = "[auth:lower][year]_thesis"
book    = "[year]/[authors2:slug]_[shorttitle:camel]"
```

The entry types you may use as keys are `article`, `preprint`, `book`,
`chapter`, `thesis`, `report`, `patent`, and `standard`. These are
borax's own names, not the CSL-JSON strings the records serialize to,
because a configuration file is written by a person and `[templates.article]`
should mean what a person means by it. The CSL strings differ in exactly
the confusing place: CSL calls a journal article `article-journal` and a
preprint `article`.

Merging is per key, not per file, so a `.borax.toml` that sets only
`templates.thesis` keeps the `default` it inherits from the global file.

### Errors are reported before any file is touched

Templates are compiled when they are loaded — before the first file is
processed — and an unknown field, an unknown filter, a bad regular
expression, or a syntax error aborts the run then and there. A template
is configuration, so a broken one is wrong for every file in the batch;
there is nothing to be gained by discovering that once per file.

The message names the template and the offending token:

```text
templates.default: unknown filter "lwoer"
templates.default: unknown field "titel"
templates.default: template syntax error at byte 6: unclosed '['
```

Rendering itself cannot fail. Once a template compiles, the same record
and the same configuration always produce the same name.

## Layout

- `crates/borax-core` — pure logic: record model (CSL-JSON superset),
  template engine, rename planning, BibTeX output. No I/O.
- `crates/borax-sources` — online source adapters, caching, rate limiting.
- `crates/borax-pdf` — PDF extraction behind an `Extractor` trait.
- `crates/borax` — the `borax` CLI binary.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
