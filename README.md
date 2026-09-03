# borax-tools

A fast, error-free CLI suite for bibliography work. Give it a file,
typically a PDF you have just downloaded. It finds the DOI or arXiv id,
looks up the identifier with Crossref, OpenAlex, and arXiv, then renames
the file after the returned record. You choose the new name with a small
template language.

The pipeline is stateless. It previews by default, never overwrites, and
never guesses. It leaves an unidentified file in place and prints the
reason.

Status: pre-release. `openspec/` specifies the behaviour, and
`openspec/STATE.md` tracks how much is built. Versions are `0.y.z` and
do not promise compatibility yet.

## Examples

Preview what a directory of downloads would be called. Nothing moves:

```console
$ borax rename papers/
papers/1-s2.0-S0009261421001234.pdf: resolved 10.1021/jacs.4c01234 via crossref
papers/1-s2.0-S0009261421001234.pdf: would rename to papers/smith2024_AwesomePaperBorax.pdf
papers/scan003.pdf: skipped, no identifier found
1 resolved, 0 renamed, 1 skipped
```

Apply the renames and merge each record into a master bibliography:

```console
$ borax rename --apply --bib library.bib papers/
papers/1-s2.0-S0009261421001234.pdf: resolved 10.1021/jacs.4c01234 via crossref (cached)
papers/1-s2.0-S0009261421001234.pdf: renamed to papers/smith2024_AwesomePaperBorax.pdf
papers/scan003.pdf: skipped, no identifier found
papers/smith2024_AwesomePaperBorax.pdf: bibliography entry smith2024 added
1 resolved, 1 renamed, 1 skipped
```

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
