// The logic of the doors into speccoder — the composer, the New Feature
// sheet, the readiness strip, the chain's stop card, the New Project
// checklist and the sample's playback — kept out of the components so it
// is tested on its own. Nothing here decides anything the host does not:
// the suggestion, the matches, the plan and the stops are the host's;
// this turns them into what a person reads and which key does what.

import type { ChainStop, PlanStep, PlanStepResult, PlanStepState, ProjectPlan, Readiness, RungPick, Through } from "./types";

/** The rungs the composer offers, in its order, with their keys. */
export const RUNGS: { rung: Exclude<RungPick, "none">; label: string; ceremony: string; keys: string }[] = [
  { rung: "bug", label: "Bug fix", ceremony: "a reproduction must fail first, then the fix is judged", keys: "⌃⏎" },
  { rung: "quick", label: "Quick change", ceremony: "one requirement, judged like a task", keys: "⌥⏎" },
  { rung: "feature", label: "Feature", ceremony: "the spec tool writes a spec; you review it", keys: "⇧⏎" },
  { rung: "ask", label: "Just ask", ceremony: "a read-only answer, nothing changes", keys: "⌘⏎" },
];

/** Whether this build has the rung: Ask arrives with the Ask panel. */
export const available = (r: RungPick): boolean => r === "bug" || r === "quick" || r === "feature";

/** What ⏎ with modifiers takes in the composer: a rung, the selected row
 *  (`selected`), or nothing. */
export function rungForKey(e: { key: string; altKey?: boolean; shiftKey?: boolean; ctrlKey?: boolean; metaKey?: boolean }): RungPick | "selected" | null {
  if (e.key !== "Enter") return null;
  if (e.altKey) return "quick";
  if (e.shiftKey) return "feature";
  if (e.ctrlKey) return "bug";
  if (e.metaKey) return "ask";
  return "selected";
}

/** The next available rung from `at` in direction `dir` (wrapping). */
export function move(at: RungPick | null, dir: 1 | -1): RungPick {
  const order = RUNGS.map((r) => r.rung).filter(available);
  const i = at ? order.indexOf(at as (typeof order)[number]) : -1;
  if (i < 0) return dir === 1 ? order[0]! : order[order.length - 1]!;
  return order[(i + dir + order.length) % order.length]!;
}

/** `text` as one double-quoted shell argument. */
export function quote(text: string): string {
  const one = text.split(/\s+/).filter(Boolean).join(" ");
  return `"${one.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

/** The skill's short name (`speckit-specify` → `specify`). */
export function short(skill: string | null | undefined): string {
  return (skill ?? "").replace(/^(speckit|openspec)-/, "");
}

/** The command line the composer's footer shows for a rung. */
export function composerCommand(rung: RungPick | null, text: string, createSkill: string | null | undefined): string | null {
  const t = text.trim() ? quote(text) : '"…"';
  switch (rung) {
    case "bug": return `speccoder bug ${t}`;
    case "quick": return `speccoder do ${t}`;
    case "feature": return createSkill ? `speccoder spec ${short(createSkill)} ${t}` : null;
    default: return null;
  }
}

/** The New Feature sheet's command lines (*Show commands*). */
export function featureCommands(createSkill: string | null | undefined, text: string, through: Through, issue?: number | null): string[] {
  const out: string[] = [];
  if (createSkill) out.push(`speccoder spec ${short(createSkill)} ${text.trim() ? quote(text) : '"…"'} --through ${through}`);
  if (issue) out.push(`speccoder issue start ${issue} --through ${through}`);
  return out;
}

/** The skills plan through runs for `through`, in order, the review
 *  between the spec and the rest. */
export function chainSkills(planning: { creates: string | null; toward: string[]; checks: string | null } | undefined, through: Through): string[] {
  if (!planning?.creates) return [];
  const out = [short(planning.creates)];
  if (through === "spec") return out;
  const toward = planning.toward.map(short);
  if (toward.length) out.push("(your review)");
  out.push(...(through === "plan" ? toward.slice(0, Math.max(1, toward.length - 1)) : toward));
  if (through === "analyze" && planning.checks) out.push(short(planning.checks));
  return out;
}

/** The *Plan it* choices: `analyze` only where the format checks its plan. */
export function throughChoices(hasChecks: boolean): { through: Through; label: string }[] {
  const c: { through: Through; label: string }[] = [
    { through: "spec", label: "spec only" },
    { through: "tasks", label: "spec → plan → tasks" },
  ];
  if (hasChecks) c.push({ through: "analyze", label: "… → analyze" });
  return c;
}

/** A count with its noun. */
export const plural = (n: number, one: string, many = `${one}s`): string => `${n} ${n === 1 ? one : many}`;

/** The readiness strip's parts: a word for each count, and whether it
 *  needs the person (a glyph, never colour alone). */
export function readinessParts(r: Readiness): { key: "requirements" | "scenarios" | "questions" | "untestable"; text: string; attention: boolean }[] {
  const out: { key: "requirements" | "scenarios" | "questions" | "untestable"; text: string; attention: boolean }[] = [];
  if (r.identifiers) out.push({ key: "requirements", text: plural(r.requirements, "requirement"), attention: false });
  out.push({ key: "scenarios", text: plural(r.scenarios, "scenario"), attention: false });
  out.push({ key: "questions", text: plural(r.open_questions, "open question"), attention: r.open_questions > 0 });
  if (r.untestable.length) {
    out.push({ key: "untestable", text: `${plural(r.untestable.length, "requirement")} without a scenario (${r.untestable.join(", ")} — can't be tested)`, attention: true });
  }
  return out;
}

/** What changed between two readings, for a polite live region: only the
 *  counts that moved, or nothing. */
export function readinessChange(before: Readiness | null, now: Readiness | null): string | null {
  if (!now) return null;
  if (!before) return null;
  const parts: string[] = [];
  const say = (n: number, was: number, one: string) => {
    if (n > was) parts.push(`${plural(n - was, one)} added`);
    if (n < was) parts.push(`${plural(was - n, one)} fewer`);
  };
  say(now.requirements, before.requirements, "requirement");
  say(now.scenarios, before.scenarios, "scenario");
  say(now.open_questions, before.open_questions, "open question");
  say(now.untestable.length, before.untestable.length, "untestable requirement");
  return parts.length ? parts.join(", ") : null;
}

/** The chain's stop card: its headline, its tone and its primary button. */
export function stopCard(stop: ChainStop, unit: string | null): { tone: "attention" | "danger" | "ok"; title: string; primary: string | null; disabled: string | null } {
  switch (stop.kind) {
    case "questions":
      return {
        tone: "attention",
        title: `Answer ${plural(stop.questions.length, "question")}, then review the spec`,
        primary: "Approve and plan",
        disabled: `Answer the ${plural(stop.questions.length, "open question")} first`,
      };
    case "review":
      return {
        tone: "attention",
        title: stop.requirements.length ? `Review the spec — ${plural(stop.requirements.length, "requirement")}. Approve them to plan.` : "Review the spec, then plan it.",
        primary: "Approve and plan",
        disabled: null,
      };
    case "failed":
      return { tone: "danger", title: `${short(stop.skill)} failed: ${stop.message}`, primary: "Retry this step", disabled: null };
    case "done":
      return {
        tone: "ok",
        title: unit ? `Planned in ${unit}${stop.plan_checks.length ? ` — ${plural(stop.plan_checks.length, "plan check")} to look at` : ""}` : "Planned",
        primary: "Commit the planning files",
        disabled: null,
      };
  }
}

/** Whether the wording approved now is still the wording shown (else the
 *  stop card says "The spec changed — review again"). */
export function sameWording(shown: { id: string; text: string }[], now: { id: string; text: string }[]): boolean {
  const at = new Map(now.map((r) => [r.id, r.text]));
  return shown.every((r) => at.get(r.id) === r.text);
}

/** A step's state as a glyph and a word. */
export function stepMark(state: PlanStepState | null | undefined): { glyph: string; word: string } {
  switch (state) {
    case "running": return { glyph: "◐", word: "running" };
    case "done": return { glyph: "✓", word: "done" };
    case "skipped": return { glyph: "=", word: "skipped" };
    case "failed": return { glyph: "✗", word: "failed" };
    default: return { glyph: "·", word: "waiting" };
  }
}

/** The plan's steps that reach the network, and the hosts they name. */
export function networkSteps(plan: ProjectPlan): { steps: PlanStep[]; hosts: string[] } {
  const steps = plan.steps.filter((s) => s.access === "network");
  const hosts = [...new Set(steps.flatMap((s) => s.hosts ?? []))];
  return { steps, hosts };
}

/** An argv as a shell would read it. */
export function shell(argv: string[]): string {
  return argv.map((a) => (a === "" || /[\s"']/.test(a) ? quote(a) : a)).join(" ");
}

/** The checklist rows from the plan and a run's events so far. */
export function checklist(plan: ProjectPlan, results: PlanStepResult[], running: string | null): { step: PlanStep; state: PlanStepState; result: PlanStepResult | null }[] {
  return plan.steps.map((step) => {
    const result = results.find((r) => r.id === step.id) ?? null;
    const state: PlanStepState = result ? result.state : running === step.id ? "running" : "waiting";
    return { step, state, result };
  });
}

/** The folder a new project gets: `<parent>/<slug of the idea>` (the
 *  host's slug rule: five significant words, ASCII, at most 40). */
export function slug(idea: string): string {
  const filler = new Set(["a", "an", "the", "that", "which", "who", "to", "of", "for", "and", "or", "in", "on", "with", "my", "our", "your", "into", "from", "by", "at", "as", "is", "it", "its", "this", "i", "we", "want", "build", "make", "write", "create", "app", "tool", "simple", "small", "little"]);
  const words = idea
    .normalize("NFKD")
    .replace(/[^\x00-\x7f]/g, "")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((w) => w && !filler.has(w))
    .slice(0, 5);
  let out = "";
  for (const w of words) {
    const next = out ? `${out}-${w}` : w;
    if (next.length > 40) {
      if (!out) out = w.slice(0, 40);
      break;
    }
    out = next;
  }
  return out || "project";
}

/** How long each playback step shows: at least 600 ms, the whole at most
 *  two minutes. */
export function pace(steps: number): number {
  if (steps <= 0) return 600;
  return Math.max(600, Math.min(1500, Math.floor(120_000 / steps)));
}

/** A function that runs `f` once input pauses for `ms`. */
export function debounce<A extends unknown[]>(f: (...a: A) => void, ms: number): ((...a: A) => void) & { cancel: () => void } {
  let t: ReturnType<typeof setTimeout> | null = null;
  const d = (...a: A) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => {
      t = null;
      f(...a);
    }, ms);
  };
  d.cancel = () => {
    if (t) clearTimeout(t);
    t = null;
  };
  return d;
}

/** Whether a key press belongs to a field (the page's keys then wait). */
export function typing(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el?.closest?.("input, textarea, select, [contenteditable='true']");
}
