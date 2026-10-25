<script lang="ts">
  import { app } from "../lib/app.svelte";
  import type { JourneyStep } from "../lib/types";
  import Preflight from "./Preflight.svelte";
  import { mark, word } from "../lib/marks";

  /** Off the board: the title and the next step only, in one strip. */
  let { compact = false }: { compact?: boolean } = $props();

  const report = $derived(app.report?.data ?? null);
  const journey = $derived<JourneyStep[]>(
    report?.journey ?? ["spec", "plan", "tasks", "commit", "run", "review"].map((name, i) => ({ name, state: i === 0 ? "now" : "todo" })),
  );
  const primary = $derived(app.primary);
  const others = $derived(app.actions.filter((a) => a.group === "next" && a !== primary));
  const busy = $derived(!!app.job?.running);
  const stops = $derived(primary?.run?.kind === "run" ? app.blockers : []);
  const commitPaths = $derived(primary?.run?.kind === "commit" ? primary.run.paths : []);
  const labels: Record<string, string> = { spec: "Spec", plan: "Plan", tasks: "Tasks", commit: "Commit", run: "Run", review: "Review" };
  const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
  const totals = $derived(report?.totals ?? {});
  const pulse = $derived(
    ["verified", "regression_only", "attested", "ticked_without_evidence", "skipped", "ready", "blocked"]
      .map((k) => ({ k, n: totals[k] ?? 0 }))
      .filter((s) => s.n > 0),
  );
  const count = $derived(pulse.reduce((a, s) => a + s.n, 0));
  const headline = $derived(primary ? (primary.run?.kind === "skill" ? cap(primary.why) : cap(primary.label)) : "");
</script>

<section class="hero" aria-label="Where the feature is">
  <div class="title">
    <div>
      <h1>{report?.feature ?? app.report?.message ?? "speccoder"}</h1>
      {#if report?.selected_by}<span class="muted why">{report.selected_by}{#if report.branch}{" · "}<code>{report.branch}</code>{/if}</span>{/if}
    </div>
    {#if count}
      <div class="pulse" aria-label={pulse.map((s) => `${s.n} ${word(s.k)}`).join(", ")}>
        <div class="seg" aria-hidden="true">{#each pulse as s (s.k)}<span class="s-{s.k}" style="flex:{s.n}"></span>{/each}</div>
        <div class="counts">{#each pulse as s (s.k)}<span class="c s-{s.k}" title={word(s.k)}>{mark(s.k)} {s.n}</span>{/each}</div>
      </div>
    {/if}
  </div>
  {#if !compact}
  <Preflight />

  <ol class="journey" aria-label="Journey">
    {#each journey as s, i (s.name)}
      <li class={s.state} aria-current={s.state === "now" ? "step" : undefined}>
        <span class="dot" aria-hidden="true">{#if s.state === "done"}✓{:else if s.state === "skipped"}–{:else}{i + 1}{/if}</span>
        <span class="name">{labels[s.name] ?? s.name}</span>
        <span class="sr-only">{s.state === "done" ? "done" : s.state === "now" ? "next" : s.state === "skipped" ? "skipped" : "to do"}</span>
      </li>
    {/each}
  </ol>
  {/if}

  {#if primary}
    <div class="next" class:busy class:slim={compact}>
      <div class="text">
        <span class="eyebrow">Next</span>
        <h2>{headline}</h2>
        {#if primary.run?.kind !== "skill"}<p class="muted">{primary.why}</p>{/if}
        {#if commitPaths.length}
          <ul class="files">
            {#each commitPaths as p}<li class="mono"><span class="plus">+</span>{p}</li>{/each}
          </ul>
          <p class="muted small">Only these are committed — nothing else you have staged.</p>
        {/if}
        {#if stops.length}
          <p class="stop">✗ This run will stop: {stops.map((s) => s.check).join(", ")} —
            {#if compact}<button class="link" onclick={() => (app.view = "board")}>see how to fix it</button>{:else}fix it above first.{/if}</p>
        {/if}
        <button class="cmd mono" title="Copy the command" onclick={() => navigator.clipboard?.writeText(primary.command)}>
          <span class="muted">$</span> {primary.command}
        </button>
      </div>
      <div class="go">
        <button class="btn big" class:primary={!stops.length} disabled={busy} onclick={() => app.perform(primary)}>
          {#if busy}Working…{:else}{primary.run?.kind === "commit" ? "Commit" : primary.input ? "Start…" : stops.length ? "Run anyway" : "Run"} <kbd>N</kbd>{/if}
        </button>
        {#each others.slice(0, 2) as o (o.label)}
          <button class="btn small ghost" disabled={busy || !!o.blocked} title={o.blocked ?? o.why} onclick={() => app.perform(o)}>{o.label}</button>
        {/each}
      </div>
    </div>
  {:else if app.report && app.report.status !== "ok"}
    <div class="next problem">
      <div class="text">
        <span class="eyebrow">{app.report.code ?? "Needs you"}</span>
        <h2>{app.report.message}</h2>
        {#each app.report.fix ?? [] as f}<p class="mono small">{f}</p>{/each}
      </div>
    </div>
  {/if}
</section>

<style>
  .hero { display: grid; gap: 1.25rem; margin-bottom: 1.75rem; container-type: inline-size; }
  @container (max-width: 640px) { .next { flex-direction: column; align-items: stretch; } .go { justify-items: start; } .journey .name { display: none; } }
  .title { display: flex; align-items: flex-end; justify-content: space-between; gap: 1.5rem; flex-wrap: wrap; }
  .why code { font-size: 0.85em; }
  .pulse { display: grid; gap: 0.4rem; min-width: 260px; }
  .seg { display: flex; height: 8px; border-radius: 4px; overflow: hidden; gap: 2px; }
  .seg span { background: currentColor; }
  .counts { display: flex; gap: 0.35rem; justify-content: flex-end; }
  .c { font-size: 0.78rem; font-weight: 700; padding: 0 0.45rem; border-radius: 999px; }
  h1 { margin: 0; font-size: 1.6rem; letter-spacing: -0.02em; }
  .why { font-size: 0.9rem; }
  .journey { list-style: none; margin: 0; padding: 0; display: flex; align-items: center; }
  .journey li { display: flex; align-items: center; gap: 0.5rem; color: var(--muted); flex: 1; }
  .journey li:not(:last-child)::after { content: ""; flex: 1; height: 2px; margin: 0 0.6rem; background: var(--border); border-radius: 2px; }
  .journey li.done:not(:last-child)::after, .journey li.skipped:not(:last-child)::after { background: var(--verified); }
  .skipped { color: var(--muted); }
  .skipped .name { text-decoration: line-through; text-decoration-color: var(--border-strong); }
  .skipped .dot { border-style: dashed; }
  .dot {
    width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; font-size: 0.8rem; font-weight: 700;
    border: 2px solid var(--border-strong); background: var(--surface); flex: none;
  }
  .done { color: var(--text-2); }
  .done .dot { border-color: var(--verified); background: var(--verified); color: var(--surface); }
  .now { color: var(--accent); font-weight: 600; }
  .now .dot { border-color: var(--accent); color: var(--accent); box-shadow: 0 0 0 5px var(--accent-soft); animation: pulse 2.4s infinite; }
  @keyframes pulse { 50% { box-shadow: 0 0 0 9px transparent; } }
  .name { white-space: nowrap; }
  .next {
    display: flex; gap: 1.5rem; justify-content: space-between; align-items: center; padding: 1.25rem 1.5rem;
    background: var(--surface); border: 1px solid var(--border); border-left: 4px solid var(--accent);
    border-radius: var(--radius); box-shadow: var(--shadow);
  }
  .slim { padding: 0.7rem 1.1rem; }
  .slim h2 { font-size: 1rem; margin: 0; }
  .slim .text { display: flex; align-items: baseline; gap: 0.75rem; flex-wrap: wrap; }
  .slim .text > :global(:not(h2, .eyebrow, .stop)) { display: none; }
  .slim .stop { margin: 0; font-size: 0.85rem; }
  .slim .big { padding: 0.4rem 0.9rem; font-size: 0.9rem; }
  .link { all: unset; cursor: pointer; text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--focus); }
  .stop { color: var(--danger); font-weight: 600; margin: 0.4rem 0; }
  .next.problem { border-left-color: var(--attention); }
  .eyebrow { text-transform: uppercase; font-size: 0.72rem; letter-spacing: 0.08em; color: var(--accent); font-weight: 700; }
  .problem .eyebrow { color: var(--attention); }
  h2 { margin: 0.2rem 0 0.35rem; font-size: 1.2rem; letter-spacing: -0.01em; }
  .text p { margin: 0 0 0.4rem; }
  .small { font-size: 0.85rem; }
  .files { list-style: none; padding: 0; margin: 0.4rem 0; }
  .files li { color: var(--add); }
  .plus { display: inline-block; width: 1.2em; }
  .cmd { margin-top: 0.4rem; border: 1px dashed var(--border-strong); background: var(--surface-2); border-radius: 6px; padding: 0.3rem 0.6rem; cursor: copy; color: var(--text-2); max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .go { display: grid; gap: 0.4rem; justify-items: end; }
  .big { padding: 0.65rem 1.3rem; font-size: 1rem; }
  .big kbd { background: transparent; color: inherit; border-color: currentColor; opacity: 0.7; }
</style>
