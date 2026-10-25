<script lang="ts">
  import { app } from "../lib/app.svelte";
  const a = $derived(app.input!.action);
  let text = $state("");
  const go = () => text.trim() && app.perform(a, text.trim());
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.input = null)}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="in-title">
    <span class="eyebrow">{a.label.charAt(0).toUpperCase() + a.label.slice(1)}</span>
    <h2 id="in-title">{a.input}</h2>
    <!-- svelte-ignore a11y_autofocus -->
    <textarea autofocus rows="5" bind:value={text} onkeydown={(e) => (e.metaKey || e.ctrlKey) && e.key === "Enter" && go()}></textarea>
    <p class="cmd mono">{a.command.replace("\"…\"", JSON.stringify(text || "…"))}</p>
    <div class="row">
      <button class="btn" onclick={() => (app.input = null)}>Cancel</button>
      <button class="btn primary" disabled={!text.trim()} onclick={go}>Start <kbd>⌘↵</kbd></button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 20; backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(620px, 100%); border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.75rem; animation: sheet 420ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.94) translateY(10px); opacity: 0; } }
  .eyebrow { color: var(--accent-ink); font-weight: 650; font-size: 0.8rem; }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  textarea { width: 100%; border: 0; border-radius: 12px; padding: 0.7rem 0.85rem; background: var(--fill); resize: vertical; outline: none; font-size: 1rem; }
  textarea:focus { box-shadow: 0 0 0 3.5px var(--focus); }
  .cmd { font-size: 0.78rem; margin: 0; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row { display: flex; gap: 0.5rem; justify-content: flex-end; }
</style>
