// Approve and Run as a person meets it: three choices, what is frozen per
// requirement, autopilot's grants unticked, and every blocker named.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { oracleView, review } from "../test/fixtures";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    view: "documents",
    perform: vi.fn(async (_offer: unknown) => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import ApproveSheet from "./ApproveSheet.svelte";

let sheet: ReturnType<typeof mount> | null = null;
const closed = vi.fn();
const refined = vi.fn();
function open(r = review()) {
  sheet = mount(ApproveSheet, { target: document.body, props: { review: r, onclose: closed, onrefine: refined } });
  flushSync();
}
const go = () => [...document.querySelectorAll("button")].find((b) => ["Approve and Run", "Keep Refining"].includes(b.textContent?.trim() ?? ""))!;
function press(key: string) {
  document.querySelector(".sheet")!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  flushSync();
}

beforeEach(() => {
  host.app.job = null;
  host.app.view = "documents";
  host.app.perform.mockClear();
  closed.mockClear();
  refined.mockClear();
});
afterEach(() => {
  if (sheet) unmount(sheet);
  sheet = null;
  document.body.innerHTML = "";
});

describe("ApproveSheet", () => {
  it("says what it approves and freezes, and runs asking by default", async () => {
    open();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Approves 1 decision line.");
    expect(text).toContain("1 test · 2 held out · target 80%");
    expect(text).toContain("$ speccoder review approve --run ask");
    go().click();
    for (let i = 0; i < 5; i++) await Promise.resolve();
    const [o] = host.app.perform.mock.calls[0]!;
    expect((o as { run: unknown }).run).toEqual({ kind: "approve_and_run", mode: "ask", grants: [] });
    expect(host.app.view).toBe("board");
  });

  it("holds only the grants ticked on autopilot (key 2)", async () => {
    open();
    press("2");
    const box = document.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(box.checked).toBe(false);
    box.click();
    flushSync();
    expect(document.body.textContent).toContain("--run autopilot --grant g-0011223344556677");
    expect(document.body.textContent).toContain("Sandbox: Seatbelt, network off");
    go().click();
    for (let i = 0; i < 5; i++) await Promise.resolve();
    const [o] = host.app.perform.mock.calls[0]!;
    expect((o as { run: unknown }).run).toEqual({ kind: "approve_and_run", mode: "autopilot", grants: ["g-0011223344556677"] });
  });

  it("is disabled naming what blocks it; Keep Refining records nothing", () => {
    const a = { scenario: "US1-2", a: { given: "x", expect: "y" }, b: { given: "x", expect: "z" }, resolution: null };
    const r = review({ oracle: [oracleView({ ambiguities: [a] })] });
    r.sheet = { ...r.sheet!, blocked: ["US1: two readings of US1-2 disagree — choose one, or reword"], autopilot_blocked: "autopilot needs the OS sandbox" };
    open(r);
    expect(go().disabled).toBe(true);
    expect(document.body.textContent).toContain("US1: two readings of US1-2 disagree");
    expect(document.body.textContent).toContain("blocked: two readings of US1-2");
    expect((document.querySelector('input[value="autopilot"]') as HTMLInputElement).disabled).toBe(true);
    press("3");
    expect(go().textContent?.trim()).toBe("Keep Refining");
    expect(go().disabled).toBe(false);
    go().click();
    expect(host.app.perform).not.toHaveBeenCalled();
    expect(refined).toHaveBeenCalled();
  });
});
