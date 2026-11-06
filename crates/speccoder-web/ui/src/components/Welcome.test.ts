// The welcome of a host outside a project, and the sample's banner and
// playback, as a person meets them.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";
import type { SampleInfo, WelcomeView } from "../lib/types";

const host = vi.hoisted(() => ({
  app: {
    welcome: null as WelcomeView | null,
    newProject: null as unknown,
    clone: false,
    view: "inbox",
    report: { data: { tasks: [{ id: "T001", commit: "a1", state: "verified", run_id: "r2" }, { id: "T002", commit: "b2", state: "verified", run_id: "r1" }, { id: "T003", commit: null, state: "ready", run_id: null }] } } as unknown,
    run: vi.fn(async (_a: unknown, _i?: string, _r?: string) => ({ status: "ok", message: "ok" }) as unknown),
    selectTask: vi.fn(async (_t: string | null) => {}),
    perform: vi.fn(async (_a: unknown) => {}),
    toast: vi.fn(),
  },
  get: vi.fn(),
  post: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get, post: host.post }));

import Welcome from "./Welcome.svelte";
import SamplePlayback from "./SamplePlayback.svelte";

let c: ReturnType<typeof mount> | null = null;
const view = (over: Partial<WelcomeView> = {}): WelcomeView => ({
  bound: null,
  start: "/Users/sam",
  start_shown: "~",
  parent: "/Users/sam/Workspaces",
  parent_shown: "~/Workspaces",
  recents: [
    { path: "/Users/sam/Workspaces/md2csv", name: "md2csv", opened_at: Math.floor(Date.now() / 1000), created: true, shown: "~/Workspaces/md2csv", exists: true, undo: null },
    { path: "/Users/sam/gone/old-thing", name: "old-thing", opened_at: 1, created: false, shown: "~/gone/old-thing", exists: false, undo: null },
  ],
  sample: true,
  sample_copy: false,
  version: "0.2.0",
  ...over,
});

beforeEach(() => {
  host.app.run.mockClear();
  host.app.selectTask.mockClear();
  host.get.mockReset();
  host.post.mockReset();
  host.get.mockResolvedValue({ folders: [] });
  host.app.newProject = null;
  host.app.clone = false;
});

afterEach(() => {
  if (c) unmount(c);
  c = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("Welcome", () => {
  it("offers the four doors, in order, and the recent projects", async () => {
    host.app.welcome = view();
    c = mount(Welcome, { target: document.body });
    flushSync();
    const cards = [...document.querySelectorAll(".card .t")].map((t) => t.textContent?.trim().replace(/\s+/g, " "));
    expect(cards).toEqual(["New Project ⌘N", "Open Project… ⌘O", "Clone from GitHub…", "Try the Sample"]);
    expect(document.body.textContent).toContain("~/Workspaces/md2csv");
    expect(document.body.textContent).toContain("today");
    // A folder that is gone: dimmed, only Forget.
    expect(document.querySelector(".gone")?.textContent).toContain("Folder not found");
    expect(button("Forget")).toBeTruthy();
    button(/New Project/).click();
    expect(host.app.newProject).toEqual({});
    expect(await violations()).toEqual([]);
  });

  it("hides Try the Sample in a build without it", () => {
    host.app.welcome = view({ sample: false });
    c = mount(Welcome, { target: document.body });
    flushSync();
    expect(document.body.textContent).not.toContain("Try the Sample");
  });

  it("opens a folder by its path and says why it cannot", async () => {
    host.app.welcome = view();
    c = mount(Welcome, { target: document.body });
    flushSync();
    button(/Open Project/).click();
    flushSync();
    const field = document.querySelector<HTMLInputElement>("#w-path")!;
    expect(field.getAttribute("role")).toBe("combobox");
    type(field, "~/nope");
    host.post.mockResolvedValueOnce({ error: "Not a folder" });
    button("Open").click();
    await settle(8);
    expect(host.post).toHaveBeenCalledWith("/api/open", { path: "~/nope" });
    expect(document.body.textContent).toContain("Not a folder");
    expect(field.getAttribute("aria-invalid")).toBe("true");
  });

  it("tries the sample, and asks before starting over a copy", async () => {
    host.app.welcome = view({ sample_copy: true });
    c = mount(Welcome, { target: document.body });
    flushSync();
    button(/Try the Sample/).click();
    flushSync();
    expect(host.app.run).not.toHaveBeenCalled();
    button(/^Start over/).click();
    await settle();
    expect((host.app.run.mock.calls[0]![0] as { run: unknown }).run).toEqual({ kind: "open_sample", fresh: true });
  });
});

describe("SamplePlayback", () => {
  const sample = (over: Partial<SampleInfo> = {}): SampleInfo => ({ path: "/s/sample/0.2.0/wordcount", recorded: { model: "claude-sonnet-5-5", recorded_at: "2026-11-02", agentplane: "0.54.0" }, runs: [{ id: "r1", result: "reproduced" }], ...over });

  const caption = () => document.querySelector(".caption")?.textContent ?? "";

  it("takes the tour: five stops on the sample's own run, one part of the screen each", async () => {
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample() } });
    flushSync();
    const banner = document.querySelector('[role="status"]')!;
    expect(banner.textContent).toContain("A recorded run of wordcount, replayed strictly — no model was called. Recorded 2026-11-02 with claude-sonnet-5-5.");
    button(/^Take the tour/).click();
    flushSync();
    expect(caption()).toContain("Tour · 1 of 5");
    expect(caption()).toContain("One feature, every task");
    expect(host.app.selectTask).toHaveBeenLastCalledWith(null);
    // → steps (and pauses): the first verified task's tests, red then green.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    await tick();
    flushSync();
    expect(caption()).toContain("2 of 5");
    expect(caption()).toContain("Red first, then green");
    expect(host.app.selectTask).toHaveBeenLastCalledWith("T001");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    await tick();
    flushSync();
    expect(caption()).toContain("1 of 5");
    expect(await violations(document.querySelector(".caption")!)).toEqual([]);
    // Esc ends it where it is.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    flushSync();
    expect(document.querySelector(".caption")).toBeNull();
  });

  it("plays at a reading pace and ends at the end card", async () => {
    vi.useFakeTimers();
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample() } });
    flushSync();
    button(/^Take the tour/).click();
    flushSync();
    await vi.advanceTimersByTimeAsync(6999);
    flushSync();
    expect(caption()).toContain("1 of 5");
    // Each stop waits up to 3s for its part of the screen (none here), then reads for 7s.
    await vi.advanceTimersByTimeAsync(10_000 * 5);
    flushSync();
    expect(document.querySelector(".caption")).toBeNull();
    expect(document.body.textContent).toContain("Start your own");
  });

  it("runs the sample's checks through their kind, or says which tool is missing", () => {
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample() } });
    flushSync();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "End" }));
    flushSync();
    button("Run the checks yourself").click();
    expect((host.app.perform.mock.calls[0]![0] as { run: unknown }).run).toEqual({ kind: "sample_checks" });
    unmount(c);
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample({ checks_missing: "cargo is not installed — install it: https://rustup.rs" }) } });
    flushSync();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "End" }));
    flushSync();
    expect(button("Run the checks yourself").disabled).toBe(true);
    expect(document.body.textContent).toContain("cargo is not installed");
  });

  it("skips to the end with End; a divergence is never played; no verified task, no stop about one", () => {
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample() } });
    flushSync();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "End" }));
    flushSync();
    expect(document.body.textContent).toContain("Start your own");
    unmount(c);
    host.app.selectTask.mockClear();
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample({ runs: [{ id: "r1", result: "diverged", finding: "diverged: an effect differs" }] }) } });
    flushSync();
    expect(document.body.textContent).toContain("This build cannot reproduce the recorded run — please report it.");
    expect(document.body.textContent).not.toContain("Take the tour");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: " " }));
    expect(host.app.selectTask).not.toHaveBeenCalled();
    unmount(c);
    const report = host.app.report;
    host.app.report = { data: { tasks: [{ id: "T003", commit: null, state: "ready", run_id: null }] } };
    c = mount(SamplePlayback, { target: document.body, props: { sample: sample() } });
    flushSync();
    button(/^Take the tour/).click();
    flushSync();
    expect(caption()).toContain("1 of 2");
    host.app.report = report;
  });
});
