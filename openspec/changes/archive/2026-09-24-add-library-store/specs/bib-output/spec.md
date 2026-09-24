## ADDED Requirements

### Requirement: A sidecar is derived citation output and governs no decision
A sidecar SHALL be derived output: borax regenerates one whenever it
writes bibliography output for a file, and SHALL NOT read one to decide
what a file's record is or what name that file is given. A sidecar is a
citation artifact for whoever reads `.bib` files — LaTeX, another
bibliography tool, a person — and borax's own knowledge of a file comes
from extraction, the services, and the content index.

A sidecar SHALL NOT be an artifact record, and nothing in the library
SHALL depend on one. The two are different files with different
readers, different lifetimes and different authority:

| | Artifact record | Citation sidecar |
|---|---|---|
| Holds | the item link, the hash history, the last-known path | a BibTeX entry and the canonical record for a work |
| Read by | borax | LaTeX, other bibliography tools, a person |
| Authoritative | yes | no; derived and regenerable |
| Lives | flat under `.borax/artifacts/`, named by artifact identity | beside the artifact, named for the file |
| Default | library state, always kept | optional, off by default |

Deleting every sidecar in a library SHALL therefore change no item, no
artifact record, no count borax reports and no name it renders. A
sidecar is written because someone asked for citation output, and its
absence says nothing about what the library holds.

What a sidecar is, where it goes and what it contains are unchanged by
this requirement: it is still named by appending the sidecar extension
to the file's final name, still carries the entry and the full record,
still never overwrites a file borax did not write, and is still off by
default.

#### Scenario: Deleted sidecars change no record and no name
- **WHEN** every sidecar in a library is deleted and a run with sidecar
  output enabled reaches the same files again
- **THEN** each file resolves to the same record, renders the same
  target name, and is reported with the same outcome as before, and its
  sidecar is written again

#### Scenario: A sidecar is not the artifact record
- **WHEN** a library holds `paper.pdf`, its `paper.pdf.bib` sidecar and
  an artifact record for it under `.borax/artifacts/`
- **THEN** `borax status` counts one artifact and one artifact record,
  the sidecar is neither, and deleting the sidecar leaves both counts
  unchanged

#### Scenario: A hand-edited sidecar decides nothing
- **WHEN** a sidecar is edited by hand to name a different work and the
  run reaches its file again
- **THEN** the file's record and its rendered name are what extraction,
  the services and the content index give, and the edited sidecar is
  overwritten with the regenerated one
