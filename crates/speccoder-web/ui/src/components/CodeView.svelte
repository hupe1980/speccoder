<script lang="ts">
  // The feature's code: what its branch changed — by commit (one per task,
  // with its verdict) and by file — and its files, browsable and readable.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { symbol, word } from "../lib/marks";
  import { language } from "../lib/highlight";
  import Diff from "./Diff.svelte";
  import Icon from "./Icon.svelte";
  import Source from "./Source.svelte";

  interface Changes {
    branch: string | null;
    base: string | null;
    commits: { id: string; subject: string; task: string | null; verdict: string | null; date: string }[];
    files: { path: string; add: number | null; del: number | null }[];
    patch: string;
    cut: boolean;
  }

  let mode = $state<"changes" | "files">("changes");
  let changes = $state<Changes | null>(null);
  let commit = $state<string | null>(null);
  let commitPatch = $state("");
  let focus = $state<string | null>(null);
  let tree = $state<{ rev: string; files: string[] } | null>(null);
  let filter = $state("");
  let open = $state<string | null>(null);
  let text = $state<{ text?: string; error?: string } | null>(null);
  let loading = $state(false);

  $effect(() => {
    void app.report;
    const feature = app.feature ?? undefined;
    loading = true;
    void get<Changes>(`/api/changes${q({ feature })}`)
      .then((c) => (changes = c))
      .finally(() => (loading = false));
  });
  $effect(() => {
    if (mode !== "files" || tree) return;
    void get<{ rev: string; files: string[] }>(`/api/tree${q({ feature: app.feature ?? undefined })}`).then((t) => (tree = t));
  });
  $effect(() => {
    const c = commit;
    commitPatch = "";
    if (c) void get<{ patch: string }>(`/api/diff${q({ commit: c })}`).then((d) => (commitPatch = d.patch));
  });
  $effect(() => {
    const p = open;
    text = null;
    if (p) void get<{ text?: string; error?: string }>(`/api/file${q({ feature: app.feature ?? undefined, path: p })}`).then((t) => (text = t));
  });

  const total = $derived((changes?.files ?? []).reduce((a, f) => ({ add: a.add + (f.add ?? 0), del: a.del + (f.del ?? 0) }), { add: 0, del: 0 }));
  const shown = $derived((tree?.files ?? []).filter((f) => !filter || f.toLowerCase().includes(filter.toLowerCase())).slice(0, 400));
  const changed = $derived(new Set((changes?.files ?? []).map((f) => f.path)));
  const taskOf = (t: string | null) => t?.split("/").pop() ?? null;
  const lines = $derived(text?.text?.split("\n") ?? []);
  const when = (d: string) => new Date(d).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
</script>

<section class="code">
  <header>
    <div class="seg" role="tablist" aria-label="Code">
      <button role="tab" aria-selected={mode === "changes"} class:on={mode === "changes"} onclick={() => (mode = "changes")}>Changes</button>
      <button role="tab" aria-selected={mode === "files"} class:on={mode === "files"} onclick={() => (mode = "files")}>Files</button>
    </div>
    {#if changes?.branch}
      <span class="ref"><Icon name="branch" size={13} /><span class="mono">{changes.branch}</span>{#if changes.base}<span class="muted">from <span class="mono">{changes.base.slice(0, 8)}</span></span>{/if}</span>
      <span class="stat"><span class="a">+{total.add}</span><span class="d">−{total.del}</span> in {changes.files.length} {changes.files.length === 1 ? "file" : "files"}</span>
    {:else if changes}
      <span class="muted">speccoder has not worked on this feature yet — no branch to show.</span>
    {/if}
  </header>

  {#if mode === "changes"}
    {#if loading && !changes}
      <div class="state"><span class="spinner"></span></div>
    {:else if changes?.branch && changes.files.length}
      <div class="split">
        <aside class="side">
          <h3 class="section-title">Commits</h3>
          <ul class="commits">
            <li><button class:on={!commit} onclick={() => { commit = null; focus = null; }}><span class="all">All changes</span><span class="muted small">{changes.commits.length} commits</span></button></li>
            {#each changes.commits as c (c.id)}
              <li>
                <button class:on={commit === c.id} onclick={() => { commit = c.id; focus = null; }}>
                  <span class="row1">
                    {#if c.verdict}<span class="v s-{c.verdict}" title={word(c.verdict)}><Icon name={symbol(c.verdict)} size={10} weight={2.6} /></span>{/if}
                    {#if taskOf(c.task)}<span class="mono task">{taskOf(c.task)}</span>{/if}
                    <span class="when">{when(c.date)}</span>
                  </span>
                  <span class="subj">{c.subject.replace(/^speccoder: T\d+ /, "")}</span>
                </button>
              </li>
            {/each}
          </ul>
          {#if !commit}
            <h3 class="section-title">Files</h3>
            <ul class="files">
              {#each changes.files as f (f.path)}
                <li>
                  <button class:on={focus === f.path} onclick={() => (focus = focus === f.path ? null : f.path)} title={f.path}>
                    <span class="fname mono">{f.path.split("/").pop()}<span class="dir">{f.path.includes("/") ? " " + f.path.slice(0, f.path.lastIndexOf("/")) : ""}</span></span>
                    <span class="a">{f.add == null ? "bin" : `+${f.add}`}</span><span class="d">{f.del == null ? "" : `−${f.del}`}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </aside>
        <div class="main">
          {#if changes.cut && !commit}<p class="muted small">The change is large; it is shown up to 2 MB. Pick a commit or a file.</p>{/if}
          {#if commit}
            <Diff patch={commitPatch} loading={!commitPatch} />
          {:else}
            <Diff patch={changes.patch} only={focus} header={false} />
          {/if}
        </div>
      </div>
    {:else if changes}
      <div class="state"><Icon name="doc" size={24} /><p><strong>No changes yet</strong></p><p class="muted">Run a task: its code appears here, commit by commit.</p></div>
    {/if}
  {:else}
    <div class="split">
      <aside class="side">
        <input class="search" placeholder="Filter files" bind:value={filter} aria-label="Filter files" />
        <ul class="files tree">
          {#each shown as f (f)}
            <li>
              <button class:on={open === f} onclick={() => (open = f)} title={f}>
                <span class="fname mono">{f.split("/").pop()}<span class="dir">{f.includes("/") ? " " + f.slice(0, f.lastIndexOf("/")) : ""}</span></span>
                {#if changed.has(f)}<span class="dot" title="Changed by this feature"></span>{/if}
              </button>
            </li>
          {/each}
        </ul>
        {#if tree && tree.files.length > shown.length}<p class="muted small">{tree.files.length - shown.length} more — filter to find them.</p>{/if}
      </aside>
      <div class="main">
        {#if !open}
          <div class="state"><Icon name="doc" size={24} /><p class="muted">Pick a file to read it{#if tree} at <span class="mono">{tree.rev}</span>{/if}.</p></div>
        {:else if !text}
          <div class="state"><span class="spinner"></span></div>
        {:else if text.error}
          <div class="state"><p class="muted">{text.error}</p></div>
        {:else}
          <div class="viewer">
            <div class="vhead"><span class="mono">{open}</span><span class="muted">{lines.length} lines</span></div>
            <div class="vbody">
              <table>
                <tbody>
                  {#each lines as l, i}
                    <tr><td class="ln">{i + 1}</td><td class="src"><Source line={l} lang={language(open)} /></td></tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .code { max-width: 1400px; margin: 0 auto; display: grid; gap: 1rem; }
  header { display: flex; align-items: center; gap: 1rem; flex-wrap: wrap; }
  .seg { display: inline-flex; padding: 2px; border-radius: 9px; background: var(--fill); }
  .seg button { height: 26px; padding: 0 1rem; border: 0; background: none; border-radius: 7px; cursor: pointer; font-weight: 500; color: var(--text-2); }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12); }
  .ref { display: inline-flex; align-items: center; gap: 0.4rem; font-size: 0.84rem; }
  .stat { margin-left: auto; font-size: 0.84rem; color: var(--text-2); }
  .a { color: var(--add); font-weight: 600; font-variant-numeric: tabular-nums; }
  .d { color: var(--del); font-weight: 600; font-variant-numeric: tabular-nums; margin-left: 0.3rem; }
  .split { display: grid; grid-template-columns: 19rem minmax(0, 1fr); gap: 1.25rem; align-items: start; }
  .side { position: sticky; top: 0; display: grid; gap: 0.4rem; max-height: calc(100vh - 9rem); overflow: auto; padding-right: 0.25rem; }
  ul { list-style: none; margin: 0 0 0.6rem; padding: 0; display: grid; gap: 1px; }
  .side button { width: 100%; text-align: left; border: 0; background: none; padding: 0.4rem 0.55rem; border-radius: 8px; cursor: pointer; display: grid; gap: 0.1rem; }
  .side button:hover { background: var(--fill); }
  .side button.on { background: var(--accent); color: #fff; }
  .side button.on .muted, .side button.on .when, .side button.on .dir, .side button.on .a, .side button.on .d { color: rgba(255, 255, 255, 0.8); }
  .all { font-weight: 600; }
  .small { font-size: 0.78rem; }
  .row1 { display: flex; align-items: center; gap: 0.4rem; font-size: 0.76rem; }
  .v { width: 16px; height: 16px; border-radius: 50%; display: grid; place-items: center; background: var(--fill-c); color: #fff; }
  .task { font-weight: 600; }
  .when { margin-left: auto; color: var(--muted); }
  .subj { font-size: 0.84rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .files button { grid-template-columns: minmax(0, 1fr) auto auto; align-items: center; display: grid; }
  .tree button { grid-template-columns: minmax(0, 1fr) auto; }
  .fname { font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dir { color: var(--muted); font-size: 0.92em; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--attention-fill); }
  .search { height: 30px; border: 0; border-radius: 8px; background: var(--fill); padding: 0 0.7rem; outline: none; }
  .search:focus { box-shadow: 0 0 0 3.5px var(--focus); }
  .main { min-width: 0; }
  .state { display: grid; justify-items: center; gap: 0.3rem; padding: 3rem 1rem; color: var(--muted); }
  .state p { margin: 0; }
  .state strong { color: var(--text); }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .viewer { border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow); overflow: hidden; }
  .vhead { display: flex; justify-content: space-between; padding: 0.5rem 0.8rem; background: var(--surface-2); border-bottom: 0.5px solid var(--separator); font-size: 0.82rem; }
  .vbody { overflow: auto; max-height: calc(100vh - 12rem); font: 0.8rem/1.6 var(--mono); }
  table { border-collapse: collapse; width: 100%; }
  td { padding: 0; vertical-align: top; }
  .ln { width: 1%; min-width: 4ch; padding: 0 0.75rem; text-align: right; color: var(--faint); user-select: none; font-variant-numeric: tabular-nums; }
  .src { white-space: pre; padding-right: 1rem; }
  @media (max-width: 900px) { .split { grid-template-columns: 1fr; } .side { position: static; max-height: 40vh; } }
</style>
