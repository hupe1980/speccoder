<script lang="ts">
  import { app } from "../lib/app.svelte";
  // Failures first: they stop a run; warnings only spoil one.
  const items = $derived([...app.preflight].sort((a, b) => (a.level === "fail" ? 0 : 1) - (b.level === "fail" ? 0 : 1)));
  const failing = $derived(app.blockers.length > 0);

  function copy(cmd: string) {
    navigator.clipboard?.writeText(cmd);
    app.toast({ tone: "ok", text: "Copied — run it in your shell", detail: cmd });
  }
  // A fix line is a command, sometimes followed by "(why)" or prose.
  const command = (f: string) => f.replace(/^→\s*/, "").replace(/\s+\(.*\)$/, "");
  const runnable = (f: string) => /^[a-z][\w.-]*( |$)/.test(command(f)) && !/\b(set|add|configure)\b.*\bto\b/i.test(command(f));
</script>

{#if items.length}
  <section class="pre" class:fail={failing} aria-label="Before you run">
    <span class="lead">{failing ? "✗ Fix before running" : "~ Worth a look"}</span>
    <ul>
      {#each items as i (i.check)}
        <li class={i.level}>
          <strong>{i.level === "fail" ? "✗" : "~"} {i.check}</strong>
          <span class="d">{i.detail}</span>
          <span class="fixes">
            {#each i.fix.slice(0, 2) as f}
              {#if runnable(f)}
                <button class="fix mono" title="Copy" onclick={() => copy(command(f))}>$ {command(f)}</button>
              {:else}
                <span class="hint">{f.replace(/^→\s*/, "")}</span>
              {/if}
            {/each}
          </span>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .pre { display: grid; gap: 0.5rem; padding: 0.75rem 1rem; border-radius: var(--radius); background: var(--surface); border: 1px solid var(--border); border-left: 3px solid var(--regression); }
  .pre.fail { border-left-color: var(--danger); }
  .lead { font-weight: 700; font-size: 0.85rem; color: var(--regression); }
  .fail .lead { color: var(--danger); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.35rem; }
  li { display: grid; grid-template-columns: 8rem 1fr; column-gap: 0.75rem; row-gap: 0.2rem; align-items: baseline; }
  .fixes { grid-column: 2; }
  strong { font-size: 0.85rem; }
  li.fail strong { color: var(--danger); }
  li.warn strong { color: var(--regression); }
  .d { color: var(--text-2); font-size: 0.88rem; }
  .fixes { display: flex; flex-wrap: wrap; gap: 0.35rem; }
  .fixes:empty { display: none; }
  .fix { border: 1px dashed var(--border-strong); background: var(--surface-2); border-radius: 6px; padding: 0.1rem 0.5rem; font-size: 0.78rem; cursor: copy; white-space: nowrap; }
  .hint { color: var(--muted); font-size: 0.8rem; }
  @media (max-width: 700px) { li { grid-template-columns: 1fr; } .fixes { grid-column: 1; } }
</style>
