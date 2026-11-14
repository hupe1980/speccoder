// The views extensions place on a feature: findings, metrics, a table, a
// document — each with when it ran and Run again; one that waits for trust
// says so and shows no data.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { settle, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    perform: vi.fn(async () => {}),
    view: "board",
    settingsSection: null as string | null,
    codeMode: "", codeFile: "", codeLine: 0,
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));
vi.mock("../lib/mermaid", () => ({ diagrams: () => ({ update() {} }) }));

import ExtensionViews from "./ExtensionViews.svelte";

const view = (name: string, widget: string, data: unknown, over: Record<string, unknown> = {}) => ({
  extension: "threatspec",
  view: { name, title: name.charAt(0).toUpperCase() + name.slice(1), source: { run: ["./check.sh"] }, widget, place: "feature", timeout: 60, exit: [0] },
  result: { inputs: "sha256:1", ran_at: new Date().toISOString(), duration_ms: 12, data },
  waits_for_trust: false,
  ...over,
});

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.perform.mockClear();
});

describe("ExtensionViews", () => {
  it("renders every widget with when it ran, and Run again goes through the dispatcher", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: { views: [
      view("findings", "findings", { widget: "findings", findings: [{ rule: "TS-C1", level: "error", message: "T-001 is unmitigated", file: "specs/001/spec.md", line: 3 }] }),
      view("posture", "metrics", { widget: "metrics", metrics: [["threats", "4"], ["critical", "1"]] }),
      view("requirements", "table", { widget: "table", columns: ["id", "status"], rows: [["SR-001", "verified"]] }),
      view("model", "markdown", { widget: "markdown", html: "<h1>Threats</h1>" }),
    ] } });
    shown = mount(ExtensionViews, { target: document.body, props: { feature: "001-login" } });
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("T-001 is unmitigated");
    expect(text).toContain("specs/001/spec.md:3");
    expect(text).toContain("critical");
    expect(text).toContain("SR-001");
    expect(document.querySelector(".prose h1")!.textContent).toBe("Threats");
    expect(text).toContain("just now");
    document.querySelector<HTMLButtonElement>('[aria-label="Run Findings again"]')!.click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "view_run", extension: "threatspec", view: "findings", feature: "001-login" } }));
    expect(await violations()).toEqual([]);
  });

  it("an extension that waits for trust shows no data and leads to its review", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: { views: [view("findings", "findings", null, { result: null, waits_for_trust: true })] } });
    shown = mount(ExtensionViews, { target: document.body, props: { feature: "001-login" } });
    await settle();
    expect(document.body.textContent).toContain("A view of threatspec waits for your trust.");
    expect(document.querySelector('[aria-label="Run Findings again"]')).toBeNull();
    document.querySelector<HTMLButtonElement>(".link")!.click();
    expect(host.app.view).toBe("settings");
    expect(host.app.settingsSection).toBe("extensions");
    expect(await violations()).toEqual([]);
  });

  it("an error is said in words, with no partial data", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: { views: [view("findings", "findings", null, { result: { inputs: "x", ran_at: new Date().toISOString(), duration_ms: 3, error: "it exited 3: no Python runtime with PyYAML found" } })] } });
    shown = mount(ExtensionViews, { target: document.body, props: { feature: "001-login" } });
    await settle();
    expect(document.body.textContent).toContain("it exited 3: no Python runtime with PyYAML found");
    expect(document.querySelector(".findings")).toBeNull();
  });
});
