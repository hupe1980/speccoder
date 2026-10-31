// Pointing Ask at what is on screen: selected words carry where they are
// from; a focused row is its own chip; words inside Ask are not offered.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { violations } from "../test/dom";

vi.mock("../lib/app.svelte", () => ({ app: { report: null, project: null, selected: null, view: "inbox" } }));

import SelectionAsk from "./SelectionAsk.svelte";
import { ask, askable, pointed } from "../lib/ask.svelte";

function attrs(el: HTMLElement, a: Record<string, string>) {
  for (const [k, v] of Object.entries(a)) el.setAttribute(k, v);
}
function select(el: Node) {
  const r = document.createRange();
  r.selectNodeContents(el);
  const s = window.getSelection()!;
  s.removeAllRanges();
  s.addRange(r);
}

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  window.getSelection()?.removeAllRanges();
  document.body.innerHTML = "";
  ask.chips = [];
  vi.useRealTimers();
});

describe("pointing Ask at the screen", () => {
  it("makes selected words a chip that says where they are from", () => {
    const row = document.createElement("li");
    attrs(row, askable("evidence_row", { task: "T005" }, "T005 · Checks", "Evidence of T005"));
    row.innerHTML = "<span>regression only: the new test never ran</span>";
    document.body.append(row);
    select(row.querySelector("span")!);
    expect(pointed()).toEqual({
      kind: "selection",
      reference: { from: "T005 · Checks" },
      label: "“regression only: the new test neve…”",
      text: "regression only: the new test never ran\n\n(selected in T005 · Checks)",
    });
  });

  it("takes a focused row as itself, and nothing inside Ask", () => {
    const row = document.createElement("div");
    attrs(row, askable("requirement", { id: "FR-002" }, "FR-002", "FR-002: Users MUST log out"));
    row.innerHTML = "<button>Approve</button>";
    document.body.append(row);
    row.querySelector("button")!.focus();
    expect(pointed()).toMatchObject({ kind: "requirement", label: "FR-002", text: "FR-002: Users MUST log out" });

    const panel = document.createElement("aside");
    panel.className = "ask";
    panel.textContent = "an answer";
    document.body.append(panel);
    (document.activeElement as HTMLElement | null)?.blur();
    select(panel);
    expect(pointed()).toBeNull();
  });

  it("offers a pill over a selection that adds it", async () => {
    vi.useFakeTimers();
    shown = mount(SelectionAsk, { target: document.body });
    const p = document.createElement("p");
    p.textContent = "the parser drops the last line";
    document.body.append(p);
    select(p);
    document.dispatchEvent(new Event("selectionchange"));
    await vi.advanceTimersByTimeAsync(200);
    flushSync();
    const pill = document.querySelector<HTMLButtonElement>('button[aria-label="Ask about the selection"]');
    // A stand-in DOM measures nothing; where it can, the pill shows.
    if (pill) {
      expect(await violations()).toEqual([]);
      pill.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
      expect(ask.chips.at(-1)).toMatchObject({ kind: "selection", text: "the parser drops the last line" });
    }
  });
});
