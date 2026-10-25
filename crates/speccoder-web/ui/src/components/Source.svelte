<script lang="ts">
  // One line of code, highlighted: tokens as text spans.
  import { tokens } from "../lib/highlight";
  let { line, lang }: { line: string; lang: string } = $props();
  const parts = $derived(tokens(line, lang));
</script>

{#each parts as p}{#if p.c}<span class="h{p.c}">{p.t}</span>{:else}{p.t}{/if}{/each}

<style>
  .hk { color: var(--syn-k); font-weight: 550; }
  .hs { color: var(--syn-s); }
  .hc { color: var(--syn-c); font-style: italic; }
  .hn { color: var(--syn-n); }
  .hy { color: var(--syn-y); }
  .hf { color: var(--syn-f); }
  .hp { color: var(--syn-p); }
</style>
