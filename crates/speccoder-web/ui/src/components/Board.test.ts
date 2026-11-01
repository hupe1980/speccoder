// A board card says why a tick is regression-only, and that a test file
// an attempt wrote never ran — in speccoder's own words.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null,
    feature: "001-greet-function",
    selected: null,
    approvals: [],
    stages: [],
    elsewhere: false,
    live: null,
    task: null,
    report: null as unknown,
    context: null,
    perform: vi.fn(async (_a: unknown) => {}),
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
});

const task = (id: string, state: string, over: object) => ({
  id, title: `do ${id}`, phase: "Phase 3", state, attestation: null, blocked_by: [], commit: null, run_id: null, last_attempt: null, note: null, ...over,
});

describe("Board", () => {
  it("names the reason in the card's accessible name and tooltip, and a file that never ran", () => {
    host.app.report = {
      data: {
        tasks: [
          task("T007", "regression_only", {
            regression: { reason: { reason: "already_passing" }, text: "already passing before the attempt" },
          }),
          task("T004", "regression_only", {}),
          task("T005", "ready", {
            last_attempt: { outcome: "verification_failed", reason: "tests not collected: `tests/unit/greet_name.rs` — Cargo builds …", run_id: null, reference: "refs/x" },
          }),
        ],
      },
    };
    shown = mount(Board, { target: document.body });
    flushSync();
    const t007 = document.querySelector('button[aria-label^="T007"]') as HTMLButtonElement;
    expect(t007.getAttribute("aria-label")).toBe("T007 do T007, Regression only: already passing before the attempt");
    expect(t007.title).toBe("Regression only: already passing before the attempt");
    const t004 = document.querySelector('button[aria-label^="T004"]') as HTMLButtonElement;
    expect(t004.title).toBe("Regression only: why was not recorded");
    expect(document.body.textContent).toContain("Tests never ran: tests/unit/greet_name.rs");
  });

  it("badges a planning task and one that is yours, and offers no Run for the latter", () => {
    host.app.report = {
      data: {
        tasks: [
          task("T002", "ready", { lane: "planning" }),
          task("T019", "ready", { lane: "yours" }),
        ],
      },
    };
    shown = mount(Board, { target: document.body });
    flushSync();
    const [planning, yours] = [...document.querySelectorAll("article.card")] as HTMLElement[];
    expect(planning!.textContent).toContain("📐 Planning");
    expect(planning!.textContent).toContain("Run");
    expect(yours!.textContent).toContain("🙋 Yours");
    expect(yours!.querySelector(".act")).toBeNull();
  });

  it("shows each open story's scenarios on its oracle and what judges a task, and the strip opens the checkpoint", async () => {
    host.app.report = {
      data: {
        tasks: [
          task("T019", "ready", { judged: { kind: "progress" } }),
          task("T020", "ready", { judged: { kind: "linked", scenarios: ["US3-2"] } }),
          task("T021", "blocked", { judged: { kind: "checkpoint" } }),
        ],
        stories: [
          // Done: proven, or ticked before its oracle — neither on the board.
          { story: "US8", proven: true, oracle: { commit: "abc1234", decided_by: "speccoder run" }, scenarios: [{ id: "US8-1", state: "green" }] },
          { story: "US9", before_oracle: true, scenarios: [{ id: "US9-1", state: "red" }] },
          {
            story: "US3",
            oracle: { commit: "abc1234", decided_by: "speccoder run" },
            scenarios: [
              { id: "US3-1", state: "red", contract: "a malformed row → an error" },
              { id: "US3-2", state: "green", turned_by: "T018" },
              { id: "US3-3", state: "untested" },
            ],
            checkpoint: "T021",
          },
        ],
      },
    };
    shown = mount(Board, { target: document.body });
    flushSync();
    expect(document.querySelector('button[aria-label^="US8"]')).toBeNull();
    expect(document.querySelector('button[aria-label^="US9"]')).toBeNull();
    const strip = document.querySelector('button[aria-label^="US3"]') as HTMLButtonElement;
    expect(strip.getAttribute("aria-label")).toBe("US3: 1 of 3 green · T018 turned US3-2 · T021 closes it");
    expect(strip.querySelectorAll(".mk.on")).toHaveLength(1);
    expect(strip.querySelectorAll(".mk.untested")).toHaveLength(1);
    expect(strip.querySelector(".mk")?.getAttribute("title")).toBe("US3-1: red — a malformed row → an error");
    strip.click();
    expect(host.app.selectTask).toHaveBeenCalledWith("T021");
    const chips = [...document.querySelectorAll(".foot.share")].map((c) => c.textContent?.trim());
    expect(chips).toEqual(expect.arrayContaining(["US3-2", "closes the story"]));
    expect(await violations()).toEqual([]);
  });
});
