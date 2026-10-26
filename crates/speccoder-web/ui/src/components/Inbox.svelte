<script lang="ts">
  // Needs You: every decision waiting for the person, across the
  // project's features, the most urgent first — the person's work once the
  // agents write the code. Each row decides in place; Open shows the task.
  import { app } from "../lib/app.svelte";
  import type { Action, InboxItem } from "../lib/types";
  import Code from "./Code.svelte";
  import Icon from "./Icon.svelte";

  const sections: Record<string, { title: string; icon: string; tone: string; why: string }> = {
    approval: { title: "Waiting for Approval", icon: "hand", tone: "needs_approval", why: "A run is paused until you decide." },
    ruling: { title: "Rule on the Tests", icon: "shield", tone: "needs_approval", why: "The tests reject an alternative the spec may allow." },
    gaming: { title: "Suspected Shortcuts", icon: "exclaim", tone: "failed", why: "The monitor saw what looks like gaming the tests." },
    stale: { title: "The Spec Changed", icon: "exclaim", tone: "needs_approval", why: "Evidence proves an earlier wording." },
    question: { title: "Open Questions", icon: "exclaim", tone: "needs_approval", why: "The spec leaves these undecided — answer before it is built on." },
    review: { title: "Review the Spec", icon: "doc", tone: "ready", why: "The result will be judged against these words." },
    revise: { title: "Change Requests", icon: "sparkle", tone: "needs_approval", why: "Revise the spec as you asked, then review it again." },
    stuck: { title: "Did Not Pass", icon: "xoctagon", tone: "failed", why: "Decide: run again, leave a note, skip or accept." },
    aside: { title: "Changes Set Aside", icon: "tray", tone: "needs_approval", why: "Found in the worktree when a run started — keep them on the branch, or let them go." },
    commit: { title: "Plans to Commit", icon: "doc", tone: "ready", why: "A run reads committed files only." },
    handoff: { title: "Ready to Hand Off", icon: "arrowup", tone: "verified", why: "Every task is done — review the branch." },
  };
  const order = Object.keys(sections);
  const items = $derived(app.inbox?.data?.items ?? []);
  const groups = $derived(order.map((k) => ({ k, ...sections[k]!, items: items.filter((i) => i.kind === k) })).filter((g) => g.items.length));
  const features = $derived(new Set(items.map((i) => i.feature)).size);

  const ago = (s: number | null) => {
    if (!s) return "";
    const m = Math.round((Date.now() / 1000 - s) / 60);
    return m < 1 ? "just now" : m < 60 ? `${m} min` : m < 1440 ? `${Math.round(m / 60)} h` : `${Math.round(m / 1440)} d`;
  };
  const verb = (a: Action) => {
    const w = a.label.replace(/ T\d+.*$/, "").replace(/ again$/, " Again");
    return w.charAt(0).toUpperCase() + w.slice(1);
  };

  async function decide(i: InboxItem, a: Action) {
    if (app.feature !== i.feature) await app.selectFeature(i.feature);
    await app.perform(a);
  }
  async function open(i: InboxItem) {
    if (app.feature !== i.feature) await app.selectFeature(i.feature);
    if (i.kind === "review" || i.kind === "revise" || i.kind === "question") {
      app.doc = "review";
      app.view = "documents";
      return;
    }
    app.view = i.kind === "handoff" || i.kind === "stale" ? "report" : "board";
    if (i.task) void app.selectTask(i.task);
  }
</script>

<section class="inbox" aria-labelledby="inbox-title">
  <header>
    <div class="titlebar"><h1 id="inbox-title">Needs You</h1><button class="btn small tinted" onclick={() => (app.quick = true)} title="A small change, judged like any other (q)"><Icon name="sparkle" size={11} weight={2.2} /> Quick Change</button></div>
    <p class="sub">
      {#if items.length}{items.length} {items.length === 1 ? "decision" : "decisions"}{#if features > 1}{" "}across {features} features{/if} — the most urgent first.{:else}Every decision the agents wait on lands here.{/if}
    </p>
  </header>

  {#if !app.inbox}
    <div class="center"><span class="spinner"></span></div>
  {:else if !items.length}
    <div class="clear">
      <span class="ok" aria-hidden="true"><Icon name="check" size={30} weight={2.4} /></span>
      <strong>Nothing needs you</strong>
      <span class="muted">The agents can work. {#if app.primary}Next: {app.primary.label}.{/if}</span>
      {#if app.primary}<button class="btn primary" disabled={!!app.job?.running} onclick={() => app.primary && app.perform(app.primary)}>{verb(app.primary)}</button>{/if}
    </div>
  {:else}
    {#each groups as g (g.k)}
      <section class="group" aria-label={g.title}>
        <h2 class="s-{g.tone}"><span class="gi"><Icon name={g.icon} size={13} weight={2.2} /></span>{g.title}<span class="count">{g.items.length}</span><span class="why">{g.why}</span></h2>
        <ul class="grouped">
          {#each g.items as i (i.feature + (i.task ?? "") + i.kind)}
            <li>
              <div class="what">
                <span class="where">
                  {#if features > 1 || items.some((x) => x.feature !== i.feature)}<span class="feat">{i.feature.replace(/^\d+-/, "")}</span>{/if}
                  {#if i.task}<span class="mono task">{i.task}</span>{/if}
                  {#if i.since}<span class="age">{ago(i.since)}</span>{/if}
                </span>
                <strong>{i.title}</strong>
                {#if i.detail}<span class="detail"><Code text={i.detail} /></span>{/if}
              </div>
              <div class="acts">
                {#each i.actions as a, n (a.label)}
                  <button class="btn small" class:primary={n === 0} class:ghost={n > 1} disabled={!!a.blocked || !!app.job?.running} title={`${a.blocked ?? a.why}\n$ ${a.command}`} onclick={() => decide(i, a)}>{verb(a)}</button>
                {/each}
                <button class="btn small ghost" onclick={() => open(i)} title="Show it with its evidence"><Icon name="chevron" size={10} weight={2.6} /> Open</button>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</section>

<style>
  .inbox { max-width: 920px; margin: 0 auto; display: grid; gap: 1.4rem; }
  .titlebar { display: flex; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; }
  h1 { margin: 0; font: 700 1.9rem/1.15 var(--display); letter-spacing: -0.022em; }
  .sub { margin: 0.25rem 0 0; color: var(--text-2); }
  .group { display: grid; gap: 0.45rem; }
  h2 { display: flex; align-items: center; gap: 0.45rem; margin: 0 0.3rem; font-size: 0.86rem; font-weight: 650; background: none; flex-wrap: wrap; }
  .gi { width: 22px; height: 22px; border-radius: 7px; display: grid; place-items: center; color: #fff; background: var(--fill-c); }
  .count { min-width: 1.4rem; height: 1.2rem; padding: 0 0.4rem; border-radius: 999px; display: grid; place-items: center; background: var(--fill); color: var(--muted); font: 600 0.72rem/1 var(--rounded); }
  .why { color: var(--muted); font-weight: 500; font-size: 0.8rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 1rem; align-items: center; padding: 0.8rem 1rem; }
  .what { display: grid; gap: 0.18rem; min-width: 0; }
  .where { display: flex; gap: 0.45rem; align-items: center; font-size: 0.74rem; color: var(--muted); }
  .feat { padding: 0 0.45rem; border-radius: 999px; background: var(--fill); color: var(--text-2); font-weight: 600; }
  .task { color: var(--text-2); }
  .detail { color: var(--text-2); font-size: 0.86rem; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .acts { display: flex; gap: 0.35rem; flex-wrap: wrap; justify-content: flex-end; max-width: 22rem; }
  .clear { display: grid; justify-items: center; gap: 0.4rem; padding: 3.5rem 1rem; text-align: center; }
  .clear strong { font: 650 1.2rem/1.3 var(--display); }
  .ok { width: 60px; height: 60px; border-radius: 50%; display: grid; place-items: center; color: #fff; background: var(--verified-fill); margin-bottom: 0.4rem; }
  .center { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 640px) { li { grid-template-columns: 1fr; } .acts { justify-content: flex-start; max-width: none; } }
</style>
