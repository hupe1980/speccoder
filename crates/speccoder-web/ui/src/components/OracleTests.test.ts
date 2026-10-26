// A requirement's Tests card as a person meets it: the tests with their
// red pills, the held-out count and never more of it, the readings that
// disagree, and the actions bound to the requirement and the test.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { oracleView } from "../test/fixtures";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    oracleStage: null as { requirement: string; stage: string; n: number; of: number } | null,
    perform: vi.fn(async (_offer: unknown, _input?: string) => {}),
    toast: vi.fn(),
    doc: "review",
    view: "documents",
  },
  add: vi.fn(async (_a: unknown, _t: string) => {}),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/comments.svelte", () => ({ comments: { add: host.add } }));

import OracleTests from "./OracleTests.svelte";

let card: ReturnType<typeof mount> | null = null;
function show(v = oracleView()) {
  card = mount(OracleTests, { target: document.body, props: { v } });
  flushSync();
}
function button(name: string): HTMLButtonElement {
  const b = [...document.querySelectorAll("button")].find((x) => x.textContent?.trim().startsWith(name));
  if (!b) throw new Error(`no button "${name}" in ${document.body.innerHTML}`);
  return b;
}

beforeEach(() => {
  host.app.job = null;
  host.app.oracleStage = null;
  host.app.perform.mockClear();
  host.add.mockClear();
});
afterEach(() => {
  if (card) unmount(card);
  card = null;
  document.body.innerHTML = "";
});

describe("OracleTests", () => {
  it("offers Write Tests with the calls it makes, before any test is written", () => {
    show(oracleView({ tests: [], state: "none", calls: 1, ambiguity_check: false }));
    expect(document.body.textContent).toContain("No tests yet.");
    expect(document.body.textContent).toContain("1 author call · ambiguity check off");
    button("Write Tests").click();
    const [o] = host.app.perform.mock.calls[0]!;
    expect((o as { run: unknown }).run).toEqual({ kind: "oracle_tests", req: "US1", op: "write", path: null, scenario: null, reading: null });
    expect((o as { command: string }).command).toBe("speccoder review tests US1 --write");
  });

  it("shows red pills, the held-out count and the coverage — never a held-out path", () => {
    show();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Held out: 2");
    expect(text).toContain("Strength target: 80%");
    expect(text).toContain("Covers 1 of 2 scenarios");
    expect(text).toContain("red: assertion — evidence");
    expect(text).toContain("already passes — no evidence");
    expect(text).toContain("1 discarded");
    expect(document.body.innerHTML).not.toContain("held_out");
  });

  it("asks for a reason to reject, bound to the test", () => {
    show();
    button("tests/acceptance_us1.rs").click();
    flushSync();
    button("Reject…").click();
    const [o] = host.app.perform.mock.calls[0]!;
    expect((o as { run: unknown }).run).toMatchObject({ kind: "oracle_tests", op: "reject", path: "tests/acceptance_us1.rs" });
    expect((o as { input: string }).input).toBe("Why is this test wrong?");
  });

  it("names both readings and resolves one, and a running job disables it", () => {
    const a = { scenario: "US1-2", a: { given: "\"\"", expect: "Hello!" }, b: { given: "\"\"", expect: "an error" }, resolution: null };
    show(oracleView({ state: "ambiguous", ambiguities: [a] }));
    expect(document.body.textContent).toContain("Two readings of US1-2.");
    button("B Is Meant").click();
    const [o] = host.app.perform.mock.calls[0]!;
    expect((o as { run: unknown }).run).toMatchObject({ op: "reading", scenario: "US1-2", reading: "B" });
  });

  it("puts a test line's comment in the tray", async () => {
    show();
    button("tests/acceptance_us1.rs").click();
    flushSync();
    (document.querySelector('button[aria-label="Comment on line 3 of tests/acceptance_us1.rs"]') as HTMLButtonElement).click();
    flushSync();
    const t = document.querySelector("textarea") as HTMLTextAreaElement;
    t.value = "assert the message too";
    t.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    button("Add Comment").click();
    for (let i = 0; i < 5; i++) await Promise.resolve();
    expect(host.add).toHaveBeenCalledWith({ kind: "test_line", req: "US1", path: "tests/acceptance_us1.rs", line: 3, quote: "    assert!(app::login());" }, "assert the message too");
  });

  it("says what a frozen and a lapsed oracle mean", () => {
    show(oracleView({ state: "lapsed", frozen_on: "Login\nold words", frozen_by: "Ada" }));
    expect(document.body.textContent).toContain("Lapsed");
    expect(document.body.textContent).toContain("old words");
  });

  it("is read-only when frozen: no Reject, Ask Again, Edit, Write Again or line comments", () => {
    show(oracleView({ state: "frozen", frozen_by: "Ada", frozen_at: 1793000000 }));
    expect(document.body.textContent).toContain("Frozen by Ada");
    expect(document.body.textContent).toContain("these are the tests the run uses");
    button("tests/acceptance_us1.rs").click();
    flushSync();
    const names = [...document.querySelectorAll("button")].map((b) => b.textContent?.trim() ?? "");
    for (const gone of ["Reject…", "Ask Again…", "Edit", "Write Again", "+"]) expect(names).not.toContain(gone);
  });
});
