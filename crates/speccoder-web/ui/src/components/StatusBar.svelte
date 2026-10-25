<script lang="ts">
  // The status bar (F-088), as VS Code's: the project on the left — the
  // feature, its branch, its verdicts, the spec's review, drift — and what
  // is happening on the right — the run, spend, the model, the sandbox, the
  // decisions waiting, the host. Each item is a button to where it leads.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  import type { Envelope } from "../lib/types";
  import Icon from "./Icon.svelte";

  const r = $derived(app.report?.data ?? null);
  const t = $derived(r?.totals ?? {});
  const stale = $derived(t.stale ?? 0);
  const review = $derived(r?.review ?? null);
  const reviewed = $derived(review ? review.approved + review.open + review.changes_requested + review.lapsed : 0);
  const waiting = $derived(app.inbox?.data?.items.length ?? 0);
  const check = (name: string) => app.doctor.find((c) => c.check === name);
  const model = $derived(check("model")?.detail.split(" ")[0] ?? null);
  const sandbox = $derived(check("sandbox"));
  const running = $derived(app.job?.running ? app.job : null);
  const stage = $derived([...app.stages].reverse().find((x) => x.state === "started")?.stage ?? null);

  interface Cost { total: { input_tokens: number; output_tokens: number; usd: number | null } }
  let cost = $state<Cost | null>(null);
  const costOf = latest();
  $effect(() => {
    void app.report;
    costOf(get<Envelope<Cost>>(`/api/cost${q({ feature: app.feature ?? undefined })}`), (c) => (cost = c.data ?? null), () => (cost = null));
  });
  const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n));
  const spend = $derived.by(() => {
    if (!cost) return null;
    const n = cost.total.input_tokens + cost.total.output_tokens;
    if (!n) return null;
    return cost.total.usd != null ? `$${cost.total.usd.toFixed(2)} · ${tokens(n)}` : `${tokens(n)} tokens`;
  });
  const go = (view: typeof app.view, doc?: string) => {
    if (doc) app.doc = doc;
    app.view = view;
  };
</script>

<footer class="status" aria-label="Status">
  <div class="side">
    {#if r}
      <button onclick={() => (app.palette = true)} title="The current feature — switch in the palette (⌘K)"><Icon name="list" size={12} />{r.feature}</button>
      {#if r.branch}
        <button onclick={() => go("code")} title="The feature's branch{r.unmerged_commits ? `: ${r.unmerged_commits} commit(s) not merged` : ''}"><Icon name="branch" size={12} />{r.branch.replace(/^speccoder\//, "")}{#if r.unmerged_commits}<span class="dim">↑{r.unmerged_commits}</span>{/if}</button>
      {/if}
      <button onclick={() => go("board")} title="Verified · regression only · ready · blocked">
        <span class="v">✓ {t.verified ?? 0}</span>{#if t.regression_only}<span class="r">~ {t.regression_only}</span>{/if}<span class="dim">· {t.ready ?? 0} ready</span>{#if t.blocked}<span class="dim">· {t.blocked} blocked</span>{/if}
      </button>
      {#if review && reviewed}
        <button class:warn={review.changes_requested + review.lapsed > 0} onclick={() => go("documents", "review")} title="Requirements approved on their current wording"><Icon name="doc" size={12} />{review.approved}/{reviewed} approved</button>
      {/if}
      {#if stale}
        <button class="warn" onclick={() => go("report")} title="Ticks proven against an earlier wording of what they cite"><Icon name="exclaim" size={12} />{stale} stale</button>
      {/if}
    {/if}
  </div>
  <div class="side right">
    {#if running}
      <button class="live" onclick={() => (app.view = "board")} title="What runs now">
        <span class="pulse" aria-hidden="true"></span>{app.task?.id ?? running.command}{#if stage}<span class="dim">· {stage}</span>{/if}
      </button>
    {/if}
    {#if spend}
      <button onclick={() => go("report")} title="What the feature cost so far"><Icon name="dollar" size={12} />{spend}</button>
    {/if}
    {#if model}
      <button onclick={() => (app.view = "settings")} title="The model (Settings › Model)"><Icon name="cpu" size={12} />{model}</button>
    {/if}
    {#if sandbox}
      <button class:warn={sandbox.level !== "pass"} onclick={() => (app.view = "settings")} title="The OS sandbox commands run in: {sandbox.detail}"><Icon name={sandbox.level === "pass" ? "lock" : "exclaim"} size={12} />{sandbox.level === "pass" ? sandbox.detail : "no sandbox"}</button>
    {/if}
    <button class:attn={waiting > 0} onclick={() => (app.view = "inbox")} title="Decisions waiting for you"><Icon name="bell" size={12} />{waiting}</button>
    <span class="conn" class:off={!app.connected} role="status" title={app.connected ? "Connected to the host" : "Reconnecting to the host…"}><span class="dot"></span>{app.connected ? "" : "Reconnecting"}</span>
  </div>
</footer>

<style>
  .status {
    grid-area: foot; display: flex; justify-content: space-between; align-items: stretch; gap: 1rem; height: 26px; padding: 0 0.4rem;
    border-top: 0.5px solid var(--separator); background: var(--surface-2); font-size: 0.74rem; color: var(--text-2); overflow: hidden;
  }
  .side { display: flex; align-items: stretch; min-width: 0; overflow: hidden; }
  .right { justify-content: flex-end; }
  button {
    display: inline-flex; align-items: center; gap: 0.32rem; padding: 0 0.55rem; border: 0; background: none; color: inherit; font: inherit;
    white-space: nowrap; cursor: pointer; border-radius: 5px; margin: 2px 0;
  }
  button:hover { background: var(--fill); color: var(--text); }
  button:focus-visible { box-shadow: inset 0 0 0 2px var(--focus); }
  .dim { color: var(--muted); }
  .v { color: var(--verified); font-weight: 600; }
  .r { color: var(--regression); font-weight: 600; }
  .warn { color: var(--attention); font-weight: 600; }
  .attn { background: var(--attention-fill); color: #fff; font-weight: 700; }
  .attn:hover { background: var(--attention-fill); color: #fff; filter: brightness(1.08); }
  .live { color: var(--running); font-weight: 600; }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: var(--running-fill); animation: beat 1.2s var(--ease) infinite; }
  @keyframes beat { 50% { transform: scale(0.6); opacity: 0.5; } }
  .conn { display: inline-flex; align-items: center; gap: 0.35rem; padding: 0 0.5rem; color: var(--attention); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--verified-fill); }
  .conn.off .dot { background: var(--attention-fill); animation: beat 1.2s ease-in-out infinite; }
  @media (max-width: 900px) { .side:first-child button:nth-child(n + 3) { display: none; } }
  @media (prefers-reduced-motion: reduce) { .pulse, .conn.off .dot { animation: none; } }
</style>
