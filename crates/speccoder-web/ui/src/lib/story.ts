// What judges a task of a story, and how far its story is on its oracle —
// in the words every surface shows (speccoder_core::story).

import type { Judged, StoryScenarios } from "./types";

/** `the scenarios it turns green` · `linked US3-1` · `its story's checkpoint` */
export function judgedWords(j: Judged): string {
  switch (j.kind) {
    case "progress":
      return "the scenarios it turns green on its story's oracle";
    case "linked":
      return `its scenarios ${j.scenarios.join(", ")} — green after it`;
    case "checkpoint":
      return "its story's checkpoint — every scenario green after it";
  }
}

/** The card's short chip, when what judges it says something a glance needs. */
export function judgedChip(j: Judged | null | undefined): string | null {
  if (!j) return null;
  switch (j.kind) {
    case "linked":
      return j.scenarios.join(" ");
    case "checkpoint":
      return "closes the story";
    default:
      return null;
  }
}

/** `2 of 3 green · T008 turned US1-1, US1-2 · T010 closes it` */
export function storyLine(s: StoryScenarios): string {
  const green = s.scenarios.filter((m) => m.state === "green").length;
  const n = s.scenarios.length;
  const by = new Map<string, string[]>();
  for (const m of s.scenarios) if (m.turned_by) by.set(m.turned_by, [...(by.get(m.turned_by) ?? []), m.id]);
  const turned = [...by].map(([t, ids]) => ` · ${t} turned ${ids.join(", ")}`).join("");
  if (!s.oracle) return `${green} of ${n} green · its oracle is written at its first code task`;
  if (green === n) return `${n} of ${n} green${turned}`;
  return `${green} of ${n} green${turned} · ${s.checkpoint ? `${s.checkpoint} closes it` : "no task closes it"}`;
}
