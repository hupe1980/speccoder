// What judges a task of a story, and how far its story is — in the words
// every surface shows (speccoder_core::story).

import type { Share, StoryScenarios } from "./types";

/** `linked US3-1` · `checkpoint: US3-2 US3-3 + together` · … */
export function shareWords(s: Share): string {
  switch (s.kind) {
    case "linked":
      return `its scenarios ${s.scenarios.join(", ")}`;
    case "checkpoint":
      return s.scenarios.length
        ? `its story's checkpoint: ${s.scenarios.join(", ")}${s.composition ? ", and together" : ""}`
        : "its story's checkpoint — every scenario already verified";
    case "authored":
      return `the test author's choice of ${s.scenarios.join(", ")}${s.checkpoint ? ` · ${s.checkpoint} judges what it leaves` : ""}`;
    case "deferred":
      return `its regression checks; its scenarios at ${s.checkpoint}`;
  }
}

/** The card's short chip, when the share says something a glance needs. */
export function shareChip(s: Share | null | undefined): string | null {
  if (!s) return null;
  switch (s.kind) {
    case "linked":
      return s.scenarios.join(" ");
    case "checkpoint":
      return s.scenarios.length ? `checkpoint · ${s.scenarios.length} left` : null;
    case "deferred":
      return `judged at ${s.checkpoint}`;
    default:
      return null;
  }
}

/** `2 of 3 verified · T021 judges the rest` */
export function storyLine(s: StoryScenarios): string {
  const done = s.scenarios.filter((m) => m.verified_by).length;
  const n = s.scenarios.length;
  if (done === n) return `${n} of ${n} verified`;
  return `${done} of ${n} verified · ${s.checkpoint ? `${s.checkpoint} judges the rest` : "no task judges the rest"}`;
}
