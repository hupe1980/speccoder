<script lang="ts">
  // A quick change: a small change in the person's words, and when
  // it is done — recorded as a story, a requirement and a task, then run and
  // judged like any other. No planning ceremony; no skipped evidence.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import Icon from "./Icon.svelte";
  import WritingHelp from "./WritingHelp.svelte";

  let request = $state(app.quickDraft);
  app.quickDraft = "";
  let checks = $state<string[]>([""]);
  const ready = $derived(request.trim().length > 0 && !app.job?.running);

  async function go(run: boolean) {
    if (!ready) return;
    const done = checks.map((c) => c.trim()).filter(Boolean);
    const quoted = request.trim().replaceAll('"', '\\"');
    app.quick = false;
    await app.perform(
      offer(
        { kind: "quick_change", request: request.trim(), checks: done, run },
        {
          group: "work",
          label: run ? "make the change" : "record the change",
          why: "a story, a requirement and a task — judged on evidence",
          command: `speccoder do "${quoted}"${done.map((c) => ` --check "${c.replaceAll('"', '\\"')}"`).join("")}${run ? "" : " --no-run"}`,
        },
      ),
    );
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") app.quick = false;
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void go(true);
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.quick = false)}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="qc-title">
    <header>
      <span class="badge" aria-hidden="true"><Icon name="sparkle" size={20} weight={2.2} /></span>
      <div>
        <h2 id="qc-title">Quick Change</h2>
        <p>Something small — a fix, a tweak, a follow-up. It becomes a story, a requirement and a task, and is judged like everything else: tests that fail first, checks that must hold.</p>
      </div>
    </header>
    <label class="field">
      <span>What should change?</span>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea rows="3" bind:value={request} autofocus placeholder="Show the version number in the footer"></textarea>
    </label>
    <WritingHelp field="quick" text={request} onshaped={(t) => (request = t)} />
    <div class="field">
      <span>Done when <em>— optional; the tests are written from these</em></span>
      {#each checks as _, i (i)}
        <div class="check">
          <span class="n mono">{i + 1}.</span>
          <input bind:value={checks[i]} placeholder={i === 0 ? "the footer of every page shows the version" : "and…"} aria-label="Done when, {i + 1}" onkeydown={(e) => { if (e.key === "Enter" && !(e.metaKey || e.ctrlKey)) { e.preventDefault(); if (checks[i]?.trim()) checks = [...checks, ""]; } }} />
          {#if checks.length > 1}<button class="x" aria-label="Remove" onclick={() => (checks = checks.filter((_, j) => j !== i))}><Icon name="xmark" size={10} weight={2.6} /></button>{/if}
        </div>
      {/each}
      <button class="add" onclick={() => (checks = [...checks, ""])}><Icon name="plus" size={10} weight={2.6} /> Add a condition</button>
    </div>
    <div class="row">
      <span class="hint muted">⌘↩ records and runs</span>
      <button class="btn" onclick={() => (app.quick = false)}>Cancel</button>
      <button class="btn" disabled={!ready} onclick={() => go(false)}>Record</button>
      <button class="btn primary" disabled={!ready} onclick={() => go(true)}>Record and Run</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(600px, 100%); border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 1rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .sheet { animation: none; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #bf5af2, #8944ab); }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); font-size: 0.9rem; }
  .field { display: grid; gap: 0.4rem; }
  .field > span { font-weight: 600; font-size: 0.88rem; }
  em { font-style: normal; font-weight: 500; color: var(--muted); }
  textarea, input { padding: 0.5rem 0.7rem; border-radius: 9px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); font: inherit; }
  textarea { resize: vertical; line-height: 1.5; font-size: 1rem; }
  textarea:focus, input:focus { outline: none; box-shadow: 0 0 0 3.5px var(--focus); }
  .check { display: flex; align-items: center; gap: 0.4rem; }
  .check input { flex: 1; min-width: 0; font-size: 0.9rem; }
  .n { width: 1.4rem; color: var(--muted); font-size: 0.8rem; text-align: right; }
  .x { width: 22px; height: 22px; border-radius: 50%; border: 0; background: var(--fill); color: var(--muted); display: grid; place-items: center; cursor: pointer; }
  .add { justify-self: start; display: inline-flex; gap: 0.3rem; align-items: center; border: 0; background: none; color: var(--accent-ink); font-weight: 600; font-size: 0.82rem; cursor: pointer; padding: 0.2rem 0; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; align-items: center; }
  .hint { margin-right: auto; font-size: 0.76rem; }
</style>
