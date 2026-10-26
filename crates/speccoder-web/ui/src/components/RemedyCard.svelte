<script lang="ts">
  // A stop speccoder knows the fix for, offered as one action: what
  // stopped, what the fix changes, the exact command it amounts to and how
  // to take it back. Applied only on the person's click, then what stopped
  // runs again. Stays until done or dismissed — not a toast that fades.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";
  const r = $derived(app.remedy!);
  let open = $state(false);
  let copied = $state(false);
  const again = $derived(!!r.then);
  // "what stopped — the tool's own words": the first is the headline.
  const [headline, detail] = $derived.by(() => {
    const at = r.message.indexOf(" — ");
    return at < 0 ? [r.message, null] : [r.message.slice(0, at), r.message.slice(at + 3)];
  });
  async function copy() {
    await navigator.clipboard?.writeText(r.remedy.command).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") app.remedy = null;
  }
</script>

<svelte:window onkeydown={key} />

<div class="card glass" role="alertdialog" aria-labelledby="r-title" aria-describedby="r-why">
  <header>
    <span class="badge" aria-hidden="true"><Icon name="key" size={16} weight={2.2} /></span>
    <div class="text">
      <p class="kicker">Stopped before any model call{#if r.code}{" · "}<span class="mono">{r.code}</span>{/if}</p>
      <h3 id="r-title">{headline}</h3>
      {#if detail}<p class="detail">{detail}</p>{/if}
      <p id="r-why" class="effect"><strong>{r.remedy.label}</strong> {r.remedy.effect}.</p>
    </div>
    <button class="close" aria-label="Not now" onclick={() => (app.remedy = null)}><Icon name="xmark" size={12} /></button>
  </header>
  <button class="disclose" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="chev" class:open><Icon name="chevron" size={10} /></span> {open ? "Hide" : "Show"} the command
  </button>
  {#if open}
    <div class="cmd">
      <code class="mono">$ {r.remedy.command}</code>
      <button class="btn small ghost" onclick={copy}><Icon name={copied ? "check" : "copy"} size={11} /> {copied ? "Copied" : "Copy"}</button>
    </div>
    {#if r.remedy.undo}<p class="undo muted"><Icon name="undo" size={11} /> Undo afterwards: <span class="mono">{r.remedy.undo}</span></p>{/if}
  {/if}
  <footer>
    <button class="btn small" onclick={() => (app.remedy = null)}>Not Now</button>
    <!-- svelte-ignore a11y_autofocus -->
    <button class="btn small primary" autofocus disabled={!!app.job?.running} onclick={() => app.applyRemedy()}>
      <Icon name="check" size={11} weight={2.4} /> {again ? `${r.remedy.label.split(" ")[0]} and Run Again` : r.remedy.label}
    </button>
  </footer>
</div>

<style>
  .card {
    position: fixed; left: 50%; bottom: 2.6rem; transform: translateX(-50%); z-index: 42;
    width: min(520px, calc(100vw - 2rem)); border-radius: 18px; padding: 0.95rem 1rem 0.85rem;
    display: grid; gap: 0.55rem; background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge);
    animation: rise 320ms var(--spring);
  }
  @keyframes rise { from { opacity: 0; transform: translate(-50%, 12px) scale(0.98); } }
  header { display: grid; grid-template-columns: auto 1fr auto; gap: 0.75rem; align-items: start; }
  .badge {
    width: 2rem; height: 2rem; border-radius: 10px; display: grid; place-items: center; color: #fff;
    background: linear-gradient(135deg, #ff9f0a, #ff375f); box-shadow: 0 6px 16px color-mix(in srgb, #ff9f0a 35%, transparent);
  }
  .text { min-width: 0; }
  .kicker { margin: 0; font-size: 0.68rem; font-weight: 650; letter-spacing: 0.02em; text-transform: uppercase; color: var(--attention, #ff9f0a); }
  h3 { margin: 0.15rem 0 0; font-size: 0.92rem; line-height: 1.35; font-weight: 600; overflow-wrap: anywhere; }
  .detail { margin: 0.25rem 0 0; font-size: 0.72rem; color: var(--muted); line-height: 1.4; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .effect { margin: 0.3rem 0 0; font-size: 0.82rem; color: var(--text-2); line-height: 1.4; }
  .close { border: 0; background: var(--fill); color: var(--muted); width: 1.4rem; height: 1.4rem; border-radius: 50%; display: grid; place-items: center; cursor: pointer; }
  .close:hover { color: var(--text); }
  .disclose { justify-self: start; border: 0; background: none; padding: 0; font: inherit; font-size: 0.76rem; font-weight: 600; color: var(--accent-ink); cursor: pointer; display: inline-flex; align-items: center; gap: 0.3rem; }
  .chev { display: inline-flex; transition: transform 160ms var(--spring); }
  .chev.open { transform: rotate(90deg); }
  .cmd { display: flex; gap: 0.5rem; align-items: start; padding: 0.5rem 0.6rem; border-radius: 10px; background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--border); }
  .cmd code { flex: 1; font-size: 0.74rem; line-height: 1.45; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-2); }
  .undo { margin: 0; font-size: 0.74rem; display: flex; gap: 0.35rem; align-items: center; }
  footer { display: flex; justify-content: flex-end; gap: 0.5rem; }
  @media (prefers-reduced-motion: reduce) { .card, .chev { animation: none; transition: none; } }
</style>
