<script lang="ts">
  import { app } from "../lib/app.svelte";
  let pick = $state("spec");
  const docs = $derived(app.documents);
  const shown = $derived(docs.find((d) => d.name === pick) ?? docs[0]);
  const names: Record<string, string> = { spec: "Spec", plan: "Plan", tasks: "Tasks", research: "Research", "data-model": "Data model", quickstart: "Quickstart" };
  // Requirement IDs become chips that say which task cites them (the report's trace).
  const cites = $derived(new Map((app.report?.data?.trace ?? []).map((t) => [t.requirement, t.task])));
  const html = $derived(
    (shown?.html ?? "").replace(/\b((?:FR|SC|NFR)-\d{2,4})\b/g, (id) => {
      const task = cites.get(id);
      return `<span class="req ${task ? "cited" : "uncited"}" title="${task ? `cited by ${task}` : "no task cites it"}">${id}</span>`;
    }),
  );
</script>

{#if docs.length === 0}
  <div class="empty muted">Nothing written yet.</div>
{:else}
  <div class="docs">
    <nav aria-label="Documents">
      {#each docs as d (d.name)}
        <button class:on={shown?.name === d.name} onclick={() => (pick = d.name)}>{names[d.name] ?? d.name}</button>
      {/each}
    </nav>
    <article class="prose">
      <p class="path mono muted">{shown?.path}</p>
      <!-- Rendered by the host from Markdown; raw HTML in the file is shown as text. -->
      {@html html}
    </article>
  </div>
{/if}

<style>
  .docs { display: grid; grid-template-columns: 11rem 1fr; gap: 1.5rem; }
  nav { display: grid; gap: 2px; align-content: start; position: sticky; top: 0; }
  nav button { text-align: left; border: 0; background: none; padding: 0.45rem 0.7rem; border-radius: 6px; cursor: pointer; color: var(--text-2); }
  nav button.on { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .prose { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 1.5rem 2rem; max-width: 72ch; line-height: 1.65; }
  .path { font-size: 0.78rem; margin: 0 0 1rem; }
  .prose :global(h1) { font-size: 1.5rem; letter-spacing: -0.02em; margin-top: 0; }
  .prose :global(h2) { font-size: 1.15rem; margin-top: 1.8rem; border-bottom: 1px solid var(--border); padding-bottom: 0.3rem; }
  .prose :global(h3) { font-size: 1rem; margin-top: 1.4rem; }
  .prose :global(code) { background: var(--surface-2); padding: 0.1em 0.35em; border-radius: 4px; }
  .prose :global(pre) { background: var(--surface-2); padding: 0.8rem 1rem; border-radius: 8px; overflow-x: auto; }
  .prose :global(pre code) { background: none; padding: 0; }
  .prose :global(table) { border-collapse: collapse; width: 100%; font-size: 0.9rem; }
  .prose :global(th), .prose :global(td) { border: 1px solid var(--border); padding: 0.35rem 0.6rem; text-align: left; }
  .prose :global(li input[type="checkbox"]) { margin-right: 0.4rem; }
  .prose :global(.req) { font-family: var(--mono); font-size: 0.8em; padding: 0.05em 0.4em; border-radius: 999px; font-weight: 600; }
  .prose :global(.req.cited) { background: var(--verified-bg); color: var(--verified); }
  .prose :global(.req.uncited) { background: var(--regression-bg); color: var(--regression); }
  .empty { padding: 3rem; text-align: center; }
</style>
