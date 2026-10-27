<script lang="ts">
  // What the future oracle of this unit will be made of, counted by the
  // host from its files as they are written: requirements, scenarios, open
  // questions, requirements no scenario covers. Each count is a link; a
  // change is announced politely, at most every five seconds.
  import { app } from "../lib/app.svelte";
  import { get, q, latest } from "../lib/api";
  import { readinessChange, readinessParts } from "../lib/start";
  import type { Readiness, Reply, Review } from "../lib/types";

  let { readiness = undefined }: { readiness?: Readiness | null } = $props();
  let fetched = $state<Readiness | null>(null);
  const shown = $derived(readiness ?? fetched);
  const fetchOf = latest();
  $effect(() => {
    if (readiness !== undefined) return;
    void app.report; // again with every refresh (a job's writes among them)
    void app.documents;
    fetchOf(
      get<Reply<Review>>(`/api/review${q({ feature: app.feature ?? undefined })}`),
      (r) => (fetched = r.data?.readiness ?? null),
      () => (fetched = null),
    );
  });

  // The live region: changed counts only, at most once every 5 s.
  let said = $state("");
  let last: Readiness | null = null;
  let quietUntil = 0;
  let pending: string | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const now = shown;
    const change = readinessChange(last, now ?? null);
    last = now ?? null;
    if (!change) return;
    const wait = quietUntil - Date.now();
    if (wait <= 0) {
      said = change;
      quietUntil = Date.now() + 5000;
    } else {
      pending = change;
      if (!timer)
        timer = setTimeout(() => {
          timer = null;
          if (pending) said = pending;
          pending = null;
          quietUntil = Date.now() + 5000;
        }, wait);
    }
  });
  $effect(() => () => {
    if (timer) clearTimeout(timer);
  });

  function go(part: "requirements" | "scenarios" | "questions" | "untestable") {
    if (part === "questions" || part === "requirements") {
      app.doc = "review";
      return;
    }
    app.doc = "spec";
    if (part === "untestable" && shown?.untestable[0]) app.section = null;
  }
</script>

{#if shown}
  <div class="strip" aria-label="Readiness">
    {#each readinessParts(shown) as p, i (p.key)}
      {#if i > 0}<span class="sep" aria-hidden="true">·</span>{/if}
      <button class="part" class:attention={p.attention} onclick={() => go(p.key)}>
        {#if p.attention}<span aria-hidden="true">{"⚠ "}</span>{/if}{p.text}
      </button>
    {/each}
  </div>
  <div class="sr-only" role="status" aria-live="polite" data-testid="readiness-live">{said}</div>
{/if}

<style>
  .strip { display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.3rem; padding: 0.55rem 0.8rem; border-radius: 10px; background: var(--surface); box-shadow: var(--shadow); font-size: 0.88rem; margin-bottom: 0.75rem; }
  .part { border: 0; background: none; padding: 0.1rem 0.15rem; border-radius: 5px; color: var(--text); cursor: pointer; font: inherit; }
  .part:hover { background: var(--fill); }
  .part:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .part.attention { color: var(--attention); font-weight: 600; }
  .sep { color: var(--faint); }
</style>
