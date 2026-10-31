// Ask as a person meets it: questions that fit the view, an answer whose
// sources are numbered only where code checked them, a citation to
// something not read marked as such, a proposal as a card — and the panel
// accessible in each state.

import { afterEach, describe, expect, it, vi } from "vitest";
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
  reads: [{ id: "r1", tool: "read_file", target: "notes.txt", path: "notes.txt", start: 1, end: 3, digest: "d" }],
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
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  ask.fresh();
  host.post.mockReset();
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
    expect(await violations()).toEqual([]);
  });

  it("stops a turn in flight: Send becomes Stop, Escape stops first, the words stay", async () => {
    let answer: (v: unknown) => void = () => {};
    host.post.mockImplementation((path: string) =>
      path === "/api/ask" ? new Promise((r) => (answer = r)) : Promise.resolve({ stopped: true }),
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
    expect(host.post).toHaveBeenCalledWith("/api/ask/stop", { conversation: null });
    expect(ask.open).toBe(true);
    expect(document.body.textContent).toContain("Stopping after this step");
    expect(document.querySelector<HTMLButtonElement>('button[aria-label="Stop"]')?.disabled).toBe(true);
    // Words after the stop are not shown.
    ask.event({ event: "ask_text", delta: " more" });
    answer({ status: "ok", message: "stopped", data: { ...report, answer: "Parsing is", citations: [], proposal: null, end: "stopped", reason: "stopped by you — ask again to go on" } });
    await settle();
    expect(document.body.textContent).toContain("Parsing is");
    expect(document.body.textContent).toContain("Stopped — the words so far");
    expect(document.body.textContent).not.toContain("more");
    // The conversation goes on: no dead end, Send is back.
    expect(document.querySelector('button[aria-label="Send"]')).not.toBeNull();
    expect(ask.conversation).toBe("c-1");
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
});
