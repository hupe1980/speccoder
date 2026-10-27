<script lang="ts">
  // Where plan through stopped, and the one decision it waits for: answer
  // the questions, approve the spec's requirements (the review's own
  // approval, as the person, then the chain goes on), retry a failed step,
  // or commit what was planned. The stop is the host's; the page holds no
  // chain state but the last result, and the files say it again on reload.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { byKind, offer } from "../lib/actions";
  import { plural, sameWording, short, stopCard } from "../lib/start";
  import type { PlanningChainReport, Reply, Review } from "../lib/types";

  let { chain }: { chain: PlanningChainReport } = $props();
  const card = $derived(stopCard(chain.stop, chain.unit ?? null));
  const unit = $derived(chain.unit ?? app.feature ?? "");
  // The wording the person sees now: what an approval is for.
  let shown = $state<{ id: string; text: string }[]>([]);
  let changed = $state(false);
  let busy = $state(false);
  $effect(() => {
    if (chain.stop.kind !== "review" && chain.stop.kind !== "questions") return;
    void get<Reply<Review>>(`/api/review${q({ feature: unit })}`).then((r) => {
      shown = (r.data?.requirements ?? []).map((x) => ({ id: x.id, text: x.text }));
    });
  });
  const open = $derived(chain.stop.kind === "questions" ? chain.stop.questions.length : 0);

  function continueChain(command: string) {
    return offer({ kind: "plan_through", from: { kind: "continue" }, through: chain.through }, { group: "plan", label: "plan on", why: "the next skills of the chain", command });
  }

  /** Approve the requirements shown — bound to their wording now — then
   *  plan on. Changed meanwhile: nothing is approved, the card says so. */
  async function approveAndPlan() {
    if (busy || open) return;
    busy = true;
    try {
      const now = await get<Reply<Review>>(`/api/review${q({ feature: unit })}`);
      const current = (now.data?.requirements ?? []).map((x) => ({ id: x.id, text: x.text }));
      if (!sameWording(shown, current)) {
        changed = true;
        shown = current;
        return;
      }
      changed = false;
      const ids = chain.stop.kind === "review" ? chain.stop.requirements : [];
      const approved = await app.run(
        offer({ kind: "review", ids, verdict: "approve" }, { group: "review", label: "approve the requirements", why: "your review, recorded", command: `speccoder review approve --feature ${unit}` }),
      );
      if (approved?.status !== "ok") return;
      app.following = true;
      await app.perform(continueChain(`speccoder spec ${short(chain.next_skill)} --through ${chain.through} --feature ${unit}`));
    } finally {
      busy = false;
    }
  }

  function retry() {
    app.following = true;
    void app.perform(continueChain(`speccoder spec ${short(chain.next_skill)} --through ${chain.through} --feature ${unit}`));
  }

  function commit() {
    const a = byKind(app.actions, "commit")[0];
    if (a) void app.perform(a);
    else app.toast({ tone: "ok", text: "Nothing to commit: the planning files are committed" });
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && chain.stop.kind === "review" && !(e.target as HTMLElement)?.closest("textarea, input")) {
      e.preventDefault();
      void approveAndPlan();
    }
  }
</script>

<svelte:window onkeydown={key} />

<section class="stop" class:attention={card.tone === "attention"} class:danger={card.tone === "danger"} aria-labelledby="stop-title">
  <h3 id="stop-title"><span aria-hidden="true">{card.tone === "danger" ? "✗" : card.tone === "ok" ? "✓" : "△"}</span> Next</h3>
  <p class="title" role={card.tone === "danger" ? "alert" : undefined}>{changed ? "The spec changed — review again" : card.title}</p>
  {#if chain.stop.kind === "done" && chain.stop.plan_checks.length}
    <ul class="checks">
      {#each chain.stop.plan_checks as c (c.code + c.tasks.join())}
        <li><span class="mono">{c.code}</span> {c.detail}</li>
      {/each}
    </ul>
  {/if}
  <div class="row">
    {#if chain.stop.kind === "review" || chain.stop.kind === "questions"}
      <button class="btn" onclick={() => (app.doc = "review")}>Open the review</button>
      <button class="btn ghost" onclick={() => { app.chain = null; app.doc = "spec"; }}>Keep refining</button>
      <button class="btn primary" disabled={!!open || busy} title={open ? card.disabled ?? "" : "Approve the requirements shown, then plan (⌘⏎)"} onclick={approveAndPlan}>
        Approve and plan <kbd>⌘⏎</kbd>
      </button>
      {#if open}<span class="why">{card.disabled}</span>{/if}
    {:else if chain.stop.kind === "failed"}
      <button class="btn primary" onclick={retry}>Retry this step</button>
      <button class="btn" onclick={() => window.dispatchEvent(new CustomEvent("speccoder:menu", { detail: "view.report" }))}>Show the log</button>
    {:else}
      <button class="btn primary" onclick={commit}>Commit the planning files</button>
      <button class="btn" onclick={() => (app.doc = "review")}>Review the tests</button>
      <span class="why">{plural(chain.ran.length, "skill")} ran · nothing is committed until you commit</span>
    {/if}
  </div>
</section>

<style>
  .stop { border-radius: 12px; padding: 0.75rem 0.9rem; background: var(--surface); box-shadow: var(--shadow); margin-bottom: 0.9rem; display: grid; gap: 0.45rem; border-left: 3px solid var(--verified-fill); }
  .stop.attention { border-left-color: var(--attention-fill); }
  .stop.danger { border-left-color: var(--danger-fill); }
  h3 { margin: 0; font: 600 0.8rem/1.2 var(--font); color: var(--muted); }
  .title { margin: 0; font-weight: 600; }
  .row { display: flex; flex-wrap: wrap; gap: 0.45rem; align-items: center; }
  .row .btn.primary { margin-left: auto; }
  .why { font-size: 0.8rem; color: var(--muted); }
  .checks { margin: 0; padding-left: 1.1rem; font-size: 0.84rem; color: var(--text-2); }
</style>
