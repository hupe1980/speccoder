// A bug, as the Bug sheet shows it: its stage strip, whether the triad can
// be recorded, and the command each step equals. Pure, so the page's rules
// are type-checked with the generated types and decide nothing by wording.

import type { BugReport, BugStage } from "./schema.gen";

/** The stages, in order, as the strip shows them. */
export const STAGES: { stage: BugStage; label: string }[] = [
  { stage: "record", label: "Record" },
  { stage: "reproduce", label: "Reproduce" },
  { stage: "confirm", label: "Confirm" },
  { stage: "unchanged", label: "Unchanged" },
  { stage: "fix", label: "Fix" },
  { stage: "ship", label: "Ship" },
];

/** Where the strip stands for `bug` (`done`: past the last stage). */
export function at(bug: BugReport | null): number {
  if (!bug) return 0;
  if (bug.stage === "done") return bug.outcome === "fixed" ? STAGES.length : STAGES.findIndex((s) => s.stage === "reproduce");
  const i = STAGES.findIndex((s) => s.stage === bug.stage);
  return i < 0 ? 0 : i;
}

/** Why the triad cannot be recorded yet, or null. */
export function blocked(symptom: string, expected: string[], tests: string[]): string | null {
  if (!symptom.trim()) return "Say what happens";
  const e = expected.map((x) => x.trim()).filter(Boolean);
  if (!e.length) return "Say what should happen instead — the reproduction asserts it";
  const t = tests.map((x) => x.trim()).filter(Boolean);
  if (t.length && t.length !== e.length) return "One test per expected behaviour, in order";
  return null;
}

const quote = (s: string) => `"${s.replaceAll('"', '\\"')}"`;

/** The `speccoder bug` command the sheet runs. */
export function command(f: { symptom: string; expected: string[]; unchanged: string[]; issue: number | null; tests: string[]; candidate: string; run: boolean }): string {
  const words = [`speccoder bug ${quote(f.symptom.trim())}`];
  for (const e of f.expected.filter((x) => x.trim())) words.push(`--expected ${quote(e.trim())}`);
  for (const u of f.unchanged.filter((x) => x.trim())) words.push(`--unchanged ${quote(u.trim())}`);
  if (f.issue) words.push(`--issue ${f.issue}`);
  for (const t of f.tests.filter((x) => x.trim())) words.push(`--test ${t.trim()}`);
  if (f.candidate.trim()) words.push(`--candidate ${f.candidate.trim()}`);
  if (!f.run) words.push("--no-run");
  return words.join(" ");
}

/** The words for an outcome. */
export function outcomeWords(b: BugReport): string {
  switch (b.outcome) {
    case "recorded": return b.confirmation ? "Confirmed — the fix runs next" : "Recorded — not reproduced yet";
    case "awaiting_confirmation": return "The reproductions fail on the untouched code: do they show the bug?";
    case "cannot_reproduce": return "Could not be reproduced — nothing was fixed";
    case "already_satisfied": return "Already fixed — every reproduction passes";
    case "unchanged_not_holding": return "An Unchanged statement does not hold today";
    case "fixed": return b.verdict?.verdict === "verified" ? "Fixed and verified" : "Fixed";
    case "failed": return "The fix did not pass";
  }
}
