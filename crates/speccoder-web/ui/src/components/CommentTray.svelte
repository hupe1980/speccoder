<script lang="ts">
  // The comment tray: the person's comments on the spec, its
  // changes, its tests and its findings, collected and sent as one revision
  // limited to the sections they touch — then kept, or undone byte for byte.
  import { app } from "../lib/app.svelte";
  import { comments } from "../lib/comments.svelte";
  import { grouped, place, quoteOf, touched } from "../lib/review";
  import { diffWords } from "../lib/words";
  import Icon from "./Icon.svelte";

  // The host's list again whenever a job ends or the feature changes.
  $effect(() => {
    void app.feature;
    void app.job?.running;
    void app.report;
    void comments.load();
  });

  const view = $derived(comments.view);
  const drafts = $derived(view?.drafts ?? []);
  const sendable = $derived(drafts.filter((d) => d.state === "draft"));
  const turn = $derived(view?.turn ?? null);
  const sections = $derived(touched(sendable));
  const running = $derived(!!app.job?.running);
  const pendingTurn = $derived(turn && turn.outcome === "pending" ? turn : null);

  function key(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z" && pendingTurn) {
      e.preventDefault();
      void comments.undo();
    }
    if (e.key === "Escape") comments.open = false;
  }
  // The menu bar's Review › Send Comments and Undo Revision.
  $effect(() => {
    const on = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      if (id === "review.send") void (sendable.length ? comments.send() : app.toast({ tone: "warn", text: "No comments to send" }));
      if (id === "review.undo") void (pendingTurn ? comments.undo() : app.toast({ tone: "warn", text: "No revision to undo" }));
    };
    window.addEventListener("speccoder:review", on);
    return () => window.removeEventListener("speccoder:review", on);
  });
</script>

{#if comments.error}
  <aside class="tray glass" role="alert" aria-label="Comments"><p class="warn small"><Icon name="exclaim" size={11} /> The comment tray cannot be read: {comments.error}</p></aside>
{:else if drafts.length || turn}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <aside class="tray glass" id="comment-tray" tabindex="-1" aria-label="Comments" onkeydown={key}>
    {#if !comments.open}
      <button class="chip" onclick={() => (comments.open = true)} aria-expanded="false">
        <Icon name="doc" size={12} weight={2.2} /> {drafts.length} {drafts.length === 1 ? "comment" : "comments"}
        {#if pendingTurn}<span class="dot" aria-label="a revision to keep or undo"></span>{/if}
      </button>
    {:else}
      <header>
        <strong>{drafts.length} {drafts.length === 1 ? "comment" : "comments"}</strong>
        <button class="btn small ghost" onclick={() => (comments.open = false)} aria-label="Close the tray"><Icon name="down" size={11} /></button>
      </header>
      {#each grouped(drafts) as g (g.section)}
        <section>
          <h4>{g.section}</h4>
          {#each g.drafts as d (d.id)}
            <div class="draft" class:outdated={d.state === "outdated"}>
              <p class="quote" title={place(d.anchor)}>“{quoteOf(d.anchor)}”</p>
              <p class="text">{d.text}</p>
              <div class="acts">
                <span class="muted mono small">{d.id} · {place(d.anchor)}</span>
                {#if d.state === "outdated"}
                  <span class="badge">outdated</span>
                  <button class="btn small" disabled={running} onclick={() => comments.keep(d.id)}>Keep</button>
                {/if}
                <button class="btn small ghost" disabled={running} onclick={() => comments.drop(d.id)} aria-label="Delete {d.id}">Delete</button>
              </div>
            </div>
          {/each}
        </section>
      {/each}
      {#if sendable.length}
        <div class="send">
          <button class="btn primary" disabled={running} title="speccoder review comments send" onclick={() => comments.send()}>Send {sendable.length} {sendable.length === 1 ? "Comment" : "Comments"}</button>
          <span class="muted small">one revision{#if view?.revise_with} by {view.revise_with}{/if}, limited to {sections.length} {sections.length === 1 ? "section" : "sections"}</span>
        </div>
      {/if}
      {#if turn}
        <div class="result" aria-live="polite">
          <p><strong>Revised {turn.changed.length} {turn.changed.length === 1 ? "section" : "sections"}</strong>{#if turn.outside.length} · <span class="warn">{turn.outside.length} outside your comments</span>{/if} <span class="muted">({turn.outcome})</span></p>
          {#if turn.outside.length}<ul class="outside">{#each turn.outside as o (o)}<li><Icon name="exclaim" size={10} /> {o}</li>{/each}</ul>{/if}
          {#if turn.asked_again.length}<p class="muted small">Tests asked again: {turn.asked_again.join(", ")}</p>{/if}
          {#if turn.beside.length}<p class="warn small"><Icon name="exclaim" size={10} /> Changed beside the revision while it ran (Undo will not overwrite it): {turn.beside.join(", ")}</p>{/if}
          {#if pendingTurn && turn.diffs.length}
            <!-- Each changed section, word by word: a bar green inside your comments, amber outside them. -->
            {#each turn.diffs as d, n (n)}
              <section class="sdiff" class:out={d.outside} aria-label="{d.heading || d.path}{d.outside ? ', outside your comments' : ''}">
                <h4>{d.heading || d.path.split("/").pop()}{#if d.outside} <span class="badge">outside your comments</span>{/if}</h4>
                <p class="words">{#each diffWords(d.before, d.after) as w, i (i)}<span class={w.kind}>{w.text}</span>{/each}</p>
              </section>
            {/each}
          {/if}
          {#if pendingTurn}
            <div class="acts">
              <button class="btn small" disabled={running} onclick={() => comments.keep(turn.id)}>Keep</button>
              <button class="btn small" disabled={running} title="⌘Z · speccoder review comments undo" onclick={() => comments.undo()}><Icon name="undo" size={11} /> Undo Revision</button>
            </div>
          {/if}
        </div>
      {/if}
    {/if}
  </aside>
{/if}

<style>
  .tray { position: fixed; right: 1.25rem; bottom: 2.4rem; z-index: 15; width: min(380px, calc(100vw - 2.5rem)); max-height: 70vh; overflow: auto; border-radius: var(--radius-lg); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 0.6rem; display: grid; gap: 0.5rem; }
  .tray:has(.chip) { width: auto; padding: 0; background: none; box-shadow: none; }
  .chip { display: inline-flex; gap: 0.35rem; align-items: center; border: 0; cursor: pointer; padding: 0.45rem 0.8rem; border-radius: 999px; background: var(--accent); color: #fff; font-weight: 600; box-shadow: var(--shadow-lg); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--attention-fill); }
  header { display: flex; justify-content: space-between; align-items: center; }
  h4 { margin: 0.2rem 0; font-size: 0.76rem; color: var(--muted); font-weight: 650; }
  .draft { padding: 0.45rem 0.6rem; border-radius: var(--radius); background: var(--surface); display: grid; gap: 0.2rem; }
  .draft.outdated { box-shadow: inset 0 0 0 1px var(--attention-fill); }
  .quote { margin: 0; font-size: 0.78rem; color: var(--muted); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .text { margin: 0; font-size: 0.88rem; }
  .acts { display: flex; gap: 0.35rem; align-items: center; flex-wrap: wrap; }
  .small { font-size: 0.72rem; }
  .badge { font-size: 0.7rem; font-weight: 650; padding: 0 0.45rem; border-radius: 999px; background: var(--attention-bg); color: var(--attention); }
  .send { display: grid; gap: 0.25rem; }
  .result { display: grid; gap: 0.3rem; padding: 0.5rem 0.6rem; border-radius: var(--radius); background: var(--surface); font-size: 0.86rem; }
  .result p { margin: 0; }
  .warn { color: var(--attention); font-weight: 600; }
  .outside { margin: 0; padding-left: 0.2rem; list-style: none; font-size: 0.78rem; color: var(--attention); }
  .sdiff { border-left: 3px solid var(--verified-fill); padding: 0.1rem 0 0.1rem 0.5rem; }
  .sdiff.out { border-left-color: var(--attention-fill); }
  .words { margin: 0; font-size: 0.78rem; white-space: pre-wrap; max-height: 9rem; overflow: auto; }
  .words .gone { text-decoration: line-through; color: var(--danger); }
  .words .new { background: var(--verified-bg); color: var(--verified); }
</style>
