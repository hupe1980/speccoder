// The sidebar's project switcher: the recent ones with what waits, ? when
// unknown; All Projects…; choosing one opens it in its own host.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    project: { name: "web", window: true },
    view: "board",
    projects: {
      projects: [
        { name: "web", path: "/p/web", shown: "", state: "idle", current: true, waiting: 0 },
        { name: "api", path: "/p/api", shown: "", state: "blocked_on_you", current: false, waiting: 2 },
        { name: "lib", path: "/p/lib", shown: "", state: "cant_tell", current: false, unknown: "held" },
      ],
      items: [],
    },
    loadProjects: vi.fn(async () => {}),
    openProject: vi.fn(async () => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import ProjectSwitch from "./ProjectSwitch.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

describe("ProjectSwitch", () => {
  it("lists the projects with their counts and opens another in its own host", async () => {
    shown = mount(ProjectSwitch, { target: document.body });
    await settle();
    expect(document.querySelector(".badge")?.textContent).toBe("2+");
    button(/web/).click();
    flushSync();
    const items = [...document.querySelectorAll('[role="menuitemradio"]')];
    expect(items.map((b) => b.textContent?.trim())).toEqual(["web", "api 2", "lib ?"]);
    expect(items[0]!.getAttribute("aria-checked")).toBe("true");
    expect(await violations()).toEqual([]);
    (items[1] as HTMLButtonElement).click();
    expect(host.app.openProject).toHaveBeenCalledWith("/p/api", "api");
  });

  it("opens All Projects", async () => {
    shown = mount(ProjectSwitch, { target: document.body });
    await settle();
    button(/web/).click();
    flushSync();
    button(/All Projects/).click();
    expect(host.app.view).toBe("projects");
  });
});
