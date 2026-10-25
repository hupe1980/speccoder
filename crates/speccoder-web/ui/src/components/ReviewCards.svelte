<script lang="ts">
  // The spec, reviewed requirement by requirement (F-087): each card is one
  // requirement's wording and the person's decision on exactly that wording.
  // A wording changed since its decision shows what was decided, word by
  // word. Change requests go to the format's revising skill as one request.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  import type { Action, Envelope, Review, ReviewItem } from "../lib/types";
  import { diffWords } from "../lib/words";
  import Icon from "./Icon.svelte";

  let review = $state<Envelope<Review> | null>(null);
  const fetchOf = latest();
  $effect(() => {
    void app.report;
    void app.job?.running;
    fetchOf(get<Envelope<Review>>(`/api/review${q({ feature: app.feature ?? undefined })}`), (r) => (review = r), () => (review = null));
  });
  const r = $derived(review?.data ?? null);
  const total = $derived(r?.requirements.length ?? 0);
  const pending = $derived((r?.requirements ?? []).filter((i) => i.state !== "approved"));
  const words: Record<ReviewItem["state"], string> = { open: "To Review", approved: "Approved", changes_requested: "Changes Requested", lapsed: "Changed Since Decided" };
  const tone: Record<ReviewItem["state"], string> = { open: "ready", approved: "verified", changes_requested: "needs_approval", lapsed: "needs_approval" };

  const act = (ids: string[], verdict: "approve" | "request" | "clear", label: string): Action => ({
    group: "plan",
    label,
    why: verdict === "request" ? "the comment says what the requirement should say" : "recorded with your name, bound to this wording",
    command: `speccoder review ${verdict}${ids.length ? " " + ids.join(" ") : ""}${verdict === "request" ? ' --comment "…"' : ""}`,
    run: { kind: "review", ids, verdict },
    blocked: null,
    input: verdict === "request" ? `What should ${ids[0]} say?` : null,
  });
  const reviseNow = () => {
    if (!r?.revise_with || !r.revision) return;
    void app.perform({
      group: "plan",
      label: `revise the spec (${r.revise_with})`,
      why: "the format's skill rewrites what you asked; you review the result",
      command: `speccoder spec ${r.revise_with} "…"`,
      run: { kind: "skill", name: r.revise_with, arguments: r.revision },
      blocked: null,
      input: null,
    });
  };
  const when = (s: number | null) => (s ? new Date(s * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "");
</script>

{#if !review}
  <div class="center"><span class="spinner"></span></div>
{:else if !r || !total}
  <div class="none muted">{review.message}</div>
{:else}
  <section class="review" aria-label="Review the spec">
    <header>
      <div>
        <h2>Review the Spec</h2>
        <p class="sub">The result will be judged against these words. Approve each one, or say what it should say.</p>
      </div>
      <div class="progress" role="img" aria-label="{r.totals.approved} of {total} approved">
        <span class="bar"><span style="width:{(r.totals.approved / total) * 100}%"></span></span>
        <span class="mono">{r.totals.approved}/{total}</span>
      </div>
    </header>
    <div class="bulk">
      {#if pending.length}
        <button class="btn small primary" disabled={!!app.job?.running} onclick={() => app.perform(act([], "approve", `approve ${pending.length} requirement(s)`))}><Icon name="check" size={11} weight={2.4} /> Approve {pending.length === total ? "All" : `the Other ${pending.length}`}</button>
      {/if}
      {#if r.totals.changes_requested && r.revise_with}
        <button class="btn small tinted" disabled={!!app.job?.running} onclick={reviseNow}><Icon name="sparkle" size={11} weight={2.2} /> Revise the Spec ({r.totals.changes_requested} {r.totals.changes_requested === 1 ? "request" : "requests"})</button>
      {/if}
      <span class="file muted">Recorded in <code>{r.file}</code> — commit it with the spec.</span>
    </div>
    <ol class="cards">
      {#each r.requirements as i (i.id)}
        <li class="card s-{tone[i.state]}-edge">
          <div class="top">
            <span class="id mono">{i.id}</span>
            <span class="chip s-{tone[i.state]}">{words[i.state]}</span>
          </div>
          <p class="text">
            {#if i.state === "lapsed" && i.decided_on}
              {#each diffWords(i.decided_on, i.text) as s, n (n)}<span class={s.kind}>{s.text}</span>{/each}
            {:else}{i.text}{/if}
          </p>
          {#if i.by}
            <p class="who muted">{i.state === "lapsed" ? "Decided on the earlier wording by" : i.state === "approved" ? "Approved by" : "Requested by"} {i.by}{#if i.at} · {when(i.at)}{/if}{#if i.comment}<span class="comment">“{i.comment}”</span>{/if}</p>
          {/if}
          <div class="acts">
            {#if i.state !== "approved"}
              <button class="btn small primary" disabled={!!app.job?.running} onclick={() => app.perform(act([i.id], "approve", `approve ${i.id}`))}><Icon name="check" size={11} weight={2.4} /> Approve</button>
            {/if}
            <button class="btn small" disabled={!!app.job?.running} onclick={() => app.perform(act([i.id], "request", `request changes to ${i.id}`))}><Icon name="sparkle" size={11} weight={2.2} /> Request Changes</button>
            {#if i.state !== "open"}
              <button class="btn small ghost" disabled={!!app.job?.running} onclick={() => app.perform(act([i.id], "clear", `clear ${i.id}`))}>Clear</button>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style>
  .review { display: grid; gap: 1rem; max-width: 78ch; }
  header { display: flex; justify-content: space-between; align-items: flex-end; gap: 1rem; flex-wrap: wrap; }
  h2 { margin: 0; font: 700 1.5rem/1.2 var(--display); letter-spacing: -0.02em; }
  .sub { margin: 0.25rem 0 0; color: var(--text-2); }
  .progress { display: flex; align-items: center; gap: 0.5rem; font-size: 0.82rem; }
  .bar { width: 9rem; height: 6px; border-radius: 6px; background: var(--fill); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--verified-fill); border-radius: 6px; transition: width 400ms var(--spring); }
  .bulk { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .file { font-size: 0.78rem; margin-left: auto; }
  .cards { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.7rem; }
  .card { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 1rem 1.2rem 1.05rem; display: grid; gap: 0.5rem; border-left: 3px solid var(--fill-c, var(--fill)); }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .id { font-weight: 650; font-size: 0.82rem; }
  .chip { font-size: 0.72rem; font-weight: 650; padding: 0.08rem 0.55rem; border-radius: 999px; }
  .text { margin: 0; font-size: 1rem; line-height: 1.55; color: var(--text); }
  .gone { color: var(--danger); text-decoration: line-through; opacity: 0.8; }
  .new { background: color-mix(in srgb, var(--verified-fill) 22%, transparent); border-radius: 4px; padding: 0 0.12em; font-weight: 550; }
  .who { margin: 0; font-size: 0.8rem; }
  .comment { display: block; margin-top: 0.2rem; color: var(--text-2); font-style: italic; }
  .acts { display: flex; gap: 0.4rem; flex-wrap: wrap; }
  .s-verified-edge { --fill-c: var(--verified-fill); }
  .s-needs_approval-edge { --fill-c: var(--attention-fill); }
  .s-ready-edge { --fill-c: var(--fill-2); }
  .none { padding: 2rem; }
  .center { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
