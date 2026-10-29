<script lang="ts">
  // The external agent making an attempt: its name, whether it asks before
  // it acts (or decides alone — then an empty Needs You proves nothing),
  // what speccoder decided of what it asked, and its plan as a checklist —
  // the agent's own words, a claim.
  import Icon from "./Icon.svelte";

  type Agent = { name: string; mode?: string | null; decides_alone: boolean; plan: { content: string; status: string }[]; asked: number; allowed: number; waited: number; refused: number };
  type Filter = "allow" | "approval" | "deny" | null;
  let { agent, dark = false, filter = null, onfilter = null, onfix = null }: { agent: Agent; dark?: boolean; filter?: Filter; onfilter?: ((f: Filter) => void) | null; onfix?: (() => void) | null } = $props();
  // Each count filters the call log to its calls (again: all of them).
  const counts = $derived(
    [
      [agent.asked, "asked", null],
      [agent.allowed, "allowed", "allow"],
      [agent.waited, "waited for you", "approval"],
      [agent.refused, "refused", "deny"],
    ].filter(([n, , f]) => f === null || f === "allow" || (n as number) > 0) as [number, string, Filter][],
  );
</script>

<div class="agent" class:dark>
  <p class="who">
    <Icon name="sparkle" size={12} />
    <strong>{agent.name}</strong>
    {#if agent.decides_alone}
      <span class="alone" title={agent.mode ?? ""}><Icon name="exclaim" size={11} weight={2.4} /> decides alone</span>
    {:else}
      <span class="asks" title={agent.mode ?? ""}>asks before acting</span>
    {/if}
  </p>
  {#if agent.decides_alone}
    <p class="mode">{agent.mode ? `${agent.mode} — ` : ""}what it does without asking never reaches speccoder.{#if onfix}{" "}<button class="link" onclick={onfix}>Make it ask first</button>{/if}</p>
  {/if}
  <p class="tally mono" title="What speccoder decided of the calls the agent asked about, its files and commands">
    {#each counts as [n, what, f], i (what)}
      {#if i}<span aria-hidden="true">{" · "}</span>{/if}
      {#if onfilter}
        <button class="count" class:on={filter === f && f !== null} onclick={() => onfilter?.(filter === f ? null : f)}>{n} {what}</button>
      {:else}<span>{n} {what}</span>{/if}
    {/each}
  </p>
  {#if agent.plan.length}
    <ol class="plan" aria-label="{agent.name}'s plan (its own words)">
      {#each agent.plan as e, i (i)}
        <li class={e.status}>
          <span class="g" aria-hidden="true">{#if e.status === "completed"}<Icon name="check" size={10} weight={3} />{:else if e.status === "in_progress"}<span class="dot"></span>{/if}</span>
          <span>{e.content}</span>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .agent { display: grid; gap: 0.25rem; padding: 0.5rem 0.6rem; border-radius: 10px; background: var(--fill); font-size: 0.8rem; }
  .agent.dark { background: rgba(255, 255, 255, 0.07); }
  p { margin: 0; }
  .who { display: flex; align-items: center; gap: 0.4rem; }
  .asks { color: var(--muted); }
  .dark .asks { color: rgba(235, 235, 245, 0.6); }
  .alone { display: inline-flex; align-items: center; gap: 0.2rem; padding: 0 0.4rem; border-radius: 6px; font-weight: 650; color: #000; background: #ff9f0a; }
  .mode { color: #ff9f0a; font-size: 0.76rem; }
  .tally { color: var(--text-2); font-size: 0.76rem; }
  .count { all: unset; cursor: pointer; border-radius: 4px; padding: 0 0.15rem; }
  .count:hover, .count:focus-visible { background: var(--fill-2); }
  .count.on { text-decoration: underline; }
  .link { all: unset; cursor: pointer; color: var(--accent); text-decoration: underline; }
  .dark .tally { color: rgba(235, 235, 245, 0.72); }
  .plan { list-style: none; margin: 0.15rem 0 0; padding: 0; display: grid; gap: 0.15rem; }
  .plan li { display: grid; grid-template-columns: 14px 1fr; gap: 0.35rem; align-items: baseline; }
  .plan li.completed span:last-child { text-decoration: line-through; opacity: 0.6; }
  .g { display: grid; place-items: center; width: 12px; height: 12px; border-radius: 50%; box-shadow: inset 0 0 0 1px currentColor; opacity: 0.7; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: #64d2ff; }
</style>
