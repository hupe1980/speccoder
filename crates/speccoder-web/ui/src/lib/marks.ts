// A state's mark and word, beside its colour: nothing depends on colour.
export const marks: Record<string, [string, string]> = {
  verified: ["✓", "verified"],
  regression_only: ["~", "regression only"],
  attested: ["A", "attested"],
  ticked_without_evidence: ["!", "no evidence"],
  evidence_without_tick: ["?", "tick missing"],
  skipped: ["–", "skipped"],
  ready: ["·", "ready"],
  blocked: ["⧗", "blocked"],
  running: ["◐", "running"],
  needs_approval: ["△", "needs you"],
  failed: ["✗", "failed"],
};

export const mark = (state: string) => marks[state]?.[0] ?? "·";
export const word = (state: string) => marks[state]?.[1] ?? state.replaceAll("_", " ");
