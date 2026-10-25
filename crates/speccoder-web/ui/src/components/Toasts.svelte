<script lang="ts">
  // Banners, as the system's notifications arrive: from the top, on glass,
  // a symbol for the tone, the message and its detail.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";
  const sym = { ok: "check", warn: "exclaim", error: "xoctagon" } as const;
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast glass {t.tone}" role="status">
      <span class="sym"><Icon name={sym[t.tone]} size={14} weight={2.4} /></span>
      <span class="text">
        <strong>{t.text}</strong>
        {#if t.detail}<span class="mono detail">{t.detail}</span>{/if}
      </span>
      <button class="x" aria-label="Dismiss" onclick={() => (app.toasts = app.toasts.filter((x) => x.id !== t.id))}><Icon name="xmark" size={10} weight={2.6} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; top: 66px; left: 50%; transform: translateX(-50%); display: grid; gap: 0.5rem; z-index: 40; width: min(440px, calc(100vw - 2rem)); }
  .toast {
    display: grid; grid-template-columns: auto 1fr auto; gap: 0.7rem; align-items: center;
    padding: 0.7rem 0.75rem 0.7rem 0.8rem; border-radius: 18px; background: var(--glass-strong);
    box-shadow: var(--shadow-lg), var(--glass-edge); animation: drop 460ms var(--spring);
  }
  @keyframes drop { from { transform: translateY(-16px) scale(0.96); opacity: 0; } }
  .sym { width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center; color: #fff; background: var(--verified-fill); }
  .warn .sym { background: var(--attention-fill); }
  .error .sym { background: var(--danger-fill); }
  .text { display: grid; min-width: 0; }
  strong { font-weight: 600; font-size: 0.92rem; }
  .detail { font-size: 0.76rem; color: var(--muted); overflow-wrap: anywhere; }
  .x { width: 20px; height: 20px; border-radius: 50%; border: 0; background: var(--fill); color: var(--muted); display: grid; place-items: center; cursor: pointer; }
</style>
