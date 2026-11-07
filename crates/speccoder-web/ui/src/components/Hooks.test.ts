// Settings › Hooks: a card per hook with its trust and last runs; Trust…
// for one that waits, Test for a trusted one; two starters when there are
// none.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    perform: vi.fn(async () => {}),
    copy: vi.fn(async () => {}),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import Hooks from "./Hooks.svelte";

const entry = (over: Record<string, unknown>) => ({
  name: "licence", role: "gate", listens: "before tick", at: "tick", on: [], runs: "cargo deny check licenses",
  state: "trusted", digest: "sha256:8f32cecdaa7b9e39f1b3", files: [], outside: [], definition: "[hooks.licence]\nat = \"tick\"\n",
  recent: [], trust_flag: "--trust-hook licence=sha256:8f32", ...over,
});
const reply = (hooks: unknown[]) => ({ status: "ok", message: "", data: { hooks } });

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.perform.mockClear();
});

describe("Hooks", () => {
  it("lists a trusted gate with its runs and an observer that waits, each with its one action", async () => {
    host.get.mockResolvedValue(reply([
      entry({ recent: [{ at: "1", what: "tick T004", outcome: "passed", duration_ms: 9 }, { at: "2", what: "tick T005", outcome: "blocked", duration_ms: 9, error: "GPL-3.0 in foo" }] }),
      entry({ name: "team-chat", role: "observer", listens: "hears task.ticked, task.stopped", at: null, on: ["*"], runs: "https://hooks.example.com/x", state: "untrusted", digest: "sha256:41a5" }),
    ]));
    shown = mount(Hooks, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Guards the tick");
    expect(text).toContain("Hears task.ticked, task.stopped");
    expect(text).toContain("Waits for your trust");
    expect(text).toContain("blocked — GPL-3.0 in foo");
    expect(document.querySelectorAll(".m.ok").length).toBe(1);
    expect(document.querySelectorAll(".m.bad").length).toBe(1);
    button("Trust…").click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "hook_trust", name: "team-chat", digest: "sha256:41a5" } }));
    button(/Test/).click();
    expect(host.app.perform).toHaveBeenLastCalledWith(expect.objectContaining({ run: { kind: "hook_test", name: "licence" } }));
    expect(await violations()).toEqual([]);
  });

  it("offers two starters to copy when there are no hooks", async () => {
    host.get.mockResolvedValue(reply([]));
    shown = mount(Hooks, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("A gate the tick waits for");
    expect(document.querySelectorAll(".starters li").length).toBe(2);
    document.querySelector<HTMLButtonElement>(".starters button")!.click();
    expect(host.app.copy).toHaveBeenCalledWith(expect.stringContaining("[hooks.licence]"), "Copied", expect.any(String));
    expect(await violations()).toEqual([]);
  });
});
