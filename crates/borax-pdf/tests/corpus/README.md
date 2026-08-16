# Real-PDF fixture corpus

Twelve small PDFs that exercise `borax-pdf` end to end: `PurePdf::open`
reads real files with a real cross-reference table, real encryption and a
real content stream, and `tiered::extract` runs over them.

Everything here is synthetic. The documents were written for this
corpus, they contain no third-party text, and every DOI uses the
`10.1234/` test prefix, which resolves to nothing. The arXiv identifiers
imitate the two numbering schemes without naming a real submission.

## Regenerating

The `.pdf` files are committed and the tests read them directly; CI has
no ghostscript and the test suite never shells out to it. Ghostscript is
needed only when a `.ps` source changes:

```sh
crates/borax-pdf/tests/corpus/generate.sh
```

The script works from any directory, refuses to run without `gs` on
`PATH`, and overwrites every `.pdf` in this directory. It was last run
with ghostscript 10.06.0.

Nine of the twelve fixtures are byte-reproducible: the sources pin the
creation dates, `xmp-prologue.ps` replaces the XMP packet pdfwrite would
otherwise stamp with a fresh UUID, and `-dOmitID=true` drops the trailer
`/ID`. The two encrypted fixtures cannot drop `/ID` — the standard
security handler derives the file key from it — and the rasterised one
is written by `pdfimage24`, which stamps a wall-clock creation date. Those
three change bytes on every run.

All pdfwrite output is `-dCompatibilityLevel=1.4`.

## The fixtures

`10.1234/borax.2024.NNN` is abbreviated to `…NNN` below.

| file | contents | `extract` yields |
| --- | --- | --- |
| `publisher-info-doi.pdf` | 2 pages. `/doi` and `/WPS-ARTICLEDOI` in the Info dictionary, both `…001`. No DOI in the text. | `EmbeddedMetadata`, DOI `…001` |
| `publisher-xmp-doi.pdf` | 2 pages. `<prism:doi>…002</prism:doi>` in the XMP packet, and nowhere else. | `EmbeddedMetadata`, DOI `…002` |
| `publisher-text-doi.pdf` | 2 pages. No DOI in Info or XMP; the masthead prints `https://doi.org/…003` on its third line. | `TextLayer`, DOI `…003` — **red today**, see below |
| `arxiv-new-id.pdf` | 2 pages. `arXiv:2401.12345v2 [cs.DL] 3 Jan 2024` as the first line of page one. No DOI. | `TextLayer`, arXiv `2401.12345` version 2 |
| `arxiv-old-id.pdf` | 2 pages. `arXiv:math.GT/0309136v1 8 Sep 2003` as the first line of page one. No DOI. | `TextLayer`, arXiv `math.GT/0309136` version 1 |
| `doi-on-third-page.pdf` | 4 pages behind two sheets of frontmatter; DOI `…006` only on page three. | `TextLayer`, DOI `…006` — page three is the last page the default limit reads — **red today**, see below |
| `doi-past-page-range.pdf` | 5 pages behind three sheets of frontmatter; DOI `…007` only on page four. | `Err(NoIdentifierFound)` — one page past the default limit |
| `encrypted-owner-only.pdf` | Permissions-only encryption: owner password set, user password empty. `/doi` `…008` in the Info dictionary. | opens without a password; `EmbeddedMetadata`, DOI `…008` |
| `encrypted-user-password.pdf` | Same source, encrypted under the user password `secret`. | `PurePdf::open` → `Err(Encrypted)` |
| `no-text-layer.pdf` | 1 page rasterised at 100 dpi by `pdfimage24`. The page shows a DOI; the file has no text layer to read it from. | `Err(NoTextLayer)` |
| `no-identifier.pdf` | 2 pages of ordinary prose. No DOI, no arXiv identifier, in text or metadata. | `Err(NoIdentifierFound)` |
| `malformed-truncated.pdf` | The first seven tenths of `publisher-info-doi.pdf`: the cross-reference table and trailer are gone. | `PurePdf::open` → `Err(Unreadable { .. })` |

Every fixture except `no-text-layer.pdf` carries an XMP packet, because
pdfwrite always writes one. Only `publisher-xmp-doi.pdf` has an
identifier in it.

## Two fixtures are red on the pure backend

`pdf-extract` 0.12 implements `Tj`, `TJ`, `T*`, `TD` and `TL` but not the
`'` and `"` text-showing operators. Ghostscript emits `'` for every line
after the first in a text object, so the pure backend currently reads
only the first line of each page:

```
(Fixtures for Tiered Identifier Extraction)Tj
18 TL
(Journal of Synthetic Instrumentation, vol 3, no 2, 2024)'
14 TL
(https://doi.org/10.1234/borax.2024.003)'
```

`publisher-text-doi.pdf` and `doi-on-third-page.pdf` therefore yield
`Err(NoIdentifierFound)` until the backend handles `'` and `"`. The
identifiers really are in the files — `gs -sDEVICE=txtwrite` prints them
— so the fix belongs in the backend, not in the fixtures. The arXiv
stamps sit on the first line of their page precisely so those two
fixtures do not depend on it.

## The sources

One `.ps` per fixture, plus two shared pieces:

- `xmp-prologue.ps` — the fixed, identifier-free XMP packet read ahead of
  every source but `publisher-xmp-doi.ps`, which supplies its own.
- `encrypted.ps` — the source for both encrypted fixtures; the
  difference between them is entirely in the ghostscript arguments.
- `no-text-layer.ps` — typeset text that is rasterised rather than
  written as a text layer, so the output contains only pixels.

`malformed-truncated.pdf` has no source of its own: `generate.sh`
truncates the generated `publisher-info-doi.pdf`.
