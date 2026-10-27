// Settings › Model for its provider: Bedrock's region shown and required,
// where its credentials come from as `doctor` names them, and a compatible
// server's keys hidden; another provider has no region.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { settle, violations } from "../test/dom";
import type { ConfigKey, ConfigReport } from "../lib/types";

const host = vi.hoisted(() => ({
  app: {
    doctor: [] as { check: string; level: string; detail: string }[],
    job: null as unknown,
    perform: vi.fn(async (_a: unknown) => {}),
    refresh: vi.fn(async () => {}),
    toast: vi.fn(),
  },
  get: vi.fn(),
  post: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get, post: host.post }));

import Settings from "./Settings.svelte";

let c: ReturnType<typeof mount> | null = null;

const key = (k: string, kind: string, value: unknown = null, over: Partial<ConfigKey> = {}): ConfigKey => ({
  key: k, kind, value, about: `${k} about`, choices: [], default: null, max: null, min: null, required: false, ...over,
});

function config(provider: string) {
  const report: ConfigReport = {
    committed: true, diff: null, error: null, exists: true, file: "speccoder.toml", valid: true,
    sections: [{
      name: "model", title: "Model", about: "The model", named: false, tables: [],
      keys: [
        key("provider", "choice", provider, { required: true, choices: ["anthropic", "openai", "openai-compatible", "bedrock"] }),
        key("model", "string", "m", { required: true }),
        key("region", "string", provider === "bedrock" ? "eu-central-1" : null),
        key("base_url", "string"),
        key("api_key_env", "string"),
        key("structured", "choice", null, { choices: ["native", "tool"] }),
      ],
    }],
  };
  host.get.mockResolvedValue({ status: "ok", data: report });
}

const names = () => [...document.querySelectorAll(".row .name")].map((n) => n.firstChild?.textContent?.trim());

async function open() {
  c = mount(Settings, { target: document.body });
  flushSync();
  await settle();
}

beforeEach(() => {
  host.get.mockReset();
  host.app.doctor = [];
});

afterEach(() => {
  if (c) unmount(c);
  c = null;
  document.body.innerHTML = "";
});

describe("Settings › Model", () => {
  it("shows Bedrock's region and credentials, and hides a server's keys", async () => {
    host.app.doctor = [{ check: "model key", level: "pass", detail: "bedrock global.anthropic.claude-sonnet-5-5 · eu-central-1 · aws login session (profile dev)" }];
    config("bedrock");
    await open();
    expect(names()).toEqual(["Provider", "Model", "Region"]);
    const region = [...document.querySelectorAll(".row")].find((r) => r.querySelector(".name")?.textContent?.startsWith("Region"))!;
    expect(region.textContent).toContain("required");
    expect(document.body.textContent).toContain("Credentials: aws login session (profile dev)");
    expect(await violations()).toEqual([]);
  });

  it("names what is missing when no AWS credentials are found", async () => {
    host.app.doctor = [{ check: "model key", level: "fail", detail: "AWS_BEARER_TOKEN_BEDROCK is not set and no AWS profile was found" }];
    config("bedrock");
    await open();
    expect(document.body.textContent).toContain("Credentials: AWS_BEARER_TOKEN_BEDROCK is not set and no AWS profile was found");
  });

  it("has no region for another provider", async () => {
    host.app.doctor = [{ check: "model key", level: "pass", detail: "anthropic m" }];
    config("anthropic");
    await open();
    expect(names()).toEqual(["Provider", "Model", "Server URL", "API key variable", "Structured"]);
    expect(document.body.textContent).not.toContain("Credentials:");
  });
});
