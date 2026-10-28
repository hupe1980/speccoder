<script lang="ts">
  // A task that needs the person, in place: why — the agent's reason, as
  // its claim, or the task's [YOU] label — and the three ways on: I did it,
  // a hint and a new attempt now (the note typed right here), or skip.
  import { app } from "../lib/app.svelte";
  import type { Offer } from "../lib/actions";
  import type { Action } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { task, claim, actions }: { task: string; claim: string | null; actions: (Action | Offer)[] } = $props();
  const of = (kind: string) => actions.find((a) => a.run?.kind === kind) ?? null;
  const done = $derived(of("done"));
  const hint = $derived(of("hint"));
  const skip = $derived(of("skip"));
  let hinting = $state(false);
  let note = $state("");

  async function send(e: SubmitEvent) {
    e.preventDefault();
    const text = note.trim();
    if (!hint || !text) return;
    await app.perform(hint, text);
    hinting = false;
    note = "";
  }
</script>

<section class="stop" aria-label="{task} needs you">
  <span class="glyph" aria-hidden="true">🙋</span>
  <div class="what">
    <strong>{task} needs you</strong>
    {#if claim}
      <p class="claim">The agent says “{claim}” <span class="muted">— a claim, not evidence</span></p>
    {:else}
      <p>Marked <code>[YOU]</code>: a step only a person can take. The run does not attempt it.</p>
    {/if}
    <div class="row">
      {#if done}<button class="btn small primary" disabled={!!app.job?.running} title={`${done.why}\n$ ${done.command}`} onclick={() => done && app.perform(done)}><Icon name="check" size={11} weight={2.4} /> I Did It</button>{/if}
      {#if hint}<button class="btn small tinted" aria-expanded={hinting} disabled={!!hint.blocked || !!app.job?.running} title={`${hint.blocked ?? hint.why}\n$ ${hint.command}`} onclick={() => (hinting = !hinting)}><Icon name="sparkle" size={11} weight={2.2} /> Hint &amp; Retry</button>{/if}
      {#if skip}<button class="btn small ghost" disabled={!!app.job?.running} title={`${skip.why}\n$ ${skip.command}`} onclick={() => skip && app.perform(skip)}>Skip…</button>{/if}
      <button class="btn icon small ghost" title="Copy what went wrong" aria-label="Copy why {task} stopped" onclick={() => app.copyError({ message: claim ? `${task} needs you — the agent says: ${claim}` : `${task} needs you: marked [YOU]` }, "stopped")}><Icon name="copy" size={11} /></button>
    </div>
    {#if hinting}
      <form class="hint" onsubmit={send}>
        <!-- svelte-ignore a11y_autofocus -->
        <input aria-label="Hint for {task}" placeholder="What the agent is missing" bind:value={note} autofocus />
        <button class="btn small primary" type="submit" disabled={!note.trim() || !!app.job?.running}>Retry</button>
      </form>
    {/if}
    {#if skip?.why}<p class="muted small">Skip: {skip.why}</p>{/if}
  </div>
</section>

<style>
  .stop {
    display: grid; grid-template-columns: auto 1fr; gap: 0.7rem; padding: 0.8rem 0.9rem; border-radius: var(--radius);
    background: color-mix(in srgb, var(--attention, #ff9f0a) 9%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--attention, #ff9f0a) 28%, transparent);
  }
  .glyph { width: 2rem; height: 2rem; border-radius: 9px; display: grid; place-items: center; font-size: 1.05rem; background: var(--surface); }
  .what { display: grid; gap: 0.35rem; min-width: 0; }
  p { margin: 0; font-size: 0.84rem; color: var(--text-2); line-height: 1.4; }
  .claim { font-style: italic; }
  .claim .muted { font-style: normal; }
  .row { display: flex; flex-wrap: wrap; gap: 0.4rem; margin-top: 0.15rem; }
  .hint { display: flex; gap: 0.4rem; }
  .hint input { flex: 1; min-width: 0; height: 28px; padding: 0 0.6rem; border-radius: 7px; border: 1px solid var(--fill-2); background: var(--surface); color: var(--text); font: inherit; font-size: 0.86rem; }
  .small { font-size: 0.78rem; }
</style>
