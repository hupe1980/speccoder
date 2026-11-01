import { describe, expect, it } from "vitest";
import { judgedChip, judgedWords, storyLine } from "./story";

describe("story words", () => {
  it("says what judges a task", () => {
    expect(judgedWords({ kind: "progress" })).toBe("the scenarios it turns green on its story's oracle");
    expect(judgedWords({ kind: "linked", scenarios: ["US3-1"] })).toBe("its scenarios US3-1 — green after it");
    expect(judgedWords({ kind: "checkpoint" })).toBe("its story's checkpoint — every scenario green after it");
    expect(judgedChip({ kind: "progress" })).toBeNull();
    expect(judgedChip({ kind: "linked", scenarios: ["US3-1", "US3-2"] })).toBe("US3-1 US3-2");
    expect(judgedChip({ kind: "checkpoint" })).toBe("closes the story");
  });

  it("says how far a story is on its oracle, and who turned what", () => {
    const oracle = { commit: "abc1234", decided_by: "speccoder run" };
    expect(
      storyLine({
        story: "US1",
        oracle,
        checkpoint: "T010",
        scenarios: [
          { id: "US1-1", state: "green", turned_by: "T008" },
          { id: "US1-2", state: "green", turned_by: "T008" },
          { id: "US1-3", state: "red", contract: "two tables → an error" },
        ],
      }),
    ).toBe("2 of 3 green · T008 turned US1-1, US1-2 · T010 closes it");
    expect(storyLine({ story: "US1", oracle, scenarios: [{ id: "US1-1", state: "green" }] })).toBe("1 of 1 green");
    expect(storyLine({ story: "US2", scenarios: [{ id: "US2-1", state: "red" }] })).toBe("0 of 1 green · its oracle is written at its first code task");
    expect(storyLine({ story: "US3", before_oracle: true, scenarios: [{ id: "US3-1", state: "red" }] })).toBe("ticked before its oracle: 1 scenario(s) never run");
  });
});
