<script lang="ts" module>
  import type { GitHubState } from "../lib/types";
  import { get, q } from "../lib/api";
  /** GitHub as the person's token sees it, for the current feature. */
  export const load = (feature: string | null) => get<GitHubState>(`/api/github${q({ feature: feature ?? undefined })}`);
</script>

<script lang="ts">
  // GitHub, through its API as the person: the open issues (each starts a
  // feature, or shows the one it started) — or why GitHub is not reachable,
  // and Sign In when that is the reason.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import { latest } from "../lib/api";
  import type { Action } from "../lib/types";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";
  import SignIn from "./SignIn.svelte";

  let signing = $state(false);

  let gh = $state<GitHubState | null>(null);
  let starting = $state<number | null>(null);
  const fetchOf = latest();
  $effect(() => {
    void app.report;
    fetchOf(load(app.feature), (s) => (gh = s), () => (gh = null));
  });

  async function start(n: number) {
    starting = n;
    try {
      const r = await get<{ description?: string; skill?: string | null; error?: string }>(`/api/github/issue${q({ number: String(n) })}`);
      if (r.error || !r.description || !r.skill) {
        app.toast({ tone: "error", text: `Issue #${n} could not be read`, detail: r.error ?? "no skill creates a feature here" });
        return;
      }
      const action = offer(
        { kind: "skill", name: r.skill, arguments: r.description },
        { group: "plan", label: `specify a feature from issue #${n}`, why: "the format's skill that creates a feature, with the issue as the description", command: `speccoder issue start ${n}` },
      );
      await app.perform(action);
    } finally {
      starting = null;
    }
  }
  // Tell the issue, in the feature's own selection, after a confirmation:
  // it is written on GitHub, as the person.
  async function tell(n: number, feature: string) {
    if (app.feature !== feature) await app.selectFeature(feature);
    const action = offer(
      { kind: "issue_comment", issue: n, assign: false },
      {
        group: "plan",
        label: `comment on #${n}`,
        why: "the issue shows it is being specified; once per feature",
        command: `speccoder issue comment ${n} --feature ${feature}`,
        confirm: `Comment on #${n}, as you, that it is being specified as ${feature} — with the spec's path and its branch. Said once: a second time changes nothing.`,
      },
    );
    await app.perform(action);
  }
  const copy = (t: string) => {
    navigator.clipboard?.writeText(t);
    app.toast({ tone: "ok", text: "Copied", detail: t });
  };
  const ago = (d: string | null) => {
    if (!d) return "";
    const days = Math.round((Date.now() - new Date(d).getTime()) / 86_400_000);
    return days <= 0 ? "today" : days === 1 ? "yesterday" : `${days} days ago`;
  };
</script>

<section class="gh" id="github" tabindex="-1" aria-label="GitHub issues">
  <h2 class="section-title"><svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 0a8 8 0 0 0-2.5 15.6c.4 0 .5-.2.5-.4v-1.5c-2.2.5-2.7-1-2.7-1-.4-.9-.9-1.2-.9-1.2-.7-.5.1-.5.1-.5.8.1 1.2.8 1.2.8.7 1.3 1.9.9 2.4.7 0-.5.3-.9.5-1.1-1.8-.2-3.6-.9-3.6-4 0-.9.3-1.6.8-2.1-.1-.2-.4-1 .1-2.1 0 0 .7-.2 2.2.8a7.5 7.5 0 0 1 4 0c1.5-1 2.2-.8 2.2-.8.4 1.1.2 1.9.1 2.1.5.6.8 1.3.8 2.1 0 3.1-1.9 3.7-3.6 3.9.3.3.6.8.6 1.5v2.2c0 .2.1.5.6.4A8 8 0 0 0 8 0z" /></svg> GitHub {#if gh?.status.repo}<a class="repo" href={gh.status.url} target="_blank" rel="noopener noreferrer">{gh.status.repo}</a>{/if}{#if gh?.status.login}<span class="as">as {gh.status.login}</span>{/if}</h2>
  {#if !gh}
    <div class="grouped empty"><span class="spinner"></span></div>
  {:else if !gh.status.ready}
    <div class="grouped off">
      <span class="w"><strong>{gh.status.host ? (gh.status.fix?.startsWith("speccoder auth") ? `Not signed in to ${gh.status.host}` : `${gh.status.host} is not reachable`) : "No GitHub remote"}</strong><span class="d"><Code text={gh.status.reason ?? ""} /></span></span>
      {#if gh.status.fix?.startsWith("speccoder auth") && gh.status.host}
        <button class="btn small primary" onclick={() => (signing = true)}><Icon name="key" size={11} weight={2.2} /> Sign In</button>
      {:else if gh.status.fix}
        <button class="fix mono" onclick={() => copy(gh?.status.fix ?? "")}>{gh.status.fix}</button>
      {/if}
    </div>
    {#if signing && gh.status.host}<SignIn host={gh.status.host} onclose={() => (signing = false)} />{/if}
  {:else if gh.issues_error}
    <div class="grouped off"><span class="w"><strong>Issues could not be read</strong><span class="d">{gh.issues_error}</span></span></div>
  {:else if !gh.issues?.issues.length}
    <div class="grouped off"><span class="w"><strong>No open issues</strong><span class="d">Start a feature from an idea instead — the next step writes its spec.</span></span></div>
  {:else}
    <ul class="grouped">
      {#each gh.issues.issues.slice(0, 12) as i (i.number)}
        <li>
          <span class="num mono">#{i.number}</span>
          <span class="w">
            <a class="t" href={i.url} target="_blank" rel="noopener noreferrer">{i.title}</a>
            <span class="meta">
              {#if i.mine}<span class="mine">assigned to you</span>{/if}
              {#each i.labels.slice(0, 3) as l}<span class="label">{l}</span>{/each}
              <span class="d">{ago(i.updated)}</span>
            </span>
          </span>
          {#if i.feature}
            <span class="acts">
              <button class="btn small ghost" disabled={!!app.job?.running} title="Tell #{i.number} it is being specified as {i.feature} (asked first; once per feature)" onclick={() => tell(i.number, i.feature!)}><Icon name="sparkle" size={10} weight={2.2} /> Comment</button>
              <button class="btn small" onclick={() => { void app.selectFeature(i.feature!); app.view = "board"; }}><Icon name="chevron" size={10} weight={2.6} /> {i.feature.replace(/^\d+-/, "")}</button>
            </span>
          {:else}
            {#if i.labels.includes("bug")}
              <button class="btn small" disabled={!!app.job?.running} title="Reproduce and fix it as a bug ($ speccoder bug … --issue {i.number}); the issue is context, your words are the triad" onclick={() => (app.bugSheet = { symptom: i.title, issue: i.number })}><Icon name="exclaim" size={10} weight={2.2} /> Fix as Bug</button>
            {/if}
            <button class="btn small tinted" disabled={!!app.job?.running || starting !== null} title="Spec Kit's specify, with the issue as the description ($ speccoder issue start {i.number})" onclick={() => start(i.number)}>
              {#if starting === i.number}<span class="spin"></span>{:else}<Icon name="sparkle" size={11} weight={2.2} />{/if} Start Feature
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .gh { display: grid; gap: 0.4rem; }
  .section-title { display: flex; align-items: center; gap: 0.4rem; }
  .repo { color: var(--accent-ink); text-decoration: none; font-weight: 500; }
  .as { color: var(--muted); font-weight: 500; font-size: 0.85em; }
  ul { list-style: none; margin: 0; padding: 0; }
  li, .off { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 0.8rem; align-items: center; padding: 0.65rem 0.95rem; }
  .off { grid-template-columns: minmax(0, 1fr) auto; }
  .num { color: var(--muted); font-size: 0.8rem; }
  .w { display: grid; gap: 0.15rem; min-width: 0; }
  .t { color: var(--text); text-decoration: none; font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t:hover { color: var(--accent-ink); }
  .meta { display: flex; flex-wrap: wrap; gap: 0.35rem; align-items: center; font-size: 0.74rem; }
  .mine { color: var(--accent-ink); font-weight: 600; }
  .label { padding: 0 0.45rem; border-radius: 999px; background: var(--fill); color: var(--text-2); }
  .d { color: var(--muted); font-size: 0.82rem; }
  .meta .d { font-size: 0.74rem; }
  .acts { display: inline-flex; gap: 0.35rem; }
  .fix { border: 0; border-radius: 7px; background: var(--fill); padding: 0.25rem 0.6rem; font-size: 0.78rem; cursor: copy; }
  .empty { display: grid; place-items: center; padding: 1.25rem; }
  .spinner { width: 18px; height: 18px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  .spin { width: 11px; height: 11px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 700ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
