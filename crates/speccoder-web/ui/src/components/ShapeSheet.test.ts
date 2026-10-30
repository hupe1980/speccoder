// Shape: the proposal inked by origin, every assumption and dropped
// sentence decided before the words can be used, and the decisions applied.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";

const host = vi.hoisted(() => {
  const SPANS = [
    { text: "The export MUST write every ", origin: "added" },
    { text: "row", origin: "yours" },
    { text: " within 2 seconds for up to 10 000 rows.", origin: "added" },
  ];
  return {
  accepted: [] as string[],
  app: { shape: null as unknown },
  post: vi.fn(async (path: string, _body: unknown): Promise<unknown> => {
    if (path === "/api/shape/judge") return { diff: { proposal: SPANS }, findings: [] };
    return {
      status: "ok",
      message: "shaped",
      data: {
        field: "requirement",
        format: "speckit",
        original: "export rows fast. never delete the source file.",
        cut: false,
        proposal: "The export MUST write every row within 2 seconds for up to 10 000 rows.",
        diff: {
          proposal: SPANS,
          original: [
            { text: "export rows ", origin: "yours" },
            { text: "fast. never delete the source file.", origin: "dropped" },
          ],
        },
        assumptions: [{ words: "up to 10 000 rows", from: "fast", question: "How many rows at most?" }],
        unexplained: ["2 seconds"],
        coverage: [
          { sentence: "export rows fast.", lines: [0], dropped: false },
          { sentence: "never delete the source file.", lines: [], dropped: true },
        ],
        questions: [],
        findings: [],
        findings_before: [],
        run_id: "run_1",
        usage: null,
      },
    };
  }),
  };
});
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), post: host.post }));

import ShapeSheet from "./ShapeSheet.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

const button = (label: string, within?: Element | null) =>
  [...(within ?? document).querySelectorAll("button")].find((b) => b.textContent?.trim() === label) as HTMLButtonElement;

describe("ShapeSheet", () => {
  it("waits for every decision, then uses the proposal as decided", async () => {
    host.accepted = [];
    host.app.shape = { field: "requirement", text: "export rows fast. never delete the source file.", accept: (t: string) => host.accepted.push(t) };
    shown = mount(ShapeSheet, { target: document.body });
    await vi.waitFor(() => expect(document.querySelector(".decide")).not.toBeNull());
    flushSync();
    // Added words are tinted, dropped ones struck — computed by code.
    expect([...document.querySelectorAll(".proposed .added")].map((s) => s.textContent)).toContain(" within 2 seconds for up to 10 000 rows.");
    expect(document.querySelector(".mine .dropped")?.textContent).toBe("fast. never delete the source file.");
    expect(button("3 to Decide").disabled).toBe(true);

    const pills = [...document.querySelectorAll(".pill")];
    button("Ask instead", pills[0]).click(); // the assumption becomes a question
    button("Remove", pills[1]).click(); // the unexplained "2 seconds" goes
    button("Add it back", pills[2]).click(); // the dropped sentence comes back
    flushSync();
    await tick();
    // Editing takes the decided text as it is: nothing is applied twice,
    // and what the person types is kept after Done.
    button("Edit").click();
    flushSync();
    const area = document.querySelector("textarea") as HTMLTextAreaElement;
    area.value = `${area.value} Mine.`;
    area.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    button("Done").click();
    flushSync();
    await tick();
    const use = button("Use It");
    expect(use.disabled).toBe(false);
    use.click();
    expect(host.accepted).toEqual([
      "The export MUST write every row for [NEEDS CLARIFICATION: How many rows at most?].\nnever delete the source file. Mine.",
    ]);
  });
});
