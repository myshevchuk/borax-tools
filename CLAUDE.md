# CLAUDE.md — borax-tools

Project-specific instructions for anyone working in this repository,
human or AI. Where these conflict with the global conventions, these
win. Behavioural requirements live in `openspec/specs/`; the conventions
around them live in `openspec/project.md`; this file records the few
rules that override both.

## Compatibility is not promised before 1.0.0

borax-tools is in `0.y.z`. Until `1.0.0` ships, **nothing about this
project is a compatibility commitment** — not the CLI surface, not the
configuration keys, not the JSONL event and record schemas, not the
on-disk formats (response cache, rename journal, ledger, run logs), and
not any `borax-*` crate API. Any of it may change in any release, with
no deprecation period and no migration path.

What this means in practice:

- **Do not design around compatibility.** No compatibility shims, no
  dual-format readers, no version negotiation, no "legacy" branches kept
  alive to preserve what an earlier `0.y.z` did. When something is
  replaced, the old thing is deleted in the same change.
- **"This would break existing users" is not an argument here.** A
  proposal, design, or review comment that rests on it is reasoning from
  a constraint the project does not yet have. Say what the right shape
  is and build that.
- **Breaking changes need no ceremony** beyond a `CHANGELOG.md` entry
  that states plainly what broke, so the history stays legible. They do
  not need a MAJOR bump, a deprecation release, or a migration guide.
- **On-disk state is disposable.** Users of a pre-1.0 release are
  expected to be able to lose their `.borax/` accounting, their cache,
  and their journal without losing data. Preserve that property — it is
  what makes the freedom above safe — rather than preserving formats.

The one thing this does *not* license: changes that can lose a user's
**files**. "Error-free wins any conflict" still governs, and the rename
path, the sidecar-collision rules, and the apply gate are as strict as
they ever were. Compatibility is cheap here; data is not.

## Version numbers are bookkeeping

Development is fast and the version number carries little signal while
the surface is still moving. Treat it as a release counter, not a
contract:

- **Do not agonize over MINOR vs PATCH.** Pick the one that reads right
  and move on; a wrong guess costs nothing and is not worth a
  discussion. The global convention's careful MAJOR/MINOR/PATCH
  reasoning applies from `1.0.0` onward.
- **`0.y.z` will not become `1.0.0` by accumulation.** The jump happens
  when the public surface is deliberately frozen — which needs at least
  one external consumer of the JSONL schemas to have proven them (see
  `openspec/STATE.md`, "Live risks") — not when the version has climbed
  far enough.
- Release when there is something worth releasing. `CHANGELOG.md` and
  the tag still have to agree with the in-repo version; that discipline
  is about not publishing an inconsistent tree, and it stays.
