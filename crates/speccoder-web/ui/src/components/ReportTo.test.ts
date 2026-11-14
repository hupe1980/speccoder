// Report to…: asks what should happen instead, then records the bug in
// the other project; or opens its sheet to edit first. Only other
// projects that still exist are offered.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    projects: {
      projects: [
        { name: "web", path: "/p/web", shown: "", state: "idle", current: true },
        { name: "api", path: "/p/api", shown: "", state: "idle", current: false },
        { name: "old", path: "/p/old", shown: "", state: "missing", current: false },
      ],
      items: [],
    },
    loadProjects: vi.fn(async () => {}),
    reportTo: vi.fn(async () => true),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import ReportTo from "./ReportTo.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.reportTo.mockClear();
});

describe("ReportTo", () => {
  it("asks what should happen instead and records the bug in the other project", async () => {
    shown = mount(ReportTo, { target: document.body, props: { feature: "002-orders", task: "T009" } });
    await settle();
    button(/Report to/).click();
    flushSync();
    expect([...document.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent?.trim())).toEqual(["api"]);
    button("api").click();
    flushSync();
    expect(button("Report to api").disabled).toBe(true);
    type(document.querySelector<HTMLInputElement>("#rt-expected")!, "every field the contract names");
    expect(await violations()).toEqual([]);
    button("Report to api").click();
    expect(host.app.reportTo).toHaveBeenCalledWith("/p/api", "api", "002-orders", "T009", "every field the contract names");
  });

  it("can open the other project's sheet to edit first", async () => {
    shown = mount(ReportTo, { target: document.body, props: { feature: "002-orders", task: "T009" } });
    await settle();
    button(/Report to/).click();
    flushSync();
    button("api").click();
    flushSync();
    button("Edit There First").click();
    expect(host.app.reportTo).toHaveBeenCalledWith("/p/api", "api", "002-orders", "T009");
  });

  it("closes on Escape before the page's own Escape and gives the focus back", async () => {
    const page = vi.fn();
    window.addEventListener("keydown", page);
    shown = mount(ReportTo, { target: document.body, props: { feature: "002-orders", task: "T009" } });
    await settle();
    const trigger = button(/Report to/);
    trigger.click();
    flushSync();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    flushSync();
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(page).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(trigger);
    window.removeEventListener("keydown", page);
  });
});
