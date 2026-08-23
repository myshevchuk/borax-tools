#!/usr/bin/env python3
"""Catch spec deltas that would silently discard living-spec content.

An OpenSpec ``## MODIFIED Requirements`` block replaces a requirement in
``openspec/specs/`` wholesale, matching it by title alone. That makes a
delta a snapshot of the requirement as it read when the delta was
written: if a sibling change archives first and edits the same
requirement, archiving this one reverts the sibling's edit without
saying so. Nothing in the tooling warns, and the loss is visible only as
deleted lines in the archive commit.

This script compares every active change's MODIFIED requirements against
the living spec they will replace and fails when the living version
carries a scenario or a paragraph the delta has dropped. Deliberate
removals are expected -- that is often the point of a change -- so a
delta may acknowledge them with a marker directly above the requirement:

    <!-- drops: the journal scenarios, replaced by the run-logs capability -->

A MODIFIED requirement whose title matches nothing in the living spec is
always an error: archiving would silently turn it into an addition, and
the requirement it meant to edit would stay as it was.

Usage: check-spec-deltas.py [openspec-root]
"""

import re
import sys
from pathlib import Path

REQUIREMENT = re.compile(r"^### Requirement:\s*(?P<title>.+?)\s*$", re.MULTILINE)
SECTION = re.compile(r"^## (?P<name>[A-Z]+) Requirements\s*$", re.MULTILINE)
SCENARIO = re.compile(r"^#### Scenario:\s*(?P<name>.+?)\s*$", re.MULTILINE)
DROPS = re.compile(r"<!--\s*drops:.*?-->", re.IGNORECASE | re.DOTALL)


def requirements_in(text, section=None):
    """Map requirement title -> body, optionally within one ## section.

    The body runs from the title line to the next requirement or section
    heading, whichever comes first.
    """
    if section is not None:
        bounds = [(m.group("name"), m.start(), m.end()) for m in SECTION.finditer(text)]
        chosen = next((b for b in bounds if b[0] == section), None)
        if chosen is None:
            return {}
        start = chosen[2]
        following = [b[1] for b in bounds if b[1] > chosen[1]]
        text = text[start : following[0] if following else len(text)]

    found = {}
    marks = list(REQUIREMENT.finditer(text))
    for index, mark in enumerate(marks):
        end = marks[index + 1].start() if index + 1 < len(marks) else len(text)
        found[mark.group("title")] = text[mark.start() : end]
    return found


# How much of a living paragraph's vocabulary must survive somewhere in
# the delta's version of the requirement for the paragraph to count as
# edited rather than dropped. Measured against the whole requirement, so
# text moved between paragraphs still counts as kept. Calibrated on the
# add-ledger-and-run-logs deltas, where paragraphs that survived scored
# 0.97 and 1.00 and the ones genuinely lost scored 0.16 to 0.45.
RETENTION = 0.75


def tokens(text):
    """The words of `text` worth comparing: short ones carry no signal."""
    return set(re.findall(r"[a-z0-9`_.-]{3,}", text.lower()))


def paragraphs_of(body):
    """The prose paragraphs of a requirement body, normalized for
    comparison: scenarios, headings and markers are not prose, and line
    wrapping is not meaning."""
    prose = body[: SCENARIO.search(body).start()] if SCENARIO.search(body) else body
    prose = DROPS.sub("", REQUIREMENT.sub("", prose))
    return [
        " ".join(block.split())
        for block in re.split(r"\n\s*\n", prose)
        if block.strip() and not block.lstrip().startswith(("-", "#"))
    ]


def check(root):
    """Report every silent drop under `root`. Returns a list of
    problems, each a preformatted line."""
    problems = []
    changes = root / "changes"
    if not changes.is_dir():
        return problems

    for delta in sorted(changes.glob("*/specs/*/spec.md")):
        if "archive" in delta.parts:
            continue
        capability = delta.parent.name
        change = delta.parents[2].name
        living_path = root / "specs" / capability / "spec.md"
        delta_text = delta.read_text(encoding="utf-8")
        modified = requirements_in(delta_text, section="MODIFIED")
        if not modified:
            continue

        if not living_path.is_file():
            problems += [
                f"{change}/{capability}: MODIFIED {title!r} but "
                f"openspec/specs/{capability}/spec.md does not exist"
                for title in modified
            ]
            continue

        living = requirements_in(living_path.read_text(encoding="utf-8"))
        for title, body in modified.items():
            if title not in living:
                problems.append(
                    f"{change}/{capability}: MODIFIED {title!r} matches no "
                    f"requirement in the living spec -- archiving would add "
                    f"it and leave the intended one untouched"
                )
                continue
            # An acknowledgment sits immediately above the requirement
            # it excuses. Trailing blank lines are stripped first, or
            # the paragraph before the heading reads as empty.
            preceding = delta_text[: delta_text.index(body)].rstrip()
            if DROPS.search(preceding.rsplit("\n\n", 1)[-1]):
                continue

            lost_scenarios = {m.group("name") for m in SCENARIO.finditer(living[title])} - {
                m.group("name") for m in SCENARIO.finditer(body)
            }
            for name in sorted(lost_scenarios):
                problems.append(
                    f"{change}/{capability}: MODIFIED {title!r} drops scenario {name!r}"
                )

            present = tokens(body)
            for prose in paragraphs_of(living[title]):
                words = tokens(prose)
                if not words:
                    continue
                retained = len(words & present) / len(words)
                if retained >= RETENTION:
                    continue
                excerpt = prose if len(prose) <= 60 else prose[:57] + "..."
                problems.append(
                    f"{change}/{capability}: MODIFIED {title!r} drops prose "
                    f"({retained:.0%} of its wording survives): {excerpt}"
                )
    return problems


def main():
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "openspec")
    problems = check(root)
    if not problems:
        print("spec deltas: no silent drops")
        return 0
    print("Spec deltas would discard living-spec content:\n")
    for problem in problems:
        print(f"  {problem}")
    print(
        "\nRestore the content if the loss was accidental. If it is "
        "deliberate,\nacknowledge it with a marker above the requirement:\n"
        "\n    <!-- drops: why this content is going away -->"
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
