<script lang="ts">
  // A task's inspector: its verdict, the evidence behind it as a grouped
  // list, the diff that ticked it, and what can be done with it.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  const evidenceOf = latest();
  const patchOf = latest();
  import { symbol, word } from "../lib/marks";
  import type { Row, Task } from "../lib/types";
  import Diff from "./Diff.svelte";
  import Drift from "./Drift.svelte";
  import Code from "./Code.svelte";
  import Icon from "./Icon.svelte";

  let tab = $state<"evidence" | "diff">("evidence");
  let data = $state<{ task: Task; banner: { mark: string; words: string }; rows: Row[] } | null>(null);
  let patch = $state("");
  let loadingPatch = $state(false);
  // The change to show: the commit that ticked the task, else the last
  // attempt's code (kept under an attempt ref when it failed or was held).
  const source = $derived(
    data?.task.commit
      ? { q: { commit: data.task.commit }, label: "Changes" }
      : data?.task.last_attempt?.reference?.startsWith("refs/")
        ? { q: { ref: data.task.last_attempt.reference }, label: "Last Attempt" }
        : null,
  );

  $effect(() => {
    const id = app.selected;
    const feature = app.feature ?? undefined;
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

  // "accept T001" in the inspector of T001 is "Accept".
  const short = (label: string) => {
    const s = label.replace(` ${app.selected}`, "").trim();
    return s.charAt(0).toUpperCase() + s.slice(1);
  };
  const actions = $derived(app.actions.filter((a) => (a.group === "task" || a.group === "work") && a.label.includes(app.selected ?? "\0")));
  const verdictTitle = $derived(data ? word(data.banner.mark).replace(/^./, (c) => c.toUpperCase()) : "");
  // The runtime's banner words, unless they only repeat the verdict.
  const verdictWords = $derived.by(() => {
    const w = data?.banner.words ?? "";
    return w.toLowerCase() === verdictTitle.toLowerCase() ? "" : w.charAt(0) + w.slice(1).toLowerCase();
  });
  // The report's, which compares the tick with the spec now.
  const drift = $derived(app.report?.data?.tasks.find((t) => t.id === app.selected)?.drift ?? null);
  const icons: Record<string, string> = { run: "play", accept: "check", skip: "skip", nudge: "sparkle", approve: "check", reject: "xmark" };
</script>

<aside class="inspector glass" aria-label="Task {app.selected}">
  <header>
    <div class="title">
      <span class="id mono">{app.selected}{#if data?.task.phase}{" · "}{data.task.phase}{/if}</span>
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
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "evidence"} class:on={tab === "evidence"} onclick={() => (tab = "evidence")}>Evidence</button>
      <button role="tab" aria-selected={tab === "diff"} class:on={tab === "diff"} disabled={!source} title={source ? "" : "No code yet: the task has no commit and no attempt"} onclick={() => (tab = "diff")}>{source?.label ?? "Changes"}</button>
    </div>
    {#if tab === "evidence"}
      <dl class="grouped">
        {#each data.rows as r (r.label + r.text)}
          <div class="row" class:claim={r.label === "Model says"}>
            <dt><span class="dot s-{r.mark}"><Icon name={symbol(r.mark)} size={11} weight={2.4} /></span>{r.label}</dt>
            <dd><Code text={r.text} /></dd>
          </div>
        {/each}
      </dl>
      {#if data.task.commit}
        <p class="note"><Icon name="shield" size={13} /> Every row comes from the commit's attestation — <code>speccoder attest verify</code> checks it.</p>
      {/if}
    {:else}
      {#if source?.label === "Last Attempt"}<p class="note"><Icon name="info" size={13} /> The attempt did not pass; its code is kept for you to read, never merged.</p>{/if}
      <Diff {patch} loading={loadingPatch} />
    {/if}
    {#if actions.length}
      <div class="acts">
        {#each actions as a (a.label)}
          {@const verb = a.label.split(" ")[0] ?? ""}
          <button class="btn small" class:primary={a.group === "work"} disabled={!!a.blocked || !!app.job?.running} title={`${a.blocked ?? a.why}\n$ ${a.command}`} onclick={() => app.perform(a)}>
            {#if icons[verb]}<Icon name={icons[verb]} size={11} weight={2.2} />{/if}{short(a.label)}
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="loading"><span class="spinner"></span></div>
  {/if}
</aside>

<style>
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
  .tabs { display: grid; grid-template-columns: 1fr 1fr; padding: 2px; border-radius: 9px; background: var(--fill); }
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
  .acts { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .loading { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
