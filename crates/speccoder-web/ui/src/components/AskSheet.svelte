<script lang="ts">
  import { app } from "../lib/app.svelte";
  import Code from "./Code.svelte";

  const w = $derived(app.waiting!);
  const p = $derived(w.presentation);
  let full = $state(false);
  let note = $state("");
  let reason = $state("");
  let rejecting = $state(false);
  let text = $state("");
  const offered = $derived(new Set(p?.request.options ?? []));
  const body = $derived((full ? p?.full : p?.body) ?? []);
  const kind = (l: string) => (l.trimStart().startsWith("+") && !l.includes("+++") ? "add" : l.trimStart().startsWith("-") && !l.includes("---") ? "del" : "");

  const approve = (scope: "once" | "task" | "run") => app.answer({ answer: { decision: "approve", scope, note: note.trim() || null, reason: null, by: null } });
  const reject = () => {
    if (!reason.trim()) return;
    app.answer({ answer: { decision: "reject", scope: "once", note: null, reason: reason.trim(), by: null } });
  };
  const stopTask = () => app.answer({ answer: { decision: "stop", scope: "once", note: null, reason: null, by: null } });

  function keydown(e: KeyboardEvent) {
    if ((e.target as HTMLElement).closest("input, textarea")) return;
    if (!p) return;
    const k = e.key.toLowerCase();
    if (k === "y" || k === "1") approve("once");
    else if ((k === "t" || k === "2") && offered.has("task")) approve("task");
    else if ((k === "r" || k === "3") && offered.has("run")) approve("run");
    else if (k === "n" || k === "4") rejecting = true;
    else if (k === "s" || k === "5") stopTask();
    else if (k === "v") full = !full;
    else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="scrim">
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="ask-title">
    {#if p}
      <header>
        <span class="eyebrow">△ Needs you · {p.request.task}{p.request.turn ? ` · turn ${p.request.turn}` : ""}</span>
        <h2 id="ask-title">{p.title} <span class="mono subject">{p.subject}</span></h2>
        {#if p.request.task_title}<p class="for muted">for <Code text={p.request.task_title} /></p>{/if}
        <span class="risk r-{p.request.risk}">risk {p.request.risk}</span>
      </header>
      <pre class="body">{#each body as l}<span class={kind(l)}>{l}
</span>{/each}</pre>
      {#if p.full.length > p.body.length}
        <button class="btn small ghost" onclick={() => (full = !full)}>{full ? "Show less" : `Show all ${p.full.length} lines`} <kbd>V</kbd></button>
      {/if}
      <dl>
        <dt>Why it stopped</dt><dd><Code text={p.request.reason} /></dd>
        <dt>Sandbox</dt><dd>{p.sandbox}</dd>
        {#if p.request.claim}<dt>Model says</dt><dd class="claim">“{p.request.claim}” <span class="muted">(a claim)</span></dd>{/if}
        <dt>"For the task" covers</dt><dd><Code text={p.scope} /></dd>
      </dl>
      <input class="note" placeholder="A note for the model with your yes (optional)" bind:value={note} />
      {#if rejecting}
        <div class="reject">
          <!-- svelte-ignore a11y_autofocus -->
          <input autofocus placeholder="Why not? The model is told." bind:value={reason} onkeydown={(e) => e.key === "Enter" && reject()} />
          <button class="btn danger" disabled={!reason.trim()} onclick={reject}>Reject</button>
          <button class="btn ghost" onclick={() => (rejecting = false)}>Cancel</button>
        </div>
      {/if}
      <div class="choices">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn primary" autofocus onclick={() => approve("once")}>Yes, once <kbd>Y</kbd></button>
        <button class="btn" disabled={!offered.has("task")} onclick={() => approve("task")} title={offered.has("task") ? "" : "the call's class is not shown in full"}>For the task <kbd>T</kbd></button>
        <button class="btn" disabled={!offered.has("run")} onclick={() => approve("run")} title={offered.has("run") ? "" : "never for high-risk calls"}>For this run <kbd>R</kbd></button>
        <button class="btn danger" onclick={() => (rejecting = true)}>No… <kbd>N</kbd></button>
        <button class="btn ghost" onclick={stopTask} title="the call keeps waiting; decide later with speccoder task approve">Stop the task <kbd>S</kbd></button>
      </div>
    {:else}
      <header>
        <span class="eyebrow">A question</span>
        <h2 id="ask-title">{w.question}</h2>
      </header>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea autofocus rows="4" bind:value={text} placeholder="Your answer"></textarea>
      <div class="choices">
        <button class="btn primary" onclick={() => app.answer({ text })}>Send</button>
        <button class="btn ghost" onclick={() => app.answer({ text: "" })}>Skip</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(10, 12, 18, 0.45); display: grid; place-items: center; z-index: 20; backdrop-filter: blur(2px); }
  .sheet { width: min(860px, 94vw); max-height: 90vh; overflow: auto; background: var(--surface); border-radius: 14px; box-shadow: var(--shadow-lg); padding: 1.25rem 1.5rem; display: grid; gap: 0.9rem; border-top: 4px solid var(--attention); }
  header { display: grid; gap: 0.25rem; position: relative; }
  .for { margin: -0.3rem 0 0; font-size: 0.88rem; }
  .eyebrow { color: var(--attention); font-weight: 700; font-size: 0.78rem; letter-spacing: 0.05em; text-transform: uppercase; }
  h2 { margin: 0; font-size: 1.2rem; }
  .subject { font-weight: 500; color: var(--text-2); }
  .risk { position: absolute; right: 0; top: 0; font-size: 0.8rem; font-weight: 700; padding: 0.1rem 0.55rem; border-radius: 999px; }
  .r-low { background: var(--verified-bg); color: var(--verified); }
  .r-medium { background: var(--regression-bg); color: var(--regression); }
  .r-high { background: var(--danger-bg); color: var(--danger); }
  .body { margin: 0; max-height: 40vh; overflow: auto; background: var(--surface-2); border-radius: 8px; padding: 0.6rem 0; font-size: 0.82rem; }
  .body span { display: block; padding: 0 0.8rem; white-space: pre; }
  .add { background: var(--add-bg); color: var(--add); }
  .del { background: var(--del-bg); color: var(--del); }
  dl { margin: 0; display: grid; grid-template-columns: 10rem 1fr; gap: 0.3rem 1rem; }
  dt { color: var(--muted); font-weight: 600; }
  dd { margin: 0; }
  .claim { font-style: italic; color: var(--text-2); }
  .note, .reject input, textarea { width: 100%; border: 1px solid var(--border-strong); border-radius: 8px; padding: 0.55rem 0.7rem; background: var(--surface); }
  .reject { display: flex; gap: 0.5rem; }
  .choices { display: flex; flex-wrap: wrap; gap: 0.5rem; }
</style>
