# Spec Kit 1.1.3 with the first-party `bug` extension

Captured on 2026-10-26 with the Spec Kit CLI `specify 1.1.3` (Homebrew):

    specify init --here --integration claude --script sh --force --ignore-agent-tools --non-interactive
    specify extension add bug        # Bug Triage Workflow 1.0.1 (spec-kit-core, catalog "default")
    specify workflow add bugfix      # Guided Bug Fix 1.0.0
    specify extension list --json > extension-list.json

`.specify/` (without its download caches) and the extension's three skills
under `.claude/skills/` are copied as installed. `extension-list.json` is the
CLI's own answer, kept to pin the inventory's CLI source.

`.specify/bugs/empty-title-crash/assessment.md` is laid out exactly as the
extension's `speckit.bug.assess` template (extension 1.0.1) writes it, with the
person's triad quoted verbatim in the Report section — the shape speccoder's
read-back is pinned to. It was written by hand to that template, not by a
model run.

Re-capture this fixture for every Spec Kit release the compatibility matrix
adds; the `assessment.md` shape is not documented as stable.
