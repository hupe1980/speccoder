// The composer as a person meets it: the host's suggestion selects a rung
// with its reason, each key opens that rung's own sheet with the text, a
// question selects nothing, an empty field says so, Esc closes.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { press, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    composer: {} as { text?: string } | null,
    project: { planning: { creates: "speckit-specify", toward: [], checks: null, revises: null }, features: { current: "001-slugify" } },
    feature: "001-slugify",
    bugSheet: null as unknown,
    quick: false,
    quickDraft: "",
    newFeature: null as unknown,
    toast: vi.fn(),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import Composer from "./Composer.svelte";

let c: ReturnType<typeof mount> | null = null;
const field = () => document.querySelector<HTMLTextAreaElement>("textarea")!;

function suggest(rung: string, available: boolean, reason: string) {
  host.get.mockResolvedValue({ status: "ok", data: { rung, available, cue: { rule: "x", matched: [] }, reason, alternatives: [], command: null } });
}

async function wait() {
  await vi.advanceTimersByTimeAsync(350);
  for (let i = 0; i < 6; i++) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  vi.useFakeTimers();
  Object.assign(host.app, { composer: {}, bugSheet: null, quick: false, quickDraft: "", newFeature: null });
  host.get.mockReset();
  c = mount(Composer, { target: document.body });
  flushSync();
});

afterEach(() => {
  if (c) unmount(c);
  c = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("Composer", () => {
  it("is a dialog with the four rungs, Just ask disabled until the Ask panel", async () => {
    const d = document.querySelector('[role="dialog"]')!;
    expect(d.getAttribute("aria-modal")).toBe("true");
    const rows = [...document.querySelectorAll('[role="radio"]')];
    expect(rows.map((r) => r.querySelector(".label")?.textContent)).toEqual(["Bug fix", "Quick change", "Feature", "Just ask"]);
    expect(rows[3]!.getAttribute("aria-disabled")).toBe("true");
    expect(rows.some((r) => r.getAttribute("aria-checked") === "true")).toBe(false);
    expect(await violations()).toEqual([]);
  });

  it("takes the host's suggestion with ⏎ and opens that rung's sheet with the text", async () => {
    suggest("bug", true, 'Why "Bug fix": the text names a wrong result ("is empty") of existing behaviour.');
    type(field(), 'the slug of "Ünïcödé" is empty');
    await wait();
    expect(host.get).toHaveBeenCalledWith(expect.stringContaining("/api/suggest?text="));
    expect(document.body.textContent).toContain('("is empty")');
    expect(document.querySelector('[aria-checked="true"] .label')?.textContent).toBe("Bug fix");
    expect(document.body.textContent).toContain('speccoder bug "the slug of \\"Ünïcödé\\" is empty"');
    press("Enter", {}, field());
    expect(host.app.composer).toBeNull();
    expect(host.app.bugSheet).toEqual({ symptom: 'the slug of "Ünïcödé" is empty' });
  });

  it("opens each rung with its own key, whatever is suggested", async () => {
    suggest("bug", true, "x");
    type(field(), "rename the Save button");
    await wait();
    press("Enter", { altKey: true }, field());
    expect(host.app.quick).toBe(true);
    expect(host.app.quickDraft).toBe("rename the Save button");
  });

  it("opens the New Feature sheet with ⇧⏎", async () => {
    suggest("quick", true, "x");
    type(field(), "export reports as CSV and remember the columns");
    await wait();
    press("Enter", { shiftKey: true }, field());
    expect(host.app.newFeature).toEqual({ text: "export reports as CSV and remember the columns" });
  });

  it("selects nothing for a question and takes nothing with ⌘⏎ while Ask is not built", async () => {
    suggest("ask", false, "This reads like a question; Ask arrives with the Ask panel — pick a rung to make it a change");
    type(field(), "why does the build take ten minutes?");
    await wait();
    expect(document.querySelector('[aria-checked="true"]')).toBeNull();
    expect(document.body.textContent).toContain("pick a rung");
    press("Enter", { metaKey: true }, field());
    press("Enter", {}, field());
    expect(host.app.composer).toEqual({});
  });

  it("says what is missing on an empty ⏎ and closes on Esc", async () => {
    press("Enter", {}, field());
    expect(field().getAttribute("aria-invalid")).toBe("true");
    expect(document.body.textContent).toContain("Describe the change first");
    press("Escape", {}, field());
    expect(host.app.composer).toBeNull();
  });

  it("moves the selection with the arrows", async () => {
    type(field(), "something");
    press("ArrowDown", {}, field());
    expect(document.querySelector('[aria-checked="true"] .label')?.textContent).toBe("Bug fix");
    press("ArrowDown", {}, field());
    expect(document.querySelector('[aria-checked="true"] .label')?.textContent).toBe("Quick change");
    press("ArrowUp", {}, field());
    press("ArrowUp", {}, field());
    expect(document.querySelector('[aria-checked="true"] .label')?.textContent).toBe("Feature");
  });
});
