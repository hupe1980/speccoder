// What a task card and the inspector say about its evidence, from the
// fields speccoder records — no component composes its own sentence.

import type { Task } from "./types";

/** A regression-only card's accessible name and tooltip: speccoder's wording. */
export function regressionLabel(t: Task): string | null {
  if (t.state !== "regression_only") return null;
  return `Regression only: ${t.regression?.text ?? "why was not recorded"}`;
}

/** The task whose tick made already-passing tests pass, when known. */
export function implementedBy(t: Task): string | null {
  const r = t.regression?.reason;
  if (r?.reason !== "already_passing" || !r.implemented_by || !("task" in r.implemented_by)) return null;
  return r.implemented_by.task;
}

/** The first test file an attempt wrote that never ran, from its saved reason. */
export function neverRan(t: Task): string | null {
  const m = t.last_attempt?.reason.match(/^tests not collected: `([^`]+)`/);
  return m?.[1] ?? null;
}
