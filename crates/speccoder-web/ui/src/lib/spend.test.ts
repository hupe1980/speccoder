import { describe, expect, it } from "vitest";
import { runLine } from "./spend";
import type { CostReport } from "./types";

const u = (input: number, output: number, usd: number | null = null) => ({ input_tokens: input, output_tokens: output, cache_read_tokens: 0, cache_write_tokens: 0, usd });
const report = (over: Partial<CostReport>): CostReport =>
  ({ feature: "001-x", tasks: [], convergence: u(0, 0), total: u(0, 0), priced_model: null, ask: u(0, 0), not_kept: u(0, 0), estimate: null, cap: null, ...over }) as CostReport;

describe("the line a run starts under", () => {
  it("states the estimate, its money and the cap, and what it rests on", () => {
    const line = runLine(report({ estimate: { tasks: 4, ticks: 3, per_task: u(290_000, 10_000, 0.85), total: u(1_160_000, 40_000, 3.4) }, cap: "2M" }));
    expect(line?.text).toBe("≈ 1.2M tokens · $3.40 for 4 tasks · capped at 2M");
    expect(line?.basis).toBe("300k a task: the average of 3 ticked tasks in this project.");
  });
  it("names the cap alone when nothing is ticked yet", () => {
    expect(runLine(report({ cap: "$5" }))?.text).toBe("Capped at $5");
  });
  it("says nothing with neither", () => {
    expect(runLine(report({}))).toBeNull();
    expect(runLine(null)).toBeNull();
  });
});
