// Settings › Extensions: a card per extension — what it contributes in
// words, its trust, what widened since — Trust… for one that waits; how to
// add one when there are none.

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

import Extensions from "./Extensions.svelte";

const ext = (over: Record<string, unknown>) => ({
  name: "threatspec", source: "agent_plugins", version: "0.2.1", description: "Threat modelling",
  folder: ".speccoder/extensions/threatspec", digest: "sha256:8f32cecdaa7b9e39f1b3",
  files: [{ path: "plugin.json", digest: "sha256:1" }],
  contributions: [
    { kind: "gate", name: "threatspec.check", line: "gate before tick: runs ./scripts/check.sh — no network" },
    { kind: "view", name: "threatspec.model", line: "view “Threat model” on each feature: reads threat-model.md" },
  ],
  trust: "trusted", trust_flag: "--trust-extension threatspec=sha256:8f32", ...over,
});
const reply = (extensions: unknown[]) => ({ status: "ok", message: "", data: { extensions } });

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.perform.mockClear();
});

describe("Extensions", () => {
  it("shows what each contributes, and for a changed one what widened first, with Trust…", async () => {
    host.get.mockResolvedValue(reply([
      ext({}),
      ext({
        name: "kitchen", source: "spec_kit", trust: "changed", digest: "sha256:41a5",
        changes: { files: ["dev.speccoder/extension.toml"], widened: ["reaches network — gate before tick: runs ./check.sh — network on"] },
      }),
    ]));
    shown = mount(Extensions, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("gate before tick: runs ./scripts/check.sh — no network");
    expect(text).toContain("Agent Plugins");
    expect(text).toContain("Spec Kit");
    expect(text).toContain("Changed — trust it again");
    const changed = document.querySelector(".changed")!;
    expect(changed.querySelector(".wide")!.textContent).toContain("reaches network");
    expect(changed.textContent).toContain("dev.speccoder/extension.toml");
    expect(document.querySelectorAll("button.primary").length).toBe(1);
    button("Trust…").click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "extension_trust", name: "kitchen", digest: "sha256:41a5" } }));
    expect(await violations()).toEqual([]);
  });

  it("names why one cannot be loaded, and offers no trust", async () => {
    host.get.mockResolvedValue(reply([ext({ trust: "untrusted", problems: ["mcp.json server `web`: `streamable-http` servers are not supported by this version (stdio only)"] })]));
    shown = mount(Extensions, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("Cannot be loaded");
    expect(document.body.textContent).toContain("not supported by this version");
    expect(document.querySelector("button.primary")).toBeNull();
    expect(await violations()).toEqual([]);
  });

  it("shows how to add one when there are none", async () => {
    host.get.mockResolvedValue(reply([]));
    shown = mount(Extensions, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("No extensions yet.");
    document.querySelector<HTMLButtonElement>(".add button")!.click();
    expect(host.app.copy).toHaveBeenCalledWith("speccoder extensions add ./path/to/extension", "Copied", expect.any(String));
    expect(await violations()).toEqual([]);
  });
});
