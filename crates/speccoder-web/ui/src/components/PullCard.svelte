<script lang="ts">
  // The feature's pull request, as GitHub has it: number and title, draft
  // or open or merged, the review, the checks' rollup — beside speccoder's
  // own evidence, never instead of it. Open Draft PR / Update confirm first.
  import { app } from "../lib/app.svelte";
  import { latest } from "../lib/api";
  import type { GitHubState } from "../lib/types";
  import { load } from "./GitHub.svelte";
  import Icon from "./Icon.svelte";

  let gh = $state<GitHubState | null>(null);
  const fetchOf = latest();
  $effect(() => {
    void app.report;
    void app.job?.running;
    fetchOf(load(app.feature), (s) => (gh = s), () => (gh = null));
  });
  const action = $derived(app.actions.find((a) => a.run?.kind === "pull_request"));
  const pull = $derived(gh?.pull ?? null);
  const issues = $derived(app.report?.data?.issues ?? []);
  const stateWord = $derived(!pull ? "" : pull.state === "MERGED" ? "Merged" : pull.state === "CLOSED" ? "Closed" : pull.draft ? "Draft" : "Open");
  const review: Record<string, string> = { APPROVED: "Approved", CHANGES_REQUESTED: "Changes requested", REVIEW_REQUIRED: "Review required" };
</script>

<section class="card wide pull">
  <h3><Icon name="branch" size={15} weight={2} /> Pull Request</h3>
  {#if !gh}
    <p class="muted">Asking GitHub…</p>
  {:else if !gh.status.ready}
    <p class="muted">{gh.status.reason ?? "GitHub is not reachable"} — <code>speccoder pr --dry-run</code> still shows what would be opened.</p>
  {:else if pull}
    <div class="pr">
      <span class="st st-{stateWord.toLowerCase()}">{stateWord}</span>
      <a class="title" href={pull.url} target="_blank" rel="noopener noreferrer">#{pull.number} {pull.title}</a>
      <span class="facts">
        {#if pull.review}<span class="rv rv-{pull.review.toLowerCase()}">{review[pull.review] ?? pull.review}</span>{/if}
        {#if pull.checks[0] + pull.checks[1] + pull.checks[2]}
          <span class="checks" title="GitHub's checks: passed, failed, running">
            <span class="ok"><Icon name="check" size={11} weight={2.6} /> {pull.checks[0]}</span>
            {#if pull.checks[1]}<span class="no"><Icon name="xmark" size={10} weight={2.6} /> {pull.checks[1]}</span>{/if}
            {#if pull.checks[2]}<span class="run">● {pull.checks[2]}</span>{/if}
          </span>
        {/if}
      </span>
      {#if action && pull.state === "OPEN"}
        <button class="btn small" disabled={!!app.job?.running} onclick={() => app.perform(action)} title="Push new ticks to the pull request"><Icon name="arrowup" size={11} weight={2.2} /> Update</button>
      {/if}
    </div>
  {:else}
    <div class="none">
      <p class="muted">No pull request for <code>{gh.branch ?? "this feature"}</code> yet.{#if issues.length} It will close {issues.map((u) => "#" + u.split("/").pop()).join(", ")}.{/if}</p>
      {#if action && gh.branch}<button class="btn small primary" disabled={!!app.job?.running} onclick={() => app.perform(action)}><Icon name="arrowup" size={11} weight={2.2} /> Open Draft PR</button>{/if}
    </div>
  {/if}
  {#if issues.length && pull}
    <p class="muted small">Closes {#each issues as u, i}{#if i}, {/if}<a href={u} target="_blank" rel="noopener noreferrer">#{u.split("/").pop()}</a>{/each}</p>
  {/if}
</section>

<style>
  .card { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 1.15rem 1.35rem 1.25rem; grid-column: 1 / -1; display: grid; gap: 0.6rem; }
  h3 { margin: 0; font: 650 1.02rem/1.3 var(--display); display: flex; align-items: center; gap: 0.4rem; }
  .pr { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; }
  .st { font-size: 0.74rem; font-weight: 700; padding: 0.1rem 0.6rem; border-radius: 999px; color: #fff; background: var(--skipped-fill); }
  .st-open { background: var(--verified-fill); }
  .st-merged { background: var(--attested-fill); }
  .st-closed { background: var(--danger-fill); }
  .title { color: var(--text); text-decoration: none; font-weight: 600; flex: 1; min-width: 12rem; }
  .title:hover { color: var(--accent-ink); }
  .facts { display: flex; gap: 0.6rem; align-items: center; font-size: 0.8rem; }
  .rv { font-weight: 600; }
  .rv-approved { color: var(--verified); }
  .rv-changes_requested { color: var(--danger); }
  .rv-review_required { color: var(--attention); }
  .checks { display: inline-flex; gap: 0.5rem; }
  .ok { color: var(--verified); }
  .no { color: var(--danger); }
  .run { color: var(--running); }
  .none { display: flex; justify-content: space-between; align-items: center; gap: 1rem; flex-wrap: wrap; }
  .none p { margin: 0; }
  .small { margin: 0; font-size: 0.8rem; }
  .small a { color: var(--accent-ink); }
</style>
