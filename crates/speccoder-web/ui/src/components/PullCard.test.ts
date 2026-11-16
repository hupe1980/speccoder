// The Pull Request card with the definition of done: what is
// open and how to close it before Open Draft PR; a stop at it offers Open
// Anyway, which hands off with the open lines in the body.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const pr = { label: "Open Draft PR", command: "speccoder pr --yes", run: { kind: "pull_request" }, confirm: "Push?" };
const host = vi.hoisted(() => ({
  app: { feature: "001-x", report: {}, actions: [] as unknown[], job: null as unknown, perform: vi.fn() },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import PullCard from "./PullCard.svelte";

const github = { status: { ready: true }, branch: "speccoder/001-x", pull: null };
const open = {
  base: "main",
  done: false,
  lines: [
    { part: "changelog", state: "not_done", summary: "[Unreleased] is empty; 2 user-facing files changed since main", findings: [{ path: "src/a.rs", what: "changed" }, { path: "src/b.rs", what: "changed" }], fixes: ["add a line under `## [Unreleased]` in CHANGELOG.md"] },
    { part: "links", state: "done", summary: "40 links in 9 pages resolve" },
    { part: "tone", state: "done", summary: "9 pages describe what is there" },
  ],
};
const allDone = { base: "main", done: true, lines: open.lines.slice(1) };

function serve(done: unknown) {
  host.get.mockImplementation((path: string) => Promise.resolve(path.startsWith("/api/done") ? { status: "ok", message: "", data: done } : github));
}

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.job = null;
  vi.clearAllMocks();
});

describe("PullCard", () => {
  it("shows what is open, its fix, and where opening will stop", async () => {
    host.app.actions = [pr];
    serve(open);
    shown = mount(PullCard, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    for (const s of ["Definition of done", "2 of 3", "Changelog", "src/a.rs", "→ add a line under ## [Unreleased] in CHANGELOG.md", "Opening stops at Changelog"]) expect(text).toContain(s);
    expect(document.querySelector(".fix code")?.textContent).toBe("## [Unreleased]");
    expect(text).not.toContain("Open Anyway");
    expect(await violations(document.body)).toEqual([]);
  });

  it("is one line when everything is done", async () => {
    host.app.actions = [pr];
    serve(allDone);
    shown = mount(PullCard, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Definition of done");
    expect(text).toContain("Done");
    expect(text).not.toContain("Opening stops");
    expect(document.querySelectorAll(".done li").length).toBe(0);
  });

  it("offers Open Anyway after a stop at the definition of done", async () => {
    host.app.actions = [pr];
    host.app.job = { id: 1, command: "speccoder pr --yes", running: false, result: { status: "needs_human", code: "DONE-NOT-DONE", message: "not met" } };
    serve(open);
    shown = mount(PullCard, { target: document.body });
    await settle();
    button("Open Anyway").click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "pull_request", force: true }, command: "speccoder pr --yes --force" }));
  });
});
