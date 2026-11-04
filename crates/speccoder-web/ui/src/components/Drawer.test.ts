// The inspector offers a ruling on a task's tests where Needs You does:
// its own test outside a story, a red scenario at a story's checkpoint —
// for the selected task only.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null,
    feature: "001-x",
    selected: "T026" as string | null,
    approvals: [],
    report: null as unknown,
    inbox: null as unknown,
    actions: [] as unknown[],
    perform: vi.fn(async (_a: unknown) => {}),
    selectTask: vi.fn(async (_t: string | null) => {}),
    toast: vi.fn(),
  },
  rows: [] as unknown[],
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (importOriginal) => {
  const real = await importOriginal<typeof import("../lib/api")>();
  return {
    ...real,
    get: vi.fn(async (url: string) => {
      if (url.startsWith("/api/evidence")) {
        return {
          task: { id: "T026", title: "Route both tables", phase: "Phase 7: Convergence", state: "ready", attestation: null, blocked_by: [], commit: null, run_id: null, last_attempt: null, note: null },
          banner: { mark: "ready", words: "Ready" },
          rows: host.rows,
        };
      }
      return {};
    }),
  };
});

import Drawer from "./Drawer.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

const ruling = (task: string) => ({
  id: { kind: "rule_scenario", subject: task },
  label: "these tests are wrong",
  why: "the next run writes it again",
  command: `speccoder task rule ${task} --tests-wrong --reason "…"`,
  run: { kind: "rule_scenario", task, scenario: null },
  input: "What `tests/acceptance_t026.txt` asserts that the spec does not say",
});

describe("Drawer", () => {
  it("offers the ruling on the selected task's own test, and only its", async () => {
    host.app.report = { data: { tasks: [{ id: "T026", judged: null }] } };
    host.app.inbox = {
      data: {
        items: [
          { kind: "stuck", task: "T026", feature: "001-x", actions: [ruling("T026")] },
          { kind: "stuck", task: "T027", feature: "001-x", actions: [ruling("T027")] },
        ],
      },
    };
    shown = mount(Drawer, { target: document.body });
    await tick();
    await new Promise((r) => setTimeout(r, 0));
    flushSync();
    expect(document.body.textContent).toContain("Its own test failed");
    const buttons = [...document.querySelectorAll("button")].filter((b) => b.textContent?.includes("Tests Are Wrong"));
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(["These Tests Are Wrong…"]);
    buttons[0]?.click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: expect.objectContaining({ task: "T026", scenario: null }) }));
  });

  it("shows what ran in the sandbox without asking, four at first", async () => {
    host.app.report = { data: { tasks: [{ id: "T026", judged: null }] } };
    host.app.inbox = { data: { items: [] } };
    const items = ["python -c 1", "python -c 2", "uv run python -m md2csv", "sh -c x", "rm -r build", "python -m pytest -q t.py"];
    host.rows = [{ label: "Ran without asking", mark: "ready", text: "6 commands the sandbox confined", items }];
    shown = mount(Drawer, { target: document.body });
    await tick();
    await new Promise((r) => setTimeout(r, 0));
    flushSync();
    expect(document.body.textContent).toContain("6 ran in the sandbox — nobody had to be asked");
    const chips = () => [...document.querySelectorAll(".cmds li.mono")].map((l) => l.textContent);
    expect(chips()).toEqual(items.slice(0, 4));
    const more = [...document.querySelectorAll("button")].find((b) => b.textContent === "+2 more");
    more?.click();
    flushSync();
    expect(chips()).toEqual(items);
    host.rows = [];
  });
});
