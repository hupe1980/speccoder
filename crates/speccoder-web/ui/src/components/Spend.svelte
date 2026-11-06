<script lang="ts">
  // What the feature cost: tokens in and out, cache reads, attempts and
  // money (when prices are set) — per task, the dearest first, with a bar
  // against the dearest, and the feature's total.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  import type { CostReport, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";
  import { tokens as n, usd } from "../lib/spend";

  let cost = $state<CostReport | null>(null);
  const costOf = latest();
  $effect(() => {
    void app.report;
    const f = app.feature ?? undefined;
    costOf(get<Reply<CostReport>>(`/api/cost${q({ feature: f })}`), (r) => (cost = r.data ?? null), () => (cost = null));
  });
  const priced = $derived(cost?.total.usd != null);
  const weight = (u: { input_tokens: number; output_tokens: number; usd: number | null }) => (priced ? (u.usd ?? 0) : u.input_tokens + u.output_tokens);
  const rows = $derived([...(cost?.tasks ?? [])].filter((t) => t.usage.input_tokens || t.usage.output_tokens).sort((a, b) => weight(b.usage) - weight(a.usage)));
  const top = $derived(Math.max(1e-9, ...rows.map((r) => weight(r.usage))));
  const wasted = $derived(cost ? weight(cost.not_kept) : 0);
  const cacheShare = $derived(cost && cost.total.input_tokens ? Math.round((cost.total.cache_read_tokens / cost.total.input_tokens) * 100) : 0);
</script>

<section class="card wide spend">
  <h3><Icon name="dollar" size={15} weight={2} /> Spend {#if cost?.priced_model}<span class="muted">priced as {cost.priced_model}</span>{/if}</h3>
  {#if !cost || (!cost.total.input_tokens && !cost.total.output_tokens)}
    <p class="muted">Nothing spent yet: no attempt has called the model.</p>
  {:else}
    <div class="totals">
      <div><span class="big">{priced ? usd(cost.total.usd) : n(cost.total.input_tokens + cost.total.output_tokens)}</span><span class="lab">{priced ? "total" : "tokens"}</span></div>
      <div><span class="big">{n(cost.total.input_tokens)}</span><span class="lab">in</span></div>
      <div><span class="big">{n(cost.total.output_tokens)}</span><span class="lab">out</span></div>
      <div><span class="big">{cacheShare}%</span><span class="lab">of input from the cache</span></div>
      <div><span class="big">{rows.reduce((a, r) => a + r.attempts, 0)}</span><span class="lab">attempts</span></div>
      {#if wasted > 0 && cost}
        <div class="lost" title="Saved attempts that never ticked: part of the total"><span class="big">{priced ? usd(cost.not_kept.usd) : n(cost.not_kept.input_tokens + cost.not_kept.output_tokens)}</span><span class="lab">on attempts not kept · {Math.round((wasted / Math.max(1e-9, weight(cost.total))) * 100)}%</span></div>
      {/if}
    </div>
    <ul class="rows">
      {#each rows as r (r.task)}
        <li>
          <button class="task mono" onclick={() => { app.view = "board"; void app.selectTask(r.task); }}>{r.task}</button>
          <span class="bar" aria-hidden="true"><span style="width:{(weight(r.usage) / top) * 100}%"></span></span>
          <span class="num">{n(r.usage.input_tokens)} in · {n(r.usage.output_tokens)} out</span>
          <span class="num money">{priced ? usd(r.usage.usd) : ""}</span>
          <span class="att">{r.attempts} {r.attempts === 1 ? "attempt" : "attempts"}</span>
        </li>
      {/each}
    </ul>
    {#if cost.ask.input_tokens || cost.ask.output_tokens}
      <p class="ask muted"><Icon name="bubble" size={12} /> Ask · {n(cost.ask.input_tokens)} in · {n(cost.ask.output_tokens)} out{priced ? ` · ${usd(cost.ask.usd)}` : ""} <span>— its own budget, not in the total</span></p>
    {/if}
    {#if !priced}
      <p class="hint muted">Set prices in <button class="link" onclick={() => (app.view = "settings")}>Settings › Model</button> to see money, not only tokens.</p>
    {/if}
  {/if}
</section>

<style>
  .card { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 1.15rem 1.35rem 1.25rem; grid-column: 1 / -1; }
  h3 { margin: 0 0 0.9rem; font: 650 1.02rem/1.3 var(--display); display: flex; align-items: center; gap: 0.4rem; }
  h3 .muted { font: 500 0.82rem var(--font); margin-left: 0.25rem; }
  .ask { display: flex; align-items: center; gap: 0.35rem; margin: 0.8rem 0 0; font-size: 0.8rem; }
  .ask span { opacity: 0.8; }
  .totals { display: flex; flex-wrap: wrap; gap: 0.5rem 2rem; margin-bottom: 1rem; }
  .totals div { display: grid; }
  .big { font: 700 1.6rem/1.1 var(--rounded); letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .lab { font-size: 0.78rem; color: var(--muted); }
  .lost .big { color: var(--attention); }
  .rows { list-style: none; margin: 0; padding: 0; display: grid; }
  .rows li { display: grid; grid-template-columns: 4.5rem minmax(4rem, 1fr) auto 4.5rem 6rem; gap: 0.9rem; align-items: center; padding: 0.4rem 0; border-top: 0.5px solid var(--separator); font-size: 0.84rem; }
  .rows li:first-child { border-top: 0; }
  .task { border: 0; background: none; padding: 0; color: var(--accent-ink); cursor: pointer; text-align: left; font-size: 0.82rem; }
  .bar { height: 6px; border-radius: 3px; background: var(--fill); overflow: hidden; }
  .bar span { display: block; height: 100%; border-radius: 3px; background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 60%, #bf5af2)); }
  .num { font-variant-numeric: tabular-nums; color: var(--text-2); white-space: nowrap; }
  .money { text-align: right; font-weight: 600; color: var(--text); }
  .att { color: var(--muted); font-size: 0.78rem; text-align: right; }
  .hint { margin: 0.75rem 0 0; font-size: 0.82rem; }
  .link { border: 0; background: none; padding: 0; color: var(--accent-ink); font-weight: 600; cursor: pointer; }
  @media (max-width: 720px) { .rows li { grid-template-columns: 4rem 1fr auto; } .money, .att { display: none; } }
</style>
