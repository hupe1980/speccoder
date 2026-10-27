// Where plan through stopped, as a person meets it: the readiness strip's
// words and its polite announcements, each stop's card, and Approve and
// plan — the review's own approval first, then the continuation; a
// wording that changed meanwhile is not approved.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";
import { reactive } from "../test/state.svelte";
import type { PlanningChainReport, Readiness as R } from "../lib/types";

const host = vi.hoisted(() => ({
  app: {
    feature: "004-csv-export",
    report: null,
    documents: [],
    actions: [{ group: "next", id: { kind: "commit", subject: null }, run: { kind: "commit", paths: ["specs/004-csv-export/spec.md"] }, command: "git commit", label: "commit", why: "" }],
    doc: "spec",
    section: null as string | null,
    chain: null as unknown,
    following: false,
    perform: vi.fn(async (_a: unknown) => {}),
    run: vi.fn(async (_a: unknown) => ({ status: "ok" })),
    toast: vi.fn(),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import ChainStop from "./ChainStop.svelte";
import Readiness from "./Readiness.svelte";

let c: ReturnType<typeof mount> | null = null;
const wording = (text: string) => ({ status: "ok", data: { requirements: [{ id: "FR-001", text }, { id: "FR-002", text: "The export MUST include the columns." }] } });
const chain = (stop: PlanningChainReport["stop"], over: Partial<PlanningChainReport> = {}): PlanningChainReport => ({
  unit: "004-csv-export",
  through: "tasks",
  ran: [{ skill: "speckit-specify", status: "ok", message: "done", run_id: "r1" }],
  stop,
  readiness: null,
  similar: [],
  source: { kind: "description" },
  next_skill: "speckit-plan",
  ...over,
});

function show(ch: PlanningChainReport) {
  c = mount(ChainStop, { target: document.body, props: { chain: ch } });
  flushSync();
}

beforeEach(() => {
  host.app.perform.mockClear();
  host.app.run.mockClear();
  host.get.mockReset();
  host.get.mockResolvedValue(wording("Users MUST be able to export a report as CSV."));
});

afterEach(() => {
  if (c) unmount(c);
  c = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("ChainStop", () => {
  it("at the review: approves the requirements shown, then plans on", async () => {
    show(chain({ kind: "review", requirements: ["FR-001", "FR-002"] }));
    await settle();
    expect(document.body.textContent).toContain("Review the spec — 2 requirements. Approve them to plan.");
    button(/^Approve and plan/).click();
    await settle(12);
    expect(host.app.run).toHaveBeenCalledTimes(1);
    expect((host.app.run.mock.calls[0]![0] as { run: unknown }).run).toEqual({ kind: "review", ids: ["FR-001", "FR-002"], verdict: "approve" });
    expect(host.app.perform).toHaveBeenCalledTimes(1);
    const next = host.app.perform.mock.calls[0]![0] as { run: unknown; command: string };
    expect(next.run).toEqual({ kind: "plan_through", from: { kind: "continue" }, through: "tasks" });
    expect(next.command).toBe("speccoder spec plan --through tasks --feature 004-csv-export");
    expect(await violations()).toEqual([]);
  });

  it("approves nothing when the wording changed after it was shown", async () => {
    show(chain({ kind: "review", requirements: ["FR-001", "FR-002"] }));
    await settle();
    host.get.mockResolvedValue(wording("Users MUST be able to export a report as CSV or TSV."));
    button(/^Approve and plan/).click();
    await settle(12);
    expect(host.app.run).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("The spec changed — review again");
  });

  it("with open questions: Approve waits, and says for what", async () => {
    show(chain({ kind: "questions", questions: ["Q1", "Q2"] }));
    await settle();
    expect(document.body.textContent).toContain("Answer 2 questions, then review the spec");
    expect(button(/^Approve and plan/).disabled).toBe(true);
    expect(document.body.textContent).toContain("Answer the 2 open questions first");
  });

  it("after a failure: retries only that step", async () => {
    show(chain({ kind: "failed", skill: "speckit-plan", message: "no context", code: null }));
    expect(document.querySelector('[role="alert"]')?.textContent).toContain("plan failed: no context");
    button("Retry this step").click();
    await settle();
    expect((host.app.perform.mock.calls[0]![0] as { run: unknown }).run).toEqual({ kind: "plan_through", from: { kind: "continue" }, through: "tasks" });
  });

  it("done: offers the commit and the tests, nothing committed by itself", async () => {
    show(chain({ kind: "done", plan_checks: [{ code: "PLAN-TEST-ONLY-TASKS", tasks: ["T004"], detail: "T004 only writes tests" }] }, { next_skill: null }));
    expect(document.body.textContent).toContain("1 plan check to look at");
    expect(document.body.textContent).toContain("T004 only writes tests");
    expect(host.app.perform).not.toHaveBeenCalled();
    button("Commit the planning files").click();
    await settle();
    expect((host.app.perform.mock.calls[0]![0] as { run: { kind: string } }).run.kind).toBe("commit");
  });
});

describe("Readiness", () => {
  const r = (over: Partial<R> = {}): R => ({ requirements: 4, scenarios: 9, open_questions: 0, untestable: [], identifiers: true, ...over });

  it("counts with words and glyphs, and announces changes at most every five seconds", async () => {
    vi.useFakeTimers();
    const props = reactive({ readiness: r() as R | null });
    c = mount(Readiness, { target: document.body, props });
    flushSync();
    expect(document.body.textContent).toContain("4 requirements");
    expect(document.body.textContent).toContain("9 scenarios");
    const live = () => document.querySelector('[data-testid="readiness-live"]')!.textContent;
    expect(live()).toBe("");
    // A write adds a question: announced.
    props.readiness = r({ open_questions: 1 });
    flushSync();
    expect(document.body.textContent).toContain("⚠ 1 open question");
    expect(live()).toBe("1 open question added");
    // Another within five seconds waits for them to pass.
    props.readiness = r({ open_questions: 1, untestable: ["US2"] });
    flushSync();
    expect(live()).toBe("1 open question added");
    await vi.advanceTimersByTimeAsync(5000);
    flushSync();
    expect(live()).toBe("1 untestable requirement added");
    expect(document.body.textContent).toContain("can't be tested");
    // Each count is a link: the questions to the review's cards.
    button(/open question/).click();
    expect(host.app.doc).toBe("review");
  });
});
