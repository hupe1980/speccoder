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

import type { OracleView, ReviewReport } from "../lib/schema.gen";

/** US1's tests as written at review, with `over` applied. */
export function oracleView(over: Partial<OracleView> = {}): OracleView {
  return {
    req: "US1",
    title: "Login",
    scenarios: [
      { id: "US1-1", text: "**Given** a user, **When** they log in, **Then** a `login` function exists." },
      { id: "US1-2", text: "**Given** an empty password, **When** they log in, **Then** they are refused." },
    ],
    state: "written",
    frozen_on: null,
    frozen_by: null,
    frozen_at: null,
    tests: [
      { path: "tests/acceptance_us1.rs", content: "#[test]\nfn login() {\n    assert!(app::login());\n}\n", scenarios: ["US1-1"], red_class: "assertion", red: true, authored_by: "model", edited_by: null },
      { path: "tests/acceptance_us1_empty.rs", content: "#[test]\nfn empty() {}\n", scenarios: ["US1-2"], red_class: "passed", red: false, authored_by: "model", edited_by: null },
    ],
    discarded: [{ path: "tests/acceptance_none.rs", why: "cites_no_scenario" }],
    rejected: [],
    uncovered: ["US1-2"],
    untestable: [],
    ambiguities: [],
    held_out: { state: "kept", count: 2, manifest_sha256: "sha256:ab", reason: null },
    strength_target: 80,
    calls: 2,
    ambiguity_check: true,
    manifest_sha256: "sha256:cd",
    error: null,
    ...over,
  };
}

/** A review with one decision line and US1's tests, with `over` applied. */
export function review(over: Partial<ReviewReport> = {}): ReviewReport {
  return {
    feature: "001-greet",
    file: "specs/001-greet/review.json",
    requirements: [{ id: "FR-001", text: "Users MUST log in.", state: "open", by: null, at: null, comment: null, decided_on: null }],
    totals: { approved: 0, changes_requested: 0, open: 1, lapsed: 0, questions: 0, ambiguous: 0 },
    revise_with: "clarify",
    revision: null,
    questions: [],
    changes: [],
    history: [],
    oracle: [oracleView()],
    tests_notice: null,
    approval: null,
    sheet: { sha256: "sha256:5e1f", approves: ["FR-001"], blocked: [], autopilot_blocked: null, sandbox: "Sandbox: Seatbelt, network off", grants: [{ id: "g-0011223344556677", class: "command", object: "run `cargo run --example demo` · argv #3f2a1b0c9d8e" }] },
    ...over,
  };
}
