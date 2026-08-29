## ADDED Requirements

### Requirement: A lookup table is a named external file with declared key and value columns
Configuration SHALL be able to declare named lookup tables, each naming a file to read, one or more columns supplying that table's keys, and one column supplying its values. A table is a map from string to string and holds no other power: it cannot execute, and it cannot influence a run except by substituting one string for another.

Declaring the key and value columns by name, rather than by position, is what lets one externally maintained file serve borax and other tools that read it in a different direction.

#### Scenario: A table declared over a curated journal file
- **WHEN** configuration declares a table `jcode` with the key column
  `title`, the value column `abbreviation`, and a path to a file whose
  header row is `abbreviation`, `title`, `shorttitle`
- **THEN** the table maps each row's title to that row's abbreviation,
  and the `shorttitle` column is ignored

#### Scenario: Two tables over one file
- **WHEN** configuration declares a second table over the same file with
  the same key column and the value column `shorttitle`
- **THEN** both tables load and each maps titles to its own column's
  values

### Requirement: Tab-separated format contract
A table file SHALL be tab-separated, with the first non-blank line a header row naming the columns. Every column the declaration names MUST be present in the header; a column named in the header but not in the declaration MUST be tolerated and ignored.

Subsequent non-blank lines are rows, keyed by the header's column names. A leading byte-order mark MUST be ignored, blank lines MUST be skipped, and both LF and CRLF line endings MUST be accepted. A row whose key cell or value cell is missing or empty MUST be skipped and reported as a warning, without aborting the load.

#### Scenario: Well-formed file loads
- **WHEN** a file's header is `abbreviation`, `title`, `shorttitle` and
  one row reads `AA`, `Amino Acids`, `Amino Acids`
- **THEN** a table keyed on `title` with value `abbreviation` maps
  `Amino Acids` to `AA`

#### Scenario: Byte-order mark and CRLF endings
- **WHEN** a table file begins with a byte-order mark and uses CRLF line
  endings
- **THEN** it loads, and the mark is not part of the first column's name

#### Scenario: Row with an empty value cell
- **WHEN** a row supplies a key but no value
- **THEN** that row is skipped, a warning names the file and the line,
  and every other row still loads

#### Scenario: Declared column absent from the header
- **WHEN** a declaration names a value column the file's header does not
  have
- **THEN** the run ends as a configuration error naming the table, the
  column, and the file

### Requirement: Matching keys are normalized by a fixed fold
Every key a table supplies and every value looked up in it SHALL be folded to a canonical matching key before comparison, by these four steps in order, and no others.

1. Transliterate: fold the Latin letters borax's `transliterate` filter
   names — `ä`→`ae`, `ö`→`oe`, `ü`→`ue`, `ß`→`ss` and their uppercase
   forms; `æ`→`ae`, `ø`→`o`, `å`→`a`, `đ`→`d`, `ł`→`l`, `ñ`→`n`,
   `ç`→`c`; the accented vowels lose their accents. A character with no
   folding passes through unchanged.
2. Lowercase, by Unicode default case conversion.
3. Replace every maximal run of characters outside `a-z0-9` with a
   single `-`.
4. Trim leading and trailing `-`.

This is the fold the `slug` filter already performs, and the two MUST stay one definition. A string whose fold is empty is not a key: a row whose key folds to empty MUST be dropped with a warning, and an input that folds to empty MUST NOT match any row.

The fold is normative rather than illustrative because it is the contract between borax and any other tool reading the same table: two tools implementing these four steps resolve the same title to the same row.

#### Scenario: Punctuation and case do not affect matching
- **WHEN** a table row's key is `Journal of the American Chemical
  Society` and a record's container title is `Journal of the American
  Chemical Society.`
- **THEN** the lookup matches that row

#### Scenario: Abbreviated forms fold alike
- **WHEN** a table row's key is `J. Am. Chem. Soc.` and a record's
  container title is `J Am Chem Soc`
- **THEN** the lookup matches that row

#### Scenario: Diacritics fold in the German manner
- **WHEN** a table row's key is `Zeitschrift für Chemie`
- **THEN** its matching key is `zeitschrift-fuer-chemie`

#### Scenario: A title that folds to nothing matches nothing
- **WHEN** a record's container title consists entirely of characters
  the fold removes
- **THEN** the lookup is a miss rather than a match against any row

### Requirement: A key claimed by two different values is a load-time error
A table in which two rows fold to the same matching key with different values SHALL end the run as a configuration error, naming the table, the matching key, and both values. Rows that fold to the same key with the same value are not a conflict and MUST load without complaint, since a curated file may legitimately repeat one form in two columns.

Keeping one of two conflicting rows silently is how a collection acquires a systematically wrong value that cannot be explained afterwards; a table that renames files is worth refusing until the ambiguity is resolved.

#### Scenario: Two rows disagree about one journal
- **WHEN** a table's key column supplies `J. Chem. Soc.` for value `JCS`
  and `J Chem Soc` for value `JCHS`
- **THEN** the run ends as a configuration error naming the table, the
  folded key, and both values

#### Scenario: One row repeats a form in two key columns
- **WHEN** a table draws keys from both `title` and `shorttitle` and a
  row's two cells are identical
- **THEN** the row contributes one key and the table loads

### Requirement: The lookup filter substitutes a value or renders empty
A `lookup("<table>")` filter SHALL replace its input with the value the named table holds for that input's matching key, and SHALL render the empty string when the table holds no such key. Rendering MUST NOT fail on a miss.

Rendering empty is what makes a miss composable: the alternatives mechanism already selects the first chain producing non-empty output, so a fallback needs no new syntax.

#### Scenario: A hit substitutes the table's value
- **WHEN** `[journal:lookup("jcode")]` renders a record whose container
  title the `jcode` table maps to `JACS`
- **THEN** the token renders `JACS`

#### Scenario: A miss falls through to the alternative
- **WHEN** `[journal:lookup("jcode") || journal:abbr]` renders a record
  whose container title the table does not hold
- **THEN** the token renders the abbreviation the `abbr` filter produces

#### Scenario: A miss with no alternative renders nothing
- **WHEN** `[journal:lookup("jcode")]` renders a record whose container
  title the table does not hold
- **THEN** the token contributes nothing and the run continues

### Requirement: Unmatched lookups are reported
A run SHALL report every distinct table-and-input pair for which a lookup found no row, and SHALL count them in its summary. Reporting is per distinct pair rather than per occurrence: a hundred files in one journal produce one report.

A miss that renders empty and says nothing is how a collection is named wrongly without anyone learning which row to add. The report exists so the user can extend the table.

#### Scenario: An unmatched journal is named once
- **WHEN** twelve files in a run resolve to the same container title and
  the table holds no row for it
- **THEN** the run reports that table and that title once, and the
  summary counts one unmatched lookup

#### Scenario: A run with no misses reports none
- **WHEN** every lookup in a run finds a row
- **THEN** the run reports no unmatched lookups and the summary carries
  a zero count without a confusing empty listing

### Requirement: A run records the tables it read
A run that loaded any table SHALL name each one in its opening event, by path and by a digest of the file's contents. Rendering is deterministic given a table, but a table is a file that changes; without this the run log records what a run did and not why it did it.

#### Scenario: The run log identifies the table
- **WHEN** a run renames files using a template that looks up a table
- **THEN** its run log's opening event names that table's path and a
  digest of the contents it was read with

### Requirement: Table failures end a run before it starts
Every failure that is a property of a table's declaration or contents SHALL be settled before the first file is processed, ending the run with the fatal exit code, the reason on stderr, and neither `run-started` nor `run-finished` on stdout. A declared file that cannot be read, a header without a declared column, and a key claimed by two values are all of this kind.

A table is configuration, so a broken one is wrong for every file in the batch and there is nothing to be gained by discovering that once per file.

#### Scenario: Declared table file is missing
- **WHEN** configuration declares a table whose path does not exist
- **THEN** the run ends as a configuration error naming the table and
  the path, having emitted no events and processed no file
