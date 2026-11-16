// The merge forecast on the Pull Request card: one line until
// the first check, problems first with who, what and how to see it, the
// rest in one quiet line; Check Open Branches runs one.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: { feature: "001-x" },
  get: vi.fn(),
  post: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get, post: host.post }));
vi.mock("../lib/editor.svelte", () => ({ openIn: vi.fn() }));

import Forecast from "./Forecast.svelte";

const now = Math.floor(Date.now() / 1000);
const pair = (branch: string, verdict: string, extra: Record<string, unknown> = {}) => ({
  branch, commit: "c", author: "Dana", updated: now - 7200, verdict, held: 0, their_oracle: false, ...extra,
});
const report = {
  feature: "001-x", branch: "speccoder/001-x", commit: "c", fetch: { state: "fetched" },
  oracle: { stories: ["US1"], scenarios: 2, tests: 2, green: 2 },
  pairs: [
    pair("origin/dana/csv", "breaks", {
      broken: [{ unit: "001-x", mine: true, scenario: "US1-2", contract: "a quoted field → stays quoted", test: "tests/us1.sh", output: "expected \"a,b\"" }],
      fixes: ["`origin/dana/csv` breaks US1-2: see it with `git merge --no-commit --no-ff origin/dana/csv`"],
    }),
    pair("speccoder/003-import", "conflicts", { author: "Agent", files: ["src/csv.rs"], fixes: ["merge it now"] }),
    pair("origin/fix/typo", "no_conflict_observed", { held: 2 }),
    pair("origin/fix/readme", "not_judged", { why: "docs only" }),
  ],
  ran: 3, reused: 0, at: now - 180,
};

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("Forecast", () => {
  it("says what it does until the first check, then runs one", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "no forecast yet" });
    host.post.mockResolvedValue({ status: "needs_human", message: "1 of 4", data: report });
    shown = mount(Forecast, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("runs your oracle on each merge");
    expect(await violations(document.body)).toEqual([]);
    button("Check Open Branches").click();
    await settle();
    expect(host.post).toHaveBeenCalledWith("/api/forecast?feature=001-x", {});
    expect(document.body.textContent).toContain("Breaks US1-2");
  });

  it("leads with the problems and folds the rest into one line", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: report });
    shown = mount(Forecast, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    for (const s of ["4 branches · 3 min ago", "origin/dana/csv", "Dana · 2 h ago", "Breaks US1-2", "US1-2: a quoted field → stays quoted", "fails on the merge, passes on its own branch", "Conflicts in src/csv.rs", "No conflict observed with 1 branch", "1 branch not judged"]) expect(text).toContain(s);
    expect(text.indexOf("Breaks US1-2")).toBeLessThan(text.indexOf("No conflict observed"));
    expect(document.querySelector(".fix code")?.textContent).toBe("origin/dana/csv");
    expect(document.querySelectorAll("button.open-in").length).toBe(2);
    expect(text).not.toMatch(/\bsafe\b/i);
    expect(await violations(document.body)).toEqual([]);
  });

  it("shows why a check could not run", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "" });
    host.post.mockResolvedValue({ status: "not_found", message: "nothing to forecast: speccoder/001-x does not exist yet" });
    shown = mount(Forecast, { target: document.body });
    await settle();
    button("Check Open Branches").click();
    await settle();
    expect(document.body.textContent).toContain("does not exist yet");
  });

  it("keeps the last forecast when a new check is refused", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: report });
    host.post.mockResolvedValue({ status: "usage_error", message: "a forecast of 001-x is already running here" });
    shown = mount(Forecast, { target: document.body });
    await settle();
    button("Check Open Branches").click();
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("already running here");
    expect(text).toContain("Breaks US1-2");
  });
});
