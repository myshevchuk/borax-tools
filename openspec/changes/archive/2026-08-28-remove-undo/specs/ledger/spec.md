## MODIFIED Requirements

<!-- drops: undo as a way an entry goes stale, and the scenario name
     that named it; deleting or moving a file by hand is now the whole
     of it -->

### Requirement: Stale entries never block re-admission
A duplicate report SHALL first verify that the entry's recorded path
still exists in the collection; if it does not (the file was deleted or
moved), the entry is stale — the incoming file is processed normally
and a warning suggests `borax ledger rebuild`. Disk is the source of
truth; the ledger alone never vetoes an admission.

#### Scenario: Duplicate of a vanished admission
- **WHEN** an incoming file matches a ledger entry whose recorded path
  no longer exists
- **THEN** the file is processed normally and the run warns that the
  ledger holds stale entries
