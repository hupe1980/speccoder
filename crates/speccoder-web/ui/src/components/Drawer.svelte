<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { mark } from "../lib/marks";
  import type { Row, Task } from "../lib/types";
  import Diff from "./Diff.svelte";
  import Code from "./Code.svelte";

  let tab = $state<"evidence" | "diff">("evidence");
  let data = $state<{ task: Task; banner: { mark: string; words: string }; rows: Row[] } | null>(null);
  let patch = $state("");

  $effect(() => {
    const id = app.selected;
    const feature = app.feature ?? undefined;
    void app.report; // refetch when the report changes
    data = null;
    patch = "";
    if (!id) return;
    void get<typeof data>(`/api/evidence${q({ feature, task: id })}`).then((d) => (data = d));
  });
  $effect(() => {
    const c = data?.task.commit;
    if (tab === "diff" && c && !patch) void get<{ patch: string }>(`/api/diff${q({ commit: c })}`).then((d) => (patch = d.patch));
  });

  // "accept T001" in the drawer of T001 is "Accept".
  const short = (label: string) => {
    const s = label.replace(` ${app.selected}`, "").trim();
    return s.charAt(0).toUpperCase() + s.slice(1);
  };
  const actions = $derived(app.actions.filter((a) => (a.group === "task" || a.group === "work") && a.label.includes(app.selected ?? "\0")));
</script>

<aside class="drawer" aria-label="Task {app.selected}">
  <header>
    <div>
      <span class="mono muted">{app.selected}</span>
      <h2><Code text={data?.task.title ?? ""} /></h2>
    </div>
    <button class="btn ghost" onclick={() => app.selectTask(null)} aria-label="Close">✕</button>
  </header>
  {#if data}
    <div class="banner s-{data.banner.mark}">
      <span class="m">{mark(data.banner.mark)}</span>{data.banner.words}
    </div>
    <div class="tabs">
      <button class:on={tab === "evidence"} onclick={() => (tab = "evidence")}>Evidence</button>
      <button class:on={tab === "diff"} disabled={!data.task.commit} onclick={() => (tab = "diff")}>Diff</button>
    </div>
    {#if tab === "evidence"}
      <dl>
        {#each data.rows as r (r.label + r.text)}
          <div class="row" class:claim={r.label === "Model says"}>
            <dt><span class="chip s-{r.mark}">{mark(r.mark)}</span>{r.label}</dt>
            <dd>{r.text}</dd>
          </div>
        {/each}
      </dl>
      {#if data.task.commit}
        <p class="muted small">Every row comes from the commit's signed attestation — <code>speccoder attest verify</code> checks it.</p>
      {/if}
    {:else}
      <Diff {patch} />
    {/if}
    {#if actions.length}
      <div class="acts">
        {#each actions as a (a.label)}
          <button class="btn small" class:primary={a.group === "work"} disabled={!!a.blocked || !!app.job?.running} title={`${a.blocked ?? a.why}\n$ ${a.command}`} onclick={() => app.perform(a)}>{short(a.label)}</button>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="muted">Loading…</p>
  {/if}
</aside>

<style>
  .drawer {
    position: fixed; top: 52px; right: 0; bottom: 0; width: min(560px, 100vw); z-index: 20;
    border-left: 1px solid var(--border); background: var(--surface); box-shadow: var(--shadow-lg);
    overflow: auto; padding: 1rem 1.25rem; display: grid; align-content: start; gap: 0.9rem;
    animation: slide var(--speed) ease-out;
  }
  @keyframes slide { from { transform: translateX(24px); opacity: 0; } }
  header { display: flex; justify-content: space-between; gap: 1rem; }
  h2 { margin: 0.15rem 0 0; font-size: 1.1rem; line-height: 1.3; }
  .banner { border-radius: 8px; padding: 0.65rem 0.85rem; font-weight: 700; letter-spacing: 0.02em; display: flex; gap: 0.6rem; align-items: center; }
  .m { font-size: 1.2rem; }
  .tabs { display: flex; gap: 0.25rem; border-bottom: 1px solid var(--border); }
  .tabs button { border: 0; background: none; padding: 0.45rem 0.75rem; cursor: pointer; color: var(--text-2); border-bottom: 2px solid transparent; }
  .tabs button.on { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
  dl { margin: 0; display: grid; gap: 0.15rem; }
  .row { display: grid; grid-template-columns: 9.5rem 1fr; gap: 0.75rem; padding: 0.45rem 0; border-bottom: 1px solid var(--border); }
  dt { font-weight: 600; display: flex; gap: 0.45rem; align-items: center; }
  dd { margin: 0; color: var(--text-2); overflow-wrap: anywhere; }
  .claim dd { color: var(--muted); font-style: italic; }
  .small { font-size: 0.82rem; }
  .acts { display: flex; flex-wrap: wrap; gap: 0.4rem; padding-top: 0.5rem; border-top: 1px solid var(--border); }
</style>
