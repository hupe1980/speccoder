<script lang="ts">
  // The running job as a live capsule floating over the page, after the
  // Dynamic Island: what runs, its stage and time, at a glance; orange when
  // it waits for you; a click opens the pipeline and the calls. It shows
  // while a job runs, and the end of one until it is put away.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  let open = $state(false);
  let dismissed = $state<number | null>(null);
  let now = $state(Date.now());
  let started = $state<number | null>(null);

  $effect(() => {
    if (app.job?.running) {
      dismissed = null;
      started ??= Date.now();
      const t = setInterval(() => (now = Date.now()), 1000);
      return () => clearInterval(t);
    }
    started = null;
  });

  const shown = $derived(!!app.job && dismissed !== app.job.id);
  const running = $derived(!!app.job?.running);
  const ok = $derived(app.job?.result?.status === "ok");
  const waiting = $derived(!!app.waiting);
  const at = $derived(new Map(app.stages.map((s) => [s.stage, s])));
  // `prepare` shows once a task reaches it (a worktree with Node or uv dependencies).
  const stages = $derived([...(at.has("prepare") ? ["prepare"] : []), "baseline", "red", "attempt", "verify", "audit", "mutation", "strength", "monitor", "commit"]);
  const current = $derived([...app.stages].reverse().find((x) => x.state === "started")?.stage);
  const passed = $derived(app.stages.filter((s) => s.state === "passed" || s.state === "skipped").length);
  const elapsed = $derived(started ? Math.max(0, Math.floor((now - started) / 1000)) : 0);
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  const sec = (ms: number | null) => (ms == null ? "" : ms < 10000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms / 1000)}s`);
  let log: HTMLElement | undefined = $state();
  $effect(() => {
    void app.calls.length;
    if (log) log.scrollTop = log.scrollHeight;
  });
</script>

{#if shown && app.job}
  <div class="island glass" class:open class:waiting class:ok={!running && ok} class:bad={!running && !ok} role="region" aria-label="The job">
    <button class="bar" onclick={() => (open = !open)} aria-expanded={open}>
      <span class="state" aria-hidden="true">
        {#if waiting}<Icon name="hand" size={15} weight={2} />
        {:else if running}<span class="spinner"></span>
        {:else if ok}<Icon name="check" size={15} weight={2.4} />
        {:else}<Icon name="xmark" size={14} weight={2.4} />{/if}
      </span>
      <span class="what">
        {#if waiting}<strong>Needs you</strong>
        {:else if running}<strong>{app.task?.id ?? "Working"}</strong>{#if current}<span class="dim">{current}</span>{/if}
        {:else}<strong>{ok ? "Done" : "Ended"}</strong><span class="dim">{app.job.result?.message ?? ""}</span>{/if}
      </span>
      {#if running}
        <span class="progress" aria-hidden="true"><span style="width:{(passed / stages.length) * 100}%"></span></span>
        <span class="time mono">{clock(elapsed)}</span>
      {/if}
      <Icon name={open ? "down" : "arrowup"} size={13} />
    </button>
    {#if open}
      <div class="body">
        <div class="meta">
          <code>{app.job.command}</code>
          {#if app.task}<span class="task">{app.task.title}</span>{/if}
        </div>
        {#if app.thinking}<p class="thinking" class:live={app.job?.running} title="Where the model's reasoning landed — its account, not evidence"><span class="therefore" aria-hidden="true">∴</span> {app.thinking}</p>{/if}
        {#if app.said}<p class="now" title="What the model says it is doing — its account, not evidence">“{app.said}”</p>{/if}
        <ol class="pipeline" aria-label="Pipeline">
          {#each stages as s}
            {@const v = at.get(s)}
            <li class="st-{v?.state ?? 'todo'}" title={v?.detail ?? ""}>
              <span class="g" aria-hidden="true">
                {#if v?.state === "passed"}<Icon name="check" size={11} weight={3} />{:else if v?.state === "failed"}<Icon name="xmark" size={10} weight={3} />{:else if v?.state === "started"}<span class="mini-spin"></span>{/if}
              </span>
              <span class="n">{s}</span>
              <span class="t mono">{v?.state === "started" ? "" : sec(v?.elapsed_ms ?? null)}</span>
            </li>
          {/each}
        </ol>
        <div class="log mono" bind:this={log} aria-live="polite">
          {#each app.calls as c}<div class:said={c.startsWith("“")} class:thought={c.startsWith("∴")} class:deny={c.includes(" deny")} class:ask={c.includes(" approval")}>{c}</div>{:else}<div class="dim">No tool calls yet.</div>{/each}
        </div>
        <div class="foot">
          {#if app.usage.in || app.usage.out}<span class="usage mono">{app.usage.in.toLocaleString()} in · {app.usage.out.toLocaleString()} out{#if app.usage.usd != null}{" · "}${app.usage.usd.toFixed(4)}{/if}</span>{/if}
          {#if running}
            <button class="btn small danger" onclick={() => app.stop()} title="Stops before the next task; the attempt stays resumable"><Icon name="stop" size={10} />Stop</button>
          {:else}
            <button class="btn small" onclick={() => { dismissed = app.job?.id ?? null; open = false; }}>Dismiss</button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .now { margin: 0; font-size: 0.8rem; line-height: 1.4; font-style: italic; color: rgba(235, 235, 245, 0.7); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .log .thought { font-family: var(--text-font, -apple-system, system-ui, sans-serif); color: #d0a8ff; white-space: normal; }
  .thinking { margin: 0.1rem 0 0.35rem; font-size: 0.8rem; line-height: 1.4; color: rgba(235, 235, 245, 0.72); }
  .thinking .therefore { color: #d0a8ff; font-weight: 700; }
  /* While the model works, its reasoning line breathes in the colours the
     running card wears. */
  .thinking.live {
    background: linear-gradient(90deg, rgba(235, 235, 245, 0.62) 0%, #e2c6ff 25%, #9ecbff 50%, rgba(235, 235, 245, 0.62) 75%);
    background-size: 300% 100%; -webkit-background-clip: text; background-clip: text; color: transparent;
    animation: shimmer 4.5s linear infinite;
  }
  .thinking.live .therefore { color: #d0a8ff; -webkit-text-fill-color: #d0a8ff; }
  @keyframes shimmer { from { background-position: 100% 0; } to { background-position: -200% 0; } }
  @media (prefers-reduced-motion: reduce) { .thinking.live { animation: none; color: rgba(235, 235, 245, 0.72); background: none; } }
  .log .said { font-family: var(--text-font, -apple-system, system-ui, sans-serif); font-style: italic; color: rgba(235, 235, 245, 0.62); white-space: normal; }
  .island {
    position: fixed; left: 50%; bottom: calc(1.25rem + 26px); transform: translateX(-50%); z-index: 15;
    width: min(420px, calc(100vw - 2rem)); border-radius: 22px; overflow: hidden;
    background: color-mix(in srgb, #000 82%, transparent); color: #f5f5f7;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.35), inset 0 0 0 0.5px rgba(255, 255, 255, 0.12);
    transition: width 420ms var(--spring), border-radius 420ms var(--spring);
    animation: rise 420ms var(--spring);
  }
  .island.open { width: min(640px, calc(100vw - 2rem)); border-radius: 26px; }
  @keyframes rise { from { transform: translate(-50%, 24px) scale(0.9); opacity: 0; } }
  .bar { all: unset; box-sizing: border-box; width: 100%; display: flex; align-items: center; gap: 0.7rem; height: 44px; padding: 0 1rem 0 0.75rem; cursor: pointer; }
  .bar:focus-visible { box-shadow: inset 0 0 0 2px var(--focus); border-radius: 22px; }
  .state { width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center; background: rgba(255, 255, 255, 0.12); flex: none; }
  .waiting .state { background: #ff9f0a; color: #000; animation: nudge 1.6s var(--ease) infinite; }
  @keyframes nudge { 50% { transform: scale(1.12); } }
  .ok .state { background: #30d158; color: #000; }
  .bad .state { background: #ff453a; color: #fff; }
  .spinner { width: 14px; height: 14px; border-radius: 50%; border: 2px solid rgba(255, 255, 255, 0.25); border-top-color: #64d2ff; animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .what { flex: 1; min-width: 0; display: flex; gap: 0.5rem; align-items: baseline; overflow: hidden; white-space: nowrap; }
  .what strong { font-weight: 650; }
  .waiting .what strong { color: #ffb340; }
  .dim { color: rgba(235, 235, 245, 0.6); overflow: hidden; text-overflow: ellipsis; font-size: 0.88rem; }
  .progress { width: 56px; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.15); overflow: hidden; flex: none; }
  .progress span { display: block; height: 100%; background: #64d2ff; transition: width 500ms var(--ease); }
  .time { font-size: 0.8rem; color: rgba(235, 235, 245, 0.7); font-variant-numeric: tabular-nums; }
  .body { display: grid; grid-template-columns: 10.5rem 1fr; gap: 0.75rem 1rem; padding: 0.25rem 1rem 1rem; }
  .meta { grid-column: 1 / -1; display: flex; gap: 0.6rem; align-items: center; min-width: 0; font-size: 0.82rem; }
  .meta code { background: rgba(255, 255, 255, 0.1); padding: 0.1rem 0.45rem; border-radius: 6px; white-space: nowrap; }
  .task { color: rgba(235, 235, 245, 0.6); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pipeline { list-style: none; margin: 0; padding: 0; display: grid; gap: 3px; align-content: start; }
  .pipeline li { display: grid; grid-template-columns: 16px 1fr auto; gap: 0.45rem; align-items: center; font-size: 0.82rem; color: rgba(235, 235, 245, 0.45); }
  .g { width: 16px; height: 16px; border-radius: 50%; display: grid; place-items: center; background: rgba(255, 255, 255, 0.08); }
  .st-passed { color: #f5f5f7; } .st-passed .g { background: #30d158; color: #000; }
  .st-failed { color: #ff6961; } .st-failed .g { background: #ff453a; color: #fff; }
  .st-started { color: #64d2ff; font-weight: 600; }
  .st-skipped { text-decoration: line-through; }
  .mini-spin { width: 10px; height: 10px; border-radius: 50%; border: 1.5px solid rgba(100, 210, 255, 0.3); border-top-color: #64d2ff; animation: spin 800ms linear infinite; }
  .t { font-size: 0.72rem; }
  .log { height: 180px; overflow: auto; background: rgba(255, 255, 255, 0.06); border-radius: 12px; padding: 0.55rem 0.75rem; font-size: 0.76rem; line-height: 1.6; color: rgba(235, 235, 245, 0.85); }
  .deny { color: #ff6961; } .ask { color: #ffb340; }
  .foot { grid-column: 1 / -1; display: flex; justify-content: flex-end; align-items: center; gap: 0.75rem; }
  .usage { margin-right: auto; font-size: 0.76rem; color: rgba(235, 235, 245, 0.6); }
  .island .btn { background: rgba(255, 255, 255, 0.14); color: #f5f5f7; }
  .island .btn.danger { background: rgba(255, 69, 58, 0.25); color: #ff6961; }
  @media (max-width: 640px) { .body { grid-template-columns: 1fr; } }
</style>
