<script lang="ts">
  // The feature's pull request, as GitHub has it: number and title, draft
  // or open or merged, the review, the checks' rollup — beside speccoder's
  // own evidence, never instead of it. Open Draft PR / Update confirm first.
  // Above them, the definition of done on the branch: what is
  // still open and how to close it; a stop at it offers Open Anyway. Then
  // the merge forecast: how the branch fares against the others.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  import type { Action, DoneReport, GitHubState, Reply } from "../lib/types";
  import { load } from "./GitHub.svelte";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";
  import OpenIn from "./OpenIn.svelte";
  import Forecast from "./Forecast.svelte";

  const action = $derived(app.actions.find((a) => a.run?.kind === "pull_request") ?? null);
  let gh = $state<GitHubState | null>(null);
  let done = $state<DoneReport | null>(null);
  const fetchOf = latest();
  const fetchDone = latest();
  $effect(() => {
    void app.report;
    void app.job?.running;
    fetchOf(load(app.feature), (s) => (gh = s), () => (gh = null));
    fetchDone(get<Reply<DoneReport>>(`/api/done${q({ feature: app.feature ?? undefined })}`), (r) => (done = r.data ?? null), () => (done = null));
  });
  const open = $derived(done?.lines.filter((l) => l.state === "not_done") ?? []);
  const held = $derived(done ? done.lines.filter((l) => l.state === "done").length : 0);
  // The last push stopped at the definition of done: hand off anyway, with
  // the open lines in the body, after the same confirm.
  const stopped = $derived(!app.job?.running && app.job?.result?.code === "DONE-NOT-DONE");
  const anyway = $derived<Action | null>(action ? { ...action, run: { kind: "pull_request", force: true }, command: `${action.command} --force`, label: "Open Anyway" } : null);
  const pull = $derived(gh?.pull ?? null);
  const issues = $derived(app.report?.data?.issues ?? []);
  const stateWord = $derived(!pull ? "" : pull.state === "MERGED" ? "Merged" : pull.state === "CLOSED" ? "Closed" : pull.draft ? "Draft" : "Open");
  const TITLE: Record<string, string> = { changelog: "Changelog", links: "Links", screenshots: "Screenshots", tone: "Tone", versions: "Versions", ci: "CI" };
  const review: Record<string, string> = { APPROVED: "Approved", CHANGES_REQUESTED: "Changes requested", REVIEW_REQUIRED: "Review required" };
</script>

<section class="card wide pull">
  <h3><Icon name="branch" size={15} weight={2} /> Pull Request</h3>
  {#if done && done.lines.length}
    <div class="done" class:open={open.length > 0}>
      <p class="done-head">
        <Icon name={open.length ? "xmark" : "check"} size={12} weight={2.6} />
        <span>Definition of done</span>
        <span class="count">{open.length ? `${held} of ${done.lines.length}` : "Done"}</span>
      </p>
      {#if open.length}
        <ul>
          {#each open as l (l.part)}
            <li>
              <p><strong>{TITLE[l.part]}</strong> {l.summary}</p>
              {#if l.findings?.length}
                <ul class="findings">
                  {#each l.findings.slice(0, 3) as f}
                    <li><code>{f.path}{f.line ? `:${f.line}` : ""}</code>{#if l.part !== "ci" && !f.path.includes("*")}<OpenIn path={f.path} line={f.line ?? null} />{/if} {f.what}</li>
                  {/each}
                  {#if l.findings.length + (l.more ?? 0) > 3}<li class="muted">{l.findings.length + (l.more ?? 0) - 3} more — <code>speccoder done</code></li>{/if}
                </ul>
              {/if}
              {#each l.fixes ?? [] as fix}<p class="fix">→ {#each fix.split("`") as part, i}{#if i % 2}<code>{part}</code>{:else}{part}{/if}{/each}</p>{/each}
            </li>
          {/each}
        </ul>
        {#if stopped && anyway}
          <div class="anyway">
            <p class="muted">The last push stopped here. Open it anyway: the pull request lists what is open.</p>
            <button class="btn small" onclick={() => app.perform(anyway)}><Icon name="arrowup" size={11} weight={2.2} /> Open Anyway</button>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
  {#if app.report?.data?.branch}<Forecast />{/if}
  {#if !gh}
    <p class="muted">Asking GitHub…</p>
  {:else if !gh.status.ready}
    <p class="muted"><Code text={gh.status.reason ?? "GitHub is not reachable"} /> — <code>speccoder pr --dry-run</code> still shows what would be opened.</p>
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
      {#if action && gh.branch}
        <div class="go">
          <button class="btn small primary" disabled={!!app.job?.running} onclick={() => app.perform(action)}><Icon name="arrowup" size={11} weight={2.2} /> Open Draft PR</button>
          {#if open[0]}<span class="hint">Opening stops at {TITLE[open[0].part]}</span>{/if}
        </div>
      {/if}
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
  .done { border-radius: var(--radius); padding: 0.55rem 0.75rem; background: var(--surface-2); display: grid; gap: 0.45rem; }
  .done.open { box-shadow: inset 3px 0 0 var(--danger); }
  .done-head { margin: 0; display: flex; align-items: center; gap: 0.4rem; font-size: 0.85rem; font-weight: 600; color: var(--verified); }
  .done.open .done-head { color: var(--danger); }
  .done-head span:first-of-type { color: var(--text); }
  .count { margin-left: auto; font-weight: 600; font-variant-numeric: tabular-nums; }
  .done ul { margin: 0; padding: 0; list-style: none; display: grid; gap: 0.55rem; }
  .done li p { margin: 0; font-size: 0.84rem; }
  .done li strong { font-weight: 650; margin-right: 0.25rem; }
  .findings { gap: 0.15rem !important; margin-top: 0.2rem !important; font-size: 0.78rem; color: var(--text-2); }
  .findings code { font-size: 0.76rem; overflow-wrap: anywhere; }
  .fix { color: var(--accent-ink); }
  .anyway { display: flex; align-items: center; justify-content: space-between; gap: 0.75rem; flex-wrap: wrap; }
  .anyway p { margin: 0; font-size: 0.8rem; }
  .go { display: flex; flex-direction: column; align-items: flex-end; gap: 0.2rem; }
  .hint { font-size: 0.74rem; color: var(--muted); }
</style>
