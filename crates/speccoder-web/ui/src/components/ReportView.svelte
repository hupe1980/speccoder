<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { mark, word } from "../lib/marks";
  const r = $derived(app.report?.data);
  const order = ["verified", "regression_only", "attested", "ticked_without_evidence", "skipped", "ready", "blocked"];
  const segments = $derived(order.map((k) => ({ k, n: r?.totals?.[k] ?? 0 })).filter((s) => s.n > 0));
  const total = $derived(segments.reduce((a, s) => a + s.n, 0));
  const uncited = $derived((r?.trace ?? []).filter((t) => !t.task));
  // One row per requirement, the tasks that prove it beside it.
  const matrix = $derived.by(() => {
    const rows = new Map<string, { task: string; verdict: string | null; test: string | null }[]>();
    for (const t of r?.trace ?? []) {
      const row = rows.get(t.requirement) ?? [];
      if (t.task) row.push({ task: t.task, verdict: t.verdict, test: t.test });
      rows.set(t.requirement, row);
    }
    return [...rows].map(([requirement, tasks]) => ({ requirement, tasks }));
  });
  const proven = $derived(matrix.filter((m) => m.tasks.some((t) => t.verdict === "verified" || t.verdict === "regression_only")).length);
  const analyze = $derived(app.actions.find((a) => a.run?.kind === "skill" && a.label.endsWith("analyze")));
</script>

{#if r}
  <div class="grid">
    <section class="card wide">
      <h3>Verdicts</h3>
      <div class="bar" role="img" aria-label={segments.map((s) => `${s.n} ${word(s.k)}`).join(", ")}>
        {#each segments as s (s.k)}<span class="s-{s.k}" style="flex:{s.n}" title="{s.n} {word(s.k)}"></span>{/each}
      </div>
      <ul class="legend">
        {#each segments as s (s.k)}<li><span class="chip s-{s.k}">{mark(s.k)} {s.n}</span> {word(s.k)}</li>{/each}
      </ul>
      <p class="muted small">{total} tasks · branch <code>{r.branch ?? "—"}</code>{#if r.unmerged_commits} · {r.unmerged_commits} commit(s) not merged{/if}</p>
    </section>
    <section class="card">
      <h3>Waiting for you <span class="muted">{app.approvals.length}</span></h3>
      {#each app.approvals as p (p.task + p.covers)}
        {@const yes = app.actions.find((a) => a.run?.kind === "approve" && "task" in a.run && a.run.task === p.task)}
        {@const no = app.actions.find((a) => a.run?.kind === "reject" && "task" in a.run && a.run.task === p.task)}
        <div class="pending">
          <p><span class="chip s-needs_approval">△ {p.task}</span> <strong>{p.presentation?.title ?? p.tool}</strong> <code>{p.covers}</code></p>
          <p class="muted small">{p.reason}</p>
          <div class="acts">
            {#if yes}<button class="btn small primary" disabled={!!app.job?.running} title={yes.command} onclick={() => app.perform(yes)}>Approve</button>{/if}
            {#if no}<button class="btn small" disabled={!!app.job?.running} title={no.command} onclick={() => app.perform(no)}>Reject…</button>{/if}
          </div>
        </div>
      {:else}<p class="muted">Nothing.</p>{/each}
    </section>
    <section class="card">
      <h3>Requirements no task cites <span class="muted">{uncited.length}</span></h3>
      {#if uncited.length}<p>{#each uncited as u}<span class="chip s-regression_only">{u.requirement}</span> {/each}</p>{:else}<p class="muted">Every requirement is cited.</p>{/if}
    </section>
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
                  <button class="chip s-{t.verdict ?? 'blocked'}" title="{word(t.verdict ?? 'blocked')}{t.test ? ` · ${t.test}` : ''}" onclick={() => { app.view = 'board'; void app.selectTask(t.task); }}>{mark(t.verdict ?? "blocked")} {t.task}</button>
                {:else}<span class="muted small">no task cites it</span>{/each}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
    {#if r.convergence.length}
      <section class="card wide">
        <h3>Convergence</h3>
        {#each r.convergence as c (c.round)}<p><strong>Round {c.round}</strong> <code>{c.commit.slice(0, 10)}</code> — {c.tasks.join("; ")}</p>{/each}
      </section>
    {/if}
  </div>
{/if}

<style>
  .pending { display: grid; gap: 0.3rem; padding: 0.5rem 0; border-bottom: 1px solid var(--border); }
  .pending p { margin: 0; }
  .acts { display: flex; gap: 0.4rem; }
  .matrix { list-style: none; margin: 0; padding: 0; display: grid; }
  .matrix li { display: grid; grid-template-columns: 7rem 1fr; gap: 0.75rem; align-items: center; padding: 0.4rem 0; border-bottom: 1px solid var(--border); }
  .matrix li.open .req { color: var(--regression); }
  .tasks { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .tasks button { border: 0; cursor: pointer; font-family: var(--mono); }
  .empty { display: grid; gap: 0.6rem; justify-items: start; padding: 0.75rem 1rem; border-radius: 8px; background: var(--regression-bg); }
  .empty p { margin: 0; }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1rem; }
  .card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 1rem 1.25rem; }
  .wide { grid-column: 1 / -1; }
  h3 { margin: 0 0 0.75rem; font-size: 0.95rem; }
  .bar { display: flex; height: 14px; border-radius: 7px; overflow: hidden; gap: 2px; background: var(--surface-2); }
  .bar span { background: currentColor; }
  .legend { list-style: none; padding: 0; display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 0.75rem 0 0.25rem; }
  .small { font-size: 0.85rem; }
</style>
