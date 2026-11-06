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
    expect(card?.querySelector("button[data-check-story]")).toBeNull();
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

  it("makes each Kiro requirement a card and each criterion its state, by N.M", () => {
    const KIRO = `<h3>Requirement 1: Automatic labels</h3>
<p><strong>User Story:</strong> As a maintainer, I want labels.</p>
<h4>Acceptance Criteria</h4>
<ol><li>WHEN a new issue mentions crash, THE Triage_Bot SHALL add the label bug</li>
<li>IF no rule matches, THEN THE Triage_Bot SHALL add needs-triage</li></ol>
<h3>Requirement 2: Audit log</h3>`;
    const div = document.createElement("div");
    div.innerHTML = living(KIRO, [
      {
        story: "Requirement 1: Automatic labels",
        oracle: { commit: "abc1234", decided_by: "speccoder run" },
        scenarios: [
          { id: "1.1", state: "green", turned_by: "2.1" },
          { id: "1.2", state: "red" },
        ],
      },
    ]);
    const [card, second] = Array.from(div.querySelectorAll("section.story"));
    expect(card?.getAttribute("aria-label")).toBe("Requirement 1: Automatic labels");
    expect(card?.querySelector(".strip")?.textContent).toBe("1 of 2 green");
    const [one, two] = Array.from(card?.querySelectorAll("li.scenario") ?? []);
    expect(one?.querySelector(".sid")?.textContent).toBe("1.1");
    expect(one?.querySelector(".sb")?.textContent).toBe("✓ 2.1");
    expect(two?.className).toContain("red");
    expect(second?.querySelector(".strip")).toBeNull();
    expect(Array.from(div.querySelectorAll(".kw")).map((k) => k.textContent)).toEqual(["SHALL", "SHALL"]);
  });

  it("says a story's scenarios have no test before its oracle is written", () => {
    const div = document.createElement("div");
    div.innerHTML = living(SPEC, [{ story: "US1", scenarios: [{ id: "US1-1", state: "red" }, { id: "US1-2", state: "red" }] }]);
    expect(div.querySelector(".strip")?.textContent).toBe("oracle not yet written");
    expect(div.querySelector("li.scenario .sb")?.textContent).toBe("no test yet");
  });

  it("says a story ticked before its oracle never ran its scenarios, without red", () => {
    const div = document.createElement("div");
    div.innerHTML = living(SPEC, [{ story: "US1", before_oracle: true, scenarios: [{ id: "US1-1", state: "red" }, { id: "US1-2", state: "red" }] }]);
    expect(div.querySelector(".strip")?.textContent).toBe("ticked before its oracle");
    expect(div.querySelectorAll(".strip .d").length).toBe(0);
    expect(div.querySelector("li.scenario")?.className).toContain("before");
    expect(div.querySelector("li.scenario .sb")?.textContent).toBe("not run");
    expect(div.querySelector<HTMLButtonElement>("button[data-check-story]")?.dataset.checkStory).toBe("US1");
  });
});
