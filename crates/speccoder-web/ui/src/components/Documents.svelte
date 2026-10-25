<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { diagrams } from "../lib/mermaid";
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
    <article class="prose" use:diagrams={html}>
      <p class="path mono muted">{shown?.path}</p>
      <!-- Rendered by the host from Markdown; raw HTML in the file is shown as text. -->
      {@html html}
    </article>
  </div>
{/if}

<style>
  .docs { display: grid; grid-template-columns: 11rem minmax(0, 1fr); gap: 2rem; max-width: 1180px; margin: 0 auto; }
  nav { display: grid; gap: 1px; align-content: start; position: sticky; top: 0; }
  nav button { text-align: left; border: 0; background: none; padding: 0.4rem 0.7rem; border-radius: 8px; cursor: pointer; color: var(--text-2); font-weight: 500; }
  nav button:hover { background: var(--fill); }
  nav button.on { background: var(--accent); color: #fff; font-weight: 600; }
  .prose {
    background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 2.25rem 2.75rem 2.75rem;
    max-width: 78ch; font-size: 15px; line-height: 1.65; letter-spacing: -0.008em;
  }
  .path { font-size: 0.75rem; margin: 0 0 1.25rem; }
  .prose :global(h1) { font: 700 1.85rem/1.15 var(--display); letter-spacing: -0.026em; margin: 0 0 1rem; }
  .prose :global(h2) { font: 650 1.3rem/1.25 var(--display); letter-spacing: -0.018em; margin: 2.25rem 0 0.75rem; }
  .prose :global(h3) { font: 650 1.05rem/1.3 var(--display); margin: 1.6rem 0 0.5rem; }
  .prose :global(p), .prose :global(li) { color: var(--text-2); }
  .prose :global(strong) { color: var(--text); font-weight: 600; }
  .prose :global(a) { color: var(--accent-ink); text-decoration: none; }
  .prose :global(code) { background: var(--fill); padding: 0.12em 0.38em; border-radius: 5px; font-size: 0.86em; }
  .prose :global(pre) { background: var(--surface-2); padding: 0.9rem 1.1rem; border-radius: var(--radius); overflow-x: auto; font-size: 0.85em; line-height: 1.55; }
  .prose :global(pre code) { background: none; padding: 0; }
  .prose :global(blockquote) { margin: 1rem 0; padding: 0.25rem 1rem; border-left: 3px solid var(--fill-2); color: var(--muted); }
  .prose :global(hr) { border: 0; border-top: 0.5px solid var(--separator); margin: 2rem 0; }
  .prose :global(table) { border-collapse: collapse; width: 100%; font-size: 0.88rem; margin: 1rem 0; }
  .prose :global(th) { text-align: left; font-weight: 600; color: var(--muted); font-size: 0.8rem; padding: 0.4rem 0.6rem; border-bottom: 0.5px solid var(--separator); }
  .prose :global(td) { padding: 0.45rem 0.6rem; border-bottom: 0.5px solid var(--separator); vertical-align: top; }
  .prose :global(ul), .prose :global(ol) { padding-left: 1.3rem; }
  .prose :global(li) { margin: 0.2rem 0; }
  .prose :global(li input[type="checkbox"]) { margin-right: 0.45rem; accent-color: var(--verified-fill); }
  .prose :global(.req) { font-family: var(--mono); font-size: 0.78em; padding: 0.1em 0.5em; border-radius: 999px; font-weight: 600; white-space: nowrap; }
  .prose :global(.req.cited) { background: var(--verified-bg); color: var(--verified); }
  .prose :global(figure.diagram) { margin: 1.25rem 0; padding: 1rem; border-radius: var(--radius); background: var(--surface-2); overflow-x: auto; text-align: center; }
  .prose :global(figure.diagram svg) { max-width: 100%; height: auto; }
  .prose :global(.diagram-error) { color: var(--danger); font-size: 0.82rem; text-align: left; margin: 0.5rem 0 0; }
  .prose :global(.req.uncited) { background: var(--regression-bg); color: var(--regression); }
  .empty { padding: 3rem; text-align: center; }
  @media (max-width: 900px) { .docs { grid-template-columns: 1fr; } nav { grid-auto-flow: column; overflow-x: auto; position: static; } .prose { padding: 1.5rem 1.25rem; } }
</style>
