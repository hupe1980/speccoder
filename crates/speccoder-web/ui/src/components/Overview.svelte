<script lang="ts">
  // Every feature at a glance — the spec of the specs: each one's ring of
  // verdicts, where it is on the journey, how many of its requirements are
  // proven by evidence, and what it waits for. A card opens its board.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import type { CostReport, Reply, Report } from "../lib/types";
  import Ring from "./Ring.svelte";
  import Icon from "./Icon.svelte";
  import GitHub from "./GitHub.svelte";

  const features = $derived(app.project?.features?.features ?? []);
  let reports = $state<Record<string, Report | null>>({});
  let costs = $state<Record<string, CostReport | null>>({});

  $effect(() => {
    void app.report; // refresh with the page
    for (const f of features) {
      void get<Reply<Report>>(`/api/report${q({ feature: f.name })}`)
        .then((r) => (reports = { ...reports, [f.name]: r.data ?? null }))
        .catch(() => (reports = { ...reports, [f.name]: null }));
      void get<Reply<CostReport>>(`/api/cost${q({ feature: f.name })}`)
        .then((r) => (costs = { ...costs, [f.name]: r.data ?? null }))
        .catch(() => (costs = { ...costs, [f.name]: null }));
    }
  });

  const labels: Record<string, string> = { spec: "Specify", plan: "Plan", tasks: "Break into tasks", commit: "Commit the plan", run: "Run", review: "Review" };
  function stage(r: Report | null | undefined) {
    const now = r?.journey?.find((s) => s.state === "now");
    return now ? labels[now.name] ?? now.name : r ? "Done" : "…";
  }
  function proven(r: Report | null | undefined) {
    const reqs = new Map<string, boolean>();
    for (const t of r?.trace ?? []) {
      const ok = t.verdict === "verified" || t.verdict === "regression_only";
      reqs.set(t.requirement, (reqs.get(t.requirement) ?? false) || ok);
    }
    return { all: reqs.size, ok: [...reqs.values()].filter(Boolean).length };
  }
  const totals = $derived(
    features.reduce(
      (a, f) => ({ done: a.done + f.done, tasks: a.tasks + f.tasks }),
      { done: 0, tasks: 0 },
    ),
  );
  function open(name: string) {
    void app.selectFeature(name);
    app.view = "board";
  }
  const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}k` : String(n));
</script>

<section class="overview">
  <header>
    <div>
      <p class="eyebrow">{app.project?.name ?? ""} · {app.project?.format ?? ""}</p>
      <h1>Overview</h1>
      <p class="sub">{features.length} {app.project?.unit ?? "feature"}{features.length === 1 ? "" : "s"} · {totals.done} of {totals.tasks} tasks finished</p>
    </div>
  </header>

  {#if features.length === 0}
    <div class="empty">
      <Icon name="doc" size={28} />
      <p><strong>No {app.project?.unit ?? "feature"}s yet</strong></p>
      <button class="btn primary" onclick={() => (app.newFeature = {})}><Icon name="sparkle" size={13} /> New {app.project?.unit ?? "feature"}</button>
      <p class="muted">The spec tool's own skill writes the spec from your description; you review it before anything is planned.</p>
    </div>
  {:else}
    <div class="cards">
      {#each features as f (f.name)}
        {@const r = reports[f.name]}
        {@const p = proven(r)}
        {@const pending = app.approvals.filter((x) => r && x.task && r.tasks.some((t) => t.id === x.task)).length}
        <button class="card" class:current={f.current} onclick={() => open(f.name)}>
          <div class="ring">
            {#if r && f.tasks}<Ring totals={r.totals} size={64} width={7} />{:else}<span class="blank"><Icon name="doc" size={22} /></span>{/if}
          </div>
          <div class="body">
            <span class="num mono">{String(f.number).padStart(3, "0")}{#if f.current}{" · "}current{/if}</span>
            <strong class="name">{app.titleOf(f.name)}</strong>
            <span class="stage"><Icon name="sparkle" size={12} weight={2} />{stage(r)}</span>
            <span class="facts">
              {#if f.tasks}<span><b>{f.done}</b>/{f.tasks} tasks</span>{/if}
              {#if p.all}<span class:good={p.ok === p.all}><b>{p.ok}</b>/{p.all} requirements proven</span>{/if}
              {#if costs[f.name]?.total && (costs[f.name]!.total.input_tokens || costs[f.name]!.total.output_tokens)}
                {@const t = costs[f.name]!.total}
                <span title="{t.input_tokens.toLocaleString()} in · {t.output_tokens.toLocaleString()} out">{#if t.usd != null}<b>${t.usd.toFixed(2)}</b> spent{:else}<b>{tokens(t.input_tokens + t.output_tokens)}</b> tokens{/if}</span>
              {/if}
              {#if pending}<span class="attn"><Icon name="hand" size={11} weight={2.2} />{pending} waiting</span>{/if}
              {#if !f.tasks}<span>{f.artifacts.join(" · ") || "empty"}</span>{/if}
            </span>
          </div>
          <span class="go"><Icon name="chevron" size={14} weight={2.2} /></span>
        </button>
      {/each}
    </div>
  {/if}

  <GitHub />
</section>

<style>
  .overview { max-width: 1180px; margin: 0 auto; display: grid; gap: 1.5rem; }
  .eyebrow { margin: 0 0 0.15rem; color: var(--muted); font-size: 0.8rem; }
  h1 { margin: 0; font: 700 2.15rem/1.12 var(--display); letter-spacing: -0.028em; }
  .sub { margin: 0.3rem 0 0; color: var(--muted); }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 1rem; }
  .card {
    all: unset; box-sizing: border-box; cursor: pointer; display: grid; grid-template-columns: auto 1fr auto; gap: 1rem; align-items: center;
    padding: 1rem 1.1rem; border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow);
    transition: transform 260ms var(--spring), box-shadow var(--speed) var(--ease);
  }
  .card:hover { transform: translateY(-2px); box-shadow: var(--shadow-md); }
  .card:focus-visible { box-shadow: 0 0 0 3.5px var(--focus); }
  .card.current { box-shadow: 0 0 0 1.5px var(--accent), var(--shadow); }
  .blank { width: 64px; height: 64px; border-radius: 50%; display: grid; place-items: center; background: var(--fill); color: var(--muted); }
  .ring :global(.n) { font-size: 1.05rem; }
  .ring :global(.of) { font-size: 0.62rem; }
  .body { display: grid; gap: 0.15rem; min-width: 0; }
  .num { font-size: 0.74rem; color: var(--muted); }
  .name { font: 650 1.08rem/1.25 var(--display); letter-spacing: -0.012em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name::first-letter { text-transform: uppercase; }
  .stage { display: inline-flex; align-items: center; gap: 0.3rem; color: var(--accent-ink); font-size: 0.82rem; font-weight: 600; }
  .facts { display: flex; flex-wrap: wrap; gap: 0.25rem 0.8rem; font-size: 0.8rem; color: var(--muted); margin-top: 0.2rem; }
  .facts b { color: var(--text); font-weight: 650; font-variant-numeric: tabular-nums; }
  .facts .good b { color: var(--verified); }
  .attn { display: inline-flex; align-items: center; gap: 0.25rem; color: var(--attention); font-weight: 600; }
  .go { color: var(--faint); }
  .empty { display: grid; justify-items: center; gap: 0.2rem; padding: 3.5rem 1rem; color: var(--muted); }
  .empty p { margin: 0; }
  .empty strong { color: var(--text); }
</style>
