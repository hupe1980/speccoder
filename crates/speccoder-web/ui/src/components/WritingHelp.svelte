<script lang="ts">
  // Under a field whose words become the judge or an instruction: the
  // writing check as you type (no model), and ✦ Shape, which opens the
  // Shape sheet over the field and puts back what the person accepts.
  import { app } from "../lib/app.svelte";
  import { post } from "../lib/api";
  import Icon from "./Icon.svelte";

  type Finding = { rule: string; words: string; reason: string; fix: string };
  let { field, text, onshaped }: { field: string; text: string; onshaped: (text: string) => void } = $props();

  let findings = $state<Finding[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;
  // Only the newest text's answer is shown: an older one arriving late
  // is dropped.
  let asked = 0;
  $effect(() => {
    const t = text;
    clearTimeout(timer);
    const n = ++asked;
    if (!t.trim()) {
      findings = [];
      return;
    }
    timer = setTimeout(() => {
      void post<{ findings: Finding[] }>("/api/shape/check", { field, text: t })
        .then((r) => {
          if (n === asked) findings = r.findings;
        })
        .catch(() => {});
    }, 450);
    return () => clearTimeout(timer);
  });
  const empty = $derived(!/[\p{L}\p{N}]/u.test(text));
</script>

<div class="help">
  <span class="hints" aria-live="polite">
    {#each findings.slice(0, 3) as f (f.rule + f.words)}
      <span class="hint" title="{f.reason} — {f.fix}">{#if f.words}<b>{f.words}</b>: {/if}{f.fix}</span>
    {/each}
    {#if findings.length > 3}<span class="hint more">+{findings.length - 3}</span>{/if}
  </span>
  <button
    type="button"
    class="shape-btn"
    disabled={empty}
    title={empty ? "Write a few words first — any words" : "Make it clear and testable; every change is shown, nothing is used until you decide"}
    onclick={() => (app.shape = { field, text, accept: onshaped })}
  ><Icon name="sparkle" size={11} weight={2.2} /> Shape</button>
</div>

<style>
  .help { display: flex; align-items: flex-start; gap: 0.5rem; justify-content: space-between; min-height: 1.5rem; }
  .hints { display: flex; flex-wrap: wrap; gap: 0.3rem; flex: 1; }
  .hint { font-size: 0.74rem; padding: 0.1rem 0.5rem; border-radius: 999px; color: var(--text-2); background: color-mix(in srgb, var(--attention) 12%, transparent); }
  .hint b { color: var(--attention); font-weight: 650; }
  .hint.more { background: var(--fill); }
  .shape-btn { flex: none; display: inline-flex; align-items: center; gap: 0.3rem; border: 0; border-radius: 999px; padding: 0.22rem 0.65rem; font: 650 0.76rem/1.2 var(--rounded); color: #8944ab; background: color-mix(in srgb, #bf5af2 14%, transparent); cursor: pointer; }
  .shape-btn:hover:not(:disabled) { background: color-mix(in srgb, #bf5af2 24%, transparent); }
  .shape-btn:disabled { opacity: 0.45; cursor: default; }
  @media (prefers-color-scheme: dark) { .shape-btn { color: #da8fff; } }
</style>
