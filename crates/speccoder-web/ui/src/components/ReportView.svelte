<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { byKind, byRole } from "../lib/actions";
  import { ORDER, symbol, word } from "../lib/marks";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";
  import Spend from "./Spend.svelte";
  import PullCard from "./PullCard.svelte";
  const r = $derived(app.report?.data);
  const order = ORDER;
  const segments = $derived(order.map((k) => ({ k, n: r?.totals?.[k] ?? 0 })).filter((s) => s.n > 0));
  const total = $derived(segments.reduce((a, s) => a + s.n, 0));
  const uncited = $derived((r?.trace ?? []).filter((t) => !t.task));
  // One row per requirement, the tasks that prove it beside it.
  const matrix = $derived.by(() => {
    const rows = new Map<string, { task: string; verdict: string | null; test: string | null; changed: boolean }[]>();
    for (const t of r?.trace ?? []) {
      const row = rows.get(t.requirement) ?? [];
      if (t.task) row.push({ task: t.task, verdict: t.verdict, test: t.test, changed: !!t.changed });
      rows.set(t.requirement, row);
    }
    return [...rows].map(([requirement, tasks]) => ({ requirement, tasks }));
  });
  // Proven: evidence for the wording the spec has now, not an earlier one.
  const proven = $derived(matrix.filter((m) => m.tasks.some((t) => !t.changed && (t.verdict === "verified" || t.verdict === "regression_only"))).length);
  const stale = $derived((r?.tasks ?? []).filter((t) => t.drift?.requirements.length));
  const recheck = $derived(byKind(app.actions, "converge")[0]);
  // The format's consistency check, by its role: whatever it is named.
  const analyze = $derived(byRole(app.actions, "check")[0]);
</script>

{#if r}
  <div class="grid">
    <section class="card wide">
      <h3>Verdicts</h3>
      <div class="bar" role="img" aria-label={segments.map((s) => `${s.n} ${word(s.k)}`).join(", ")}>
        {#each segments as s (s.k)}<span class="s-{s.k}" style="flex:{s.n}" title="{s.n} {word(s.k)}"></span>{/each}
      </div>
      <ul class="legend">
        {#each segments as s (s.k)}<li><span class="chip s-{s.k}"><Icon name={symbol(s.k)} size={11} weight={2.4} /> {s.n}</span> {word(s.k)}</li>{/each}
      </ul>
      <p class="muted small">{total} tasks · branch <code>{r.branch ?? "—"}</code>{#if r.unmerged_commits}{" · "}{r.unmerged_commits} {r.unmerged_commits === 1 ? "commit" : "commits"} not merged{/if}</p>
    </section>
    {#if stale.length}
      <section class="card wide stale">
        <div>
          <h3><Icon name="exclaim" size={15} weight={2.2} /> The spec changed under {stale.length} {stale.length === 1 ? "tick" : "ticks"}</h3>
          <p>{stale.map((t) => `${t.id} (${t.drift!.requirements.map((q) => q.id).join(", ")})`).join(" · ")} — proven against an earlier wording.</p>
        </div>
        {#if recheck}<button class="btn small primary" disabled={!!recheck.blocked || !!app.job?.running} title={`${recheck.blocked ?? recheck.why}\n$ ${recheck.command}`} onclick={() => app.perform(recheck)}><Icon name="sparkle" size={11} weight={2.2} /> Re-check Against the Spec</button>{/if}
      </section>
    {/if}
    <section class="card" class:wide={!matrix.length}>
      <h3>Waiting for you <span class="muted">{app.approvals.length}</span></h3>
      {#each app.approvals as p (p.task + p.covers)}
        {@const yes = byKind(app.actions, "approve", p.task)[0]}
        {@const no = byKind(app.actions, "reject", p.task)[0]}
        <div class="pending">
          <p><span class="chip s-needs_approval"><Icon name="hand" size={11} weight={2.2} /> {p.task}</span> <strong>{p.presentation?.title ?? p.tool}</strong> <Code text={p.covers} /></p>
          <p class="muted small">{p.reason}</p>
          <div class="acts">
            {#if yes}<button class="btn small primary" disabled={!!app.job?.running} title={yes.command} onclick={() => app.perform(yes)}>Approve</button>{/if}
            {#if no}<button class="btn small" disabled={!!app.job?.running} title={no.command} onclick={() => app.perform(no)}>Reject…</button>{/if}
          </div>
        </div>
      {:else}<p class="muted">Nothing.</p>{/each}
    </section>
    {#if matrix.length}
    <section class="card">
      <h3>Requirements no task cites <span class="muted">{uncited.length}</span></h3>
      {#if uncited.length}<p>{#each uncited as u}<span class="chip s-regression_only">{u.requirement}</span> {/each}</p>{:else}<p class="muted">Every requirement is cited.</p>{/if}
    </section>
    {/if}
    <section class="card wide">
      <h3>Requirements <span class="muted">{proven} of {matrix.length} proven by evidence</span></h3>
      {#if !matrix.length}
        <p class="muted">The spec names no requirement IDs (FR-001, SC-001, …), so there is nothing to trace.</p>
      {:else if uncited.length === matrix.length}
        <div class="empty">
          <p><strong>No task names a requirement yet.</strong> The spec has {matrix.length} requirements, but tasks.md cites none of them, so no evidence can be traced back to what was asked for.</p>
          {#if analyze}<button class="btn small" disabled={!!app.job?.running} onclick={() => app.perform(analyze)}>Find the gaps: {analyze.label}</button>{/if}
        </div>
      {:else}
        <ul class="matrix">
          {#each matrix as m (m.requirement)}
            <li class:open={!m.tasks.length}>
              <span class="req mono">{m.requirement}</span>
              <span class="tasks">
                {#each m.tasks as t (t.task)}
                  <button class="chip s-{t.changed ? 'needs_approval' : (t.verdict ?? 'blocked')}" title="{t.changed ? `${word(t.verdict ?? 'blocked')} against an earlier wording — the requirement changed since` : word(t.verdict ?? 'blocked')}{t.test ? ` · ${t.test}` : ''}" onclick={() => { app.view = 'board'; void app.selectTask(t.task); }}><Icon name={t.changed ? "exclaim" : symbol(t.verdict ?? "blocked")} size={11} weight={2.4} /> {t.task}{#if t.changed}<span class="was">earlier wording</span>{/if}</button>
                {:else}<span class="muted small">no task cites it</span>{/each}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
    <PullCard />
    <Spend />
    {#if r.convergence.length}
      <section class="card wide">
        <h3>Convergence</h3>
        {#each r.convergence as c (c.round)}<p><strong>Round {c.round}</strong> <code>{c.commit.slice(0, 10)}</code> — {c.tasks.join("; ")}</p>{/each}
      </section>
    {/if}
  </div>
{/if}

<style>
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1.25rem; max-width: 1180px; margin: 0 auto; }
  .card { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 1.15rem 1.35rem 1.25rem; min-width: 0; }
  .wide { grid-column: 1 / -1; }
  h3 { margin: 0 0 0.85rem; font: 650 1.02rem/1.3 var(--display); letter-spacing: -0.01em; display: flex; align-items: baseline; gap: 0.5rem; }
  h3 :global(.muted) { font: 500 0.85rem var(--font); }
  .bar { display: flex; height: 12px; border-radius: 6px; overflow: hidden; gap: 2px; background: var(--fill); }
  .bar span { background: var(--fill-c); }
  .legend { list-style: none; padding: 0; display: flex; flex-wrap: wrap; gap: 0.5rem 1.1rem; margin: 0.85rem 0 0.4rem; font-size: 0.86rem; color: var(--text-2); }
  .legend li { display: inline-flex; align-items: center; gap: 0.4rem; }
  .small { font-size: 0.82rem; }
  .pending { display: grid; gap: 0.35rem; padding: 0.7rem 0; border-top: 0.5px solid var(--separator); }
  .pending:first-of-type { border-top: 0; padding-top: 0; }
  .pending p { margin: 0; }
  .acts { display: flex; gap: 0.4rem; margin-top: 0.15rem; }
  .matrix { list-style: none; margin: 0; padding: 0; display: grid; }
  .matrix li { display: grid; grid-template-columns: 7rem 1fr; gap: 0.75rem; align-items: center; padding: 0.5rem 0; border-top: 0.5px solid var(--separator); }
  .matrix li:first-child { border-top: 0; }
  .req { font-size: 0.84rem; font-weight: 600; }
  .matrix li.open .req { color: var(--regression); }
  .tasks { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .tasks button { border: 0; cursor: pointer; font-family: var(--mono); }
  .was { font-family: var(--text-font, inherit); font-weight: 500; margin-left: 0.3rem; opacity: 0.85; }
  .stale { display: flex; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; background: var(--attention-bg); }
  .stale p { margin: 0.2rem 0 0; color: var(--text-2); }
  .empty { display: grid; gap: 0.7rem; justify-items: start; padding: 0.9rem 1.1rem; border-radius: var(--radius); background: var(--regression-bg); }
  .empty p { margin: 0; color: var(--text-2); }
  @media (max-width: 900px) { .grid { grid-template-columns: 1fr; } }
</style>
