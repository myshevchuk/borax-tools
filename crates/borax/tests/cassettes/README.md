# End-to-end cassettes

Four recorded-shaped responses for the fixtures the end-to-end test runs
over, one per identifier the corpus carries.

Everything here is synthetic, as the PDFs they answer for are. The
corpus in `crates/borax-pdf/tests/corpus` uses the `10.1234/` test DOI
prefix and arXiv identifiers that name no real submission, so no service
has a response to record for them. These are written to the shape the
real ones have — Crossref's `message` envelope, arXiv's Atom feed — and
carry the fields the readers in `borax-sources` actually consume.

They are not a substitute for the recorded cassettes in
`crates/borax-sources/tests/cassettes`, which are real responses and are
what pins the readers' behaviour against the services. These exist so a
whole run can be exercised over real PDFs without a network, and they
prove composition, not parsing.

Each response's `title` matches the Info-dictionary title of the fixture
it answers for, so the conflict check clears the file rather than
skipping it. A title deliberately at odds with a fixture belongs in a
conflict test, not here.

| cassette | identifier | fixture |
| --- | --- | --- |
| `crossref-borax-001.json` | `10.1234/borax.2024.001` | `publisher-info-doi.pdf` |
| `crossref-borax-002.json` | `10.1234/borax.2024.002` | `publisher-xmp-doi.pdf` |
| `arxiv-2401.12345.xml` | `2401.12345` | `arxiv-new-id.pdf` |
| `arxiv-math.GT-0309136.xml` | `math.GT/0309136` | `arxiv-old-id.pdf` |
