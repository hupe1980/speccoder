// Before you run: plan checks as rows — the catalog title, the tasks as
// chips that open them, the remedy as one button; warnings never make it
// "Fix before running".

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    view: "board",
    preflight: [] as unknown[],
    blockers: [] as unknown[],
    perform: vi.fn(async (_a: unknown) => {}),
    selectTask: vi.fn(async (_t: string) => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import Preflight from "./Preflight.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

describe("Preflight", () => {
  it("shows each plan check with its tasks and its remedy", () => {
    host.app.preflight = [
      { check: "plan", level: "warn", code: "PLAN-BEHAVIOUR-EARLY", tasks: ["T003"], detail: "T003 (Foundational) names `greet`, which US1 is about", fix: [],
        remedy: { label: "Write US1's acceptance at T003", effect: "T003's tick can be verified on US1's scenarios", command: "speccoder task acceptance-at T003 --story US1", undo: null, history: false, run: { kind: "acceptance_at", feature: "001", story: "US1", task: "T003", revoke: false } } },
      { check: "plan", level: "warn", code: "PLAN-TEST-ONLY-TASKS", tasks: ["T005", "T006"], detail: "T005, T006 only write tests", fix: ["speccoder spec tasks  (re-plan with the spec tool)"] },
    ];
    shown = mount(Preflight, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Worth a look");
    expect(text).toContain("A story's behaviour is planned before the story");
    expect(text).toContain("Tasks that only write tests");
    const chip = document.querySelector('button[aria-label="Open T003"]') as HTMLButtonElement;
    chip.click();
    flushSync();
    expect(host.app.selectTask).toHaveBeenCalledWith("T003");
    const remedy = [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("Write US1's acceptance at T003"))!;
    expect(remedy.title).toContain("$ speccoder task acceptance-at T003 --story US1");
    expect(document.querySelector('section[aria-label="Before you run"]')).not.toBeNull();
  });

  it("says who does what as information, not a warning", () => {
    host.app.preflight = [
      { check: "lanes", level: "pass", detail: "25 tasks · 23 code · 1 📐 planning (T002) · 1 🙋 yours (T019)", fix: [], tasks: ["T002", "T019"] },
    ];
    host.app.blockers = [];
    shown = mount(Preflight, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Who does what");
    expect(text).toContain("1 📐 planning (T002)");
    expect(text).not.toContain("Fix before running");
    expect(document.querySelector("li.pass")).not.toBeNull();
    expect(document.querySelector('button[aria-label="Open T019"]')).not.toBeNull();
  });
});
