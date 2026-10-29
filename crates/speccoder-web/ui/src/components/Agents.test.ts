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
    expect(text).toContain("What an agent does without asking stays outside");
  });
});

describe("AgentStrip", () => {
  it("counts what speccoder decided and warns when the agent decides alone", () => {
    shown = mount(AgentStrip, {
      target: document.body,
      props: { agent: { name: "Codex", mode: "Codex is in Auto review", decides_alone: true, plan: [{ content: "Add login", status: "completed" }], asked: 14, allowed: 12, waited: 1, refused: 1 } },
    });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("14 asked · 12 allowed · 1 waited for you · 1 refused");
    expect(text).toContain("decides alone");
    expect(text).toContain("Add login");
    expect(document.querySelector(".alone")).not.toBeNull();
  });

  it("says an agent that asks first asks", () => {
    shown = mount(AgentStrip, {
      target: document.body,
      props: { agent: { name: "GitHub Copilot", mode: null, decides_alone: false, plan: [], asked: 3, allowed: 3, waited: 0, refused: 0 } },
    });
    flushSync();
    expect(document.body.textContent).toContain("asks before acting");
    expect(document.body.textContent).toContain("3 asked · 3 allowed");
    expect(document.querySelector(".alone")).toBeNull();
  });
});
