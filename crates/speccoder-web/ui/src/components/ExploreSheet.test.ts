// Explore as a person meets it: an idea, approaches as cards picked with
// a number key, what they keep and add, the brief with its digest, and the
// spec started from it in the New feature sheet — accessible in each state.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, press, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    explore: null as { id?: string | null; idea?: string } | null,
    newFeature: null as unknown,
    job: null as { id: string; command: string; running: boolean; result?: unknown } | null,
    run: vi.fn(async (_a?: unknown): Promise<unknown> => null),
    toast: vi.fn(),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", async () => ({ app: (await import("../test/reactive.svelte")).reactive(host.app) }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import ExploreSheet from "./ExploreSheet.svelte";
import { app } from "../lib/app.svelte";

const approach = (t: string) => ({ title: t, summary: `${t}: how`, tradeoffs: [`${t} costs`], risks: [`${t} risks`] });
const session = { version: 1, id: "ex-abc", idea: "reports people can take with them", skill: "speccoder.explore", selected_because: "fallback", state: "open", created_at: 1, updated_at: 2, runs: [], reports: [], qa: [], choices: [] };
const explored = {
  session,
  approaches: [approach("Stream CSV"), approach("Build a PDF"), approach("A fresh link")],
  constraints: [{ text: "under a second", source: "model" }],
  open_questions: [{ text: "which columns?", source: "nobody" }],
};
const chosen = {
  ...explored,
  session: { ...session, state: "chosen" },
  choice: { approach: 2, constraints: [{ text: "under a second", source: "model" }, { text: "no new dependency", source: "person" }], open_questions: [{ text: "which columns?", source: "nobody" }], by: "Ada", via: "web", basis: "prompt", at: 3, digest: "3f9a0c1d2e4b5a6b7c8d", bytes: 200 },
  brief: "Explore brief 3f9a0c1d2e4b (session ex-abc)\n\n## Approach\n\nBuild a PDF\n",
};

let shown: ReturnType<typeof mount> | null = null;
function open(state: { id?: string; idea?: string } = {}) {
  app.explore = state;
  shown = mount(ExploreSheet, { target: document.body });
  flushSync();
}
beforeEach(() => host.get.mockImplementation(async () => ({ data: { sessions: [] } })));
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  app.job = null;
  app.newFeature = null;
  host.app.run.mockReset();
});

describe("ExploreSheet", () => {
  it("explores an idea, picks a card by its number, and makes the brief from what was kept", async () => {
    host.app.run.mockResolvedValueOnce({ status: "needs_human", message: "3 approaches", data: explored });
    open({ idea: "reports people can take with them" });
    expect(await violations()).toEqual([]);
    press("Enter", { metaKey: true });
    await settle();
    expect(host.app.run).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "explore", idea: "reports people can take with them", skill: null, resume: null } }));
    await settle();
    const cards = [...document.querySelectorAll('[role="radio"]')];
    expect(cards.map((c) => c.querySelector("strong")?.textContent)).toEqual(["Stream CSV", "Build a PDF", "A fresh link"]);
    expect(document.body.textContent).toContain("unanswered");
    expect(await violations()).toEqual([]);

    // 2 picks the second card; a constraint is added as the person's.
    press("2");
    flushSync();
    expect(cards[1]!.getAttribute("aria-checked")).toBe("true");
    const add = document.querySelector<HTMLInputElement>('input[aria-label="Add a constraint"]')!;
    type(add, "no new dependency");
    press("Enter", {}, add);
    flushSync();
    button(/^Make the brief$/).click();
    await settle();
    expect(host.app.run).toHaveBeenLastCalledWith(
      expect.objectContaining({ run: { kind: "explore_choose", session: "ex-abc", approach: 2, constraints: ["under a second", "no new dependency"], questions: ["which columns?"], flag: false } }),
    );
  });

  it("starts the spec from the brief in the New feature sheet, verbatim with its digest", async () => {
    host.get.mockImplementation(async (path?: string) => (String(path).startsWith("/api/explore/session") ? { data: chosen } : { data: { sessions: [] } }));
    open({ id: "ex-abc" });
    await settle();
    expect(document.querySelector(".brief code")?.textContent).toBe("3f9a0c1d2e4b");
    expect(await violations()).toEqual([]);
    press("Enter", { metaKey: true });
    flushSync();
    expect(app.newFeature).toEqual({ text: chosen.brief, source: { kind: "explore", session: "ex-abc", digest: chosen.choice.digest }, through: "tasks" });
    expect(app.explore).toBeNull();
  });

  it("asks to choose again once the kept items change", async () => {
    host.get.mockImplementation(async (path?: string) => (String(path).startsWith("/api/explore/session") ? { data: chosen } : { data: { sessions: [] } }));
    open({ id: "ex-abc" });
    await settle();
    document.querySelector<HTMLButtonElement>('button[aria-label="Drop “no new dependency”"]')!.click();
    flushSync();
    expect(document.body.textContent).toContain("choose again");
    expect(button(/Make the brief again/)).toBeTruthy();
  });

  it("lists explorations to pick up when it opens empty", async () => {
    host.get.mockImplementation(async () => ({ data: { sessions: [{ id: "ex-1", idea: "a digest email", skill: "speccoder.explore", state: "chosen", updated_at: 1_790_000_000 }] } }));
    open();
    await settle();
    expect(document.querySelector(".sessions")?.textContent).toContain("a digest email");
    expect(await violations()).toEqual([]);
  });

  it("says in the sheet why an exploration gave nothing to choose from", async () => {
    host.app.run.mockResolvedValueOnce({ status: "run_failed", message: "speccoder.explore ended without approaches to choose from", data: { ...explored, approaches: [] } });
    open({ idea: "x" });
    await settle();
    press("Enter", { metaKey: true });
    await settle();
    await settle();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain("ended without approaches");
    expect(button(/Explore again/)).toBeTruthy();
    expect(await violations()).toEqual([]);
  });
});
