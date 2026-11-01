import { describe, expect, it } from "vitest";
import { living } from "./living";

const SPEC = `<h2 id="user-scenarios">User Scenarios</h2>
<h3 id="us1">User Story 1 - Count words (Priority: P1) 🎯 MVP</h3>
<p>A developer counts words.</p>
<p><strong>Why this priority</strong>: the one job.</p>
<p><strong>Independent Test</strong>: call <code>word_count</code>.</p>
<p><strong>Acceptance Scenarios</strong>:</p>
<ol><li><strong>Given</strong> <code>hello world</code>, <strong>When</strong> counted, <strong>Then</strong> it returns <code>2</code>.</li>
<li><strong>Given</strong> <code>a, b</code>, <strong>When</strong> counted, <strong>Then</strong> it returns <code>2</code>.</li></ol>
<h2 id="requirements">Requirements</h2>
<ul><li>FR-001: The library MUST count words; it MAY trim.</li></ul>`;

describe("the living spec", () => {
  it("makes each story a card and each scenario its steps with its state", () => {
    const div = document.createElement("div");
    div.innerHTML = living(SPEC, [
      {
        story: "US1",
        oracle: { commit: "abc1234", decided_by: "speccoder run" },
        scenarios: [
          { id: "US1-1", state: "green", turned_by: "T001", contract: "hello world → 2" },
          { id: "US1-2", state: "red" },
        ],
      },
    ]);
    const card = div.querySelector("section.story");
    expect(card?.querySelector("h3")?.id).toBe("us1");
    // The heading keeps its words: outline, links and comments find it.
    expect(card?.querySelector("h3")?.textContent).toBe("User Story 1 - Count words (Priority: P1) 🎯 MVP");
    expect(card?.querySelector(".pri")?.getAttribute("data-p")).toBe("P1");
    expect(card?.querySelector(".strip")?.textContent).toBe("1 of 2 green");
    const [one, two] = Array.from(card?.querySelectorAll("li.scenario") ?? []);
    expect(one?.className).toContain("green");
    expect(one?.querySelector(".sid")?.textContent).toBe("US1-1");
    expect(one?.querySelector(".sb")?.textContent).toBe("✓ T001");
    expect(Array.from(one?.querySelectorAll(".step .k") ?? []).map((k) => k.textContent)).toEqual(["Given", "When", "Then"]);
    expect(two?.className).toContain("red");
    expect(card?.querySelector("p.itest")).not.toBeNull();
    // The next section is not the story's.
    expect(div.querySelector("h2#requirements")?.closest("section.story")).toBeNull();
    expect(Array.from(div.querySelectorAll(".kw")).map((k) => k.textContent)).toEqual(["MUST", "MAY"]);
  });

  it("says a story's scenarios have no test before its oracle is written", () => {
    const div = document.createElement("div");
    div.innerHTML = living(SPEC, [{ story: "US1", scenarios: [{ id: "US1-1", state: "red" }, { id: "US1-2", state: "red" }] }]);
    expect(div.querySelector(".strip")?.textContent).toBe("oracle not yet written");
    expect(div.querySelector("li.scenario .sb")?.textContent).toBe("no test yet");
  });
});
