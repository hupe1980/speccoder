// A bug unit is one card: its stage, its state in words and one next step;
// its tasks are a checklist that closes with the verified fix.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { UnitBug } from "../lib/schema.gen";

const host = vi.hoisted(() => ({
  app: {
    job: null as null | { running: boolean },
    feature: "empty-title",
    report: null as unknown,
    bugSheet: null as unknown,
    approvals: [],
    stages: [],
    elsewhere: false,
    live: null,
    task: null,
    selected: null,
    context: null,
    perform: vi.fn(async (_a: unknown) => {}),
    withAgent: <T,>(a: T) => a,
    selectTask: vi.fn(async (_t: string) => {}),
    toast: vi.fn(),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import Board from "./Board.svelte";
import { violations } from "../test/dom";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.job = null;
  host.app.bugSheet = null;
  host.app.perform.mockClear();
});

const task = (id: string, state: string) => ({
  id, title: `do ${id}`, phase: "1.", state, attestation: null, blocked_by: [], commit: null, run_id: null, last_attempt: null, note: null,
});

function board(outcome: UnitBug["outcome"], verified = false) {
  const bug: UnitBug = {
    key: "empty-title/bug:empty-title",
    title: "empty title",
    stage: outcome === null ? "record" : outcome === "awaiting_confirmation" ? "confirm" : "done",
    outcome,
    verdict: verified ? { verdict: "verified" } : null,
  };
  const state = verified ? "verified" : "ready";
  host.app.report = { data: { bug, tasks: [task("1", state), task("3.1", verified ? "verified" : "blocked")] } };
  shown = mount(Board, { target: document.body });
  flushSync();
}

const buttons = () => [...document.querySelectorAll(".act button")].map((b) => b.textContent?.trim());

describe("BugCard", () => {
  it("is one card with no task lanes and no per-task Run", async () => {
    board(null);
    expect(document.querySelectorAll("article.bug")).toHaveLength(1);
    expect(document.querySelector(".board")).toBeNull();
    expect(document.querySelector("article.card")).toBeNull();
    expect(document.body.textContent).toContain("Not run yet");
    expect(document.body.textContent).toContain("Ticked together when the fix is verified");
    expect(document.querySelector('[aria-current="step"]')?.textContent).toBe("Record");
    expect(buttons()).toEqual(["Run the Bug"]);
    expect(await violations(document.body)).toEqual([]);
  });

  it("runs the bug, and not while a job runs", () => {
    board(null);
    (document.querySelector(".act button") as HTMLButtonElement).click();
    expect(host.app.perform).toHaveBeenCalledTimes(1);
    const a = host.app.perform.mock.calls[0]![0] as { run: { kind: string; task: string | null }; command: string };
    expect(a.run).toEqual({ kind: "run", task: null });
    expect(a.command).toBe("speccoder run --feature empty-title");
    unmount(shown!);
    host.app.job = { running: true };
    shown = mount(Board, { target: document.body });
    flushSync();
    expect((document.querySelector(".act button") as HTMLButtonElement).disabled).toBe(true);
  });

  it("opens the Bug sheet at the confirmation", () => {
    board("awaiting_confirmation");
    expect(buttons()).toEqual(["Confirm the Reproductions"]);
    (document.querySelector(".act button") as HTMLButtonElement).click();
    expect(host.app.bugSheet).toEqual({ key: "empty-title/bug:empty-title" });
  });

  it("offers to run again after a stop, in words", () => {
    board("cannot_reproduce");
    expect(document.body.textContent).toContain("Could not be reproduced — nothing was fixed");
    expect(buttons()).toEqual(["Run the Bug Again"]);
    unmount(shown!);
    board("already_satisfied");
    expect(document.body.textContent).toContain("Already fixed — every reproduction passes");
    expect(buttons()).toEqual(["Run the Bug Again"]);
  });

  it("says fixed and verified, ticks every task and offers nothing", async () => {
    board("fixed", true);
    expect(document.body.textContent).toContain("Fixed and verified");
    expect(buttons()).toEqual([]);
    expect(document.querySelectorAll(".tasks li.closed")).toHaveLength(2);
    expect(document.querySelector('[aria-current="step"]')).toBeNull();
    expect(await violations(document.body)).toEqual([]);
  });
});
