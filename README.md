# borax-tools

A fast, configurable CLI suite for bibliography work. Give it a file,
typically a PDF you have just downloaded. It finds the DOI or arXiv id,
looks up the identifier with Crossref, OpenAlex, and arXiv, then renames
the file after the returned record. You choose the new name with a small
template language.

By default on a terminal, `rename` asks before each move. A batch rename
previews unless you pass `--apply`. It never overwrites or guesses. It
leaves an unidentified file in place and prints the reason. Inside a
library established by `.borax.toml` or the `library-root` setting, an
applying rename also records the logical works and physical files the
library holds. Outside a library, it writes no library state.

Status: pre-release. `openspec/` specifies the behaviour, and
`openspec/STATE.md` tracks how much is built. Versions are `0.y.z` and
do not promise compatibility yet.

## Examples

Review a directory of downloads one file at a time. borax asks about
each file it cannot settle on its own. When it finds a record and can
rename the file, it shows the identifier, the record, the file's own
metadata, and the proposed name before it waits for your choice:

```console
$ borax rename papers/
```

```text
── 3 of 17 ──────────────────────────────────────────────────────────
file        50-Article Text-95-2-10-20240507.pdf
identifier  doi:10.15407/bioorganica2023.01.010, from the text layer
record      Crossref
type        journal article
title       Applications of chiral sulfinyl auxiliaries in the asymmetric
            synthesis of fluorinated amines and amino acids
authors     Nataliya V. Lyutenko, Alexander E. Sorochinsky, Vadim A.
            Soloshonok
issued      2023
in          Ukrainica Bioorganica Acta 18(1), 10–21
file says   no title in its metadata
new name    lyutenko2023_ApplicationsChiralSulfinyl.pdf
? What should happen to this file?
> Rename
  Supply an identifier
  Skip
  Quit
[↑↓ to move, enter to select]
```

Press Enter to rename, or choose `Supply an identifier`, `Skip`, or
`Quit`. You can supply a DOI, an arXiv identifier, or a prefixed PMID or
ISBN when borax finds no identifier, finds the wrong one, cannot resolve
one, or reports a title conflict. Accepted answers are remembered in the
content index after the rename, so later runs can use them without asking
again. To preview the whole plan without moving anything, use `--batch`:

```console
$ borax rename --batch papers/
papers/1-s2.0-S0009261421001234.pdf: resolved doi:10.1021/jacs.4c01234 to "An Awesome Paper on Borax" (Smith, 2024) via crossref, from the network
papers/1-s2.0-S0009261421001234.pdf: would rename to papers/smith2024_AwesomePaperBorax.pdf
papers/scan003.pdf: skipped, no identifier found; the pages read hold no text
1 resolved, 0 renamed, 1 skipped
```

Apply the renames and merge each record into a master bibliography:

```console
$ borax rename --apply --bib library.bib papers/
papers/1-s2.0-S0009261421001234.pdf: resolved doi:10.1021/jacs.4c01234 to "An Awesome Paper on Borax" (Smith, 2024) via crossref, from the content index
papers/1-s2.0-S0009261421001234.pdf: renamed to papers/smith2024_AwesomePaperBorax.pdf
papers/scan003.pdf: skipped, no identifier found; the pages read hold no text
papers/smith2024_AwesomePaperBorax.pdf: bibliography entry smith2024 added
1 resolved, 1 renamed, 1 skipped
```

Inspect a library, record files borax already knows from its local
content index, and repair records after moving files in a file manager:

```console
$ borax status papers/
$ borax adopt papers/
$ borax reconcile papers/
```

`status` only counts the tree and stores unless you add `--identify`.
`adopt` queries no service and moves nothing. Use `rename --apply` for
files the content index does not know; it can resolve, rename, and record
them. `reconcile` updates existing records and never adopts an orphan.

To use another naming scheme, put a template in a `.borax.toml` beside
the files. This example creates one directory per year:

```toml
[templates]
default = "[year]/[auth:lower][year]_[shorttitle3:camel]"
thesis  = "[auth:lower][year]_thesis"
```

When configuration files disagree, use `borax config` to find where a
setting came from. It prints each setting with its source layer. For
example:

```console
$ borax config
mailto = "you@example.org"  # global /home/you/.config/borax/config.toml
rename.collision = "skip"  # override /home/you/papers/.borax.toml
sources = ["crossref", "openalex", "arxiv"]  # defaults
```

Every command also emits JSON Lines with `--json`, so you can pipe a run
into another program.

## Installation

Each [release][releases] includes prebuilt binaries for Linux (static,
musl), macOS (Apple silicon and Intel), and Windows. Download and unpack
the archive for your platform, then put `borax` on your `PATH`.

To install from source with Cargo:

```console
$ cargo install --git https://github.com/myshevchuk/borax-tools borax
```

[releases]: https://github.com/myshevchuk/borax-tools/releases

## Building from source

You need Rust 1.85 or newer because the workspace uses the 2024 edition.
You can install it with [rustup](https://rustup.rs/).

```console
$ git clone https://github.com/myshevchuk/borax-tools
$ cd borax-tools
$ cargo build --release
$ ./target/release/borax --help
```

The tests are offline and run on every platform:

```console
$ cargo test --workspace
```

Tests that call the real Crossref, OpenAlex, and arXiv APIs are
`#[ignore]`d. CI runs them weekly to detect when service responses drift
from the recorded cassettes. Run them yourself with
`cargo test -p borax-sources -- --ignored`.

## Documentation

- **[The manual](docs/manual.org)** — what a run does, every command and
  setting, the template language, external lookup tables, and run logs.
  Start here.
- [CHANGELOG.md](CHANGELOG.md) — what changed in each release.
- `openspec/specs/` — the behavioural specifications for the code.
  `openspec/project.md` describes their conventions.

## Layout

- `crates/borax-core` — pure logic: record model (CSL-JSON superset),
  template engine, rename planning, and BibTeX output. No I/O.
- `crates/borax-sources` — online source adapters, caching, rate
  limiting.
- `crates/borax-pdf` — PDF extraction behind an `Extractor` trait.
- `crates/borax` — the `borax` CLI binary.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
