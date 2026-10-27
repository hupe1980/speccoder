// The logic behind the doors: keys, commands, readiness words, stop cards,
// checklists and pacing — what a person reads and which key does what.

import { describe, expect, it, vi } from "vitest";
import { chainSkills, checklist, composerCommand, debounce, featureCommands, move, networkSteps, pace, quote, readinessChange, readinessParts, rungForKey, sameWording, short, slug, stepMark, stopCard, throughChoices } from "./start";
import type { ProjectPlan, Readiness } from "./types";

const ready = (over: Partial<Readiness> = {}): Readiness => ({ requirements: 4, scenarios: 9, open_questions: 1, untestable: ["FR-003"], identifiers: true, ...over });

describe("the composer's keys", () => {
  it("maps ⏎ and its modifiers to the rungs and never anything else", () => {
    expect(rungForKey({ key: "Enter" })).toBe("selected");
    expect(rungForKey({ key: "Enter", altKey: true })).toBe("quick");
    expect(rungForKey({ key: "Enter", shiftKey: true })).toBe("feature");
    expect(rungForKey({ key: "Enter", ctrlKey: true })).toBe("bug");
    expect(rungForKey({ key: "Enter", metaKey: true })).toBe("ask");
    // ⌘⇧N, ⌘⇧B and ⌘. belong to the window: the composer binds none.
    expect(rungForKey({ key: "N", metaKey: true, shiftKey: true })).toBeNull();
    expect(rungForKey({ key: "B", metaKey: true, shiftKey: true })).toBeNull();
    expect(rungForKey({ key: ".", metaKey: true })).toBeNull();
  });

  it("moves through the available rungs only, wrapping", () => {
    expect(move(null, 1)).toBe("bug");
    expect(move("bug", 1)).toBe("quick");
    expect(move("feature", 1)).toBe("bug");
    expect(move("bug", -1)).toBe("feature");
    expect(move(null, -1)).toBe("feature");
  });

  it("shows the CLI equivalent of the selected rung, quoted", () => {
    expect(composerCommand("bug", 'say "hi"', "speckit-specify")).toBe('speccoder bug "say \\"hi\\""');
    expect(composerCommand("quick", "a\n  b", null)).toBe('speccoder do "a b"');
    expect(composerCommand("feature", "x", "openspec-propose")).toBe('speccoder spec propose "x"');
    expect(composerCommand("feature", "", "speckit-specify")).toBe('speccoder spec specify "…"');
    expect(composerCommand("ask", "x", null)).toBeNull();
    expect(quote("a\\b")).toBe('"a\\\\b"');
    expect(short("speckit-plan")).toBe("plan");
  });
});

describe("the New Feature sheet", () => {
  const planning = { creates: "speckit-specify", toward: ["speckit-plan", "speckit-tasks"], checks: "speckit-analyze" };

  it("names the skills that will run, the review between them", () => {
    expect(chainSkills(planning, "spec")).toEqual(["specify"]);
    expect(chainSkills(planning, "tasks")).toEqual(["specify", "(your review)", "plan", "tasks"]);
    expect(chainSkills(planning, "analyze")).toEqual(["specify", "(your review)", "plan", "tasks", "analyze"]);
    expect(chainSkills({ creates: "openspec-propose", toward: [], checks: null }, "tasks")).toEqual(["propose"]);
    expect(chainSkills(undefined, "tasks")).toEqual([]);
  });

  it("offers analyze only where the format checks its plan", () => {
    expect(throughChoices(true).map((c) => c.through)).toEqual(["spec", "tasks", "analyze"]);
    expect(throughChoices(false).map((c) => c.through)).toEqual(["spec", "tasks"]);
  });

  it("shows the exact commands, an issue's too", () => {
    expect(featureCommands("speckit-specify", "Export CSV", "tasks")).toEqual(['speccoder spec specify "Export CSV" --through tasks']);
    expect(featureCommands("speckit-specify", "x", "spec", 42)).toEqual(['speccoder spec specify "x" --through spec', "speccoder issue start 42 --through spec"]);
  });
});

describe("the readiness strip", () => {
  it("words every count, with attention where a person is needed", () => {
    const p = readinessParts(ready());
    expect(p.map((x) => x.text)).toEqual(["4 requirements", "9 scenarios", "1 open question", "1 requirement without a scenario (FR-003 — can't be tested)"]);
    expect(p.map((x) => x.attention)).toEqual([false, false, true, true]);
    // A format without requirement ids: no requirement count.
    expect(readinessParts(ready({ identifiers: false, untestable: [] })).map((x) => x.key)).toEqual(["scenarios", "questions"]);
  });

  it("announces only what changed", () => {
    expect(readinessChange(null, ready())).toBeNull();
    expect(readinessChange(ready(), ready())).toBeNull();
    expect(readinessChange(ready({ open_questions: 0 }), ready())).toBe("1 open question added");
    expect(readinessChange(ready(), ready({ requirements: 6, untestable: [] }))).toBe("2 requirements added, 1 untestable requirement fewer");
  });
});

describe("the chain's stop card", () => {
  it("says what the person decides, and why Approve is disabled", () => {
    expect(stopCard({ kind: "questions", questions: ["Q1", "Q2"] }, "004-x")).toMatchObject({ title: "Answer 2 questions, then review the spec", disabled: "Answer the 2 open questions first" });
    expect(stopCard({ kind: "review", requirements: ["FR-001", "FR-002", "FR-003", "FR-004"] }, "004-x").title).toBe("Review the spec — 4 requirements. Approve them to plan.");
    expect(stopCard({ kind: "failed", skill: "speckit-plan", message: "no context", code: null }, "004-x")).toMatchObject({ tone: "danger", title: "plan failed: no context", primary: "Retry this step" });
    expect(stopCard({ kind: "done", plan_checks: [] }, "004-x")).toMatchObject({ tone: "ok", primary: "Commit the planning files" });
  });

  it("approves only the wording shown", () => {
    const shown = [{ id: "FR-001", text: "Users MUST export." }];
    expect(sameWording(shown, [{ id: "FR-001", text: "Users MUST export." }, { id: "FR-002", text: "x" }])).toBe(true);
    expect(sameWording(shown, [{ id: "FR-001", text: "Users MUST export CSV." }])).toBe(false);
  });
});

describe("the New Project checklist", () => {
  const plan = {
    steps: [
      { id: "git_init", argv: ["git", "init"], cwd: "/t", access: "local", writes: ".git/", limit_s: 120 },
      { id: "scaffold.2", argv: ["uv", "add", "--dev", "pytest"], cwd: "/t", access: "network", hosts: ["pypi.org", "files.pythonhosted.org"], writes: "", limit_s: 600 },
      { id: "commit", argv: ["git", "commit", "-m", "new project: t"], cwd: "/t", access: "local", writes: "", limit_s: 120 },
    ],
  } as unknown as ProjectPlan;

  it("lists the network steps and their hosts once", () => {
    const n = networkSteps(plan);
    expect(n.steps.map((s) => s.id)).toEqual(["scaffold.2"]);
    expect(n.hosts).toEqual(["pypi.org", "files.pythonhosted.org"]);
  });

  it("gives every row its state, with a glyph and a word", () => {
    const rows = checklist(plan, [{ id: "git_init", state: "done", exit: 0, duration_ms: 40, output_tail: [] }], "scaffold.2");
    expect(rows.map((r) => r.state)).toEqual(["done", "running", "waiting"]);
    expect(stepMark("failed")).toEqual({ glyph: "✗", word: "failed" });
    expect(stepMark(null)).toEqual({ glyph: "·", word: "waiting" });
  });

  it("derives the folder from the idea as the host does", () => {
    expect(slug("a CLI that turns Markdown tables into CSV")).toBe("cli-turns-markdown-tables-csv");
    expect(slug("Ünïcödé café")).toBe("unicode-cafe");
    expect(slug("   ")).toBe("project");
  });
});

describe("pacing and debouncing", () => {
  it("shows each playback step at least 600 ms and the whole in two minutes", () => {
    expect(pace(3)).toBe(1500);
    expect(pace(150)).toBe(800);
    expect(pace(1000)).toBe(600);
    expect(pace(0)).toBe(600);
  });

  it("runs once input pauses", () => {
    vi.useFakeTimers();
    const f = vi.fn();
    const d = debounce(f, 300);
    d("a");
    d("ab");
    vi.advanceTimersByTime(299);
    expect(f).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(f).toHaveBeenCalledWith("ab");
    d("x");
    d.cancel();
    vi.advanceTimersByTime(500);
    expect(f).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});
