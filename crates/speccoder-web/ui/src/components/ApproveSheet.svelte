<script lang="ts">
  // Approve the spec and run: one of three — ask on risky
  // calls, autopilot, or keep refining — with what the approval records and
  // freezes in view. Autopilot asks nobody: a call that would ask is refused
  // and listed afterwards; it holds only the grants ticked here, each shown
  // in full, none ticked by default.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import { approveCommand, blocks, freezes, sheetBlocked } from "../lib/review";
  import type { Review, RunMode } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { review, onclose, onrefine }: { review: Review; onclose: () => void; onrefine?: () => void } = $props();

  type Choice = RunMode | "refine";
  let mode = $state<Choice>("ask");
  let ticked = $state<string[]>([]);
  const sheet = $derived(review.sheet);
  const blocked = $derived(sheetBlocked(sheet, mode, !!app.job?.running));
  const autopilotOff = $derived(sheet?.autopilot_blocked ?? null);
  const grants = $derived(mode === "autopilot" ? ticked : []);
  const command = $derived(approveCommand(mode, grants));
  const approves = $derived(sheet?.approves.length ?? 0);
  const frozen = $derived(review.oracle.filter((v) => v.tests.some((t) => t.red) && !blocks(v)).length);

  let dialog = $state<HTMLElement>();
  $effect(() => {
    dialog?.querySelector<HTMLElement>("input[type=radio]:checked")?.focus();
  });
  function key(e: KeyboardEvent) {
    // The sheet's keys are its own: not the views' 1–6 behind it.
    e.stopPropagation();
    if (e.key === "Escape") return onclose();
    const typing = (e.target as HTMLElement)?.closest("textarea, input[type=text]");
    if (!typing && ["1", "2", "3"].includes(e.key)) {
      const next = (["ask", "autopilot", "refine"] as const)[Number(e.key) - 1]!;
      if (next !== "autopilot" || !autopilotOff) mode = next;
      e.preventDefault();
    }
    // The sheet keeps the focus: Tab cycles inside it.
    if (e.key === "Tab" && dialog) {
      const f = [...dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled])")];
      const first = f[0];
      const last = f[f.length - 1];
      if (!first || !last) return;
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  }
  async function go() {
    if (mode === "refine") {
      onclose();
      onrefine?.();
      return;
    }
    if (blocked.length) return;
    const m: RunMode = mode;
    onclose();
    await app.perform(
      offer(
        { kind: "approve_and_run", mode: m, grants, sheet: sheet?.sha256 ?? null },
        { group: "review", label: m === "autopilot" ? "approve and run on autopilot" : "approve and run", why: `approves ${approves} decision lines and freezes the written tests`, command },
      ),
    );
    app.view = "board";
  }
  const toggle = (id: string) => (ticked = ticked.includes(id) ? ticked.filter((g) => g !== id) : [...ticked, id]);
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="as-title" tabindex="-1" bind:this={dialog} onkeydown={key}>
    <header>
      <span class="badge" aria-hidden="true"><Icon name="check" size={20} weight={2.4} /></span>
      <div>
        <h2 id="as-title">Approve the spec and run</h2>
        <p>Approves {approves} decision {approves === 1 ? "line" : "lines"}.</p>
      </div>
    </header>
    {#if review.oracle.length}
      <table class="freeze">
        <thead><tr><th>Requirement</th><th>Freezes</th></tr></thead>
        <tbody>
          {#each review.oracle as v (v.req)}
            <tr class:blockedrow={blocks(v)}><td><span class="mono">{v.req}</span> {v.title}</td><td>{#if blocks(v)}<Icon name="exclaim" size={11} /> {/if}{freezes(v)}</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
    <fieldset class="choices">
      <legend class="sr">How the run asks</legend>
      <label class="choice" class:on={mode === "ask"}>
        <input type="radio" name="mode" value="ask" bind:group={mode} />
        <span><strong>Ask on Risky Calls</strong> <kbd>1</kbd><br /><span class="muted">You approve commands and protected writes as they come.</span></span>
      </label>
      <label class="choice" class:on={mode === "autopilot"} class:off={!!autopilotOff}>
        <input type="radio" name="mode" value="autopilot" bind:group={mode} disabled={!!autopilotOff} />
        <span><strong>Autopilot</strong> <kbd>2</kbd><br /><span class="muted">Nothing asks you. A call that would ask is refused and listed for you afterwards. Runs in the sandbox.</span>
          {#if autopilotOff}<span class="bad"><Icon name="exclaim" size={11} /> {autopilotOff}</span>{/if}
        </span>
      </label>
      {#if mode === "autopilot"}
        <div class="grants">
          {#if sheet?.grants.length}
            <p class="muted">Also allow for this run:</p>
            {#each sheet.grants as g (g.id)}
              <label class="grant"><input type="checkbox" checked={ticked.includes(g.id)} onchange={() => toggle(g.id)} /> <span class="mono">{g.object}</span> <span class="muted mono id">{g.id}</span></label>
            {/each}
          {:else}
            <p class="muted">No grants were decided for this feature: the run holds none.</p>
          {/if}
          {#if sheet?.sandbox}<p class="muted"><Icon name="shield" size={11} /> {sheet.sandbox}</p>{/if}
        </div>
      {/if}
      <label class="choice" class:on={mode === "refine"}>
        <input type="radio" name="mode" value="refine" bind:group={mode} />
        <span><strong>Keep Refining</strong> <kbd>3</kbd><br /><span class="muted">Nothing is recorded. Add comments and send them as one revision.</span></span>
      </label>
    </fieldset>
    {#if blocked.length}
      <ul class="blocked" aria-live="polite">{#each blocked as b (b)}<li><Icon name="exclaim" size={11} /> {b}</li>{/each}</ul>
    {/if}
    {#if mode !== "refine"}<p class="note muted">Approved, it froze tests for {frozen} {frozen === 1 ? "requirement" : "requirements"}; review.json is committed with it.</p>{/if}
    <p class="cli mono">$ {command}</p>
    <div class="row">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={mode !== "refine" && blocked.length > 0} onclick={go}>{mode === "refine" ? "Keep Refining" : "Approve and Run"}</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(680px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.85rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #4aa3ff, var(--accent)); }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); }
  .freeze { border-collapse: collapse; width: 100%; font-size: 0.86rem; }
  .freeze th { text-align: left; font-weight: 600; color: var(--muted); font-size: 0.78rem; padding: 0.3rem 0.5rem; border-bottom: 0.5px solid var(--separator); }
  .freeze td { padding: 0.35rem 0.5rem; border-bottom: 0.5px solid var(--separator); }
  .blockedrow td:last-child { color: var(--attention); font-weight: 600; }
  .choices { border: 0; margin: 0; padding: 0; display: grid; gap: 0.45rem; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
  .choice { display: flex; gap: 0.6rem; align-items: flex-start; padding: 0.65rem 0.8rem; border-radius: var(--radius); background: var(--surface); box-shadow: inset 0 0 0 1px var(--separator); cursor: pointer; font-size: 0.9rem; }
  .choice.on { box-shadow: inset 0 0 0 2px var(--accent); }
  .choice.off { opacity: 0.7; cursor: not-allowed; }
  .choice input { margin-top: 0.2rem; accent-color: var(--accent); }
  kbd { font: 0.68rem var(--mono); padding: 0 0.3rem; border-radius: 4px; background: var(--fill); color: var(--muted); }
  .bad { display: flex; gap: 0.3rem; align-items: center; color: var(--danger); font-weight: 600; font-size: 0.8rem; margin-top: 0.25rem; }
  .grants { display: grid; gap: 0.3rem; padding: 0 0.8rem 0 2.2rem; font-size: 0.84rem; }
  .grants p { margin: 0; }
  .grant { display: flex; gap: 0.4rem; align-items: baseline; }
  .grant .id { font-size: 0.7rem; }
  .blocked { margin: 0; padding: 0.55rem 0.8rem; list-style: none; border-radius: var(--radius); background: var(--attention-bg); color: var(--attention); font-size: 0.84rem; display: grid; gap: 0.2rem; }
  .note { margin: 0; font-size: 0.8rem; }
  .cli { margin: 0; font-size: 0.76rem; color: var(--muted); overflow-wrap: anywhere; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
