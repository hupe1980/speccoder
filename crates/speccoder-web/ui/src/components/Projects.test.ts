// Projects: every project by where it stands, what waits across them,
// one click to each — nothing decided here; unknown is never 0.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const report = {
  projects: [
    { name: "api", path: "/p/api", shown: "~/p/api", state: "blocked_on_you", current: false, feature: "001-auth", waiting: 1 },
    { name: "web", path: "/p/web", shown: "~/p/web", state: "working", current: true, feature: "002-orders", waiting: 0, live: { task: "T004", stage: "verify" } },
    { name: "lib", path: "/p/lib", shown: "~/p/lib", state: "cant_tell", current: false, unknown: "held by process 4711" },
    { name: "old", path: "/p/old", shown: "~/p/old", state: "missing", current: false, unknown: "folder not found" },
  ],
  items: [
    { project: "api", path: "/p/api", feature: "001-auth", task: "T005", kind: "approval", group: "blocked_on_you", title: "T005 waits for your approval", since: Math.floor(Date.now() / 1000) - 720 },
    { project: "web", path: "/p/web", feature: "002-orders", task: "T009", kind: "stuck", group: "blocked_on_you", title: "T009 did not pass" },
  ],
};
const host = vi.hoisted(() => ({
  app: {
    job: null,
    projects: null as unknown,
    project: { window: false },
    view: "projects",
    loadProjects: vi.fn(async () => {}),
    openProject: vi.fn(async () => {}),
    selectFeature: vi.fn(async () => {}),
    selectTask: vi.fn(async () => {}),
  },
  post: vi.fn(async () => ({})),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), post: host.post }));

import Projects from "./Projects.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("Projects", () => {
  it("groups by state, merges what waits, and opens another project only in its own host", async () => {
    host.app.projects = report;
    shown = mount(Projects, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    for (const s of ["Blocked on You", "Working", "Can't Tell", "Gone", "12 min", "T004 · verify", "? waiting — held by process 4711", "folder not found"]) expect(text).toContain(s);
    expect(text).not.toMatch(/\b0 waiting/);
    button(/Decide in api/).click();
    expect(host.app.openProject).toHaveBeenCalledWith("/p/api", "api", "inbox:001-auth/T005");
    button(/Decide Here/).click();
    expect(host.app.selectFeature).toHaveBeenCalledWith("002-orders");
    // No decision anywhere on the page.
    expect([...document.querySelectorAll("button")].map((b) => b.textContent?.trim())).not.toContain("Approve");
    button("Forget").click();
    expect(host.post).toHaveBeenCalledWith("/api/recent/forget", { path: "/p/old" });
    expect(await violations()).toEqual([]);
  });

  it("says how to add a project when there is none", async () => {
    host.app.projects = { projects: [], items: [] };
    shown = mount(Projects, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("No projects yet");
  });
});
