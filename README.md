# borax-tools

A fast, error-free CLI suite for bibliography work. The core job: given a
file (typically a PDF), resolve its bibliographic metadata from online
sources (Crossref, OpenAlex, arXiv) and rename the file by configurable
template rules — as a stateless pipeline that previews by default, never
overwrites, never guesses, and can undo.

Status: pre-release. The architecture and behaviour are specified in
`openspec/` (see the `add-core-pipeline` change); implementation is in
progress.

## Layout

- `crates/borax-core` — pure logic: record model (CSL-JSON superset),
  template engine, rename planning, BibTeX output. No I/O.
- `crates/borax-sources` — online source adapters, caching, rate limiting.
- `crates/borax-pdf` — PDF extraction behind an `Extractor` trait.
- `crates/borax` — the `borax` CLI binary.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
