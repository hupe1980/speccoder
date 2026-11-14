// Explain: a waiting call's facts, no model; Ask about it on request.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: { explain: { task: "001-graph/T005", call: "" } as { task: string; call: string } | null },
  ask: { add: vi.fn() },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/ask.svelte", () => ({ ask: host.ask }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import ExplainSheet from "./ExplainSheet.svelte";

const x = {
  task: "001-graph/T005", call: "r9:4", title: "Fetch the crate index", question: "Allow this command?",
  program: "curl", path: "/usr/bin/curl", hosts: ["crates.io"], outside: [], reach: "the network is off for commands",
  rule: "no rule allows it", provenance: ["contains text read from README.md (turn 3)"],
  said: "I need the index.", reasoning: "So fetch it.", scenarios: ["US1-2"],
};

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.explain = { task: "001-graph/T005", call: "" };
});

describe("ExplainSheet", () => {
  it("shows the facts, the model's words marked as its own, and asks about the call on request", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: x });
    shown = mount(ExplainSheet, { target: document.body });
    await settle();
    expect(host.get).toHaveBeenCalledWith("/api/approvals/explain?task=001-graph%2FT005");
    const text = document.body.textContent ?? "";
    for (const s of ["/usr/bin/curl", "crates.io", "no rule allows it", "README.md (turn 3)", "US1-2", "a claim, not evidence", "“I need the index.”", "no model was asked"]) expect(text).toContain(s);
    expect(text).not.toMatch(/recommend|confidence/i);
    expect(await violations()).toEqual([]);
    button(/Ask About This Call/).click();
    expect(host.ask.add).toHaveBeenCalledWith(expect.objectContaining({ kind: "call", reference: { task: "001-graph/T005", call: "r9:4" } }));
    expect(host.app.explain).toBeNull();
  });

  it("says why when the call no longer waits", async () => {
    host.get.mockResolvedValue({ status: "not_found", message: "001-graph/T005 has no call waiting for you", data: null });
    shown = mount(ExplainSheet, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("has no call waiting for you");
    expect(button(/Ask About This Call/).disabled).toBe(true);
  });
});
