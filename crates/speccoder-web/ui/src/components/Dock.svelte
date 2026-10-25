<script lang="ts">
  import { app } from "../lib/app.svelte";
  // Open while a job runs; a finished one found on arrival stays a line.
  let open = $state(false);
  $effect(() => {
    if (app.job?.running) open = true;
  });
  const stages = ["baseline", "red", "attempt", "verify", "audit", "mutation", "strength", "monitor", "commit"];
  const at = $derived(new Map(app.stages.map((s) => [s.stage, s])));
  const sec = (ms: number | null) => (ms == null ? "" : ms < 10000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms / 1000)}s`);
  const glyph: Record<string, string> = { started: "◐", passed: "✓", failed: "✗", skipped: "–" };
  let log: HTMLElement | undefined = $state();
  $effect(() => {
    void app.calls.length;
    if (log) log.scrollTop = log.scrollHeight;
  });
</script>

{#if app.job}
  <section class="dock" class:open aria-label="The job">
    <header>
      <button class="btn ghost small" onclick={() => (open = !open)} aria-expanded={open}>{open ? "▾" : "▸"}</button>
      <span class="state" class:run={app.job.running}>{app.job.running ? "◐ Working" : app.job.result?.status === "ok" ? "✓ Done" : "■ Ended"}</span>
      <code class="cmd">{app.job.command}</code>
      {#if app.task}<span class="muted">{app.task.id} {app.task.title}</span>{/if}
      {#if app.usage.in || app.usage.out}<span class="usage muted mono">{app.usage.in.toLocaleString()} in · {app.usage.out.toLocaleString()} out{#if app.usage.usd != null} · ${app.usage.usd.toFixed(4)}{/if}</span>{/if}
      {#if app.job.running}<button class="btn small danger" onclick={() => app.stop()} title="stops before the next task; the attempt stays resumable">Stop</button>{/if}
    </header>
    {#if open}
      <div class="body">
        <ol class="pipeline" aria-label="Pipeline">
          {#each stages as s}
            {@const v = at.get(s)}
            <li class="st-{v?.state ?? 'todo'}" title={v?.detail ?? ""}>
              <span class="g">{v ? glyph[v.state] ?? "·" : "·"}</span>{s}<span class="t">{v?.state === "started" ? (v.detail ?? "") : sec(v?.elapsed_ms ?? null)}</span>
            </li>
          {/each}
        </ol>
        <div class="log mono" bind:this={log} aria-live="polite">
          {#each app.calls as c}<div class:deny={c.includes(" deny")} class:ask={c.includes(" approval")}>{c}</div>{/each}
          {#if app.job.result}<div class="result s-{app.job.result.status === 'ok' ? 'verified' : 'failed'}">{app.job.result.message}</div>{/if}
        </div>
      </div>
    {/if}
  </section>
{/if}

<style>
  .dock { grid-area: dock; border-top: 1px solid var(--border); background: var(--surface); }
  header { display: flex; align-items: center; gap: 0.8rem; padding: 0.45rem 1rem; }
  .state { font-weight: 700; }
  .state.run { color: var(--running); }
  .cmd { background: var(--surface-2); padding: 0.1rem 0.45rem; border-radius: 5px; }
  .usage { margin-left: auto; font-size: 0.8rem; }
  .body { display: grid; grid-template-columns: auto 1fr; gap: 1rem; padding: 0 1rem 0.8rem; height: 190px; }
  .pipeline { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; align-content: start; min-width: 12rem; }
  .pipeline li { display: grid; grid-template-columns: 1.4rem 1fr auto; font-size: 0.85rem; color: var(--muted); }
  .t { font-size: 0.78rem; }
  .st-started { color: var(--running); font-weight: 600; }
  .st-passed { color: var(--verified); }
  .st-failed { color: var(--danger); }
  .log { overflow: auto; background: var(--surface-2); border-radius: 8px; padding: 0.5rem 0.75rem; font-size: 0.8rem; line-height: 1.55; }
  .deny { color: var(--danger); }
  .ask { color: var(--attention); }
  .result { margin-top: 0.4rem; padding: 0.3rem 0.5rem; border-radius: 6px; font-weight: 600; }
</style>
