import { describe, expect, it } from "vitest";
import { shareChip, shareWords, storyLine } from "./story";

describe("story words", () => {
  it("says what judges a task in every share", () => {
    expect(shareWords({ kind: "linked", scenarios: ["US3-1"] })).toBe("its scenarios US3-1");
    expect(shareWords({ kind: "checkpoint", scenarios: ["US3-1", "US3-2"], composition: true })).toBe("its story's checkpoint: US3-1, US3-2, and together");
    expect(shareWords({ kind: "checkpoint", scenarios: [], composition: false })).toBe("its story's checkpoint — every scenario already verified");
    expect(shareWords({ kind: "authored", scenarios: ["US3-1"], checkpoint: "T021" })).toBe("the test author's choice of US3-1 · T021 judges what it leaves");
    expect(shareWords({ kind: "deferred", checkpoint: "T021" })).toBe("its regression checks; its scenarios at T021");
    expect(shareChip({ kind: "authored", scenarios: ["US3-1"] })).toBeNull();
    expect(storyLine({ story: "US1", scenarios: [{ id: "US1-1", verified_by: "T1" }] })).toBe("1 of 1 verified");
    expect(storyLine({ story: "US2", scenarios: [{ id: "US2-1" }] })).toBe("0 of 1 verified · no task judges the rest");
  });
});
