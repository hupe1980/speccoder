<script lang="ts">
  import { app } from "../lib/app.svelte";
  const a = $derived(app.input!.action);
  let text = $state("");
  const go = () => text.trim() && app.perform(a, text.trim());
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.input = null)}>
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="in-title">
    <span class="eyebrow">{a.label}</span>
    <h2 id="in-title">{a.input}</h2>
    <!-- svelte-ignore a11y_autofocus -->
    <textarea autofocus rows="5" bind:value={text} onkeydown={(e) => (e.metaKey || e.ctrlKey) && e.key === "Enter" && go()}></textarea>
    <p class="muted mono small">{a.command.replace("\"…\"", JSON.stringify(text || "…"))}</p>
    <div class="row">
      <button class="btn primary" disabled={!text.trim()} onclick={go}>Run <kbd>⌘↵</kbd></button>
      <button class="btn ghost" onclick={() => (app.input = null)}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(10, 12, 18, 0.45); display: grid; place-items: center; z-index: 20; }
  .sheet { width: min(640px, 94vw); background: var(--surface); border-radius: 14px; box-shadow: var(--shadow-lg); padding: 1.25rem 1.5rem; display: grid; gap: 0.7rem; border-top: 4px solid var(--accent); }
  .eyebrow { color: var(--accent); font-weight: 700; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.05em; }
  h2 { margin: 0; font-size: 1.15rem; }
  textarea { width: 100%; border: 1px solid var(--border-strong); border-radius: 8px; padding: 0.6rem 0.75rem; background: var(--surface); resize: vertical; }
  .small { font-size: 0.82rem; margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row { display: flex; gap: 0.5rem; }
</style>
