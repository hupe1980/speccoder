import { describe, expect, it } from "vitest";
import { STAGES, at, blocked, command, outcomeWords } from "./bug";
import type { BugReport } from "./schema.gen";
import { bugReport } from "../test/fixtures";

describe("blocked", () => {
  it("needs the symptom and at least one expected behaviour", () => {
    expect(blocked("", ["x"], [])).toBe("Say what happens");
    expect(blocked("  ", ["x"], [])).toBe("Say what happens");
    expect(blocked("it crashes", [], [])).toMatch(/^Say what should happen instead/);
    expect(blocked("it crashes", ["", "  "], [])).toMatch(/^Say what should happen instead/);
    expect(blocked("it crashes", ["it is refused"], [])).toBeNull();
  });

  it("wants one named test per expected behaviour", () => {
    expect(blocked("it crashes", ["a", "b"], ["t.rs::a"])).toBe("One test per expected behaviour, in order");
    expect(blocked("it crashes", ["a", "b"], ["t.rs::a", "t.rs::b"])).toBeNull();
    expect(blocked("it crashes", ["a"], ["  "])).toBeNull();
  });
});

describe("command", () => {
  it("is the command the sheet runs, quoted, blanks left out", () => {
    expect(
      command({ symptom: ' say "hi" ', expected: ["a", " "], unchanged: ["b"], issue: 42, tests: ["t.rs::a"], candidate: " pr:7 ", run: false }),
    ).toBe('speccoder bug "say \\"hi\\"" --expected "a" --unchanged "b" --issue 42 --test t.rs::a --candidate pr:7 --no-run');
    expect(command({ symptom: "x", expected: ["y"], unchanged: [], issue: null, tests: [], candidate: "", run: true })).toBe(
      'speccoder bug "x" --expected "y"',
    );
  });
});

describe("at", () => {
  it("follows the stage, and a finished bug only past the end when fixed", () => {
    expect(at(null)).toBe(0);
    expect(at(bugReport({ stage: "confirm" }))).toBe(STAGES.findIndex((s) => s.stage === "confirm"));
    expect(at(bugReport({ stage: "done", outcome: "fixed" }))).toBe(STAGES.length);
    expect(at(bugReport({ stage: "done", outcome: "cannot_reproduce" }))).toBe(1);
  });
});

describe("outcomeWords", () => {
  it("names every outcome, and verified only when it was", () => {
    const outcomes: BugReport["outcome"][] = ["recorded", "awaiting_confirmation", "cannot_reproduce", "already_satisfied", "unchanged_not_holding", "fixed", "failed"];
    for (const outcome of outcomes) expect(outcomeWords(bugReport({ outcome }))).toBeTruthy();
    expect(outcomeWords(bugReport({ outcome: "fixed", verdict: { verdict: "verified" } }))).toBe("Fixed and verified");
    expect(outcomeWords(bugReport({ outcome: "fixed", verdict: null }))).toBe("Fixed");
  });
});
