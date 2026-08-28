## REMOVED Requirements

### Requirement: Undo reverts the last applied run safely
**Reason**: `borax undo` restores the exact names a run replaced, which
is not the state a user who dislikes the result wants — they want a
better name, not the previous one. Because the pipeline identifies a
file by its content hash and not by its name, correcting the template or
the metadata and re-running already reaches the wanted state from
wherever the files now sit. What re-running cannot reconstruct is the
old-to-new mapping, and the apply-run log already records it, so
reversing the moves recovers no information the log does not hold.
Reverting is also the less consistent recovery: it never appends to or
corrects the ledger, leaving entries pointing at paths that no longer
exist, and it never moves the sidecar written beside the new name,
stranding it beside the restored one. Re-running has neither problem.

**Migration**: Correct the template or the metadata and run `rename
--apply` again; the files are found by content wherever the previous run
put them, including subdirectories a template created. To recover the
names a run replaced, read its apply-run log under `.borax/runs/` (or
the XDG state directory): every `renamed` line carries the original
path, the new path and the content hash the file had when it moved. The
log is unchanged by this removal, and remains mandatory and flushed
before the first rename.
