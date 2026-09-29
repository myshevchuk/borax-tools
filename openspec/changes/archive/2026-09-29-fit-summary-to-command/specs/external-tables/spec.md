## MODIFIED Requirements

### Requirement: Unmatched lookups are reported
A run SHALL report every distinct table-and-input pair for which a lookup found no row, and SHALL count them in its summary. Reporting is per distinct pair rather than per occurrence: a hundred files in one journal produce one report.

The summary that counts them is the `unmatched` total of `run-finished`,
in the JSON output and in the run log. In human output, `rename` and
`bib` state a nonzero count in their closing summary line. `adopt` has
no summary line, and its misses are reported on lines of their own
directly above its closing totals, which is where a person reads how
many there were.

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
