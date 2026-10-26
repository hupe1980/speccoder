// The review's own logic, beside the host's payloads: what an
// approval freezes per requirement, when Approve and Run may run, the
// comment tray grouped by section, a selection found in its document's
// Markdown, and the Code view's nudge lines. Pure: the components render
// what these say, the tests hold them.

import type { ApproveSheet, CodeComment, CommentAnchor, CommentDraft, OracleView, ReviewChange, RevisionView, RunMode } from "./schema.gen";

/** What approving freezes for a requirement, as the sheet says it (the
 *  host's `OracleView::freezes`). */
export function freezes(v: OracleView): string {
  const red = v.tests.filter((t) => t.red).length;
  const open = v.ambiguities.find((a) => !a.resolution);
  if (open) return `blocked: two readings of ${open.scenario}`;
  if (v.state === "frozen") return `${red} ${red === 1 ? "test" : "tests"} frozen`;
  if (red === 0) return "tests written at run time";
  const parts = [`${red} ${red === 1 ? "test" : "tests"}`];
  if (v.held_out.state === "kept") parts.push(`${v.held_out.count} held out`);
  if (v.strength_target != null) parts.push(`target ${v.strength_target}%`);
  return parts.join(" · ");
}

/** Whether a requirement blocks the run choices: a reading still open. */
export const blocks = (v: OracleView): boolean => v.ambiguities.some((a) => !a.resolution);

/** How many scenarios red tests cover: `Covers 2 of 3 scenarios`. */
export function coverage(v: OracleView): string {
  const n = v.scenarios.length;
  const k = n - Math.min(n, v.uncovered.length);
  return `Covers ${k} of ${n} ${n === 1 ? "scenario" : "scenarios"}`;
}

/** The author calls Write Tests makes, as the card says it. */
export function calls(v: OracleView): string {
  return v.calls === 1 ? "1 author call · ambiguity check off" : `${v.calls} author calls`;
}

/** A stage of writing, as the strip says it: `Writing (1/2)`. */
export function stageLabel(stage: string, n: number, of: number): string {
  const name: Record<string, string> = { writing: "Writing", comparing: "Comparing", checking_red: "Checking red", held_out: "Held out" };
  const word = name[stage] ?? stage.replaceAll("_", " ");
  return stage === "writing" && of > 1 ? `${word} (${n}/${of})` : word;
}

/** The terminal's form of an oracle action (tooltips). */
export function testsCommand(req: string, op: "write" | "reject" | "again" | "edit" | "reading", arg?: string, scenario?: string): string {
  switch (op) {
    case "write": return `speccoder review tests ${req} --write`;
    case "reject": return `speccoder review tests ${req} --reject ${arg ?? "PATH"} --reason "…"`;
    case "again": return `speccoder review tests ${req} --again ${arg ?? "PATH"} --comment "…"`;
    case "edit": return `speccoder review tests ${req} --edit ${arg ?? "PATH"} --from FILE`;
    case "reading": return `speccoder review tests ${req} --reading ${arg ?? "A"} ${scenario ?? "SCENARIO"}`;
  }
}

/** Why Approve and Run cannot run in `mode` now (empty: it can). */
export function sheetBlocked(sheet: ApproveSheet | null, mode: RunMode | "refine", running: boolean): string[] {
  if (mode === "refine") return [];
  const out = [...(sheet?.blocked ?? [])];
  // The click binds to what the sheet shows: without its digest, nothing.
  if (!sheet?.sha256 && !out.length) out.push("the sheet cannot say what it would freeze: open it again");
  if (running && !out.some((b) => b.includes("run is active"))) out.push("a run is active");
  if (mode === "autopilot" && sheet?.autopilot_blocked) out.push(sheet.autopilot_blocked);
  return out;
}

/** The terminal's form of the sheet's choice. */
export function approveCommand(mode: RunMode | "refine", grants: string[]): string {
  if (mode === "refine") return "speccoder review comments";
  return `speccoder review approve --run ${mode}${grants.map((g) => ` --grant ${g}`).join("")}`;
}

/** Where an anchor is, in a few words (the host's `CommentAnchor::place`). */
export function place(a: CommentAnchor): string {
  switch (a.kind) {
    case "passage": return a.start_line === a.end_line ? `${a.path}:${a.start_line}` : `${a.path}:${a.start_line}-${a.end_line}`;
    case "diff_line": return `${a.path}:${a.line} (${a.side})`;
    case "test_line": return `${a.req} ${a.path}:${a.line}`;
    case "finding": return `${a.source} ${a.id}`;
  }
}

/** The words an anchor is about. */
export const quoteOf = (a: CommentAnchor): string => (a.kind === "finding" ? a.text : a.quote);

/** The section a draft is grouped under in the tray. */
export function sectionOf(a: CommentAnchor): string {
  switch (a.kind) {
    case "passage": return a.section || a.path.split("/").pop() || a.path;
    case "diff_line": return "Since the last commit";
    case "test_line": return `Tests of ${a.req}`;
    case "finding": return "Findings";
  }
}

/** The tray's drafts grouped by section, in the order first seen. */
export function grouped(drafts: CommentDraft[]): { section: string; drafts: CommentDraft[] }[] {
  const out: { section: string; drafts: CommentDraft[] }[] = [];
  for (const d of drafts) {
    const s = sectionOf(d.anchor);
    const g = out.find((x) => x.section === s);
    if (g) g.drafts.push(d);
    else out.push({ section: s, drafts: [d] });
  }
  return out;
}

/** The sections a send would revise (passages and changed lines; tests are
 *  asked again), each by its document and heading as the host keys them —
 *  the same heading in two documents is two sections. */
export function touched(drafts: CommentDraft[]): string[] {
  const key = (a: CommentAnchor) => (a.kind === "passage" ? `${a.path}: ${a.section || "(before the first heading)"}` : a.kind === "diff_line" ? `${a.path}:${a.line}` : "");
  return [...new Set(drafts.filter((d) => d.anchor.kind === "passage" || d.anchor.kind === "diff_line").map((d) => key(d.anchor)))];
}

/** A comment's anchor on a line changed since the last commit: the line as
 *  it reads now (or, removed, as it read), with its number on that side. */
export function driftAnchor(c: ReviewChange): CommentAnchor | null {
  if (c.now != null && c.now_line != null) return { kind: "diff_line", path: c.path, side: "new", line: c.now_line, quote: c.now };
  if (c.then != null && c.then_line != null) return { kind: "diff_line", path: c.path, side: "old", line: c.then_line, quote: c.then };
  return null;
}

/** The headings of document `path` a pending revision changed: inside the
 *  comments (`false`) or outside them (`true`), by the heading's own words. */
export function revisedHeadings(turn: RevisionView | null | undefined, path: string): Map<string, boolean> {
  const out = new Map<string, boolean>();
  if (!turn || turn.outcome !== "pending") return out;
  for (const d of turn.diffs) {
    if (d.path !== path || !d.heading) continue;
    const words = d.heading.split(" › ").pop() ?? d.heading;
    out.set(words, (out.get(words) ?? false) || d.outside);
  }
  return out;
}

/** One Code-view comment as the nudge says it (the host's `nudge_line`). */
export function nudgeLine(c: CodeComment): string {
  const quote = [...c.quote.trim()].slice(0, 80).join("");
  return `${c.path}:${c.line} "${quote}" — ${c.text.trim()}`;
}

/** Text without its Markdown marks and runs of spaces: what a reader sees. */
export function plain(s: string): string {
  return s
    .replace(/^\s{0,3}(#{1,6}\s+|[-*+]\s+(\[[ xX]\]\s+)?|\d+[.)]\s+|>\s?)/, "")
    .replace(/[*_`~]/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

/** Where a rendered selection stands in its document's Markdown: the
 *  first and last line (1-based) of the smallest run of lines that holds
 *  it, and those lines as written — the quote the host checks. `null`:
 *  the words are not in the document (a selection across rendered chrome). */
export function locate(markdown: string, selected: string): { start: number; end: number; quote: string } | null {
  const want = plain(selected.replace(/\n+/g, " "));
  if (!want) return null;
  const lines = markdown.split("\n");
  const flat = lines.map(plain);
  for (let i = 0; i < lines.length; i++) {
    let joined = "";
    for (let j = i; j < Math.min(lines.length, i + 60); j++) {
      joined = joined && flat[j] ? `${joined} ${flat[j]}` : joined || flat[j]!;
      if (joined.includes(want)) {
        // The smallest run ending here: drop leading lines while it holds.
        let from = i;
        while (from < j && rest(flat, from + 1, j).includes(want)) from++;
        return { start: from + 1, end: j + 1, quote: lines.slice(from, j + 1).join("\n") };
      }
    }
  }
  return null;
}

function rest(flat: string[], from: number, to: number): string {
  return flat.slice(from, to + 1).filter(Boolean).join(" ");
}

/** A passage anchor for `selected` in the document at `path`. */
export function passage(path: string, markdown: string, selected: string): CommentAnchor | null {
  const at = locate(markdown, selected);
  if (!at) return null;
  return { kind: "passage", path, start_line: at.start, end_line: at.end, quote: at.quote, quote_sha256: "", section: "" };
}

/** Whether a path is one of the frozen tests a review shows. */
export function frozenTests(oracle: OracleView[]): Set<string> {
  return new Set(oracle.filter((v) => v.state === "frozen").flatMap((v) => v.tests.map((t) => t.path)));
}

/** Why a written test was not kept, in words. */
export function discardWord(reason: string, req: string): string {
  const words: Record<string, string> = {
    cites_no_scenario: `cites no scenario of ${req}`,
    not_a_new_test_file: "not a new test file",
    exists: "the file exists already",
    command_not_allowed: "its command is not allowed",
    command_shared: "its command also runs another test",
    red_without_it: "its command fails without it",
    limit: "more than four tests",
  };
  return words[reason] ?? reason.replaceAll("_", " ");
}
