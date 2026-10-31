// An answer's words as blocks to render: paragraphs, list items, headings
// and code, with inline code, bold and citation markers — parsed here and
// rendered by components, never as HTML (the words are the model's).
import type { Citation } from "./schema.gen";

export type Part =
  | { t: "text"; text: string }
  | { t: "code"; text: string }
  | { t: "bold"; text: string }
  | { t: "cite"; n: number | null; citation: Citation }
  | { t: "mark" };
export type Block = { kind: "p" | "li" | "ol" | "h"; parts: Part[] } | { kind: "pre"; text: string };

const MARK = /\[(?:r|c)\d+(?::\d+(?:-\d+)?)?\]/g;

/** Inline parts of one line: `code`, **bold**, then [r3] markers — while
 * the words still stream (`live`), a marker is a placeholder until the
 * turn's sources are checked. */
export function inline(line: string, citations: Citation[], live = false): Part[] {
  const read = citations.filter((c) => c.state === "read");
  const out: Part[] = [];
  const pieces = line.split(/(`[^`]+`|\*\*[^*]+\*\*)/);
  for (const piece of pieces) {
    if (!piece) continue;
    if (piece.startsWith("`") && piece.endsWith("`") && piece.length > 1) {
      out.push({ t: "code", text: piece.slice(1, -1) });
    } else if (piece.startsWith("**") && piece.endsWith("**") && piece.length > 3) {
      out.push({ t: "bold", text: piece.slice(2, -2) });
    } else {
      let at = 0;
      for (const m of piece.matchAll(MARK)) {
        if (m.index! > at) out.push({ t: "text", text: piece.slice(at, m.index) });
        const c = citations.find((x) => x.marker === m[0]);
        if (c) {
          const i = read.findIndex((x) => x.marker === c.marker);
          out.push({ t: "cite", n: i >= 0 ? i + 1 : null, citation: c });
        } else out.push(live ? { t: "mark" } : { t: "text", text: m[0] });
        at = m.index! + m[0].length;
      }
      if (at < piece.length) out.push({ t: "text", text: piece.slice(at) });
    }
  }
  return out;
}

/** The answer as blocks; `live` while it streams (a marker cut mid-way
 * waits for its end). */
export function blocks(text: string, citations: Citation[], live = false): Block[] {
  const out: Block[] = [];
  const lines = (live ? text.replace(/\[(?:[rc]\d*(?::\d*(?:-\d*)?)?)?$/, "") : text).replace(/\r/g, "").split("\n");
  let para: string[] = [];
  const flush = () => {
    if (para.length) out.push({ kind: "p", parts: inline(para.join(" "), citations, live) });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    if (line.trimStart().startsWith("```")) {
      flush();
      const code: string[] = [];
      for (i++; i < lines.length && !(lines[i] ?? "").trimStart().startsWith("```"); i++) code.push(lines[i] ?? "");
      out.push({ kind: "pre", text: code.join("\n") });
      continue;
    }
    const li = line.match(/^\s*[-*•]\s+(.*)$/);
    const ol = line.match(/^\s*\d+[.)]\s+(.*)$/);
    const h = line.match(/^#{1,4}\s+(.*)$/);
    if (li || ol || h) {
      flush();
      out.push({ kind: li ? "li" : ol ? "ol" : "h", parts: inline((li ?? ol ?? h)?.[1] ?? "", citations, live) });
    } else if (!line.trim()) flush();
    else para.push(line.trim());
  }
  flush();
  return out;
}
