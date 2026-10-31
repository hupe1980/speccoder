<script lang="ts">
  // A quick change or bug fix that grew, offered as a spec of its own —
  // the Needs You item, shown where the task is: its inspector and Ask.
  // What was measured is said; nothing here is a model's opinion.
  import { app } from "../lib/app.svelte";
  import type { InboxItem } from "../lib/schema.gen";
  import Icon from "./Icon.svelte";

  let { item, onstart, compact = false }: { item: InboxItem; onstart?: () => void; compact?: boolean } = $props();
  const what = $derived(item.title.replace(/^\S+\s+/, ""));
</script>

<section class="offer" class:compact aria-label="{item.task} {what}">
  <span class="glyph" aria-hidden="true"><Icon name="sparkle" size={compact ? 13 : 15} weight={2} /></span>
  <div class="ow">
    <strong>{item.task} {what}</strong>
    <span title={item.detail}>{item.detail}</span>
  </div>
  <div class="oa">
    <button type="button" class="btn small" onclick={() => (onstart ? onstart() : item.actions[0] && void app.perform(item.actions[0]))}>Start a spec</button>
    {#if item.actions[1]}<button type="button" class="later" onclick={() => void app.perform(item.actions[1]!)}>Not now</button>{/if}
  </div>
</section>

<style>
  .offer { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 0.6rem; padding: 0.65rem 0.75rem; border-radius: 14px; background: color-mix(in srgb, #bf5af2 8%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, #bf5af2 22%, transparent); animation: rise 280ms var(--spring); }
  .offer.compact { gap: 0.5rem; padding: 0.5rem 0.6rem; border-radius: 12px; }
  .glyph { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: #fff; background: linear-gradient(135deg, #d17bff, #8e44ec); }
  .compact .glyph { width: 24px; height: 24px; border-radius: 7px; }
  .ow { display: grid; min-width: 0; }
  strong { font-size: 0.84rem; }
  .compact strong { font-size: 0.8rem; }
  .ow span { font-size: 0.74rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .oa { display: flex; align-items: center; gap: 0.5rem; }
  .later { border: 0; background: none; padding: 0; color: var(--accent-ink); font: inherit; font-size: 0.78rem; font-weight: 600; cursor: pointer; }
  @keyframes rise { from { transform: translateY(6px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .offer { animation: none; } }
</style>
