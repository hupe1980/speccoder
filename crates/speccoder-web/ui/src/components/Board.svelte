<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { phaseLabel as short, symbol, word } from "../lib/marks";
  import { get, q } from "../lib/api";
  import type { Action, Task } from "../lib/types";
  import Code from "./Code.svelte";
  import Icon from "./Icon.svelte";
  let expanded = $state<Record<string, boolean>>({});
  const LIMIT = 5;

  const tasks = $derived<Task[]>(app.report?.data?.tasks ?? []);
  const waitingFor = $derived(new Set(app.approvals.map((p) => p.task)));
  const running = $derived(app.job?.running ? app.task?.id : undefined);

  // Lanes in the order work flows; a task is in one lane.
  const lanes = [
    { key: "needs_approval", title: "Waiting for You" },
    { key: "running", title: "Running" },
    { key: "ready", title: "Ready" },
    { key: "blocked", title: "Blocked" },
    { key: "verified", title: "Verified" },
    { key: "regression_only", title: "Regression Only" },
    { key: "attested", title: "Attested" },
    { key: "ticked_without_evidence", title: "No Evidence" },
    { key: "skipped", title: "Skipped" },
  ];
  const laneOf = (t: Task) =>
    waitingFor.has(t.id) ? "needs_approval" : running === t.id ? "running" : t.state === "evidence_without_tick" ? "ticked_without_evidence" : t.state;
  const grouped = $derived(lanes.map((l) => ({ ...l, tasks: tasks.filter((t) => laneOf(t) === l.key) })).filter((l) => l.tasks.length));

  const strength = (s?: string | null) => {
    const m = s?.match(/^(\d\.\d\d)/);
    return m ? Number(m[1]) : null;
  };
  const stage = $derived([...app.stages].reverse().find((x) => x.state === "started")?.stage ?? "starting");

  // Right-click: the task's own commands, from the action model.
  async function menu(t: Task, e: MouseEvent) {
    e.preventDefault();
    const x = e.clientX;
    const y = e.clientY;
    const actions = await get<Action[]>(`/api/actions${q({ feature: app.feature ?? undefined, task: t.id })}`).catch(() => [] as Action[]);
    const mine = actions.filter((a) => (a.group === "task" || a.group === "work") && a.label.includes(t.id));
    const verb = (a: Action) => a.label.replace(` ${t.id}`, "").replace(/^./, (c) => c.toUpperCase());
    const icons: Record<string, string> = { run: "play", accept: "check", skip: "skip", nudge: "sparkle" };
    const copy = (text: string, what: string) => () => {
      void navigator.clipboard?.writeText(text);
      app.toast({ tone: "ok", text: `Copied ${what}`, detail: text });
    };
    app.context = {
      x,
      y,
      title: `${t.id} · ${word(laneOf(t))}`,
      items: [
        { label: "Show Evidence", icon: "shield", act: () => void app.selectTask(t.id) },
        ...mine.map((a) => ({ label: verb(a) + (a.input ? "…" : ""), icon: icons[a.label.split(" ")[0] ?? ""], disabled: a.blocked ?? (app.job?.running ? "a job is running" : null), hint: `$ ${a.command}`, act: () => void app.perform(a) })),
        { label: "Copy Task ID", icon: "copy", act: copy(t.id, "the task ID") },
        { label: "Copy the Run Command", icon: "terminal", act: copy(`speccoder run --task ${t.id}`, "the command") },
      ],
    };
  }

  function runTask(t: Task, e: MouseEvent) {
    e.stopPropagation();
    app.perform({ group: "work", label: `run ${t.id}`, why: t.title, command: `speccoder run --task ${t.id}`, run: { kind: "run", task: t.id }, blocked: null, input: null });
  }
</script>

{#if tasks.length === 0}
  <div class="empty">
    <Icon name="list" size={28} />
    <p><strong>No tasks yet</strong></p>
    <p class="muted">The next step above writes the task list.</p>
  </div>
{:else}
  <div class="board" role="list">
    {#each grouped as lane (lane.key)}
      <section class="lane" aria-label={lane.title}>
        <h3 class="s-{lane.key}"><Icon name={symbol(lane.key)} size={14} weight={2} /><span class="title">{lane.title}</span><span class="count">{lane.tasks.length}</span></h3>
        {#each expanded[lane.key] ? lane.tasks : lane.tasks.slice(0, LIMIT) as t (t.id)}
          {@const s = strength(t.strength)}
          {@const live = running === t.id}
          <article class="card" class:on={app.selected === t.id} class:live role="listitem" oncontextmenu={(e) => menu(t, e)}>
            <button class="open" onclick={() => app.selectTask(t.id)} aria-label="{t.id} {t.title}, {word(laneOf(t))}">
              <span class="row"><span class="id mono">{t.id}</span>{#if t.phase}<span class="phase" title={t.phase}>{short(t.phase)}</span>{/if}</span>
              <span class="t"><Code text={t.title} /></span>
              {#if t.drift?.requirements.length}
                <span class="foot changed" title="The spec changed since this tick: its evidence is for the earlier wording"><Icon name="exclaim" size={12} weight={2.2} />Spec changed: {t.drift.requirements.map((r) => r.id).join(", ")}</span>
              {/if}
              {#if live}
                <span class="stage" aria-live="polite"><span class="pulse" aria-hidden="true"></span>{stage}…</span>
              {:else if s !== null}
                <span class="foot"><span class="meter" title="Acceptance strength {t.strength}"><span style="width:{s * 100}%" class:low={s < 0.8}></span></span><span class="sv mono">{s.toFixed(2)}</span></span>
              {:else if t.blocked_by.length}
                <span class="foot muted"><Icon name="hourglass" size={12} />Waits for {t.blocked_by.join(", ")}</span>
              {:else if t.last_attempt}
                <span class="foot fail" title={t.last_attempt.reason}><Icon name="xoctagon" size={12} />Last attempt {t.last_attempt.outcome.replaceAll("_", " ")}</span>
              {/if}
            </button>
            {#if lane.key === "ready"}
              <div class="act">
                <button class="btn small tinted" disabled={!!app.job?.running} onclick={(e) => runTask(t, e)}><Icon name="play" size={11} />Run</button>
              </div>
            {/if}
          </article>
        {/each}
        {#if lane.tasks.length > LIMIT}
          <button class="more" onclick={() => (expanded = { ...expanded, [lane.key]: !expanded[lane.key] })}>
            <Icon name={expanded[lane.key] ? "arrowup" : "down"} size={13} />
            {expanded[lane.key] ? "Show Less" : `${lane.tasks.length - LIMIT} More`}
          </button>
        {/if}
      </section>
    {/each}
  </div>
{/if}

<style>
  .board { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 1.25rem; align-items: start; max-width: 1180px; margin: 0 auto; }
  .lane { display: grid; gap: 0.55rem; min-width: 0; }
  h3 { display: flex; align-items: center; gap: 0.4rem; margin: 0 0.25rem 0.15rem; font-size: 0.82rem; font-weight: 650; background: none; }
  h3 .title { color: var(--text); }
  .count { margin-left: auto; min-width: 1.5rem; height: 1.25rem; padding: 0 0.4rem; border-radius: 999px; display: grid; place-items: center; background: var(--fill); color: var(--muted); font: 600 0.72rem/1 var(--rounded); }
  .card {
    position: relative; background: var(--surface); border-radius: var(--radius); box-shadow: var(--shadow);
    transition: transform 260ms var(--spring), box-shadow var(--speed) var(--ease);
  }
  .card:hover { transform: translateY(-2px); box-shadow: var(--shadow-md); }
  .card.on { box-shadow: 0 0 0 2px var(--accent), var(--shadow-md); }
  .open { all: unset; display: grid; gap: 0.35rem; padding: 0.8rem 0.9rem 0.85rem; cursor: pointer; width: 100%; box-sizing: border-box; border-radius: var(--radius); }
  .open:focus-visible { box-shadow: 0 0 0 3.5px var(--focus); }
  .row { display: flex; align-items: center; gap: 0.4rem; min-width: 0; }
  .id { font-size: 0.74rem; color: var(--muted); }
  .phase { min-width: 0; font-size: 0.7rem; color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t { font-weight: 500; line-height: 1.38; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .foot { display: flex; align-items: center; gap: 0.35rem; font-size: 0.76rem; }
  .fail { color: var(--danger); }
  .changed { color: var(--attention); font-weight: 600; }
  .meter { flex: 1; height: 4px; background: var(--fill); border-radius: 4px; overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--verified-fill); border-radius: 4px; }
  .meter span.low { background: var(--regression-fill); }
  .sv { font-size: 0.72rem; color: var(--muted); }
  .act { padding: 0 0.9rem 0.8rem; }

  /* The running card: a slow, many-coloured edge, as the system marks
     what its intelligence is working on. */
  .card.live { box-shadow: var(--shadow-md); }
  .card.live::before {
    content: ""; position: absolute; inset: -2px; border-radius: calc(var(--radius) + 2px); z-index: -1;
    background: conic-gradient(from var(--angle), #ff9f0a, #ff375f, #bf5af2, #0a84ff, #64d2ff, #30d158, #ff9f0a);
    animation: turn 3.2s linear infinite; filter: saturate(1.1);
  }
  .card.live::after {
    content: ""; position: absolute; inset: -6px; border-radius: calc(var(--radius) + 6px); z-index: -2;
    background: inherit; background: conic-gradient(from var(--angle), #ff9f0a, #ff375f, #bf5af2, #0a84ff, #64d2ff, #30d158, #ff9f0a);
    filter: blur(14px); opacity: 0.45; animation: turn 3.2s linear infinite;
  }
  @property --angle { syntax: "<angle>"; initial-value: 0deg; inherits: false; }
  @keyframes turn { to { --angle: 360deg; } }
  .stage { display: inline-flex; align-items: center; gap: 0.45rem; font-size: 0.78rem; font-weight: 600; color: var(--running); }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--running-fill); animation: beat 1.2s var(--ease) infinite; }
  @keyframes beat { 50% { transform: scale(0.6); opacity: 0.5; } }

  .more { display: inline-flex; align-items: center; gap: 0.3rem; justify-self: start; border: 0; background: none; color: var(--accent-ink); cursor: pointer; padding: 0.2rem 0.35rem; font-weight: 600; font-size: 0.82rem; border-radius: 6px; }
  .more:hover { background: var(--fill); }
  .empty { display: grid; justify-items: center; gap: 0.15rem; padding: 3.5rem 1rem; color: var(--muted); max-width: 1180px; margin: 0 auto; }
  .empty p { margin: 0; }
  .empty strong { color: var(--text); font-size: 1.05rem; }
</style>
