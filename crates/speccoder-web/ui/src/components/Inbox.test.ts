// Needs You's diagnosis of a mostly unverified feature: the reasons by
// count, each task a button, and the actions with their commands.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    feature: "001-greet-function",
    view: "inbox",
    inbox: null as unknown,
    primary: null,
    quick: false,
    perform: vi.fn(async (_a: unknown) => {}),
    selectFeature: vi.fn(async (_f: string) => {}),
    selectTask: vi.fn(async (_t: string) => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import Inbox from "./Inbox.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

const action = (label: string, command: string, kind: string) => ({
  id: { kind, subject: null },
  group: "next",
  label,
  why: "w",
  command,
  blocked: null,
  input: null,
  run: null,
});

describe("Inbox", () => {
  it("lists the reasons of an unverified feature with their tasks as buttons", () => {
    host.app.inbox = {
      data: {
        features: 1,
        items: [
          {
            kind: "unverified",
            group: "ready_to_review",
            feature: "001-greet-function",
            task: null,
            title: "Little was verified in `001-greet-function`: 14 of 14 ticks are regression-only",
            detail: "",
            since: null,
            actions: [action("show the plan checks", "speccoder doctor", "doctor"), action("converge", "speccoder converge", "converge"), action("open the report", "speccoder report", "report")],
            unverified: {
              ticks: 14,
              regression_only: 14,
              counts: [
                { reason: "no_story", n: 7, tasks: ["T001", "T002"], text: "7 had no user story" },
                { reason: "already_passing", n: 7, tasks: ["T005"], implemented_by: ["T003"], text: "7 were already passing — implemented by T003" },
              ],
            },
          },
        ],
      },
    };
    shown = mount(Inbox, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Little Was Verified");
    expect(text).toContain("7 had no user story");
    expect(text).toContain("7 were already passing — implemented by T003");
    const chip = document.querySelector('button[aria-label="Open T005"]') as HTMLButtonElement;
    expect(chip).not.toBeNull();
    chip.click();
    flushSync();
    expect(host.app.selectTask).toHaveBeenCalledWith("T005");
    const titles = [...document.querySelectorAll("button")].map((b) => b.getAttribute("title") ?? "");
    expect(titles.some((t) => t.includes("$ speccoder converge"))).toBe(true);
    expect(document.querySelectorAll("ul.reasons li")).toHaveLength(2);
  });
});
