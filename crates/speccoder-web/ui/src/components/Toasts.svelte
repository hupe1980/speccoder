<script lang="ts">
  import { app } from "../lib/app.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.tone}" role="status">
      <strong>{t.text}</strong>
      {#if t.detail}<span class="mono muted">{t.detail}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 1rem; bottom: 1rem; display: grid; gap: 0.5rem; z-index: 40; max-width: min(440px, 92vw); }
  .toast { background: var(--surface); border: 1px solid var(--border); border-left: 4px solid var(--verified); border-radius: 10px; box-shadow: var(--shadow-lg); padding: 0.7rem 0.9rem; display: grid; gap: 0.2rem; animation: in 180ms ease-out; }
  .toast.warn { border-left-color: var(--attention); }
  .toast.error { border-left-color: var(--danger); }
  .mono { font-size: 0.8rem; overflow-wrap: anywhere; }
  @keyframes in { from { transform: translateY(8px); opacity: 0; } }
</style>
