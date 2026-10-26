import { describe, expect, it } from "vitest";
import { approveCommand, calls, coverage, discardWord, freezes, grouped, locate, nudgeLine, passage, place, sheetBlocked, stageLabel, testsCommand, touched } from "./review";
import type { CommentDraft } from "./schema.gen";
import { oracleView, review } from "../test/fixtures";

describe("freezes", () => {
  it("says what approval freezes, as the host does", () => {
    expect(freezes(oracleView())).toBe("1 test · 2 held out · target 80%");
    expect(freezes(oracleView({ strength_target: null, held_out: { state: "off", count: 0, manifest_sha256: null, reason: null } }))).toBe("1 test");
    expect(freezes(oracleView({ tests: [] }))).toBe("tests written at run time");
    expect(freezes(oracleView({ state: "frozen" }))).toBe("1 test frozen");
  });

  it("is blocked by a reading still open", () => {
    const a = { scenario: "US1-2", a: { given: "\"\"", expect: "Hello!" }, b: { given: "\"\"", expect: "an error" }, resolution: null };
    expect(freezes(oracleView({ ambiguities: [a] }))).toBe("blocked: two readings of US1-2");
    expect(freezes(oracleView({ ambiguities: [{ ...a, resolution: { reading: "A", by: "Ada", at: 1 } }] }))).toBe("1 test · 2 held out · target 80%");
  });
});

describe("the card's words", () => {
  it("counts calls, coverage and stages", () => {
    expect(calls(oracleView())).toBe("2 author calls");
    expect(calls(oracleView({ calls: 1 }))).toBe("1 author call · ambiguity check off");
    expect(coverage(oracleView())).toBe("Covers 1 of 2 scenarios");
    expect(stageLabel("writing", 2, 2)).toBe("Writing (2/2)");
    expect(stageLabel("writing", 1, 1)).toBe("Writing");
    expect(stageLabel("checking_red", 1, 1)).toBe("Checking red");
    expect(testsCommand("US1", "reading", "B", "US1-2")).toBe("speccoder review tests US1 --reading B US1-2");
    expect(testsCommand("US1", "write")).toBe("speccoder review tests US1 --write");
  });
});

describe("the approve sheet", () => {
  it("is enabled only when nothing blocks the chosen run", () => {
    const sheet = review().sheet;
    expect(sheetBlocked(sheet, "ask", false)).toEqual([]);
    expect(sheetBlocked(sheet, "ask", true)).toEqual(["a run is active"]);
    expect(sheetBlocked({ ...sheet!, autopilot_blocked: "autopilot needs the OS sandbox" }, "autopilot", false)).toEqual(["autopilot needs the OS sandbox"]);
    expect(sheetBlocked({ ...sheet!, blocked: ["US1: two readings of US1-2 disagree"] }, "refine", true)).toEqual([]);
    // Without the digest the click could not bind to what is shown.
    expect(sheetBlocked({ ...sheet!, sha256: null }, "ask", false)).toEqual(["the sheet cannot say what it would freeze: open it again"]);
    expect(approveCommand("autopilot", ["g-1", "g-2"])).toBe("speccoder review approve --run autopilot --grant g-1 --grant g-2");
    expect(approveCommand("ask", [])).toBe("speccoder review approve --run ask");
  });
});

const draft = (id: string, anchor: CommentDraft["anchor"]): CommentDraft => ({ id, anchor, text: "x", by: "Ada", at: 0, state: "draft", kept: false });

describe("the tray", () => {
  it("groups drafts by section and names their places", () => {
    const a = draft("c1", { kind: "passage", path: "specs/001/spec.md", start_line: 40, end_line: 42, quote: "q", quote_sha256: "", section: "Requirements" });
    const b = draft("c2", { kind: "test_line", req: "US1", path: "tests/a.rs", line: 3, quote: "q" });
    const c = draft("c3", { kind: "passage", path: "specs/001/spec.md", start_line: 50, end_line: 50, quote: "q", quote_sha256: "", section: "Requirements" });
    expect(grouped([a, b, c]).map((g) => [g.section, g.drafts.map((d) => d.id)])).toEqual([["Requirements", ["c1", "c3"]], ["Tests of US1", ["c2"]]]);
    expect(place(a.anchor)).toBe("specs/001/spec.md:40-42");
    expect(place(c.anchor)).toBe("specs/001/spec.md:50");
    expect(place(b.anchor)).toBe("US1 tests/a.rs:3");
    expect(touched([a, b, c])).toEqual(["Requirements"]);
  });

  it("writes a nudge line as the host does", () => {
    expect(nudgeLine({ path: "src/a.rs", line: 12, quote: "  let x = 1;", text: " use a constant " })).toBe('src/a.rs:12 "let x = 1;" — use a constant');
  });
});

describe("locate", () => {
  const md = "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Sessions MUST\n  expire after `30` minutes.\n";
  it("finds a rendered selection in the Markdown, marks and wraps aside", () => {
    expect(locate(md, "Users MUST log in.")).toEqual({ start: 5, end: 5, quote: "- **FR-001**: Users MUST log in." });
    expect(locate(md, "Sessions MUST expire after 30")).toEqual({ start: 6, end: 7, quote: "- **FR-002**: Sessions MUST\n  expire after `30` minutes." });
    expect(locate(md, "not in it")).toBeNull();
    expect(locate(md, "   ")).toBeNull();
    expect(passage("specs/001/spec.md", md, "FR-001: Users")).toMatchObject({ kind: "passage", start_line: 5, end_line: 5 });
  });
});

describe("discardWord", () => {
  it("says why a test was not kept, in words", () => {
    expect(discardWord("cites_no_scenario", "US1")).toBe("cites no scenario of US1");
    expect(discardWord("something_else", "US1")).toBe("something else");
  });
});
