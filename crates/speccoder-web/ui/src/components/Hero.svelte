<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { key } from "../lib/actions";
  import type { JourneyStep, StatusTotals } from "../lib/types";
  import Preflight from "./Preflight.svelte";
  import Ring from "./Ring.svelte";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";
  import FixSteps from "./FixSteps.svelte";
  import { ORDER, symbol, word } from "../lib/marks";

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
  const totals = $derived<Partial<StatusTotals>>(report?.totals ?? {});
  const legend = $derived(ORDER.map((k) => ({ k, n: totals[k] ?? 0 })).filter((s) => s.n > 0));
  const headline = $derived(primary ? (primary.run?.kind === "skill" ? cap(primary.why) : cap(primary.label)) : "");
  const verb = $derived(
    busy ? "Working…" : primary?.run?.kind === "commit" ? "Commit" : primary?.input ? "Start…" : stops.length ? "Run Anyway" : "Run",
  );
  let copied = $state(false);
  function copy() {
    if (!primary) return;
    navigator.clipboard?.writeText(primary.command);
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }
  const title = $derived(report ? app.titleOf(report.feature) : "");
  // Run with …: the agents that can start here, the choice kept per project.
  const isRun = $derived(primary?.run?.kind === "run");
  $effect(() => {
    if (isRun) void app.loadAgents(false).catch(() => {});
  });
  const runnable = $derived((app.agents?.cards ?? []).filter((c) => c.installed.state === "yes"));
  const withName = $derived(app.runWith ? (runnable.find((c) => c.id === app.runWith)?.name ?? app.runWith) : null);
  let choosing = $state(false);
  const number = $derived(report?.feature.match(/^\d+/)?.[0]);
</script>

<section class="hero" class:compact aria-label="Where the {app.project?.unit ?? 'feature'} is">
  <div class="top">
    <div class="heading">
      {#if report}
        <p class="eyebrow"><span class="mono">{number ?? ""}</span>{#if report.branch}<Icon name="branch" size={13} /><span class="mono">{report.branch}</span>{/if}</p>
        <h1>{title}</h1>
      {:else}
        <h1>{app.report?.message ?? "speccoder"}</h1>
      {/if}
    </div>
    {#if legend.length && !compact}
      <div class="stats">
        <ul class="legend" aria-hidden="true">
          {#each legend as s (s.k)}
            <li class="s-{s.k}"><Icon name={symbol(s.k)} size={13} weight={2} /><span class="n">{s.n}</span><span class="w">{word(s.k)}</span></li>
          {/each}
        </ul>
        <Ring {totals} size={88} width={9} />
      </div>
    {:else if legend.length}
      <Ring {totals} size={40} width={5} label={false} />
    {/if}
  </div>

  {#if !compact}
    <ol class="journey" aria-label="Journey">
      {#each journey as s, i (s.name)}
        <li class={s.state} aria-current={s.state === "now" ? "step" : undefined}>
          <span class="dot" aria-hidden="true">
            {#if s.state === "done"}<Icon name="check" size={13} weight={2.6} />{:else if s.state === "skipped"}–{:else}{i + 1}{/if}
          </span>
          <span class="name">{labels[s.name] ?? s.name}</span>
          <span class="sr-only">{s.state === "done" ? "done" : s.state === "now" ? "next" : s.state === "skipped" ? "skipped" : "to do"}</span>
        </li>
      {/each}
    </ol>
    <Preflight />
  {/if}

  {#if app.elsewhere && app.live?.live}
    {@const l = app.live.live}
    <!-- Another speccoder holds the project: what it does, not a Run that would wait. -->
    <div class="next elsewhere">
      <div class="text">
        <span class="kicker"><span class="ring" aria-hidden="true"></span> Running in a terminal</span>
        <h2><span class="mono">{l.task}</span> {l.title}</h2>
        <p class="why">{l.stage ?? "starting"}{#if l.stage_detail}{" "}{l.stage_detail}{/if}{#if l.feature !== report?.feature}{" · in "}{l.feature}{/if} — another speccoder (pid {l.pid}) holds this project; its progress shows here as it goes.</p>
        {#if l.thinking}<p class="why thought">∴ {l.thinking}</p>{/if}
      </div>
    </div>
  {:else if primary}
    <div class="next" class:busy class:warned={stops.length > 0}>
      <div class="text">
        <span class="kicker"><Icon name="sparkle" size={13} weight={2} /> Up next</span>
        <h2><Code text={headline} /></h2>
        {#if !compact}
          {#if primary.run?.kind !== "skill"}<p class="why"><Code text={cap(primary.why)} /></p>{/if}
          {#if commitPaths.length}
            <ul class="files">
              {#each commitPaths as p}<li class="mono"><span class="plus">+</span>{p}</li>{/each}
            </ul>
            <p class="why small">Only these are committed — nothing else you have staged.</p>
          {/if}
        {/if}
        {#if stops.length}
          <p class="stop"><Icon name="exclaim" size={14} weight={2} /> This run would stop: {stops.map((s) => s.check).join(", ")}.
            {#if compact}<button class="link" onclick={() => (app.view = "board")}>How to fix it</button>{/if}</p>
        {/if}
        {#if !compact}
          <button class="cmd" title="Copy the command" onclick={copy}>
            <Icon name="terminal" size={13} /><span class="mono">{primary.command}</span>
            <span class="c">{#if copied}<Icon name="check" size={13} />{:else}<Icon name="copy" size={13} />{/if}</span>
          </button>
        {/if}
      </div>
      <div class="go">
        <span class="split">
          <button class="btn large" class:primary={!stops.length} disabled={busy} onclick={() => app.perform(app.withAgent(primary))}>
            {#if busy}<span class="spin" aria-hidden="true"></span>{:else if primary.run?.kind !== "commit"}<Icon name="play" size={13} />{/if}
            {verb}{#if isRun && withName && !busy}{" "}with {withName}{/if}<kbd>N</kbd>
          </button>
          {#if isRun && runnable.length}
            <button class="btn large more" class:primary={!stops.length} disabled={busy} aria-label="Run with …" aria-expanded={choosing} onclick={() => (choosing = !choosing)}>▾</button>
            {#if choosing}
              <span class="menu" role="menu">
                <button role="menuitem" class:on={!app.runWith} onclick={() => { choosing = false; app.chooseRunWith(null); }}>As configured</button>
                {#each runnable as c (c.id)}
                  <button role="menuitem" class:on={app.runWith === c.id} onclick={() => { choosing = false; app.chooseRunWith(c.id); }}>Run with {c.name}</button>
                {/each}
              </span>
            {/if}
          {/if}
        </span>
        {#if !compact}
          {#each others.slice(0, 2) as o (key(o))}
            <button class="btn small ghost" disabled={busy || !!o.blocked} title={o.blocked ?? o.why} onclick={() => app.perform(o)}>{cap(o.label)}</button>
          {/each}
        {/if}
      </div>
    </div>
  {:else if app.report && app.report.status !== "ok"}
    <div class="next problem">
      <div class="text">
        <span class="kicker"><Icon name="exclaim" size={13} weight={2} /> {app.report.code ?? "Needs you"}</span>
        <h2>{app.report.message}</h2>
        {#if app.report.fix_steps?.length}
          <p class="fixes"><FixSteps steps={app.report.fix_steps} /></p>
        {:else}
          {#each app.report.fix ?? [] as f}<p class="mono small">{f}</p>{/each}
        {/if}
      </div>
      <div class="go">
        <button class="btn small ghost" title="Copy what went wrong" onclick={() => app.report && app.copyError(app.report)}><Icon name="copy" size={11} /> Copy</button>
      </div>
    </div>
  {/if}
</section>

<style>
  .split { position: relative; display: inline-flex; gap: 1px; }
  .split .more { padding: 0 0.6rem; }
  .menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 20; display: grid; min-width: 12rem; padding: 0.25rem; border-radius: 10px; background: var(--glass-strong, var(--bg)); box-shadow: var(--shadow-lg); }
  .menu button { all: unset; padding: 0.35rem 0.6rem; border-radius: 6px; cursor: pointer; font-size: 0.85rem; }
  .menu button:hover, .menu button:focus-visible { background: var(--fill); }
  .menu button.on::after { content: " ✓"; color: var(--accent); }
  .elsewhere .kicker { color: #a78bfa; display: inline-flex; align-items: center; gap: 0.4rem; }
  .elsewhere .thought { color: color-mix(in srgb, #bf5af2 60%, var(--text-2)); }
  .ring { width: 10px; height: 10px; border-radius: 50%; background: conic-gradient(from var(--a, 0deg), #ff9f0a, #ff375f, #bf5af2, #0a84ff, #64d2ff, #30d158, #ff9f0a); animation: turn 2.4s linear infinite; mask: radial-gradient(circle, transparent 30%, #000 34%); }
  @property --a { syntax: "<angle>"; initial-value: 0deg; inherits: false; }
  @keyframes turn { to { --a: 360deg; } }
  @media (prefers-reduced-motion: reduce) { .ring { animation: none; } }
  .hero { display: grid; gap: 1.5rem; margin: 0 auto 2.25rem; max-width: 1180px; container-type: inline-size; }
  .hero.compact { gap: 1rem; margin-bottom: 1.5rem; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 2rem; }
  .heading { min-width: 0; }
  .eyebrow { display: flex; align-items: center; gap: 0.4rem; margin: 0 0 0.15rem; color: var(--muted); font-size: 0.8rem; }
  .eyebrow .mono:first-child { margin-right: 0.35rem; }
  h1 { margin: 0; font: 700 2.15rem/1.12 var(--display); letter-spacing: -0.028em; }
  h1::first-letter { text-transform: uppercase; }
  .compact h1 { font-size: 1.5rem; letter-spacing: -0.02em; }
  .stats { display: flex; align-items: center; gap: 1.25rem; }
  .legend { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.2rem; }
  .legend li { display: grid; grid-template-columns: 16px 2ch auto; align-items: center; gap: 0.35rem; background: none; font-size: 0.82rem; }
  .legend .n { font: 650 0.85rem var(--rounded); text-align: right; font-variant-numeric: tabular-nums; }
  .legend .w { color: var(--text-2); }

  /* The journey: numbered nodes on one rule, done in green, now in blue. */
  .journey { list-style: none; margin: 0; padding: 0; display: flex; align-items: center; }
  .journey li { display: flex; align-items: center; gap: 0.5rem; color: var(--muted); flex: 1; font-size: 0.88rem; font-weight: 500; }
  .journey li:last-child { flex: none; }
  .journey li:not(:last-child)::after { content: ""; flex: 1; height: 2px; margin: 0 0.75rem; background: var(--fill-2); border-radius: 2px; }
  .journey li.done:not(:last-child)::after, .journey li.skipped:not(:last-child)::after { background: var(--verified-fill); }
  .dot {
    width: 24px; height: 24px; border-radius: 50%; display: grid; place-items: center; flex: none;
    font: 650 0.75rem/1 var(--rounded); background: var(--fill); color: var(--muted);
  }
  .done { color: var(--text-2); }
  .done .dot { background: var(--verified-fill); color: #fff; }
  .skipped .dot { background: transparent; box-shadow: inset 0 0 0 1.5px var(--fill-2); }
  .skipped .name { text-decoration: line-through; text-decoration-color: var(--faint); }
  .now { color: var(--accent-ink); font-weight: 650; }
  .now .dot { background: var(--accent); color: #fff; box-shadow: 0 0 0 4px var(--accent-soft); animation: breathe 2.6s var(--ease) infinite; }
  @keyframes breathe { 50% { box-shadow: 0 0 0 7px transparent; } }
  .name { white-space: nowrap; }

  /* The next step: the one card that asks for action. */
  .next {
    position: relative; display: flex; gap: 2rem; justify-content: space-between; align-items: center;
    padding: 1.4rem 1.6rem; border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow-md); overflow: hidden;
  }
  .next::before {
    content: ""; position: absolute; inset: 0; pointer-events: none;
    background: radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--accent) 9%, transparent), transparent 55%);
  }
  .next.warned::before { background: radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--danger-fill) 8%, transparent), transparent 55%); }
  .compact .next { padding: 0.8rem 0.9rem 0.8rem 1.25rem; border-radius: var(--radius); box-shadow: var(--shadow); }
  .compact .text { display: flex; align-items: center; gap: 0.9rem; flex-wrap: wrap; }
  .text { position: relative; min-width: 0; display: grid; gap: 0.3rem; justify-items: start; }
  .kicker { display: inline-flex; align-items: center; gap: 0.3rem; color: var(--accent-ink); font-size: 0.78rem; font-weight: 650; }
  .warned .kicker { color: var(--danger); }
  h2 { margin: 0; font: 650 1.35rem/1.25 var(--display); letter-spacing: -0.018em; }
  .compact h2 { font-size: 1rem; }
  .why { margin: 0; color: var(--text-2); }
  .small { font-size: 0.85rem; }
  .files { list-style: none; padding: 0.5rem 0.75rem; margin: 0.25rem 0; border-radius: var(--radius-sm); background: var(--add-bg); font-size: 0.84rem; }
  .files li { color: var(--add); }
  .plus { display: inline-block; width: 1.2em; font-weight: 700; }
  .stop { display: inline-flex; align-items: center; gap: 0.35rem; margin: 0.1rem 0; color: var(--danger); font-weight: 600; font-size: 0.9rem; }
  .compact .stop { font-size: 0.85rem; }
  .link { border: 0; background: none; padding: 0; color: var(--accent-ink); font-weight: 600; cursor: pointer; margin-left: 0.25rem; }
  .cmd {
    display: inline-flex; align-items: center; gap: 0.5rem; max-width: 100%; margin-top: 0.45rem; height: 28px; padding: 0 0.3rem 0 0.6rem;
    border: 0; border-radius: 7px; background: var(--fill); color: var(--text-2); cursor: copy; font-size: 0.82rem;
  }
  .cmd:hover { background: var(--fill-2); }
  .cmd .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .c { display: grid; place-items: center; width: 20px; color: var(--muted); }
  .go { position: relative; display: grid; gap: 0.45rem; justify-items: end; flex: none; }
  .compact .go .btn { height: 32px; font-size: 0.92rem; }
  .spin { width: 13px; height: 13px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 700ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .problem .kicker { color: var(--attention); }

  @container (max-width: 760px) {
    .top { flex-direction: column; align-items: flex-start; gap: 1rem; }
    .next { flex-direction: column; align-items: stretch; gap: 1rem; }
    .go { justify-items: start; }
    .journey .name { display: none; }
    .journey li.now .name { display: inline; }
  }
</style>
