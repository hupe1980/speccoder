// The Bug sheet as a person meets it: Record stays disabled until there is
// an expected behaviour, a confirmation is bound to the digest shown, and a
// reopened sheet (a reload) resumes from the host's state.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { bugReport } from "../test/fixtures";

const host = vi.hoisted(() => ({
  app: {
    bugSheet: null as { key?: string; symptom?: string; issue?: number } | null,
    job: null as { running: boolean; result: unknown } | null,
    perform: vi.fn(async (_offer: unknown) => {}),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import BugSheet from "./BugSheet.svelte";

let sheet: ReturnType<typeof mount> | null = null;

function open() {
  sheet = mount(BugSheet, { target: document.body });
  flushSync();
}

const settle = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  flushSync();
};

function button(name: string): HTMLButtonElement {
  const b = [...document.querySelectorAll("button")].find((x) => x.textContent?.trim() === name);
  if (!b) throw new Error(`no button "${name}" in ${document.body.innerHTML}`);
  return b;
}

function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

beforeEach(() => {
  host.app.bugSheet = null;
  host.app.job = null;
  host.app.perform.mockClear();
  host.get.mockReset();
});

afterEach(() => {
  if (sheet) unmount(sheet);
  sheet = null;
  document.body.innerHTML = "";
});

describe("BugSheet", () => {
  it("keeps Record disabled until an expected behaviour is given", async () => {
    host.app.bugSheet = { symptom: "Saving an empty title crashes" };
    open();
    expect(button("Record").disabled).toBe(true);
    expect(button("Record and Reproduce").disabled).toBe(true);
    expect(document.body.textContent).toContain("Say what should happen instead");

    // ⌘↩ records nothing either.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", metaKey: true }));
    await settle();
    expect(host.app.perform).not.toHaveBeenCalled();

    type(document.querySelector<HTMLInputElement>('input[aria-label="Expected, 1"]')!, "An empty title is refused with a message");
    expect(button("Record").disabled).toBe(false);
    button("Record and Reproduce").click();
    await settle();
    expect(host.app.perform).toHaveBeenCalledTimes(1);
    const offered = host.app.perform.mock.calls[0]?.[0] as { run: Record<string, unknown>; command: string };
    expect(offered.run).toMatchObject({
      kind: "bug_fix",
      symptom: "Saving an empty title crashes",
      expected: ["An empty title is refused with a message"],
      unchanged: [],
      run: true,
    });
    expect(offered.command).toBe('speccoder bug "Saving an empty title crashes" --expected "An empty title is refused with a message"');
  });

  it("is disabled while a job runs", () => {
    host.app.bugSheet = { symptom: "it crashes" };
    host.app.job = { running: true, result: null };
    open();
    type(document.querySelector<HTMLInputElement>('input[aria-label="Expected, 1"]')!, "it does not");
    expect(button("Record").disabled).toBe(true);
  });

  it("resumes a bug from the host's state and confirms the digest shown", async () => {
    const bug = bugReport();
    host.get.mockResolvedValue({ data: bug, message: "", status: "ok" });
    // A reload: the sheet reopens on the bug's key, nothing typed.
    host.app.bugSheet = { key: "001-bugfixes/T001" };
    open();
    await settle();
    expect(host.get).toHaveBeenCalledWith("/api/bug?task=001-bugfixes%2FT001");
    expect(document.querySelector("h2")?.textContent).toBe(bug.title);
    expect(document.body.textContent).toContain("an empty title is refused with a message");
    expect(document.body.textContent).toContain("tests/repro_e1.rs :: e1");
    // The exact command the person confirms.
    expect(document.body.textContent).toContain("$ cargo test --test repro_e1");
    expect(document.querySelector(".strip .now")?.textContent).toBe("Confirm");
    // No form to fill again.
    expect(document.querySelector('input[aria-label="Expected, 1"]')).toBeNull();

    button("Confirm and Fix").click();
    await settle();
    const confirm = host.app.perform.mock.calls[0]?.[0] as { run: Record<string, unknown>; command: string };
    expect(confirm.run).toEqual({ kind: "bug_confirm", task: "001-bugfixes/T001", digest: bug.confirm_digest, reject: false });
    expect(confirm.command).toBe(`speccoder bug confirm 001-bugfixes/T001 --digest ${bug.confirm_digest}`);

    button("Reject with Reason").click();
    await settle();
    const reject = host.app.perform.mock.calls[1]?.[0] as { run: Record<string, unknown>; input: string | null };
    expect(reject.run).toMatchObject({ kind: "bug_confirm", digest: bug.confirm_digest, reject: true });
    expect(reject.input).toBeTruthy();
  });

  it("offers no confirmation once the bug is past it", async () => {
    host.get.mockResolvedValue({ data: bugReport({ outcome: "cannot_reproduce", stage: "done", confirm_digest: null }), message: "", status: "ok" });
    host.app.bugSheet = { key: "001-bugfixes/T001" };
    open();
    await settle();
    expect(document.body.textContent).toContain("Could not be reproduced");
    expect([...document.querySelectorAll("button")].some((b) => b.textContent?.includes("Confirm"))).toBe(false);
  });
});
