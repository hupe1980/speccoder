<script lang="ts">
  // An action that leaves the machine waits for a yes: what it does in
  // words, and — for a pull request — the dry run: what is pushed where,
  // the commands, the title and the body.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import type { Envelope } from "../lib/types";
  import Icon from "./Icon.svelte";

  const a = $derived(app.confirming!.action);
  interface Preview { branch: string; base: string; remote: string; title: string; body: string; commands: string[] }
  let preview = $state<Envelope<Preview> | null>(null);
  let showBody = $state(false);
  $effect(() => {
    if (a.run?.kind !== "pull_request") return;
    void get<Envelope<Preview>>(`/api/pr/preview${q({ feature: app.feature ?? undefined })}`).then((p) => (preview = p));
  });
  const blocked = $derived(preview && preview.status !== "ok" ? preview.message : null);
  const go = () => app.perform(a, undefined, true);
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") app.confirming = null;
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.confirming = null)}>
  <div class="sheet glass" role="alertdialog" aria-modal="true" aria-labelledby="c-title" aria-describedby="c-what">
    <header>
      <span class="badge" aria-hidden="true"><Icon name="arrowup" size={20} weight={2.2} /></span>
      <div>
        <h2 id="c-title">{a.label.charAt(0).toUpperCase() + a.label.slice(1)}?</h2>
        <p id="c-what">{a.confirm}</p>
      </div>
    </header>
    {#if a.run?.kind === "pull_request"}
      {#if !preview}
        <div class="center"><span class="spinner"></span></div>
      {:else if blocked}
        <p class="bad"><Icon name="exclaim" size={14} /> {blocked}</p>
      {:else if preview.data}
        <dl class="grouped">
          <div><dt>Branch</dt><dd class="mono">{preview.data.branch} → {preview.data.remote}</dd></div>
          <div><dt>Into</dt><dd class="mono">{preview.data.base}</dd></div>
          <div><dt>Title</dt><dd>{preview.data.title}</dd></div>
        </dl>
        <ol class="cmds mono">{#each preview.data.commands as c}<li>{c}</li>{/each}</ol>
        <button class="btn small ghost" onclick={() => (showBody = !showBody)}>{showBody ? "Hide" : "Show"} the description</button>
        {#if showBody}<pre class="body">{preview.data.body}</pre>{/if}
      {/if}
    {/if}
    <p class="cli mono">$ {a.command}</p>
    <div class="row">
      <button class="btn" onclick={() => (app.confirming = null)}>Cancel</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn primary" autofocus disabled={(a.run?.kind === "pull_request" && (!preview || !!blocked)) || !!app.job?.running} onclick={go}>
        {a.run?.kind === "pull_request" ? "Push and Open Draft" : "Continue"}
      </button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(640px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.9rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #4aa3ff, var(--accent)); }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); }
  dl { margin: 0; }
  dl > div { display: grid; grid-template-columns: 5rem 1fr; gap: 1rem; padding: 0.5rem 0.85rem; font-size: 0.88rem; }
  dt { color: var(--muted); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .cmds { margin: 0; padding: 0.6rem 0.9rem 0.6rem 2rem; border-radius: var(--radius); background: var(--surface-2); font-size: 0.76rem; color: var(--text-2); display: grid; gap: 0.2rem; }
  .body { margin: 0; max-height: 30vh; overflow: auto; padding: 0.7rem 0.9rem; border-radius: var(--radius); background: var(--surface-2); font-size: 0.76rem; white-space: pre-wrap; }
  .cli { margin: 0; font-size: 0.76rem; color: var(--muted); }
  .bad { margin: 0; display: flex; gap: 0.4rem; align-items: center; color: var(--danger); font-weight: 600; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; }
  .center { display: grid; place-items: center; padding: 1.5rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
