import { describe, expect, it } from "vitest";
import { neverRan, regressionLabel } from "./evidence";
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
        reason: { reason: "in_progress" },
        text: "turned none of its story's scenarios green and broke none: the story goes on",
      },
    });
    expect(regressionLabel(t)).toBe("Regression only: turned none of its story's scenarios green and broke none: the story goes on");
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
