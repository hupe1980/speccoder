// A change Ask suggested, as a person meets it: their words to edit, code
// checking them again as they type, what will be recorded in plain sight,
// and nothing performed until Confirm.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, press, settle, type, violations } from "../test/dom";
import type { Proposal } from "../lib/schema.gen";

const host = vi.hoisted(() => ({
  app: {
    report: { data: { feature: "001-graph", tasks: [] } },
    project: { name: "graph" },
    selected: null,
    view: "board",
    job: null as { running: boolean } | null,
    perform: vi.fn(),
    toast: vi.fn(),
  },
  post: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), post: host.post }));

import ProposalSheet from "./ProposalSheet.svelte";
import { ask } from "../lib/ask.svelte";

const quick: Proposal = {
  kind: "quick_change",
  title: "Trim the login name",
  fields: { request: "trim the login name", checks: ["a trailing space is ignored"], expected: [], unchanged: [] },
  command: 'speccoder do "trim the login name" --check "a trailing space is ignored"',
  runnable: { kind: "quick_change", request: "trim the login name", checks: ["a trailing space is ignored"], run: true },
};

// The server's check of the edited words: the command follows the request.
function checks() {
  host.post.mockImplementation(async (path: string, body: { proposal?: Proposal }) => {
    if (path !== "/api/ask/proposal") return {};
    const p = body.proposal!;
    if (p.kind === "quick_change") {
      const r = p.fields.request ?? "";
      return r ? { proposal: { ...p, blocked: null, command: `speccoder do "${r}"`, runnable: { kind: "quick_change", request: r, checks: p.fields.checks, run: true } } } : { proposal: { ...p, runnable: null, blocked: "a quick change needs its request" } };
    }
    return { proposal: p };
  });
}

let shown: ReturnType<typeof mount> | null = null;
function open(p: Proposal) {
  ask.sheet = { proposal: p, turn: 1 };
  shown = mount(ProposalSheet, { target: document.body });
  flushSync();
}
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  ask.sheet = null;
  host.post.mockReset();
  host.app.perform.mockReset();
  host.app.job = null;
  vi.useRealTimers();
});

describe("ProposalSheet", () => {
  it("shows a quick change as the person's words and what will be recorded", async () => {
    checks();
    open(quick);
    expect(document.querySelector("h2")?.textContent).toBe("Quick change");
    expect(document.querySelector("textarea")?.value).toBe("trim the login name");
    expect(document.querySelector<HTMLInputElement>('input[aria-label="Done when, 1"]')?.value).toBe("a trailing space is ignored");
    expect(document.querySelector(".record code")?.textContent).toContain('speccoder do "trim the login name"');
    expect(button("Record and Run").disabled).toBe(false);
    expect(await violations()).toEqual([]);
  });

  it("checks the edited words again, and an emptied request blocks Confirm", async () => {
    vi.useFakeTimers();
    checks();
    open(quick);
    type(document.querySelector("textarea")!, "trim and lowercase the login name");
    await vi.advanceTimersByTimeAsync(300);
    await settle();
    expect(host.post).toHaveBeenLastCalledWith("/api/ask/proposal", expect.objectContaining({ feature: "001-graph", proposal: expect.objectContaining({ fields: expect.objectContaining({ request: "trim and lowercase the login name" }) }) }));
    expect(document.querySelector(".record code")?.textContent).toBe('speccoder do "trim and lowercase the login name"');

    type(document.querySelector("textarea")!, "");
    await vi.advanceTimersByTimeAsync(300);
    await settle();
    expect(document.querySelector(".record .blocked")?.textContent).toContain("needs its request");
    expect(button("Record and Run").disabled).toBe(true);
    press("Enter", { metaKey: true });
    expect(host.app.perform).not.toHaveBeenCalled();
  });

  it("performs only on Confirm, with the person's words", async () => {
    checks();
    open({ kind: "nudge", title: "Use the fixture", fields: { target: "T005", note: "use the fixture", checks: [], expected: [], unchanged: [] }, command: 'speccoder task nudge T005 --note "use the fixture"', runnable: { kind: "nudge", task: "T005" }, input: "use the fixture" });
    expect(document.querySelector("h2")?.textContent).toBe("Nudge · T005");
    expect(host.app.perform).not.toHaveBeenCalled();
    press("Enter", { metaKey: true });
    await settle();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "nudge", task: "T005" }, why: "suggested in Ask, confirmed by you" }), "use the fixture");
    expect(ask.sheet).toBeNull();
  });

  it("lists a bug's expectations to edit, and Escape records nothing", async () => {
    checks();
    open({ kind: "bug_fix", title: "Login fails", fields: { symptom: "login fails on a trailing space", checks: [], expected: ["it is trimmed"], unchanged: ["passwords keep spaces"] }, command: "speccoder bug …", runnable: null, blocked: null });
    expect(document.querySelector<HTMLInputElement>('input[aria-label="What should happen instead, 1"]')?.value).toBe("it is trimmed");
    expect(document.querySelector<HTMLInputElement>('input[aria-label="What must keep working, 1"]')?.value).toBe("passwords keep spaces");
    // The next empty row invites another.
    expect(document.querySelector('input[aria-label="What should happen instead, 2"]')).not.toBeNull();
    expect(await violations()).toEqual([]);
    press("Escape");
    expect(ask.sheet).toBeNull();
    expect(host.app.perform).not.toHaveBeenCalled();
  });

  it("does not confirm while a run holds the project", () => {
    checks();
    host.app.job = { running: true };
    open(quick);
    expect(button("Record and Run").disabled).toBe(true);
  });
});
