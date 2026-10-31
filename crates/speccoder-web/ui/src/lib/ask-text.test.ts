// An answer's words while they stream: a citation marker is a placeholder
// until the turn's sources are checked, and a marker cut mid-way waits.
import { describe, expect, it } from "vitest";
import { blocks } from "./ask-text";

describe("blocks", () => {
  it("keeps markers as text once done, placeholders while live", () => {
    const done = blocks("in `a.rs` [r1:1-6].", []);
    expect(done[0]).toEqual({ kind: "p", parts: [{ t: "text", text: "in " }, { t: "code", text: "a.rs" }, { t: "text", text: " " }, { t: "text", text: "[r1:1-6]" }, { t: "text", text: "." }] });
    const live = blocks("in [r1:1-6] and [r2", [], true);
    expect(live[0]).toEqual({ kind: "p", parts: [{ t: "text", text: "in " }, { t: "mark" }, { t: "text", text: " and" }] });
  });
});
