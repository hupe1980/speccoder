// The comment tray after a revision, as a person meets it: each changed
// section word by word, the ones outside the comments said so, what changed
// beside the revision named — and a tray that cannot be read said, never
// shown as empty.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: { feature: null, job: null as { running: boolean } | null, report: null, toast: vi.fn() },
  comments: {
    view: null as unknown,
    open: true,
    error: null as string | null,
    load: vi.fn(async () => {}),
    keep: vi.fn(),
    undo: vi.fn(),
    send: vi.fn(),
    drop: vi.fn(),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/comments.svelte", () => ({ comments: host.comments }));

import CommentTray from "./CommentTray.svelte";

let tray: ReturnType<typeof mount> | null = null;
const show = () => {
  tray = mount(CommentTray, { target: document.body });
  flushSync();
};
const turn = {
  id: "t1",
  comments: ["c1"],
  sections: ["specs/001/spec.md: Spec › Requirements"],
  skill: "speckit-clarify",
  changed: ["specs/001/spec.md: Spec › Requirements", "specs/001/spec.md: Spec › Edge Cases"],
  outside: ["specs/001/spec.md: Spec › Edge Cases"],
  outcome: "pending",
  files: ["specs/001/spec.md"],
  asked_again: [],
  beside: ["specs/001/plan.md"],
  diffs: [
    { path: "specs/001/spec.md", heading: "Spec › Requirements", before: "Users MUST log in.", after: "Users MUST sign in.", outside: false },
    { path: "specs/001/spec.md", heading: "Spec › Edge Cases", before: "none", after: "an empty name", outside: true },
  ],
};

beforeEach(() => {
  host.comments.view = { feature: "001", drafts: [], turn, revise_with: "clarify" };
  host.comments.error = null;
});
afterEach(() => {
  if (tray) unmount(tray);
  tray = null;
  document.body.innerHTML = "";
});

describe("CommentTray", () => {
  it("shows each changed section word by word, outside the comments said so", () => {
    show();
    const sections = [...document.querySelectorAll("section.sdiff")];
    expect(sections).toHaveLength(2);
    expect(sections[0]!.classList.contains("out")).toBe(false);
    expect(sections[1]!.classList.contains("out")).toBe(true);
    expect(sections[1]!.textContent).toContain("outside your comments");
    expect(sections[0]!.querySelector(".gone")?.textContent).toBe("log");
    expect(sections[0]!.querySelector(".new")?.textContent).toBe("sign");
    expect(document.body.textContent).toContain("Changed beside the revision while it ran (Undo will not overwrite it): specs/001/plan.md");
  });

  it("says a tray that cannot be read, never an empty one", () => {
    host.comments.view = null;
    host.comments.error = "the comment tray … cannot be read: expected value";
    show();
    expect(document.querySelector("[role=alert]")?.textContent).toContain("The comment tray cannot be read");
  });
});
