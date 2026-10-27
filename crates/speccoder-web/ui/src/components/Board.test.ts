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
            regression: { reason: { reason: "already_passing", implemented_by: { task: "T003", commit: "d433d77" } }, text: "already passing before the attempt — implemented by T003" },
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
    expect(t007.getAttribute("aria-label")).toBe("T007 do T007, Regression only: already passing before the attempt — implemented by T003");
    expect(t007.title).toBe("Regression only: already passing before the attempt — implemented by T003");
    const t004 = document.querySelector('button[aria-label^="T004"]') as HTMLButtonElement;
    expect(t004.title).toBe("Regression only: why was not recorded");
    expect(document.body.textContent).toContain("Tests never ran: tests/unit/greet_name.rs");
  });
});
