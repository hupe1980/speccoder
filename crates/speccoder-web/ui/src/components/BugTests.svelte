<script lang="ts">
  // A bug unit's Tests tab (spec 093): its reproductions as the person
  // confirmed them, read only — a bug's tests are written and confirmed by
  // its own pipeline, never at review.
  import type { BugTests } from "../lib/schema.gen";
  import Icon from "./Icon.svelte";

  let { bugs }: { bugs: BugTests[] } = $props();
  const when = (s: number) => new Date(s * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  const word = (c: string) => (c === "assertion" ? "red: assertion" : c === "passed" ? "passes (guards what stays)" : c.replaceAll("_", " "));
</script>

{#each bugs as b (b.task)}
  <article class="bug" aria-label="{b.task} reproductions">
    <header>
      <span class="id mono">{b.task}</span> <span class="title">{b.title}</span>
      <span class="chip"><Icon name="lock" size={10} weight={2.2} /> {`Confirmed by ${b.confirmation.by} · ${when(b.confirmation.at)}`}</span>
    </header>
    {#each b.reproductions as t (t.file)}
      <details class="repro">
        <summary><span class="sc mono">{t.statement}</span> <span class="mono path">{t.file}</span> <span class="muted mono">{t.test}</span> <span class="pill">{word(t.red_class)}</span></summary>
        {#if t.content != null}<pre class="mono" aria-label="{t.file} as confirmed">{t.content}</pre>{:else}<p class="bad"><Icon name="exclaim" size={11} /> The confirmed copy is missing or changed: run the bug again.</p>{/if}
      </details>
    {/each}
    <p class="muted small">Read only: <code>speccoder bug confirm {b.task}</code> decided them.</p>
  </article>
{/each}

<style>
  .bug { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 0.9rem 1.1rem; display: grid; gap: 0.45rem; }
  header { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .id { font-weight: 650; font-size: 0.82rem; }
  .title { font-weight: 600; }
  .chip { margin-left: auto; display: inline-flex; gap: 0.25rem; align-items: center; font-size: 0.72rem; font-weight: 650; padding: 0.08rem 0.55rem; border-radius: 999px; background: var(--verified-bg); color: var(--verified); }
  .repro { border-radius: var(--radius); background: var(--surface-2); padding: 0.4rem 0.7rem; font-size: 0.8rem; }
  .repro summary { cursor: pointer; display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; }
  .sc { font-size: 0.7rem; padding: 0.05rem 0.4rem; border-radius: 999px; background: var(--fill); }
  .pill { margin-left: auto; font-size: 0.72rem; font-weight: 650; }
  pre { margin: 0.4rem 0 0; white-space: pre-wrap; font-size: 0.76rem; }
  .bad { margin: 0.3rem 0 0; color: var(--danger); font-size: 0.8rem; }
  .small { font-size: 0.76rem; margin: 0; }
</style>
