<script lang="ts">
  // The agents that can make attempts, one card each, with three honest
  // ticks — installed, signed in, asks first — and the one action a
  // missing tick has: its install command, its own login in a terminal of
  // yours, or why it cannot ask first. Cards show what is known at once,
  // then what each installed agent says when asked for a session (never a
  // prompt). Choosing one is `[executor] agent`.
  import { app } from "../lib/app.svelte";
  import type { AgentCard, AgentTick } from "../lib/types";
  import Icon from "./Icon.svelte";
  import FixSteps from "./FixSteps.svelte";

  let { use = null, compact = false }: { use?: ((id: string) => void) | null; compact?: boolean } = $props();
  let checking = $state(true);
  let failed = $state<string | null>(null);
  $effect(() => {
    void app
      .loadAgents(false)
      .then(() => app.loadAgents(true))
      .catch((e) => (failed = String((e as Error).message ?? e)))
      .finally(() => (checking = false));
  });
  const cards = $derived(app.agents?.cards ?? []);
  const ticks = (c: AgentCard): [string, AgentTick][] => [
    ["Installed", c.installed],
    ["Signed in", c.signed_in],
    ["Asks first", c.asks_first],
  ];
  const ready = (c: AgentCard) => c.installed.state === "yes" && c.signed_in.state !== "no" && c.asks_first.state !== "no";
</script>

<section class="agents" class:compact aria-label="Agents">
  {#if failed}<p class="muted small">The agents could not be listed: {failed}</p>{/if}
  <ul>
    {#each cards as c (c.id)}
      <li class:chosen={c.chosen} class:ready={ready(c)}>
        <header>
          <strong>{c.name}</strong>
          {#if c.chosen}<span class="badge">In use</span>{/if}
          {#if use && !c.chosen && c.installed.state === "yes"}
            <button class="btn small tinted" onclick={() => use?.(c.id)}>Use {c.name}</button>
          {/if}
        </header>
        {#each ticks(c) as [what, t] (what)}
          <div class="tick {t.state}">
            <span class="g" aria-hidden="true">
              {#if t.state === "yes"}<Icon name="check" size={11} weight={3} />{:else if t.state === "no"}<Icon name="xmark" size={10} weight={3} />{:else if checking && what === "Signed in"}<span class="spin"></span>{:else}·{/if}
            </span>
            <span class="what">{what}<span class="sr-only">: {t.state}</span></span>
            <span class="d">{t.detail}</span>
            {#if t.state !== "yes" && t.fix_steps.length}<span class="fix"><FixSteps steps={t.fix_steps} limit={1} /></span>{/if}
          </div>
        {/each}
      </li>
    {:else}
      {#if checking}<li class="muted small">Looking for agents…</li>{/if}
    {/each}
  </ul>
  {#if !compact}
    <p class="muted small note"><Icon name="shield" size={11} /> speccoder decides every call an agent asks about, and the files and commands it runs through speccoder. Everything the agent does itself asks you — it runs outside the sandbox; you can allow plain reads for the task. What it does without asking — an allow rule in its own settings — stays outside, and the run says so when it sees it.</p>
  {/if}
</section>

<style>
  .agents ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.6rem; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); }
  .compact ul { grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); }
  li { display: grid; gap: 0.35rem; padding: 0.7rem 0.8rem; border-radius: 12px; background: var(--fill); box-shadow: inset 0 0 0 0.5px var(--separator, transparent); }
  li.chosen { box-shadow: inset 0 0 0 1.5px var(--accent); }
  header { display: flex; align-items: center; gap: 0.5rem; }
  header strong { flex: 1; }
  .badge { font-size: 0.7rem; font-weight: 650; color: var(--accent); }
  .tick { display: grid; grid-template-columns: 16px auto 1fr; gap: 0.1rem 0.45rem; align-items: baseline; font-size: 0.8rem; }
  .g { display: grid; place-items: center; width: 14px; height: 14px; border-radius: 50%; font-size: 0.8rem; color: var(--muted); }
  .yes .g { background: #34c759; color: #fff; }
  .no .g { background: #ff3b30; color: #fff; }
  .what { font-weight: 600; white-space: nowrap; }
  .d { color: var(--muted); min-width: 0; overflow-wrap: anywhere; }
  .fix { grid-column: 2 / -1; }
  .spin { width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--fill-2); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .note { margin: 0.6rem 0 0; display: flex; gap: 0.35rem; align-items: baseline; }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
