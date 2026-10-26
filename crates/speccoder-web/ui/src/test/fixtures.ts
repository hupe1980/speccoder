// What the page's tests are shown: a bug as the host's `bug` payload has it.
// Test-only; nothing the page imports.

import type { BugReport } from "../lib/schema.gen";

/** A bug waiting for the person's confirmation, with `over` applied. */
export function bugReport(over: Partial<BugReport> = {}): BugReport {
  return {
    acceptance: null,
    candidate: null,
    code: null,
    commit: null,
    confirm_digest: "sha256:5f1c0d9e8b7a6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f10",
    confirmation: null,
    handoff: null,
    held_out: [],
    held_out_sealed: null,
    home: { kind: "standing_speckit", location: "specs/001-bugfixes", reason: "Spec Kit without the `bug` extension" },
    issue: null,
    max_tries: 3,
    outcome: "awaiting_confirmation",
    pass_to_pass: null,
    recorded_commit: "8b70757",
    rejections: [],
    reproductions: [
      { argv: ["cargo", "test", "--test", "repro_e1"], baseline: ["assertion", "assertion"], digest: "sha256:e1", file: "tests/repro_e1.rs", final: "not_run", revert: "not_run", source: "author", statement: "E1", test: "e1" },
    ],
    requirement: "FR-001",
    revert: null,
    run_id: null,
    special_case: [],
    stage: "confirm",
    task: "T001",
    title: "Saving an empty title crashes",
    triad: {
      current: { id: "C", text: "saving an empty title crashes" },
      digest: "sha256:cc21",
      expected: [{ id: "E1", text: "an empty title is refused with a message" }],
      unchanged: [],
    },
    tries: [],
    unit: "001-bugfixes",
    verdict: null,
    ...over,
  };
}
