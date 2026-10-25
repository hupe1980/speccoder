<script lang="ts">
  // A progress ring after the Activity rings: one stroke per verdict, in
  // the work's order, round-capped, on a faint track. The centre may hold a
  // number. Its label reads the counts aloud.
  import { ORDER, word } from "../lib/marks";

  let {
    totals,
    size = 96,
    width = 10,
    label = true,
  }: { totals: Record<string, number>; size?: number; width?: number; label?: boolean } = $props();

  const parts = $derived(ORDER.map((k) => ({ k, n: totals[k] ?? 0 })).filter((p) => p.n > 0));
  const total = $derived(parts.reduce((a, p) => a + p.n, 0));
  const done = $derived((totals.verified ?? 0) + (totals.regression_only ?? 0) + (totals.attested ?? 0) + (totals.skipped ?? 0));
  const r = $derived((size - width) / 2);
  const c = $derived(2 * Math.PI * r);
  // A small gap between segments, none when one segment fills the ring.
  const gap = $derived(parts.length > 1 ? Math.min(width * 0.9, c * 0.02) : 0);
  const segments = $derived.by(() => {
    let at = 0;
    return parts.map((p) => {
      const len = (p.n / Math.max(total, 1)) * c;
      const s = { k: p.k, n: p.n, dash: Math.max(len - gap, 0.01), offset: -at };
      at += len;
      return s;
    });
  });
</script>

<div class="ring" style="width:{size}px;height:{size}px" role="img" aria-label="{done} of {total} finished: {parts.map((p) => `${p.n} ${word(p.k)}`).join(', ')}">
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    <circle cx={size / 2} cy={size / 2} {r} fill="none" stroke="var(--fill)" stroke-width={width} />
    <g transform="rotate(-90 {size / 2} {size / 2})">
      {#each segments as s (s.k)}
        <circle
          class="seg s-{s.k}"
          cx={size / 2}
          cy={size / 2}
          {r}
          fill="none"
          stroke="var(--fill-c)"
          stroke-width={width}
          stroke-linecap={parts.length > 1 ? "butt" : "round"}
          stroke-dasharray="{s.dash} {c}"
          stroke-dashoffset={s.offset}
        />
      {/each}
    </g>
  </svg>
  {#if label && total}
    <div class="centre">
      <span class="n">{done}</span><span class="of">of {total}</span>
    </div>
  {/if}
</div>

<style>
  .ring { position: relative; flex: none; }
  .seg { background: none; transition: stroke-dasharray 700ms var(--ease), stroke-dashoffset 700ms var(--ease); }
  .centre { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; line-height: 1.05; }
  .n { font: 700 1.55rem/1 var(--rounded); letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .of { font-size: 0.72rem; color: var(--muted); font-weight: 500; }
</style>
