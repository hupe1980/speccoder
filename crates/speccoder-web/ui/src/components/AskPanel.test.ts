// Ask as a person meets it: questions that fit the view, an answer whose
// sources are numbered only where code checked them, a citation to
// something not read marked as such, a proposal as a card — and the panel
// accessible in each state.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, press, settle, violations } from "../test/dom";
import { blocks } from "../lib/ask-text";

const host = vi.hoisted(() => ({
  app: {
    report: { data: { feature: "001-graph", tasks: [{ id: "T001", title: "Parse the graph", state: "ready" }] } },
    project: { name: "graph" },
    selected: null as string | null,
    view: "board",
    job: null,
    newFeature: null as unknown,
    codeMode: "changes",
    codeFile: null as string | null,
    codeLine: null as number | null,
    codeCited: null as unknown,
    inbox: null as unknown,
    perform: vi.fn(),
    selectFeature: vi.fn(async () => {}),
    selectTask: vi.fn(),
    toast: vi.fn(),
  },
  post: vi.fn(),
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), post: host.post, get: host.get }));

import AskPanel from "./AskPanel.svelte";
import { ask } from "../lib/ask.svelte";

const report = {
  conversation: "c-1",
  turn: 1,
  question: "What is left in 001-graph?",
  chips: [],
  reads: [{ id: "r1", tool: "read_file", target: "notes.txt", path: "notes.txt", start: 1, end: 3, digest: "d", content_sha256: "abc" }],
  answer: "Parsing is left [r1:1-2]; see [r9].",
  citations: [
    { marker: "[r1:1-2]", id: "r1", lines: [1, 2], state: "read", path: "notes.txt", label: "notes.txt 1–2" },
    { marker: "[r9]", id: "r9", state: "not_read", label: "[r9]" },
  ],
  proposal: { kind: "quick_change", title: "Add the parser", fields: { request: "add a parser", checks: [], expected: [], unchanged: [] }, command: 'speccoder do "add a parser"', runnable: { kind: "quick_change", request: "add a parser", checks: [], run: true } },
  usage: { input: 900, output: 120, cache_read: 0 },
  end: "answered",
};

let shown: ReturnType<typeof mount> | null = null;
beforeEach(() => host.get.mockResolvedValue({}));
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  ask.fresh();
  ask.recent = [];
  host.post.mockReset();
  host.get.mockReset();
  host.app.selected = null;
  host.app.inbox = null;
});

describe("AskPanel", () => {
  it("offers questions that fit the view, and is accessible empty", async () => {
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Ask about 001-graph");
    expect(text).toContain("What is left in 001-graph?");
    expect(document.querySelector('aside[aria-label="Ask"]')).not.toBeNull();
    expect(document.querySelector(".chip.view")?.textContent).toContain("001-graph");
    expect(await violations()).toEqual([]);
  });

  it("numbers only checked sources, marks the unread one, shows the proposal", async () => {
    host.post.mockResolvedValue({ status: "ok", message: "answered with 1 sources", data: report });
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    flushSync();
    button(/What is left in 001-graph/).click();
    await settle();
    expect(host.post).toHaveBeenCalledWith("/api/ask", expect.objectContaining({ text: "What is left in 001-graph?", conversation: null }));
    const cite = document.querySelector<HTMLButtonElement>('button[aria-label="Source 1: notes.txt 1–2"]');
    expect(cite?.textContent).toBe("1");
    expect(document.querySelector(".cite.unread")).not.toBeNull();
    expect(document.body.textContent).toContain("not read this turn");
    expect(document.body.textContent).toContain("Add the parser");
    // A source opens where it lives.
    cite!.click();
    expect([host.app.view, host.app.codeFile, host.app.codeLine]).toEqual(["code", "notes.txt", 1]);
    // Its lines are marked, and whether the file still says that is asked
    // of the same read, over the whole range it returned.
    expect(host.app.codeCited).toEqual({ path: "notes.txt", from: 1, to: 2, changed: null });
    expect(host.get).toHaveBeenCalledWith("/api/ask/changed?path=notes.txt&start=1&end=3&sha=abc");
    expect(await violations()).toEqual([]);
  });

  it("stops a turn in flight at once: the words stay, the next question waits for the last call", async () => {
    const answers: ((v: unknown) => void)[] = [];
    host.post.mockImplementation((path: string) =>
      path === "/api/ask" ? new Promise((r) => answers.push(r)) : Promise.resolve({ stopped: true }),
    );
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    flushSync();
    button(/What is left in 001-graph/).click();
    flushSync();
    ask.event({ event: "ask_text", delta: "Parsing is" });
    flushSync();
    expect(document.querySelector('button[aria-label="Send"]')).toBeNull();
    expect(await violations()).toEqual([]);
    press("Escape", {}, document.querySelector("textarea")!);
    flushSync();
    // A new conversation's id is chosen before its first question, so Stop
    // names exactly that turn.
    const asked = host.post.mock.calls.find((c) => c[0] === "/api/ask")![1] as { id: string };
    expect(asked.id).toMatch(/^c-[0-9a-f]+$/);
    expect(host.post).toHaveBeenCalledWith("/api/ask/stop", { conversation: asked.id });
    expect(ask.open).toBe(true);
    // Stopped at once: no waiting line, Send is back, the words stay.
    expect(document.body.textContent).toContain("Stopped — the words so far");
    expect(document.querySelector('button[aria-label="Stop"]')).toBeNull();
    expect(document.querySelector('button[aria-label="Send"]')).not.toBeNull();
    // Words after the stop are not shown.
    ask.event({ event: "ask_text", delta: " more" });
    flushSync();
    expect(document.body.textContent).not.toContain("more");
    // A question asked now waits for the stop to land, then goes.
    const box = document.querySelector("textarea")!;
    box.value = "and the tests?";
    box.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    press("Enter", {}, box);
    flushSync();
    expect(ask.queued).toBe("and the tests?");
    expect(answers).toHaveLength(1);
    expect(await violations()).toEqual([]);
    answers[0]!({ status: "ok", message: "stopped", data: { ...report, answer: "Parsing is", citations: [], proposal: null, end: "stopped", reason: "stopped by you — ask again to go on" } });
    await settle();
    expect(document.body.textContent).toContain("Parsing is");
    expect(document.body.textContent).toContain("Stopped — the words so far");
    // The conversation goes on: no dead end, the queued question is asked.
    expect(ask.conversation).toBe("c-1");
    expect(answers).toHaveLength(2);
    expect(ask.pending?.question).toBe("and the tests?");
    expect(ask.queued).toBeNull();
    answers[1]!({ status: "error", message: "done" });
    await settle();
  });

  it("closes on Escape outside the question field", () => {
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    flushSync();
    press("Escape", {}, document.querySelector("header button")!);
    expect(ask.open).toBe(false);
  });
});

describe("answer text", () => {
  it("is blocks of parts, never HTML", () => {
    const b = blocks("A **b** `c` [r1]\n\n- one\n```\n<x>\n```", [{ marker: "[r1]", id: "r1", state: "read", label: "x" }]);
    expect(b.map((x) => x.kind)).toEqual(["p", "li", "pre"]);
    expect(b[0]).toMatchObject({ parts: [{ t: "text", text: "A " }, { t: "bold", text: "b" }, { t: "text", text: " " }, { t: "code", text: "c" }, { t: "text", text: " " }, { t: "cite", n: 1 }] });
    expect(b[2]).toEqual({ kind: "pre", text: "<x>" });
  });

  it("picks up where you left off: the three most recent conversations", async () => {
    const conv = (n: number) => ({ id: `c-${n}`, title: `Question ${n}`, at: 1_790_000_000 + n, turns: n, open: true });
    host.get.mockImplementation(async (path: string) => {
      if (path.startsWith("/api/ask/conversations")) return { data: { conversations: [conv(4), conv(3), conv(2), conv(1)] } };
      if (path.startsWith("/api/ask/conversation")) return { data: { conversation: { id: "c-3" }, turns: [{ ...report, conversation: "c-3", question: "Question 3" }] } };
      throw new Error(path);
    });
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    await settle();
    const rows = [...document.querySelectorAll(".recent .conv")].map((b) => b.querySelector(".ct")?.textContent);
    expect(rows).toEqual(["Question 4", "Question 3", "Question 2"]);
    expect(await violations()).toEqual([]);
    document.querySelectorAll<HTMLButtonElement>(".recent .conv")[1]!.click();
    await settle();
    expect(ask.conversation).toBe("c-3");
  });

  it("offers promotion for a task in context that grew; Start a spec carries this conversation", async () => {
    const start = { label: "Start a spec from T001", command: "speccoder spec promote 001-graph/T001", run: { kind: "plan_through", from: { kind: "create", description: "Parse the graph", source: { kind: "promotion", task: "001-graph/T001", conversations: ["c-0"] } }, through: "tasks" } };
    const later = { label: "Not now", command: "speccoder spec promote 001-graph/T001 --decline", run: { kind: "decline_promotion", task: "001-graph/T001" } };
    host.app.selected = "T001";
    host.app.inbox = { data: { items: [{ kind: "promote", group: "ready_to_review", feature: "001-graph", task: "T001", title: "T001 grew past a quick change", detail: "11 files changed (more than 8)", since: null, actions: [start, later] }] } };
    ask.conversation = "c-1";
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    flushSync();
    expect(document.querySelector(".offer")?.textContent).toContain("11 files changed (more than 8)");
    expect(await violations()).toEqual([]);
    button(/^Start a spec$/).click();
    expect(host.app.perform).toHaveBeenLastCalledWith(expect.objectContaining({
      run: { ...start.run, from: { ...start.run.from, source: { ...start.run.from.source, conversations: ["c-1", "c-0"] } } },
      command: "speccoder spec promote 001-graph/T001 --conversation c-1 --conversation c-0",
    }));
    button(/^Not now$/).click();
    expect(host.app.perform).toHaveBeenLastCalledWith(later);
  });

  it("shows under its turn what a confirmed proposal created, and opens it", async () => {
    host.get.mockImplementation(async (path: string) => {
      if (path.startsWith("/api/ask/conversations")) return { data: { conversations: [] } };
      return { data: { conversation: { id: "c-1" }, turns: [report], results: [{ turn: 1, kind: "change", unit: "002-quick-changes", requirement: "FR-001", task: "T001" }, { kind: "spec", unit: "003-sign-in-log" }] } };
    });
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    await ask.resume("c-1");
    await settle();
    const lines = [...document.querySelectorAll(".made")].map((b) => b.textContent?.replace(/\s+/g, " ").trim());
    expect(lines).toEqual(["Recorded T001 in 002-quick-changes Open", "Started 003-sign-in-log from this conversation Open"]);
    expect(await violations()).toEqual([]);
    document.querySelector<HTMLButtonElement>(".made")!.click();
    await settle();
    expect(host.app.selectFeature).toHaveBeenCalledWith("002-quick-changes");
    expect(host.app.selectTask).toHaveBeenCalledWith("T001");
  });

  it("opens a run's account at the turns a citation names, in place", async () => {
    const turn = { ...report, reads: [{ id: "r2", tool: "journal_account", target: "account of run r-7", start: 2, end: 3, digest: "d", subject: "r-7" }], answer: "It retried twice [r2].", citations: [{ marker: "[r2]", id: "r2", state: "read", label: "account of run r-7" }], proposal: null };
    host.post.mockResolvedValue({ status: "ok", message: "answered", data: turn });
    host.get.mockImplementation(async (path: string) => (path.startsWith("/api/account") ? { turns: [{ turn: 1, reasoning: null, said: "a" }, { turn: 2, reasoning: null, said: "b" }, { turn: 3, reasoning: null, said: "c" }] } : {}));
    ask.toggle(true);
    shown = mount(AskPanel, { target: document.body });
    flushSync();
    button(/What is left in 001-graph/).click();
    await settle();
    document.querySelector<HTMLButtonElement>("button.cite")!.click();
    await settle();
    expect(host.get).toHaveBeenCalledWith("/api/account?run=r-7");
    expect(document.querySelector('[role="dialog"][aria-label="Account of run r-7"]')).not.toBeNull();
    expect(ask.peek?.marked).toEqual([2, 3]);
    expect(await violations()).toEqual([]);
    ask.peek = null;
  });
});
