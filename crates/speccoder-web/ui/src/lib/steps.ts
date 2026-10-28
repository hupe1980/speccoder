// A fix line or a next step as the page takes it — the host says how
// (`How`): speccoder performs it, a view opens, the person's own command
// runs here after a yes, or it belongs in their shell. And what went
// wrong as one block to copy: the twin of the runtime's `copy::block`.

import type { How } from "./types";

/** The verb a step's button says. */
export function verb(how: How): string {
  switch (how.how) {
    case "perform":
      return how.input ? "Run…" : "Run";
    case "view":
      return "Open";
    case "command":
      return how.interactive ? "Open in Terminal" : "Run…";
    case "shell":
    case "words":
      return "Copy";
  }
}

/** The line a step shows: without a leading `→` or a trailing "(why)"
 *  said beside it. */
export function shown(line: string): string {
  return line.replace(/^→\s*/, "").replace(/\s+\(.*\)$/, "");
}

/** A `→` line: words for a person, read rather than taken. */
export function prose(line: string): boolean {
  return /^→/.test(line);
}

/** An action's label for a speccoder command: its words after `speccoder`. */
export function labelOf(line: string): string {
  return line.replace(/^speccoder /, "");
}

/** Argvs as a shell reads them — the twin of the runtime's
 *  `step::render`: what a card shows is exactly what runs. */
export function render(argvs: string[][]): string {
  const word = (w: string) => (w !== "" && /^[A-Za-z0-9_.\/:=,@+-]+$/.test(w) ? w : `'${w.replaceAll("'", "'\\''")}'`);
  return argvs.map((a) => a.map(word).join(" ")).join(" && ");
}

/** The argvs are speccoder's own command (a job of speccoder's). */
export function ownCommand(argvs: string[][]): boolean {
  return argvs.length === 1 && argvs[0]?.[0] === "speccoder";
}

/** Where the codes are documented (the catalog's `DOCS`): a code's
 *  section is its name in lower case. */
export const DOCS = "https://hupe1980.github.io/speccoder/docs/codes/";

/** What a result says went wrong, as the host sends it. */
export interface Wrong {
  message: string;
  code?: string | null;
  fix?: string[] | null;
  docs?: string | null;
  /** A run's payload names its run (`run_id`). */
  data?: unknown;
}

/** Where it happened: speccoder's version, the OS, the run. */
export interface Where {
  version?: string | null;
  os?: string | null;
  run?: string | null;
}

/** `error[CODE]: message`, each fix line, the docs link, then version ·
 *  OS · run. The host masked the message and the fixes already; nothing
 *  here unmasks or adds a value. */
export function copyBlock(r: Wrong, where: Where = {}, kind = "error"): string {
  const lines = [r.code ? `${kind}[${r.code}]: ${r.message}` : r.message];
  for (const f of r.fix ?? []) lines.push(`fix: ${f.replace(/^→\s*/, "")}`);
  const docs = r.docs ?? (r.code ? `${DOCS}#${r.code.toLowerCase()}` : null);
  if (docs) lines.push(`docs: ${docs}`);
  const run = where.run ?? (r.data as { run_id?: string } | null | undefined)?.run_id;
  const tail = [`speccoder ${where.version ?? "?"}`, where.os, run ? `run ${run}` : null].filter(Boolean);
  lines.push(tail.join(" · "));
  return lines.join("\n") + "\n";
}

/** A line of a command's output, cut into text, links and device codes
 *  (`ABCD-1234`, a login's) — the page makes the last two clickable. */
export type Segment = { text: string; kind: "text" | "url" | "code" };

export function segments(line: string): Segment[] {
  const out: Segment[] = [];
  const re = /(https?:\/\/[^\s"'<>)]+)|\b([A-Z0-9]{4}-[A-Z0-9]{4})\b/g;
  let at = 0;
  for (const m of line.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > at) out.push({ text: line.slice(at, i), kind: "text" });
    out.push({ text: m[0], kind: m[1] ? "url" : "code" });
    at = i + m[0].length;
  }
  if (at < line.length || !out.length) out.push({ text: line.slice(at), kind: "text" });
  return out;
}
