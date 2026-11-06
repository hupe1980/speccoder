<script lang="ts">
  // A unit that is a bug, as one card: it runs as one — reproduced,
  // confirmed by the person once, fixed — and its tasks close together
  // with the verified fix, so the board offers no task columns for it.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import { STAGES, at, bugStep, unitWords } from "../lib/bug";
  import type { UnitBug } from "../lib/schema.gen";
  import type { Task } from "../lib/types";
  import Code from "./Code.svelte";
  import Icon from "./Icon.svelte";

  let { bug, tasks }: { bug: UnitBug; tasks: Task[] } = $props();

  const step = $derived(bugStep(bug));
  const now = $derived(at(bug));
  const closed = (t: Task) => t.state !== "ready" && t.state !== "blocked";
  const open = () => (app.bugSheet = { key: bug.key });
  const command = $derived(`speccoder run${app.feature ? ` --feature ${app.feature}` : ""}`);

  function run() {
    app.perform(app.withAgent(offer({ kind: "run", task: null }, { group: "work", label: "run the bug", why: "reproduced, confirmed by you once, fixed — verified only when the revert is rejected", command })));
  }
</script>

<article class="bug" aria-labelledby="bug-card-title">
  <button class="head" onclick={open} title="Open the bug: its reproductions, confirmation and verdict">
    <span class="chip"><Icon name="exclaim" size={11} weight={2.2} />Bug</span>
    <h3 id="bug-card-title">{bug.title}</h3>
    <span class="words">{unitWords(bug)}</span>
  </button>

  <ol class="strip" aria-label="Stages">
    {#each STAGES as s, i (s.stage)}
      <li class:done={i < now} class:now={i === now} aria-current={i === now ? "step" : undefined}>{s.label}</li>
    {/each}
  </ol>

  {#if step}
    <div class="act">
      {#if step === "confirm"}
        <button class="btn tinted" onclick={open}><Icon name="check" size={12} />Confirm the Reproductions</button>
      {:else}
        <button class="btn tinted" disabled={!!app.job?.running} title="$ {command}" onclick={run}><Icon name="play" size={11} />{bug.outcome === null ? "Run the Bug" : "Run the Bug Again"}</button>
      {/if}
    </div>
  {/if}

  {#if tasks.length}
    <section class="tasks" aria-label="The spec's tasks">
      <p class="note">Ticked together when the fix is verified</p>
      <ul>
        {#each tasks as t (t.id)}
          <li class:closed={closed(t)}><span class="box" aria-hidden="true">{#if closed(t)}<Icon name="check" size={10} weight={2.4} />{/if}</span><span class="id mono">{t.id}</span><span class="t"><Code text={t.title} /></span><span class="sr">{closed(t) ? "done" : "open"}</span></li>
        {/each}
      </ul>
    </section>
  {/if}
</article>

<style>
  .bug { display: grid; gap: 0.9rem; max-width: 640px; margin: 0 auto; padding: 1.1rem 1.2rem 1.2rem; background: var(--surface); border-radius: var(--radius); box-shadow: var(--shadow); }
  .head { all: unset; display: grid; gap: 0.3rem; cursor: pointer; border-radius: 8px; }
  .head:focus-visible { box-shadow: 0 0 0 3.5px var(--focus); }
  .head:hover h3 { color: var(--accent-ink); }
  .chip { justify-self: start; display: inline-flex; align-items: center; gap: 0.3rem; padding: 0.05rem 0.5rem; border-radius: 999px; font-size: 0.7rem; font-weight: 650; background: color-mix(in srgb, var(--danger) 14%, var(--surface)); color: var(--danger); }
  h3 { margin: 0; font-size: 1.05rem; font-weight: 650; background: none; }
  .words { color: var(--text-2); font-size: 0.86rem; }
  .strip { display: flex; gap: 0.25rem; list-style: none; margin: 0; padding: 0; }
  .strip li { flex: 1; text-align: center; font-size: 0.72rem; padding: 0.25rem 0; border-radius: 6px; background: var(--fill); color: var(--muted); }
  .strip li.done { background: var(--verified-bg); color: var(--verified); font-weight: 600; }
  .strip li.now { background: var(--accent); color: #fff; font-weight: 650; }
  .act { display: flex; }
  .tasks { display: grid; gap: 0.35rem; }
  .note { margin: 0; font-size: 0.74rem; color: var(--muted); }
  .tasks ul { display: grid; gap: 0.2rem; margin: 0; padding: 0; list-style: none; }
  .tasks li { display: flex; align-items: baseline; gap: 0.45rem; font-size: 0.82rem; color: var(--text-2); }
  .tasks li.closed { color: var(--text); }
  .box { flex: none; width: 12px; height: 12px; display: inline-grid; place-items: center; border-radius: 3px; box-shadow: inset 0 0 0 1.5px var(--faint); transform: translateY(1px); }
  .closed .box { background: var(--verified-fill); color: #fff; box-shadow: none; }
  .id { flex: none; font-size: 0.72rem; color: var(--muted); min-width: 1.6rem; }
  .t { min-width: 0; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
  @media (max-width: 520px) { .strip li { font-size: 0.64rem; } }
</style>
