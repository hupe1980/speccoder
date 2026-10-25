<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { mark, word } from "../lib/marks";
  import type { Task } from "../lib/types";
  import Code from "./Code.svelte";
  let expanded = $state<Record<string, boolean>>({});
  const LIMIT = 5;

  const tasks = $derived<Task[]>(app.report?.data?.tasks ?? []);
  const waitingFor = $derived(new Set(app.approvals.map((p) => p.task)));
  const running = $derived(app.job?.running ? app.task?.id : undefined);

  // Lanes in the order work flows; a task is in one lane.
  const lanes = [
    { key: "needs_approval", title: "Waits for you" },
    { key: "running", title: "Running" },
    { key: "ready", title: "Ready" },
    { key: "blocked", title: "Blocked" },
    { key: "verified", title: "Verified" },
    { key: "regression_only", title: "Regression only" },
    { key: "attested", title: "Attested" },
    { key: "ticked_without_evidence", title: "No evidence" },
    { key: "skipped", title: "Skipped" },
  ];
  const laneOf = (t: Task) =>
    waitingFor.has(t.id) ? "needs_approval" : running === t.id ? "running" : t.state === "evidence_without_tick" ? "ticked_without_evidence" : t.state;
  const grouped = $derived(lanes.map((l) => ({ ...l, tasks: tasks.filter((t) => laneOf(t) === l.key) })).filter((l) => l.tasks.length));

  const strength = (s?: string | null) => {
    const m = s?.match(/^(\d\.\d\d)/);
    return m ? Number(m[1]) : null;
  };

  function runTask(t: Task, e: MouseEvent) {
    e.stopPropagation();
    app.perform({ group: "work", label: `run ${t.id}`, why: t.title, command: `speccoder run --task ${t.id}`, run: { kind: "run", task: t.id }, blocked: null, input: null });
  }
</script>

{#if tasks.length === 0}
  <div class="empty muted">No task list yet — the next step above writes it.</div>
{:else}
  <div class="board" role="list">
    {#each grouped as lane (lane.key)}
      <section class="lane" aria-label={lane.title}>
        <h3><span class="chip s-{lane.key}">{mark(lane.key)} {lane.title}</span><span class="muted">{lane.tasks.length}</span></h3>
        {#each expanded[lane.key] ? lane.tasks : lane.tasks.slice(0, LIMIT) as t (t.id)}
          {@const s = strength(t.strength)}
          <article class="card" class:on={app.selected === t.id} class:live={app.job?.running && app.task?.id === t.id} role="listitem">
            <button class="open" onclick={() => app.selectTask(t.id)} aria-label="{t.id} {t.title}, {word(laneOf(t))}">
              <span class="id mono">{t.id}</span>
              <span class="t"><Code text={t.title} /></span>
              <span class="foot">
                {#if app.job?.running && app.task?.id === t.id}
                  <span class="stage" aria-live="polite">▶ {[...app.stages].reverse().find((x) => x.state === "started")?.stage ?? "starting"}…</span>
                {:else if s !== null}
                  <span class="meter" title="acceptance strength {t.strength}"><span style="width:{s * 100}%" class:low={s < 0.8}></span></span>
                  <span class="muted small">{s.toFixed(2)}</span>
                {:else if t.blocked_by.length}
                  <span class="muted small">waits for {t.blocked_by.join(", ")}</span>
                {:else if t.last_attempt}
                  <span class="small fail" title={t.last_attempt.reason}>✗ last attempt: {t.last_attempt.outcome.replaceAll("_", " ")}</span>
                {/if}
              </span>
            </button>
            {#if lane.key === "ready"}
              <div class="act">
                <button class="btn small primary" disabled={!!app.job?.running} onclick={(e) => runTask(t, e)}>▶ Run task</button>
              </div>
            {/if}
          </article>
        {/each}
        {#if lane.tasks.length > LIMIT}
          <button class="more" onclick={() => (expanded = { ...expanded, [lane.key]: !expanded[lane.key] })}>
            {expanded[lane.key] ? "Show fewer" : `Show ${lane.tasks.length - LIMIT} more`}
          </button>
        {/if}
      </section>
    {/each}
  </div>
{/if}

<style>
  .board { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 1rem; padding-bottom: 0.5rem; align-items: start; }
  .lane { background: var(--surface-2); border-radius: var(--radius); padding: 0.6rem; display: grid; gap: 0.5rem; }
  h3 { margin: 0.1rem 0.2rem 0.3rem; font-size: 0.85rem; display: flex; justify-content: space-between; align-items: center; }
  .card { position: relative; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; box-shadow: var(--shadow); transition: border-color var(--speed), transform var(--speed); }
  .card:hover { border-color: var(--border-strong); transform: translateY(-1px); }
  .card.on { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .open { all: unset; display: grid; gap: 0.3rem; padding: 0.7rem 0.75rem; cursor: pointer; width: 100%; box-sizing: border-box; }
  .open:focus-visible { outline: 2px solid var(--focus); border-radius: 8px; }
  .id { font-size: 0.78rem; color: var(--muted); }
  .t { font-weight: 500; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .card.live { border-color: var(--running); box-shadow: 0 0 0 3px var(--running-bg); animation: glow 1.8s ease-in-out infinite; }
  @keyframes glow { 50% { box-shadow: 0 0 0 6px var(--running-bg); } }
  .stage { font-size: 0.78rem; color: var(--running); font-weight: 600; }
  .act { padding: 0 0.75rem 0.7rem; }
  .more { border: 0; background: none; color: var(--accent); cursor: pointer; padding: 0.3rem; font-weight: 600; font-size: 0.85rem; }
  .foot { display: flex; align-items: center; gap: 0.5rem; min-height: 1rem; }
  .meter { flex: 1; height: 5px; background: var(--surface-3); border-radius: 4px; overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--verified); }
  .meter span.low { background: var(--regression); }
  .small { font-size: 0.78rem; }
  .fail { color: var(--danger); }
  
  .empty { padding: 3rem; text-align: center; border: 1px dashed var(--border-strong); border-radius: var(--radius); }
</style>
