import { describe, expect, it } from "vitest";
import { implementedBy, neverRan, regressionLabel } from "./evidence";
import type { Task } from "./types";

const task = (over: Partial<Task>): Task => ({
  id: "T007",
  title: "Implement greeting",
  phase: "Phase 3",
  state: "regression_only",
  attestation: null,
  blocked_by: [],
  commit: null,
  run_id: null,
  last_attempt: null,
  note: null,
  ...over,
}) as Task;

describe("evidence words", () => {
  it("names the reason of a regression-only tick, and says when none was recorded", () => {
    const t = task({
      regression: {
        reason: { reason: "already_passing", implemented_by: { task: "T003", commit: "d433d77" } },
        text: "already passing before the attempt — implemented by T003",
      },
    });
    expect(regressionLabel(t)).toBe("Regression only: already passing before the attempt — implemented by T003");
    expect(implementedBy(t)).toBe("T003");
    expect(regressionLabel(task({}))).toBe("Regression only: why was not recorded");
    expect(regressionLabel(task({ state: "verified" }))).toBeNull();
  });

  it("reads the file that never ran from a failed attempt's reason", () => {
    const t = task({
      state: "ready",
      last_attempt: { outcome: "verification_failed", reason: "tests not collected: `tests/unit/greet_name.rs` — Cargo builds …", run_id: null, reference: "refs/x" },
    });
    expect(neverRan(t)).toBe("tests/unit/greet_name.rs");
    expect(neverRan(task({}))).toBeNull();
  });
});
