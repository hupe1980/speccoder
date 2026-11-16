<script lang="ts">
  // Findings a gate or a view left (SARIF): one row each — the level as a
  // mark, the rule, the message, and the place, which opens the code view
  // at that line. Long lists show ten, then the rest on demand.
  import { app } from "../lib/app.svelte";
  import OpenIn from "./OpenIn.svelte";
  import type { SarifFinding } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { findings }: { findings: SarifFinding[] } = $props();
  let all = $state(false);
  const SHOWN = 10;
  const shown = $derived(all ? findings : findings.slice(0, SHOWN));
  const FINDING = { icon: "circle", label: "Finding" };
  const LEVEL: Record<string, { icon: string; label: string }> = {
    error: { icon: "xoctagon", label: "Error" },
    warning: { icon: "exclaim", label: "Warning" },
    note: { icon: "info", label: "Note" },
  };
  function open(f: SarifFinding) {
    if (!f.file) return;
    app.codeMode = "files";
    app.codeFile = f.file;
    app.codeLine = f.line ?? 1;
    app.view = "code";
  }
</script>

{#if findings.length === 0}
  <p class="none"><Icon name="check" size={12} weight={2.4} /> No findings</p>
{:else}
  <ul class="findings" aria-label="Findings">
    {#each shown as f, i (i)}
      {@const lv = LEVEL[f.level] ?? FINDING}
      <li class={f.level}>
        <span class="lv" role="img" title={lv.label} aria-label={lv.label}><Icon name={lv.icon} size={13} weight={2.2} /></span>
        <span class="msg">{f.message}</span>
        <span class="rule mono">{f.rule}</span>
        {#if f.file}
<span class="where"><button class="at mono" onclick={() => open(f)} title="Open {f.file}{f.line ? ` at line ${f.line}` : ''}">{f.file}{f.line ? `:${f.line}` : ""}</button><OpenIn path={f.file} line={f.line ?? null} /></span>
        {/if}
      </li>
    {/each}
  </ul>
  {#if findings.length > SHOWN}
    <button class="btn small ghost more" onclick={() => (all = !all)}>{all ? "Show fewer" : `Show all ${findings.length}`}</button>
  {/if}
{/if}

<style>
  .findings { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; border-radius: var(--radius); overflow: hidden; background: var(--line); }
  li { display: grid; grid-template-columns: auto 1fr auto; grid-template-areas: "lv msg rule" "lv at at"; gap: 0.1rem 0.55rem; align-items: baseline; padding: 0.5rem 0.7rem; background: var(--surface); }
  .lv { grid-area: lv; align-self: start; padding-top: 0.1rem; color: var(--muted); }
  li.error .lv { color: var(--danger); }
  li.warning .lv { color: var(--attention); }
  li.note .lv { color: var(--accent); }
  .msg { grid-area: msg; font-size: 0.86rem; overflow-wrap: anywhere; }
  .rule { grid-area: rule; font-size: 0.72rem; color: var(--muted); }
  .at { grid-area: at; justify-self: start; all: unset; grid-area: at; cursor: pointer; font-size: 0.74rem; color: var(--accent-ink); overflow-wrap: anywhere; }
  .at:hover { text-decoration: underline; }
  .at:focus-visible { outline: 2px solid var(--focus); border-radius: 3px; }
  .none { margin: 0; display: inline-flex; gap: 0.35rem; align-items: center; color: var(--verified); font-size: 0.85rem; }
  .more { margin-top: 0.35rem; }
  .where { grid-area: at; display: inline-flex; align-items: center; gap: 0.15rem; min-width: 0; }
</style>
