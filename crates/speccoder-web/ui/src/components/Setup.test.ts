// The setup assistant's model step as a person meets it: three providers,
// Amazon Bedrock chosen when AWS credentials are the only ones found, its
// region prefilled and sent, and the command it runs shown.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    view: "setup",
    project: { name: "demo", root: "/p" } as Record<string, unknown>,
    actions: [] as unknown[],
    newProject: null as unknown,
    newFeature: null as unknown,
    refresh: vi.fn(async () => {}),
    toast: vi.fn(),
  },
  get: vi.fn(),
  post: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get, post: host.post }));

import Setup from "./Setup.svelte";

let c: ReturnType<typeof mount> | null = null;

function preview(keys: [string, boolean][], bedrock: { source: string | null; region: string | null }) {
  host.get.mockResolvedValue({
    doctor: { status: "ok", data: { checks: [{ check: "project", level: "pass", detail: "Spec Kit", fix: [] }] } },
    preview: { configured: false, checks: [], allow: [], formatter: null, keys, bedrock },
  });
}

async function open() {
  c = mount(Setup, { target: document.body });
  flushSync();
  await settle();
  button("Continue").click();
  flushSync();
}

const field = (name: string) => [...document.querySelectorAll("label")].find((l) => l.querySelector("span")?.textContent === name)!.querySelector("input")!;

beforeEach(() => {
  host.get.mockReset();
  host.post.mockReset();
});

afterEach(() => {
  if (c) unmount(c);
  c = null;
  document.body.innerHTML = "";
});

describe("Setup", () => {
  it("offers Amazon Bedrock and chooses it when AWS credentials are the only ones found", async () => {
    preview([["ANTHROPIC_API_KEY", false], ["OPENAI_API_KEY", false]], { source: "aws login session (profile dev)", region: "eu-central-1" });
    await open();
    const cards = [...document.querySelectorAll<HTMLButtonElement>("[role=radio]")];
    expect(cards.map((b) => b.querySelector(".pname")?.textContent)).toEqual(["Anthropic", "OpenAI", "Amazon Bedrock"]);
    const bedrock = cards[2]!;
    expect(bedrock.getAttribute("aria-checked")).toBe("true");
    expect(bedrock.textContent).toContain("Claude and other models through AWS");
    expect(bedrock.textContent).toContain("aws login session (profile dev)");
    expect(field("Region").value).toBe("eu-central-1");
    expect(await violations()).toEqual([]);
  });

  it("says how to get AWS credentials, and runs init with the region", async () => {
    preview([["ANTHROPIC_API_KEY", true], ["OPENAI_API_KEY", false]], { source: null, region: null });
    await open();
    button(/Amazon Bedrock/).click();
    flushSync();
    expect(document.body.textContent).toContain("AWS credentials not found");
    expect(document.body.textContent).toContain("Set AWS_BEARER_TOKEN_BEDROCK or run aws login where you start speccoder");
    type(field("Region"), "us-west-2");
    button("Continue").click();
    flushSync();
    button("Continue").click();
    flushSync();
    expect(document.body.textContent).toContain("speccoder init --provider bedrock --region us-west-2");
    host.post.mockResolvedValue({ status: "ok", message: "wrote", data: { ran: [], wrote: ["speccoder.toml"], committed: null }, to_commit: [] });
    button("Set Up").click();
    await settle();
    expect(host.post).toHaveBeenCalledWith("/api/setup", { provider: "bedrock", model: null, region: "us-west-2" });
  });
});
