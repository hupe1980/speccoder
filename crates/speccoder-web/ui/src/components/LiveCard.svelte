<script lang="ts">
  // A run another speccoder process is doing for this project — a terminal's
  // `speccoder run` — as the status bar opens it: what it works on, where it
  // is, what the model last thought and said (a claim), and the way to it.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import Icon from "./Icon.svelte";
  import AgentStrip from "./AgentStrip.svelte";
  let { onclose }: { onclose: () => void } = $props();
  const r = $derived(app.live?.live ?? null);
  const interrupted = $derived(app.interrupted);
  // Seconds since it started, ticking while the card is open.
  let tick = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (tick = Date.now()), 1000);
    return () => clearInterval(t);
  });
  const offset = $derived((app.live?.now ?? Math.floor(Date.now() / 1000)) - Math.floor(Date.now() / 1000));
  const since = (t: number) => {
    const s = Math.max(0, Math.floor(tick / 1000) + offset - t);
    return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  };
  const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}k` : String(n));
  function show() {
    if (!r) return;
    void app.selectFeature(r.feature).then(() => {
      app.view = "board";
      void app.selectTask(r.task);
    });
    onclose();
  }
  function resume() {
    if (!r) return;
    void app.selectFeature(r.feature).then(() =>
      app.perform(offer({ kind: "run", task: r.task }, { group: "work", label: `run ${r.task}`, why: "resumes the interrupted attempt from its journal", command: `speccoder run --task ${r.task}` })),
    );
    onclose();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />
<div class="scrim" role="presentation" onclick={onclose}></div>
{#if r}
  <div class="card glass" class:interrupted role="dialog" tabindex="-1" aria-label="The project's run">
    <header>
      <span class="orb" aria-hidden="true"></span>
      <div>
        <p class="kicker">{interrupted ? "Interrupted" : "Running in another speccoder"} · <span class="mono">pid {r.pid}</span></p>
        <h3><span class="mono">{r.task}</span> {r.title}</h3>
        <p class="meta">{r.feature} · {r.stage ?? "starting"}{#if r.stage_detail}{" "}{r.stage_detail}{/if} · {interrupted ? `stopped ${since(r.updated_at)} ago` : since(r.started_at)}</p>
      </div>
    </header>
    {#if r.agent}<AgentStrip agent={r.agent} />{/if}
    {#if r.last_call}<p class="row"><Icon name="terminal" size={12} /><span class="mono">{r.last_call}</span></p>{/if}
    {#if r.thinking}<p class="row thought"><span class="therefore" aria-hidden="true">∴</span><span>{r.thinking}</span></p>{/if}
    {#if r.said}<p class="row said">“{r.said}”</p>{/if}
    <p class="row tokens muted">{tokens(r.input_tokens)} in · {tokens(r.output_tokens)} out{#if r.usd != null}{" · "}${r.usd.toFixed(4)}{/if}</p>
    <footer>
      <button class="btn small" onclick={show}><Icon name="grid" size={12} /> Show on Board</button>
      {#if interrupted}
        <button class="btn small tinted" onclick={resume}><Icon name="play" size={11} /> Resume</button>
      {:else}
        <span class="hint muted small">It holds this project: another run here is refused until it finishes.</span>
      {/if}
    </footer>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 44; }
  .card {
    position: fixed; right: 1rem; bottom: 2.4rem; z-index: 45; width: min(420px, calc(100vw - 2rem));
    border-radius: 16px; padding: 0.9rem 1rem 0.8rem; display: grid; gap: 0.45rem;
    background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); animation: rise 260ms var(--spring);
  }
  @keyframes rise { from { opacity: 0; transform: translateY(8px) scale(0.98); } }
  header { display: grid; grid-template-columns: auto 1fr; gap: 0.7rem; align-items: start; }
  .orb {
    width: 1.6rem; height: 1.6rem; border-radius: 50%; margin-top: 0.15rem;
    background: conic-gradient(from var(--angle, 0deg), #ff9f0a, #ff375f, #bf5af2, #0a84ff, #64d2ff, #30d158, #ff9f0a);
    animation: spin 3.2s linear infinite; box-shadow: 0 0 18px rgba(10, 132, 255, 0.35);
    mask: radial-gradient(circle, transparent 42%, #000 44%);
  }
  .interrupted .orb { background: var(--attention, #ff9f0a); animation: none; box-shadow: none; }
  @property --angle { syntax: "<angle>"; initial-value: 0deg; inherits: false; }
  @keyframes spin { to { --angle: 360deg; } }
  .kicker { margin: 0; font-size: 0.7rem; font-weight: 650; letter-spacing: 0.02em; text-transform: uppercase; color: var(--running, #0a84ff); }
  .interrupted .kicker { color: var(--attention, #ff9f0a); }
  h3 { margin: 0.1rem 0 0; font-size: 0.95rem; line-height: 1.3; }
  .meta { margin: 0.15rem 0 0; font-size: 0.78rem; color: var(--muted); }
  .row { margin: 0; display: flex; gap: 0.45rem; align-items: baseline; font-size: 0.8rem; color: var(--text-2); min-width: 0; }
  .row .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .thought { color: color-mix(in srgb, #bf5af2 60%, var(--text-2)); }
  .therefore { font-weight: 700; }
  .said { font-style: italic; }
  .tokens { font-size: 0.74rem; }
  footer { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.2rem; flex-wrap: wrap; }
  .hint { flex: 1; text-align: right; }
  @media (prefers-reduced-motion: reduce) { .orb, .card { animation: none; } }
</style>
