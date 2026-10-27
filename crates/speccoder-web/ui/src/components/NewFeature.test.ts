// The New Feature sheet as a person meets it: what will run and what it
// writes, an empty description refused, the states that stop Start, the
// plan through it performs with the description verbatim, an issue as the
// source until the text is edited, and the similarity hint's Extend.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, press, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    newFeature: {} as Record<string, unknown> | null,
    project: {
      format: "Spec Kit",
      unit: "feature",
      planning: { creates: "speckit-specify", toward: ["speckit-plan", "speckit-tasks"], checks: "speckit-analyze", revises: "speckit-clarify" },
      features: { current: "003-reports", features: [{ name: "001-graph", number: 1 }, { name: "003-reports", number: 3 }] },
    } as Record<string, unknown>,
    doctor: [{ check: "model key", level: "pass", detail: "openai gpt" }] as { check: string; level: string; detail: string }[],
    job: null as { running: boolean; command: string } | null,
    following: false,
    view: "overview",
    doc: "spec",
    feature: "003-reports",
    perform: vi.fn(async (_a: unknown) => {}),
    run: vi.fn(async (_a: unknown, _input?: string) => ({ status: "ok" })),
    selectFeature: vi.fn(async (_n: string) => {}),
    toast: vi.fn(),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import NewFeature from "./NewFeature.svelte";

let s: ReturnType<typeof mount> | null = null;
const field = () => document.querySelector<HTMLTextAreaElement>("textarea")!;
const match = { unit: "003-reports", requirement: "FR-004", text: "Users can export a report as PDF.", score: 0.48, level: "close", shared: ["export", "report"], at: { path: "specs/003-reports/spec.md", line: 14, quote: "- **FR-004**: Users can export a report as PDF." } };

function routes(matches: unknown[] = [], limited = false) {
  host.get.mockImplementation(async (path: string) => {
    if (path.startsWith("/api/similar")) return { status: "ok", data: { query: "", compared: 4, limited, matches } };
    if (path.startsWith("/api/skill-cost")) return { cost: null, model: "gpt-5.4-mini" };
    throw new Error(path);
  });
}

function open(state: Record<string, unknown> = {}) {
  host.app.newFeature = state;
  s = mount(NewFeature, { target: document.body });
  flushSync();
}

async function wait() {
  await vi.advanceTimersByTimeAsync(350);
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  host.app.perform.mockClear();
  host.app.run.mockClear();
  host.app.selectFeature.mockClear();
  host.app.job = null;
  host.app.doctor = [{ check: "model key", level: "pass", detail: "openai gpt" }];
  host.app.following = false;
  routes();
});

afterEach(() => {
  if (s) unmount(s);
  s = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("NewFeature", () => {
  it("says which skills run, what they write, what it costs, and the exact command", async () => {
    open({ text: "Users can export a report as CSV" });
    await wait();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Spec Kit specify → (your review) → plan → tasks");
    expect(text).toContain("specs/004-…/");
    expect(text).toContain("planning files are left for you to commit");
    expect(text).toContain("cost: unknown until it runs — gpt-5.4-mini");
    expect(text).toContain('speccoder spec specify "Users can export a report as CSV" --through tasks');
    const d = document.querySelector('[role="dialog"]')!;
    expect(d.getAttribute("aria-modal")).toBe("true");
    expect(d.getAttribute("aria-labelledby")).toBe("nf-title");
    expect(await violations()).toEqual([]);
  });

  it("refuses an empty description and runs nothing", async () => {
    open({});
    button(/^Start/).click();
    await settle();
    expect(host.app.perform).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("Describe what it should do");
    expect(field().getAttribute("aria-invalid")).toBe("true");
  });

  it("starts plan through with the description verbatim and how far, with ⌘⏎", async () => {
    open({ text: "Users can export a report as CSV" });
    const spec = document.querySelector<HTMLInputElement>('input[value="spec"]')!;
    spec.click();
    flushSync();
    press("Enter", { metaKey: true }, field());
    await settle();
    expect(host.app.perform).toHaveBeenCalledTimes(1);
    const a = host.app.perform.mock.calls[0]![0] as { run: unknown; command: string };
    expect(a.run).toEqual({ kind: "plan_through", from: { kind: "create", description: "Users can export a report as CSV", source: { kind: "description" } }, through: "spec" });
    expect(a.command).toBe('speccoder spec specify "Users can export a report as CSV" --through spec');
    expect(host.app.following).toBe(true);
    expect(host.app.view).toBe("documents");
    expect(host.app.newFeature).toBeNull();
  });

  it("disables Start without a model key, while a job runs, and where no skill creates a unit", async () => {
    host.app.doctor = [{ check: "model key", level: "fail", detail: "OPENAI_API_KEY is not set" }];
    open({ text: "x y z" });
    expect(button(/^Start/).disabled).toBe(true);
    expect(document.body.textContent).toContain("No model key: set OPENAI_API_KEY where you start speccoder");
    expect(button("Settings › Model")).toBeTruthy();
    unmount(s!);
    host.app.doctor = [];
    host.app.job = { running: true, command: "run" };
    open({ text: "x y z" });
    expect(document.body.textContent).toContain("A job is running — run");
    unmount(s!);
    host.app.job = null;
    const planning = host.app.project.planning;
    host.app.project.planning = { creates: null, toward: [], checks: null, revises: null };
    open({ text: "x" });
    expect(document.body.textContent).toContain("Spec Kit has no skill that creates a feature");
    expect(button(/^Start/).disabled).toBe(true);
    host.app.project.planning = planning;
  });

  it("names the issue as the source until its text is edited", async () => {
    open({ text: "Export as CSV\n\nFrom #42", source: { kind: "issue", number: 42, title: "CSV export" } });
    expect(document.body.textContent).toContain("issue #42 “CSV export”");
    expect(document.querySelector<HTMLInputElement>('input[value="issue"]')!.checked).toBe(true);
    button(/^Start/).click();
    await settle();
    const a = host.app.perform.mock.calls[0]![0] as { run: { from: { source: unknown; description: string } } };
    expect(a.run.from.source).toEqual({ kind: "issue", number: 42 });
    expect(a.run.from.description).toBe("Export as CSV\n\nFrom #42");
    unmount(s!);
    host.app.perform.mockClear();
    open({ text: "Export as CSV", source: { kind: "issue", number: 42, title: "CSV export" } });
    type(field(), "Export as CSV and TSV");
    expect(document.querySelector<HTMLInputElement>('input[value="description"]')!.checked).toBe(true);
    button(/^Start/).click();
    await settle();
    const b = host.app.perform.mock.calls[0]![0] as { run: { from: { source: unknown } } };
    expect(b.run.from.source).toEqual({ kind: "description" });
  });

  it("shows a close requirement and extends that feature with a comment draft instead", async () => {
    routes([match]);
    open({ text: "Users can export a report as CSV" });
    await wait();
    expect(document.querySelector('[role="status"]')?.textContent).toContain("003-reports FR-004");
    expect(document.body.textContent).toContain("shares: export, report");
    button("Extend 003 instead").click();
    await settle(12);
    expect(host.app.run).toHaveBeenCalledTimes(2);
    expect((host.app.run.mock.calls[0]![0] as { run: unknown }).run).toEqual({ kind: "feature" });
    expect(host.app.run.mock.calls[0]![1]).toBe("003-reports");
    const comment = host.app.run.mock.calls[1]![0] as { run: { kind: string; op: string; anchor: { path: string; start_line: number } } };
    expect(comment.run).toMatchObject({ kind: "comment", op: "add", anchor: { kind: "passage", path: "specs/003-reports/spec.md", start_line: 14, end_line: 14 } });
    expect(host.app.run.mock.calls[1]![1]).toBe("Users can export a report as CSV");
    // Nothing else ran: no creating skill, no revision sent.
    expect(host.app.perform).not.toHaveBeenCalled();
  });

  it("says when the comparison was limited to the most recent features", async () => {
    routes([match], true);
    open({ text: "Users can export a report as CSV" });
    await wait();
    expect(document.body.textContent).toContain("compared with the 2 000 most recently changed features");
  });

  it("says what these skills cost before, from the runs on record", async () => {
    host.get.mockImplementation(async (path: string) => {
      if (path.startsWith("/api/similar")) return { status: "ok", data: { query: "", compared: 0, limited: false, matches: [] } };
      if (path.startsWith("/api/skill-cost")) return { cost: { tokens: 42000, calls: 7, usd: 0.31, runs: 2 }, model: "m" };
      throw new Error(path);
    });
    open({ text: "Users can export a report as CSV" });
    await wait();
    expect(document.body.textContent).toContain("about 7 model calls · 42k tokens · ~$0.31 (from your last 2 runs of these skills)");
  });

  it("shows no hint when nothing is close", async () => {
    routes([]);
    open({ text: "dark mode for the settings page" });
    await wait();
    expect(document.body.textContent).not.toContain("Close to");
  });
});
