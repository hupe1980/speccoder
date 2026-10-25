// A state's mark, symbol and word, beside its colour: nothing depends on
// colour. The mark is the CLI's (for text and screen readers); the symbol is
// what the page draws.
export const marks: Record<string, [string, string, string]> = {
  verified: ["✓", "verified", "seal"],
  regression_only: ["~", "regression only", "loop"],
  attested: ["A", "attested", "signature"],
  ticked_without_evidence: ["!", "no evidence", "exclaim"],
  evidence_without_tick: ["?", "tick missing", "exclaim"],
  skipped: ["–", "skipped", "skip"],
  ready: ["·", "ready", "circle"],
  blocked: ["⧗", "blocked", "hourglass"],
  running: ["◐", "running", "sparkle"],
  needs_approval: ["△", "needs you", "hand"],
  failed: ["✗", "failed", "xoctagon"],
};

export const mark = (state: string) => marks[state]?.[0] ?? "·";
export const word = (state: string) => marks[state]?.[1] ?? state.replaceAll("_", " ");
export const symbol = (state: string) => marks[state]?.[2] ?? "circle";
/** The order work flows in, for bars and rings. */
export const ORDER = ["verified", "regression_only", "attested", "ticked_without_evidence", "skipped", "ready", "blocked"];

// "Phase 3: User Story 1 - Login (Priority: P1) 🎯 MVP" → "US1 · Login".
export const phaseLabel = (phase: string): string => {
  const m = phase.match(/User Story (\d+)\s*[-–—:]\s*(.*?)\s*(?:\(Priority[^)]*\))?\s*(?:🎯\s*MVP)?\s*$/);
  return m ? `US${m[1]} · ${m[2]}` : phase.replace(/^Phase \d+:\s*/, "");
};
