<script lang="ts">
  // A task's inspector: its verdict, the evidence behind it as a grouped
  // list, the diff that ticked it, and what can be done with it.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  const evidenceOf = latest();
  const patchOf = latest();
  import { phaseLabel, symbol, word } from "../lib/marks";
  import { KINDS, key, offer, ofTask } from "../lib/actions";
  import type { AccountTurn, Row, Task } from "../lib/types";
  import Account from "./Account.svelte";
  import Diff from "./Diff.svelte";
  import Drift from "./Drift.svelte";
  import { implementedBy } from "../lib/evidence";
  import Code from "./Code.svelte";
  import Icon from "./Icon.svelte";
  import Tour from "./Tour.svelte";

  let tab = $state<"evidence" | "tour" | "diff">("evidence");
  let data = $state<{ task: Task; banner: { mark: string; words: string }; rows: Row[] } | null>(null);
  let patch = $state("");
  let loadingPatch = $state(false);
  // The change to show: the commit that ticked the task, else the last
  // attempt's code (kept under an attempt ref when it failed or was held).
  // Changes set aside before the task's run, when the person asks to see them.
  let showAside = $state(false);
  const aside = $derived(data?.task.set_aside ?? null);
  // The tick that made already-passing tests pass: one click away.
  const culprit = $derived(data ? implementedBy(data.task) : null);
  const source = $derived(
    showAside && aside
      ? { q: { ref: aside.reference }, label: "Set Aside" }
      : data?.task.commit
      ? { q: { commit: data.task.commit }, label: "Changes" }
      : data?.task.last_attempt?.reference?.startsWith("refs/")
        ? { q: { ref: data.task.last_attempt.reference }, label: "Last Attempt" }
        : null,
  );

  $effect(() => {
    const id = app.selected;
    const feature = app.feature ?? undefined;
    showAside = false;
    void app.report; // refetch when the report changes
    data = null;
    patch = "";
    if (!id) return;
    evidenceOf(get<typeof data>(`/api/evidence${q({ feature, task: id })}`), (d) => (data = d));
  });
  $effect(() => {
    const s = source;
    if (tab !== "diff" || !s || patch) return;
    loadingPatch = true;
    patchOf(
      get<{ patch: string }>(`/api/diff${q(s.q)}`),
      (d) => {
        patch = d.patch;
        loadingPatch = false;
      },
      () => {
        patch = "";
        loadingPatch = false;
      },
    );
  });

  // The selected task's own actions, by their subject: "Accept", "Skip"…
  const actions = $derived(app.selected ? ofTask(app.actions, app.selected) : []);
  const verdictTitle = $derived(data ? word(data.banner.mark).replace(/^./, (c) => c.toUpperCase()) : "");
  // The runtime's banner words, unless they only repeat the verdict.
  const verdictWords = $derived.by(() => {
    const w = data?.banner.words ?? "";
    return w.toLowerCase() === verdictTitle.toLowerCase() ? "" : w.charAt(0) + w.slice(1).toLowerCase();
  });
  // The report's, which compares the tick with the spec now.
  const drift = $derived(app.report?.data?.tasks.find((t) => t.id === app.selected)?.drift ?? null);
  // The attempt whose narration the journal holds: the tick's, else the last.
  const run = $derived(data?.task.run_id ?? data?.task.last_attempt?.run_id ?? null);
  let account = $state<AccountTurn[] | null>(null);
  let accountOf = $state<string | null>(null);
  let accountError = $state<string | null>(null);
  async function loadAccount() {
    if (!run || accountOf === run) return;
    accountOf = run;
    account = null;
    accountError = null;
    const r = await get<{ turns?: AccountTurn[]; error?: string }>(`/api/account${q({ run })}`).catch((e) => ({ turns: undefined, error: String(e) }));
    accountError = r.error ?? null;
    // An error is said as one, never shown as a model that said nothing;
    // closing and reopening the section asks again.
    if (accountError) accountOf = null;
    account = r.turns ?? [];
  }
  // Open unless the person closed it once (a per-viewer preference).
  const read = () => { try { return localStorage.getItem("speccoder-account") !== "closed"; } catch { return true; } };
  let accountOpen = $state(read());
  function toggled(e: Event) {
    accountOpen = (e.currentTarget as HTMLDetailsElement).open;
    try { localStorage.setItem("speccoder-account", accountOpen ? "open" : "closed"); } catch { /* storage off */ }
    if (accountOpen) void loadAccount();
  }
  $effect(() => {
    if (accountOpen && run) void loadAccount();
  });
  const icons: Record<string, string> = { run: "play", accept: "check", skip: "skip", nudge: "sparkle", approve: "check", reject: "xmark" };
</script>

<aside class="inspector glass" aria-label="Task {app.selected}">
  <header>
    <div class="title">
      <span class="id mono">{app.selected}{#if data?.task.phase}{" · "}{phaseLabel(data.task.phase)}{/if}</span>
      <h2><Code text={data?.task.title ?? ""} /></h2>
    </div>
    <button class="close" onclick={() => app.selectTask(null)} aria-label="Close" title="Close (Esc)"><Icon name="xmark" size={12} weight={2.6} /></button>
  </header>
  {#if data}
    <div class="verdict s-{data.banner.mark}">
      <span class="sym"><Icon name={symbol(data.banner.mark)} size={20} weight={2} /></span>
      <span><strong>{verdictTitle}</strong>{#if verdictWords}<span class="words">{verdictWords}</span>{/if}</span>
    </div>
    {#if drift}<Drift {drift} />{/if}
    {#if aside && app.selected}
      {@const id = app.selected}
      <section class="aside-card" aria-label="Changes set aside">
        <span class="glyph"><Icon name="tray" size={16} weight={2} /></span>
        <div class="what">
          <strong>Changes set aside</strong>
          <p>{aside.files.length} {aside.files.length === 1 ? "file was" : "files were"} changed in the worktree when this task's run started — your edits, or an interrupted run's. They are not on the branch yet.</p>
          <ul class="files mono">{#each aside.files.slice(0, 5) as f (f)}<li>{f}</li>{/each}{#if aside.files.length > 5}<li class="more">+{aside.files.length - 5} more</li>{/if}</ul>
          <div class="row">
            <button class="btn small" onclick={() => { showAside = true; tab = "diff"; }}><Icon name="doc" size={12} /> Show Changes</button>
            <button class="btn small tinted" disabled={!!app.job?.running} onclick={() => app.perform(offer({ kind: "keep_set_aside", task: id }, { group: "task", label: `keep the changes set aside before ${id}`, why: "committed onto the feature's branch as yours", command: `speccoder task restore ${id}` }))}><Icon name="check" size={12} weight={2.4} /> Keep on the Branch</button>
            <button class="btn small ghost danger" disabled={!!app.job?.running} onclick={() => app.perform(offer({ kind: "drop_set_aside", task: id }, { group: "task", label: `discard the changes set aside before ${id}`, why: "deleted; the branch stays as it is", command: `speccoder task drop ${id}`, confirm: `Discard the ${aside.files.length} changed file(s) set aside before ${id}? They are deleted, not kept anywhere.` }))}>Discard…</button>
          </div>
        </div>
      </section>
    {/if}
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "evidence"} class:on={tab === "evidence"} onclick={() => (tab = "evidence")}>Evidence</button>
      <button role="tab" aria-selected={tab === "tour"} class:on={tab === "tour"} disabled={!data.task.commit} title={data.task.commit ? "The evidence, step by step" : "No commit carries this task's evidence"} onclick={() => (tab = "tour")}>Tour</button>
      <button role="tab" aria-selected={tab === "diff"} class:on={tab === "diff"} disabled={!source} title={source ? "" : "No code yet: the task has no commit and no attempt"} onclick={() => (tab = "diff")}>{source?.label ?? "Changes"}</button>
    </div>
    {#if tab === "evidence"}
      <dl class="grouped">
        {#each data.rows as r, n (r.label + r.text)}
          <div class="row" class:claim={r.label === "Model says"}>
            <dt><span class="dot s-{r.mark}"><Icon name={symbol(r.mark)} size={11} weight={2.4} /></span>{r.label}</dt>
            <dd><Code text={r.text} />{#if n === 0 && culprit}{" "}<button class="link" aria-label="Open task {culprit}" onclick={() => app.selectTask(culprit)}>Open {culprit}</button>{/if}</dd>
          </div>
        {/each}
      </dl>
      {#if data.task.commit}
        <p class="note"><Icon name="shield" size={13} /><span>Every row comes from the commit's attestation — <code>speccoder attest verify</code> checks it.</span></p>
      {/if}
    {:else if tab === "tour" && app.selected}
      <Tour feature={app.feature ?? undefined} task={app.selected} />
    {:else}
      {#if source?.label === "Set Aside"}<p class="note"><Icon name="info" size={13} /> What the worktree held when the run started, set aside: not on the branch until you keep it.</p>{/if}
      {#if source?.label === "Last Attempt"}<p class="note"><Icon name="info" size={13} /> The attempt did not pass; its code is kept for you to read, never merged.</p>{/if}
      <Diff {patch} loading={loadingPatch} />
    {/if}
    {#if run}
      <details class="account" open={accountOpen} ontoggle={toggled}>
        <summary><Icon name="sparkle" size={12} weight={2.2} /> The Model's Account <span class="muted">— its reasoning and its words; a claim, not evidence</span></summary>
        {#if accountError}<p class="muted small">The journal could not be read: {accountError}</p>
        {:else if account === null}<p class="muted small">Reading the journal…</p>
        {:else if !account.length}<p class="muted small">The model neither reasoned aloud nor said anything beside its calls in this attempt.</p>
        {:else}
          <Account turns={account} />
        {/if}
      </details>
    {/if}
    {#if actions.length}
      <div class="acts">
        {#each actions as a (key(a))}
          {@const kind = KINDS[a.id.kind]}
          <button class="btn small" class:primary={a.group === "work"} disabled={!!a.blocked || !!app.job?.running} title={`${a.blocked ?? a.why}\n$ ${a.command}`} onclick={() => app.perform(a)}>
            {#if kind.icon}<Icon name={kind.icon} size={11} weight={2.2} />{/if}{kind.verb}{a.input ? "…" : ""}
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="loading"><span class="spinner"></span></div>
  {/if}
</aside>

<style>
  .link { border: 0; background: none; padding: 0; color: var(--accent-ink); font-weight: 600; cursor: pointer; }
  .aside-card {
    display: grid; grid-template-columns: auto 1fr; gap: 0.7rem; padding: 0.8rem 0.9rem; border-radius: var(--radius);
    background: color-mix(in srgb, var(--attention, #ff9f0a) 9%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--attention, #ff9f0a) 28%, transparent);
  }
  .aside-card .glyph { width: 2rem; height: 2rem; border-radius: 9px; display: grid; place-items: center; color: #fff; background: var(--attention, #ff9f0a); }
  .aside-card .what { display: grid; gap: 0.35rem; min-width: 0; }
  .aside-card p { margin: 0; font-size: 0.84rem; color: var(--text-2); line-height: 1.4; }
  .aside-card .files { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 0.3rem; font-size: 0.74rem; }
  .aside-card .files li { padding: 0.1rem 0.45rem; border-radius: 6px; background: var(--fill); color: var(--text-2); }
  .aside-card .files .more { color: var(--muted); }
  .aside-card .row { display: flex; flex-wrap: wrap; gap: 0.4rem; margin-top: 0.15rem; }
  .danger { color: var(--danger); }
  .inspector {
    position: fixed; top: 64px; right: 10px; bottom: 10px; width: min(500px, calc(100vw - 20px)); z-index: 12;
    border-radius: var(--radius-lg); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge);
    overflow: auto; padding: 1.1rem 1.15rem 1.25rem; display: grid; align-content: start; gap: 1rem;
    animation: slide 380ms var(--spring);
  }
  @keyframes slide { from { transform: translateX(28px); opacity: 0; } }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; }
  .id { font-size: 0.76rem; color: var(--muted); }
  h2 { margin: 0.15rem 0 0; font: 650 1.12rem/1.32 var(--display); letter-spacing: -0.012em; }
  .close { flex: none; width: 24px; height: 24px; border-radius: 50%; border: 0; background: var(--fill); color: var(--muted); display: grid; place-items: center; cursor: pointer; }
  .close:hover { background: var(--fill-2); color: var(--text); }
  .verdict { display: flex; align-items: center; gap: 0.75rem; padding: 0.75rem 0.9rem; border-radius: var(--radius); }
  .verdict .sym { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; background: var(--fill-c); color: #fff; flex: none; }
  .verdict strong { display: block; font-weight: 650; }
  .words { display: block; font-size: 0.84rem; color: var(--text-2); }
  .tabs { display: grid; grid-template-columns: 1fr 1fr 1fr; padding: 2px; border-radius: 9px; background: var(--fill); }
  .tabs button { height: 26px; border: 0; background: none; border-radius: 7px; cursor: pointer; font-weight: 500; font-size: 0.86rem; color: var(--text-2); }
  .tabs button.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12); }
  .tabs button[disabled] { opacity: 0.4; cursor: default; }
  dl { margin: 0; }
  .row { display: grid; grid-template-columns: 8.5rem 1fr; gap: 0.75rem; padding: 0.6rem 0.85rem; }
  dt { font-weight: 550; display: flex; gap: 0.5rem; align-items: center; font-size: 0.88rem; }
  .dot { width: 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; background: var(--fill-c); color: #fff; flex: none; }
  dd { margin: 0; color: var(--text-2); font-size: 0.88rem; overflow-wrap: anywhere; }
  .claim dd { font-style: italic; color: var(--muted); }
  .note { display: flex; align-items: center; gap: 0.35rem; margin: -0.25rem 0.25rem 0; font-size: 0.78rem; color: var(--muted); }
  .note code { font-size: 0.95em; }
  .account { border-radius: var(--radius); background: var(--surface); padding: 0.6rem 0.85rem; }
  .account summary { cursor: pointer; font-weight: 600; font-size: 0.88rem; display: flex; gap: 0.35rem; align-items: center; flex-wrap: wrap; }
  .account summary .muted { font-weight: 500; font-size: 0.78rem; }
  .small { font-size: 0.82rem; margin: 0.4rem 0 0; }
  .acts { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .loading { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
