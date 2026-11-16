// Story claims: who else holds a story of a feature.
import type { ClaimsReport } from "./types";

/** "Dana has US2, US3": other people's claims that stand (live, or done
 *  and not merged yet), one line per holder. */
export function others(r: ClaimsReport | null | undefined): string[] {
  const by = new Map<string, string[]>();
  for (const c of r?.claims ?? []) {
    if ((c.state !== "theirs" && c.state !== "done") || !c.claim) continue;
    by.set(c.claim.holder, [...(by.get(c.claim.holder) ?? []), c.story]);
  }
  return [...by].map(([who, stories]) => `${who} has ${stories.join(", ")}`);
}
