## MODIFIED Requirements

<!-- drops: nothing; the requirement gains the scope it was silent
     about — which command compiles which template table -->

### Requirement: Templates fail fast at load time
The engine SHALL report unknown fields, unknown filters, unknown lookup
tables, and template syntax errors as configuration errors when the
template is loaded — before any file is processed — naming the template
and the offending token.

A `lookup` naming a table no configuration declared is of this kind: the
set of declared tables is known when templates are compiled, so a
template that could never resolve is refused then rather than rendering
empty on every file.

A run SHALL compile the template tables it renders from and no others. A
command that renders no filename therefore does not compile the filename
templates, and a filename template that will not compile cannot end it —
a run must not be refused over a value it was never going to render. A
command that renders neither, such as one that only reports the
effective configuration, compiles neither and cannot be ended by a
template at all; reporting a configuration is most useful on the
configurations other commands refuse.

Lookup tables are loaded regardless, since loading one is what validates
the `lookup` tokens in whichever templates are compiled, and a table
that will not load is a fault in the configuration rather than in a
template.

#### Scenario: Typo in a filter name
- **WHEN** a configured template contains `[title:lwoer]`
- **THEN** the run aborts before processing files with an error naming the
  unknown filter `lwoer`

#### Scenario: Lookup names no declared table
- **WHEN** a configured template contains `[journal:lookup("jcode")]`
  and no `jcode` table is declared
- **THEN** the run aborts before processing files with an error naming
  the template and the unknown table `jcode`

#### Scenario: A filename template stops only the runs that render one
- **WHEN** `templates.default` will not compile and `borax bib` runs
- **THEN** the bibliography run proceeds, and `borax rename` in the same
  directory still aborts before processing files naming that template
