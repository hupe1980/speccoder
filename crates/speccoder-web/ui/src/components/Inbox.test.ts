// Needs You's diagnosis of a mostly unverified feature: the reasons by
// count, each task a button, and the actions with their commands.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    feature: "001-greet-function",
    view: "inbox",
    inbox: null as unknown,
    primary: null,
    quick: false,
    perform: vi.fn(async (_a: unknown) => {}),
    selectFeature: vi.fn(async (_f: string) => {}),
    selectTask: vi.fn(async (_t: string) => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import Inbox from "./Inbox.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

const action = (label: string, command: string, kind: string) => ({
  id: { kind, subject: null },
  group: "next",
  label,
  why: "w",
  command,
  blocked: null,
  input: null,
  run: null,
});

describe("Inbox", () => {
  it("lists the reasons of an unverified feature with their tasks as buttons", () => {
    host.app.inbox = {
      data: {
        features: 1,
        items: [
          {
            kind: "unverified",
            group: "ready_to_review",
            feature: "001-greet-function",
            task: null,
            title: "Little was verified in `001-greet-function`: 14 of 14 ticks are regression-only",
            detail: "",
            since: null,
            actions: [action("show the plan checks", "speccoder doctor", "doctor"), action("converge", "speccoder converge", "converge"), action("open the report", "speccoder report", "report")],
            unverified: {
              ticks: 14,
              regression_only: 14,
              counts: [
                { reason: "no_story", n: 7, tasks: ["T001", "T002"], text: "7 had no user story" },
                { reason: "already_passing", n: 7, tasks: ["T005"], implemented_by: ["T003"], text: "7 were already passing — implemented by T003" },
              ],
            },
          },
        ],
      },
    };
    shown = mount(Inbox, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Little Was Verified");
    expect(text).toContain("7 had no user story");
    expect(text).toContain("7 were already passing — implemented by T003");
    const chip = document.querySelector('button[aria-label="Open T005"]') as HTMLButtonElement;
    expect(chip).not.toBeNull();
    chip.click();
    flushSync();
    expect(host.app.selectTask).toHaveBeenCalledWith("T005");
    const titles = [...document.querySelectorAll("button")].map((b) => b.getAttribute("title") ?? "");
    expect(titles.some((t) => t.includes("$ speccoder converge"))).toBe(true);
    expect(document.querySelectorAll("ul.reasons li")).toHaveLength(2);
  });

  it("shows a stop as a card: the agent's claim and its three actions", () => {
    const act = (kind: string, input: string | null) => ({ ...action(kind, `speccoder task ${kind} T019`, kind), id: { kind, subject: "T019" }, group: "task", input, run: { kind, task: "T019" } });
    host.app.inbox = {
      data: {
        features: 1,
        items: [
          { kind: "yours", group: "blocked_on_you", feature: "001-greet-function", task: "T019", title: "T019 needs you: the agent stopped", detail: "needs a GitHub repository", since: null, actions: [act("done", null), act("hint", "The hint"), act("skip", "Why it is skipped")], unverified: null },
        ],
      },
    };
    shown = mount(Inbox, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Yours to Do");
    expect(text).toContain("“needs a GitHub repository”");
    const done = [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("I Did It")) as HTMLButtonElement;
    done.click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "done", task: "T019" } }));
  });

  const item = (kind: string, group: string, feature: string, extra: Record<string, unknown> = {}) => ({
    kind, group, feature, task: null, title: `${kind} in ${feature}`, detail: "", since: null, actions: [], unverified: null, ...extra,
  });
  const approval = (task: string, tool: string) => {
    const act = (kind: string, input: string | null) => ({ ...action(kind, `speccoder task ${kind} ${task}`, kind), id: { kind, subject: task }, input, run: { kind, task } });
    return item("approval", "blocked_on_you", "001-x", { task, tool, title: `${task} waits for your approval`, actions: [act("approve", null), act("reject", "Why (the model is told)")] });
  };

  it("puts what blocks work first, the riskiest review next, then working and idle", () => {
    host.app.inbox = {
      data: {
        features: 3,
        groups: [],
        working: [{ feature: "002-run", task: "T003", started: Math.floor(Date.now() / 1000) - 600, pid: 7, state: "running" }],
        idle: ["003-quiet"],
        items: [
          item("question", "blocked_on_you", "001-x"),
          item("brand_new_kind", "blocked_on_you", "001-x"),
          item("unverified", "ready_to_review", "001-x", { risk: [{ reason: "regression_only", count: 3, text: "3 regression-only" }, { reason: "attested", count: 1, text: "1 attested" }] }),
          item("handoff", "ready_to_review", "004-strong"),
        ],
      },
    };
    shown = mount(Inbox, { target: document.body });
    flushSync();
    const headings = [...document.querySelectorAll("h2 .gtitle")].map((h) => h.textContent);
    expect(headings).toEqual(["Blocked on you", "Ready to review"]);
    const text = document.body.textContent ?? "";
    // A kind is a row's label; one the page does not know keeps its name.
    expect(text).toContain("Open Questions");
    expect(text).toContain("brand new kind");
    expect([...document.querySelectorAll(".risk")].map((r) => r.textContent?.trim())).toEqual(["3 regression-only", "1 attested", "verified"]);
    expect(text).toContain("T003");
    expect(text).toContain("working · 10 min");
    const idle = [...document.querySelectorAll(".idle button")].map((b) => b.textContent);
    expect(idle).toEqual(["quiet"]);
    expect(document.querySelector(".sub")?.textContent?.replace(/\s+/g, " ").trim()).toBe("2 blocked on you · 2 ready to review · 1 working");
  });

  it("decides several approvals of one tool, each on its own", async () => {
    host.app.perform.mockClear();
    host.app.inbox = { data: { features: 1, groups: [], working: [], idle: [], items: [approval("T1", "run_command"), approval("T2", "run_command"), approval("T3", "write_file")] } };
    shown = mount(Inbox, { target: document.body });
    flushSync();
    const pick = (t: string) => (document.querySelector(`input[aria-label="Select ${t}"]`) as HTMLInputElement).click();
    pick("T1");
    pick("T3");
    flushSync();
    expect(document.querySelector(".batch")?.textContent).toContain("One kind of call at a time");
    pick("T3");
    pick("T2");
    flushSync();
    const approve = [...document.querySelectorAll(".batch button")].find((b) => b.textContent === "Approve 2") as HTMLButtonElement;
    approve.click();
    await vi.waitFor(() => expect(host.app.perform).toHaveBeenCalledTimes(2));
    expect(host.app.perform).toHaveBeenNthCalledWith(1, expect.objectContaining({ run: { kind: "approve", task: "T1" } }), undefined);
    expect(host.app.perform).toHaveBeenNthCalledWith(2, expect.objectContaining({ run: { kind: "approve", task: "T2" } }), undefined);
  });
});
