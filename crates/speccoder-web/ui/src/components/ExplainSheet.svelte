<script lang="ts">
  // A waiting call, explained by the facts speccoder holds — no model is
  // asked. "Ask about this call" opens Ask with it attached; the call keeps
  // waiting and is decided where it was.
  import { app } from "../lib/app.svelte";
  import { ask } from "../lib/ask.svelte";
  import { get, q } from "../lib/api";
  import type { Explanation, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";

  const want = $derived(app.explain!);
  let reply = $state<Reply<Explanation> | null>(null);
  $effect(() => {
    const { task, call } = want;
    reply = null;
    get<Reply<Explanation>>(`/api/approvals/explain${q({ task, call: call || undefined })}`)
      .then((r) => (reply = r))
      .catch((e) => (reply = { message: String(e) } as Reply<Explanation>));
  });
  const x = $derived(reply?.data ?? null);
  const close = () => (app.explain = null);
  // A modal holds the focus while open.
  let sheet = $state<HTMLElement | null>(null);
  $effect(() => sheet?.focus());
  function askAbout() {
    if (!x) return;
    ask.add({ kind: "call", reference: { task: x.task, call: x.call }, label: `${x.program ?? "call"} · ${x.task}` });
    close();
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="x-title" tabindex="-1" bind:this={sheet}>
    <header>
      <span class="badge" aria-hidden="true"><Icon name="info" size={20} weight={2.2} /></span>
      <div>
        <h2 id="x-title">{x?.question ?? "Explain"}</h2>
        <p class="sub">The facts speccoder holds — no model was asked. The call keeps waiting.</p>
      </div>
    </header>
    {#if !reply}
      <div class="center"><span class="spinner"></span></div>
    {:else if !x}
      <p class="bad"><Icon name="exclaim" size={14} /> {reply.message || "The call cannot be explained."}</p>
    {:else}
      <dl class="grouped">
        {#if x.program}<div><dt>Program</dt><dd><span class="mono">{x.program}</span>{#if x.path && x.path !== x.program}<span class="muted mono gap">{x.path}</span>{:else if !x.path}<span class="muted gap">— not found on PATH</span>{/if}</dd></div>{/if}
        <div><dt>Reaches</dt><dd>
          {#if x.hosts?.length}the network: <span class="mono">{x.hosts.join(", ")}</span>{#if x.outside?.length}; {/if}{/if}
          {#if x.outside?.length}outside the workspace: <span class="mono">{x.outside.join(", ")}</span>{/if}
          {#if !x.hosts?.length && !x.outside?.length}no host or path outside the workspace in its arguments{/if}
          {#if x.reach}<span class="muted block">{x.reach}</span>{/if}
        </dd></div>
        <div><dt>Asks because</dt><dd>{x.rule}</dd></div>
        {#if x.provenance?.length}<div><dt>Arguments from</dt><dd><ul>{#each x.provenance as l (l)}<li>{l}</li>{/each}</ul></dd></div>{/if}
        <div><dt>Task</dt><dd><span class="mono">{x.task}</span> {x.title}{#if x.scenarios?.length}<span class="muted block">its story's oracle checks {x.scenarios.join(", ")}</span>{/if}</dd></div>
      </dl>
      {#if x.said || x.reasoning}
        <section class="model" aria-label="The model's words before the call">
          <p class="label"><Icon name="sparkle" size={11} /> The model, before it asked — a claim, not evidence</p>
          {#if x.reasoning}<p class="claim">∴ {x.reasoning}</p>{/if}
          {#if x.said}<p class="claim">“{x.said}”</p>{/if}
        </section>
      {/if}
    {/if}
    <div class="row">
      <button class="btn" disabled={!x?.call} title="Ask reads what it needs and cites it; it never recommends a decision" onclick={askAbout}><Icon name="sparkle" size={12} /> Ask About This Call</button>
      <button class="btn primary" onclick={close}>Done</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 24; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet:focus { outline: none; }
  .sheet { width: min(600px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.3rem 1.4rem 1.1rem; display: grid; gap: 0.85rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #4aa3ff, var(--accent)); }
  h2 { margin: 0; font: 650 1.15rem/1.25 var(--display); letter-spacing: -0.016em; overflow-wrap: anywhere; }
  .sub { margin: 0.3rem 0 0; color: var(--text-2); font-size: 0.86rem; }
  dl { margin: 0; }
  dl > div { display: grid; grid-template-columns: 7.5rem 1fr; gap: 0.8rem; padding: 0.45rem 0.85rem; font-size: 0.86rem; }
  dt { color: var(--muted); }
  dd { margin: 0; overflow-wrap: anywhere; }
  dd ul { margin: 0; padding-left: 1rem; }
  .gap { margin-left: 0.5em; }
  .block { display: block; font-size: 0.8rem; margin-top: 0.15rem; }
  .muted { color: var(--muted); }
  .model { border-radius: var(--radius); background: var(--surface-2); padding: 0.55rem 0.85rem; display: grid; gap: 0.25rem; }
  .model p { margin: 0; }
  .label { font-size: 0.76rem; color: var(--muted); display: flex; gap: 0.3rem; align-items: center; }
  .claim { color: var(--text-2); font-style: italic; font-size: 0.85rem; white-space: pre-wrap; }
  .bad { margin: 0; display: flex; gap: 0.4rem; align-items: center; color: var(--danger); font-weight: 600; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; flex-wrap: wrap; }
  .center { display: grid; place-items: center; padding: 1.5rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 480px) { dl > div { grid-template-columns: 1fr; gap: 0.15rem; } }
</style>
