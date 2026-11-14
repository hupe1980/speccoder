// The attempt's thread: the model's words muted, calls one line, reads
// folded, evidence the only bold rows; a nudge for the next attempt.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    pulse: 0,
    elsewhere: false,
    live: null,
    explain: null as { task: string; call: string } | null,
    perform: vi.fn(async () => {}),
    open: vi.fn(async () => {}),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import Thread from "./Thread.svelte";

const thread = {
  feature: "001-csv",
  task: "T004",
  attempt: { run: "r2", outcome: "verified" },
  attempts: [{ run: "r1", outcome: "failed" }, { run: "r2", outcome: "verified" }],
  live: false,
  more: 0,
  rows: [
    { kind: "thought", text: "The parser splits on commas.\n\nSo quoting needs its own state.", conclusion: "So quoting needs its own state." },
    { kind: "fold", text: "Read 3 files · searched 1", items: [{ kind: "call", verb: "Read", object: "src/csv.rs", outcome: "ok" }, { kind: "call", verb: "Searched", object: "quote", outcome: "ok" }] },
    { kind: "call", verb: "Edited", object: "src/csv.rs", added: 12, removed: 3, outcome: "ok", detail: "+ fn quoted()" },
    { kind: "call", verb: "Ran", object: "cargo test", outcome: "ok", decided: "contained", detail: "exit 0 · 2.1 s" },
    { kind: "words", text: "Quoting handled." },
    { kind: "approval", verb: "Run", object: "curl https://crates.io", outcome: "waiting", call: "r2:7" },
    { kind: "divider", text: "US1-1 red → green", tone: "verified" },
    { kind: "divider", text: "Ticked T004 — verified", tone: "verified" },
  ],
};

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.perform.mockClear();
  host.app.explain = null;
});

describe("Thread", () => {
  it("shows each kind, folds reads, and makes evidence the only bold rows", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: thread });
    shown = mount(Thread, { target: document.body, props: { feature: "001-csv", task: "T004" } });
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("So quoting needs its own state.");
    expect(text).not.toContain("The parser splits on commas.");
    expect(text).toContain("src/csv.rs +12 −3");
    expect(text).toContain("contained");
    expect(document.querySelectorAll('[role="separator"]')).toHaveLength(2);
    expect([...document.querySelectorAll("option")].map((o) => o.textContent)).toEqual(["attempt 1 · failed", "attempt 2 · verified"]);
    // The fold opens to list its calls.
    button(/Read 3 files/).click();
    flushSync();
    expect(document.querySelector(".items")?.textContent).toContain("src/csv.rs");
    // The thought opens to its whole text.
    button(/So quoting/).click();
    flushSync();
    expect(document.body.textContent).toContain("The parser splits on commas.");
    expect(await violations()).toEqual([]);
  });

  it("explains a waiting call in place", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: thread });
    shown = mount(Thread, { target: document.body, props: { feature: "001-csv", task: "T004" } });
    await settle();
    button(/Explain/).click();
    expect(host.app.explain).toEqual({ task: "001-csv/T004", call: "r2:7" });
  });

  it("sends a nudge the next attempt reads", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: { ...thread, rows: [] } });
    shown = mount(Thread, { target: document.body, props: { feature: "001-csv", task: "T004" } });
    await settle();
    type(document.querySelector<HTMLInputElement>("input[aria-label='Nudge the next attempt']")!, "use the fixture");
    button("Nudge").click();
    expect(host.app.perform).toHaveBeenCalledWith(expect.objectContaining({ run: { kind: "nudge", task: "T004" } }), "use the fixture");
  });

  it("gathers rows that arrive while scrolled away behind a pill", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: thread });
    shown = mount(Thread, { target: document.body, props: { feature: "001-csv", task: "T004", live: true } });
    await settle();
    const box = document.querySelector<HTMLElement>(".rows")!;
    Object.defineProperty(box, "scrollHeight", { value: 1000, configurable: true });
    Object.defineProperty(box, "clientHeight", { value: 200, configurable: true });
    box.scrollTop = 100;
    box.dispatchEvent(new Event("scroll"));
    host.get.mockResolvedValue({ status: "ok", message: "", data: { ...thread, rows: [...thread.rows, { kind: "words", text: "one" }, { kind: "words", text: "two" }] } });
    const pick = document.querySelector<HTMLSelectElement>("select")!;
    pick.value = "r1";
    pick.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    expect(button(/2 new/).textContent).toBe("↓ 2 new");
    button(/2 new/).click();
    flushSync();
    expect(document.querySelector(".pill")).toBeNull();
  });
});
