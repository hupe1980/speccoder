<script lang="ts">
  // The views extensions place here: on a feature (`feature`) or the
  // project (none). Each card is data shown by speccoder's own widgets —
  // findings, a table, metrics, a document with its diagrams — with when it
  // ran and Run again; nothing of the extension reaches the page but data.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { diagrams } from "../lib/mermaid";
  import type { Reply, ViewShown, ViewsReport } from "../lib/types";
  import Findings from "./Findings.svelte";
  import Icon from "./Icon.svelte";

  let { feature = null }: { feature?: string | null } = $props();
  let reply = $state<Reply<ViewsReport> | null>(null);
  let loading = $state(false);
  // Again when the feature changes or a job ends (a Run again, a trust).
  $effect(() => {
    void app.job?.running;
    loading = true;
    void get<Reply<ViewsReport>>(`/api/extensions/views${q({ feature: feature ?? undefined })}`)
      .then((r) => (reply = r))
      .catch(() => {})
      .finally(() => (loading = false));
  });
  const all = $derived(reply?.data?.views ?? []);
  // Views of an extension that waits for trust: one line per extension.
  const views = $derived(all.filter((v) => !v.waits_for_trust));
  const waiting = $derived([...new Set(all.filter((v) => v.waits_for_trust).map((v) => v.extension))]);
  const count = (ext: string) => all.filter((v) => v.extension === ext).length;

  function ago(iso: string) {
    const s = Math.max(0, (Date.now() - Date.parse(iso)) / 1000);
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400) return `${Math.round(s / 3600)} h ago`;
    return new Date(iso).toLocaleDateString();
  }
  const again = (v: ViewShown) =>
    app.perform(
      offer({ kind: "view_run", extension: v.extension, view: v.view.name, feature: feature ?? undefined }, {
        group: "extensions",
        label: `run ${v.view.title} again`,
        why: "",
        command: "",
      }),
    );
  const review = () => {
    app.view = "settings";
    app.settingsSection = "extensions";
  };
</script>

{#if all.length}
  <section class="views" aria-label="Extensions">
    <h3><Icon name="grid" size={14} weight={2} /> Extensions</h3>
    {#each waiting as ext (ext)}
      <p class="wait"><Icon name="hand" size={13} weight={2.2} /> {count(ext) === 1 ? "A view" : `${count(ext)} views`} of {ext} {count(ext) === 1 ? "waits" : "wait"} for your trust. <button class="link" onclick={review}>Review {ext}…</button></p>
    {/each}
    <div class="grid">
      {#each views as v (v.extension + v.view.name)}
        {@const r = v.result}
        <article class="card" class:wide={v.view.widget === "markdown" || v.view.widget === "table"}>
          <header>
            <strong>{v.view.title}</strong>
            <span class="from">{v.extension}</span>
            {#if r}<span class="when" title={r.ran_at}>{ago(r.ran_at)}</span>{/if}
            {#if "run" in v.view.source}
              <button class="icon-btn" aria-label="Run {v.view.title} again" title="Run again" disabled={!!app.job?.running} onclick={() => again(v)}><Icon name="loop" size={13} weight={2.2} /></button>
            {/if}
          </header>
          {#if !r}
            <div class="center"><span class="spinner" aria-label="Loading"></span></div>
          {:else if r.error}
            <p class="bad"><Icon name="exclaim" size={13} /> {r.error}</p>
          {:else if r.data?.widget === "findings"}
            <Findings findings={r.data.findings} />
          {:else if r.data?.widget === "metrics"}
            <dl class="metrics">
              {#each r.data.metrics as [name, value] (name)}<div><dt>{name.replaceAll("_", " ")}</dt><dd>{value}</dd></div>{/each}
            </dl>
          {:else if r.data?.widget === "table"}
            <div class="scroll">
              <table>
                <thead><tr>{#each r.data.columns as c (c)}<th scope="col">{c}</th>{/each}</tr></thead>
                <tbody>{#each r.data.rows as row, i (i)}<tr>{#each row as cell, j (j)}<td>{cell}</td>{/each}</tr>{/each}</tbody>
              </table>
            </div>
          {:else if r.data?.widget === "markdown"}
            <div class="prose" use:diagrams={r.data.html}>{@html r.data.html}</div>
          {/if}
        </article>
      {/each}
    </div>
    {#if loading && !reply}<span class="sr">Loading the extensions' views</span>{/if}
  </section>
{/if}

<style>
  .views { display: grid; gap: 0.6rem; margin-top: 1.25rem; }
  h3 { margin: 0; display: flex; gap: 0.4rem; align-items: center; font: 650 0.95rem/1.2 var(--display); color: var(--text-2); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 0.75rem; }
  .card { border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow); padding: 0.8rem 0.95rem; display: grid; gap: 0.55rem; align-content: start; min-width: 0; }
  .card.wide { grid-column: 1 / -1; }
  header { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  header strong { font: 650 0.92rem/1.2 var(--display); }
  .from { font-size: 0.72rem; padding: 0.05rem 0.45rem; border-radius: 999px; background: var(--fill); color: var(--muted); }
  .when { font-size: 0.74rem; color: var(--muted); margin-left: auto; }
  .icon-btn { all: unset; cursor: pointer; width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; color: var(--muted); }
  .icon-btn:hover { background: var(--fill); color: var(--text); }
  .icon-btn:focus-visible { box-shadow: 0 0 0 2.5px var(--focus); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }
  .wait { margin: 0; color: var(--attention); font-size: 0.85rem; display: flex; gap: 0.35rem; align-items: center; flex-wrap: wrap; }
  .link { all: unset; cursor: pointer; color: var(--accent-ink); text-decoration: underline; }
  .bad { margin: 0; color: var(--danger); font-size: 0.85rem; display: flex; gap: 0.35rem; align-items: baseline; overflow-wrap: anywhere; }
  .metrics { margin: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: 0.5rem; }
  .metrics div { padding: 0.5rem 0.6rem; border-radius: 10px; background: var(--surface-2); display: grid; gap: 0.1rem; }
  .metrics dt { font-size: 0.72rem; color: var(--muted); }
  .metrics dd { margin: 0; font: 650 1.15rem/1.2 var(--display); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  .scroll { overflow: auto; max-height: 50vh; border-radius: 10px; border: 1px solid var(--line); }
  table { border-collapse: collapse; width: 100%; font-size: 0.82rem; }
  th { position: sticky; top: 0; background: var(--surface-2); text-align: left; font-weight: 600; color: var(--muted); }
  th, td { padding: 0.4rem 0.6rem; border-bottom: 1px solid var(--line); vertical-align: top; overflow-wrap: anywhere; }
  .prose { font-size: 0.9rem; overflow-wrap: anywhere; max-height: 60vh; overflow: auto; }
  .prose :global(h1) { font: 650 1.2rem/1.25 var(--display); margin: 0.2rem 0 0.5rem; }
  .prose :global(h2) { font: 650 1.05rem/1.3 var(--display); margin: 0.9rem 0 0.4rem; }
  .prose :global(h3) { font-size: 0.95rem; margin: 0.8rem 0 0.3rem; }
  .prose :global(table) { border-collapse: collapse; margin: 0.6rem 0; font-size: 0.84rem; }
  .prose :global(th), .prose :global(td) { padding: 0.35rem 0.6rem; border-bottom: 1px solid var(--line); text-align: left; vertical-align: top; }
  .prose :global(th) { color: var(--muted); font-weight: 600; background: var(--surface-2); }
  .center { display: grid; place-items: center; padding: 1rem; }
  .spinner { width: 18px; height: 18px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
</style>
