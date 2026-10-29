// The agent cards: three ticks each, the one action a missing tick has,
// Use for an installed agent; the live strip: the tally and, in warning
// style, an agent that decides alone.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";

const yes = (detail: string) => ({ state: "yes", detail, fix: [], fix_steps: [] });
const card = (id: string, name: string, over: Record<string, unknown> = {}) => ({
  id, name, chosen: false, installed: yes("on PATH"), signed_in: yes("it opened a session"), asks_first: yes("it asks first"), ...over,
});
const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    agents: null as { cards: unknown[]; probed: boolean } | null,
    loadAgents: vi.fn(async (_probe: boolean) => [] as unknown[]),
    take: vi.fn(),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import Agents from "./Agents.svelte";
import AgentStrip from "./AgentStrip.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

describe("Agents", () => {
  it("shows each agent's ticks and the one action a missing one has", async () => {
    host.app.agents = {
      probed: true,
      cards: [
        card("copilot", "GitHub Copilot", { chosen: true }),
        card("opencode", "OpenCode", {
          signed_in: { state: "no", detail: "Run `opencode auth login` in the terminal", fix: ["opencode auth login"],
            fix_steps: [{ command: "opencode auth login", how: "command", argvs: [["opencode", "auth", "login"]], interactive: true }] },
        }),
        card("codex", "Codex", { installed: { state: "no", detail: "it runs through `npx`, which needs Node.js", fix: ["brew install node"],
          fix_steps: [{ command: "brew install node", how: "command", argvs: [["brew", "install", "node"]], interactive: false, outside: "installs node on this machine with Homebrew" }] } }),
      ],
    };
    const use = vi.fn();
    shown = mount(Agents, { target: document.body, props: { use } });
    await tick();
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("GitHub Copilot");
    expect(text).toContain("In use");
    expect(text).toContain("Signed in");
    expect(text).toContain("Asks first");
    // The missing tick's action: the agent's own login, as a step.
    const login = [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("opencode auth login"))!;
    login.click();
    expect(host.app.take).toHaveBeenCalledWith(expect.objectContaining({ command: "opencode auth login" }));
    // Use: only for an installed agent not in use.
    const buttons = [...document.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(buttons).toContain("Use OpenCode");
    expect(buttons).not.toContain("Use GitHub Copilot");
    expect(buttons).not.toContain("Use Codex");
    [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Use OpenCode")!.click();
    expect(use).toHaveBeenCalledWith("opencode");
    expect(text).toContain("Everything the agent does itself asks you");
  });
});

describe("AgentStrip", () => {
  it("filters the call log by a count, and offers the fix for an agent that decides alone", () => {
    const onfilter = vi.fn();
    const onfix = vi.fn();
    shown = mount(AgentStrip, {
      target: document.body,
      props: { agent: { name: "Codex", mode: null, decides_alone: true, plan: [], asked: 5, allowed: 3, waited: 0, refused: 2 }, onfilter, onfix },
    });
    flushSync();
    [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "2 refused")!.click();
    expect(onfilter).toHaveBeenCalledWith("deny");
    [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Make it ask first")!.click();
    expect(onfix).toHaveBeenCalled();
    // A count of none is not shown, but "allowed" always is.
    expect(document.body.textContent).not.toContain("waited");
  });

  it("counts what speccoder decided and warns when the agent decides alone", () => {
    shown = mount(AgentStrip, {
      target: document.body,
      props: { agent: { name: "Codex", mode: "Codex is in Auto review", decides_alone: true, plan: [{ content: "Add login", status: "completed" }], asked: 14, allowed: 12, waited: 1, refused: 1 } },
    });
    flushSync();
    const text = (document.body.textContent ?? "").replace(/\s+/g, " ");
    expect(text).toContain("14 asked · 12 allowed · 1 waited for you · 1 refused");
    expect(text).toContain("decides alone");
    expect(text).toContain("Add login");
    expect(document.querySelector(".alone")).not.toBeNull();
  });

  it("says when it is not known whether an agent asks, and never calls that deciding alone", () => {
    shown = mount(AgentStrip, {
      target: document.body,
      props: { agent: { name: "Kimi", mode: null, decides_alone: false, known: false, plan: [], asked: 0, allowed: 0, waited: 0, refused: 0 } },
    });
    flushSync();
    expect(document.body.textContent).toContain("not known whether it asks");
    expect(document.querySelector(".alone")).toBeNull();
  });

  it("says an agent that asks first asks", () => {
    shown = mount(AgentStrip, {
      target: document.body,
      props: { agent: { name: "GitHub Copilot", mode: null, decides_alone: false, plan: [], asked: 3, allowed: 3, waited: 0, refused: 0 } },
    });
    flushSync();
    expect(document.body.textContent).toContain("asks before acting");
    expect((document.body.textContent ?? "").replace(/\s+/g, " ")).toContain("3 asked · 3 allowed");
    expect(document.querySelector(".alone")).toBeNull();
  });
});
