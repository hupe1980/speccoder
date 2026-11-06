// Money and tokens as people read them, and the line a run is started
// under: what it will likely cost here, and what it may.
import type { CostReport } from "./types";

export const tokens = (x: number) => (x >= 1e6 ? `${(x / 1e6).toFixed(1)}M` : x >= 1e3 ? `${(x / 1e3).toFixed(x < 1e4 ? 1 : 0)}k` : String(x));
export const usd = (x: number | null | undefined) => (x == null ? "—" : x < 0.01 ? `$${x.toFixed(4)}` : `$${x.toFixed(2)}`);

/** `≈ 1.2M tokens · $3.40 for 4 tasks · capped at 2M`, and what it rests on; null with neither. */
export function runLine(cost: CostReport | null): { text: string; basis: string } | null {
  const e = cost?.estimate;
  const capped = cost?.cap ? `capped at ${cost.cap}` : "";
  if (!e) return capped ? { text: cap(capped), basis: "No task is ticked here yet: no estimate." } : null;
  const t = e.total.input_tokens + e.total.output_tokens;
  const per = e.per_task.input_tokens + e.per_task.output_tokens;
  const money = e.total.usd != null ? ` · ${usd(e.total.usd)}` : "";
  return {
    text: `≈ ${tokens(t)} tokens${money} for ${e.tasks} ${e.tasks === 1 ? "task" : "tasks"}${capped ? ` · ${capped}` : ""}`,
    basis: `${tokens(per)} a task: the average of ${e.ticks} ticked ${e.ticks === 1 ? "task" : "tasks"} in this project.`,
  };
}

const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
