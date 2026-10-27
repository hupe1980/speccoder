<script lang="ts">
  // A task's evidence as six steps in a fixed order — requirement, frozen
  // tests, red, diff, green, strength — each labelled with where it comes
  // from. The same steps and labels as the check run's tour: the runtime
  // builds them, this only lays them out.
  import { get, latest, q } from "../lib/api";
  import type { EvidenceTour } from "../lib/schema.gen";
  import Code from "./Code.svelte";

  let { feature, task }: { feature: string | undefined; task: string } = $props();
  const tourOf = latest();
  let tour = $state<EvidenceTour | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    tour = null;
    error = null;
    tourOf(
      get<{ data?: EvidenceTour; message?: string; error?: string }>(`/api/tour${q({ feature, task })}`),
      (r) => {
        if (r.data) tour = r.data;
        else error = r.error ?? r.message ?? "no tour";
      },
      (e) => (error = String(e)),
    );
  });

  const sources: Record<string, string> = { signed: "signed", committed: "committed", not_recorded: "not recorded" };
  // The first line is the step's statement; the rest is quoted text or a
  // fenced block, shown as written.
  const split = (md: string) => {
    const [first, ...rest] = md.split("\n");
    const block = rest
      .filter((l) => !l.startsWith("```"))
      .map((l) => l.replace(/^> ?/, ""))
      .join("\n")
      .trim();
    return { first: first ?? "", block };
  };
</script>

{#if error}
  <p class="muted small">{error}</p>
{:else if !tour}
  <p class="muted small">Reading the evidence…</p>
{:else}
  <ol class="tour" aria-label="Evidence tour of {tour.task}">
    {#each tour.steps as s (s.kind)}
      {@const body = split(s.body_markdown)}
      <li class="step src-{s.source}">
        <div class="head">
          <strong>{s.title}</strong>
          <span class="src" title="Where this step comes from">{sources[s.source]}</span>
        </div>
        <p><Code text={body.first} /></p>
        {#if body.block}<pre class="mono">{body.block}</pre>{/if}
      </li>
    {/each}
  </ol>
  {#if tour.unverifiable}
    <p class="unverifiable">Unverifiable: {tour.unverifiable}. The steps after the requirement are not shown: nothing verifiable backs them.</p>
  {/if}
  <p class="note">Read as recorded: <code>speccoder attest verify --branch</code> checks the signatures.</p>
{/if}

<style>
  .tour { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.5rem; counter-reset: step; }
  .step { counter-increment: step; padding: 0.6rem 0.85rem; border-radius: var(--radius); background: var(--surface); display: grid; gap: 0.3rem; }
  .head { display: flex; justify-content: space-between; align-items: baseline; gap: 0.5rem; }
  .head strong::before { content: counter(step) ". "; color: var(--muted); font-weight: 500; }
  .src { font-size: 0.72rem; padding: 0.05rem 0.45rem; border-radius: 6px; background: var(--fill); color: var(--text-2); }
  .src-signed .src { color: var(--ok, #30d158); }
  .src-not_recorded { opacity: 0.85; }
  .src-not_recorded .src { color: var(--muted); }
  p { margin: 0; font-size: 0.86rem; color: var(--text-2); overflow-wrap: anywhere; }
  pre { margin: 0; padding: 0.5rem 0.6rem; border-radius: 8px; background: var(--fill); font-size: 0.76rem; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 14rem; overflow: auto; }
  .unverifiable { color: var(--danger); font-size: 0.84rem; margin-top: 0.5rem; }
  .note { margin-top: 0.4rem; font-size: 0.78rem; color: var(--muted); }
  .muted { color: var(--muted); }
  .small { font-size: 0.82rem; }
</style>
