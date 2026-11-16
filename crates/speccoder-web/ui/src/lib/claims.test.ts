import { describe, expect, it } from "vitest";
import { others } from "./claims";
import type { ClaimsReport } from "./types";

const claim = (story: string, holder: string, done?: string) => ({
  feature: "004-export", story, holder, email: `${holder.toLowerCase()}@x`, since: 1, until: 2, branch: "speccoder/004-export", ...(done ? { done } : {}),
});

describe("others", () => {
  it("names each other holder once, with their stories; never mine or a stale one", () => {
    const r = {
      feature: "004-export", live: false, at: 3,
      claims: [
        { story: "US1", state: "mine", claim: claim("US1", "Me") },
        { story: "US2", state: "theirs", claim: claim("US2", "Dana") },
        { story: "US3", state: "done", claim: claim("US3", "Dana", "abc") },
        { story: "US4", state: "stale", claim: claim("US4", "Lee") },
        { story: "US5", state: "unreadable" },
      ],
    } as ClaimsReport;
    expect(others(r)).toEqual(["Dana has US2, US3"]);
    expect(others(null)).toEqual([]);
  });
});
