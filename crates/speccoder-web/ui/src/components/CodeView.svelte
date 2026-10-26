<script lang="ts">
  // The feature's code: what its branch changed — by commit (one per task,
  // with its verdict) and by file — and its files, browsable and readable.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import { get, latest, q } from "../lib/api";
  import { symbol, word } from "../lib/marks";
  import { language } from "../lib/highlight";
  import Diff from "./Diff.svelte";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";
  import Source from "./Source.svelte";
  import FileTree from "./FileTree.svelte";
  import { diagrams } from "../lib/mermaid";
  import QuickOpen from "./QuickOpen.svelte";
  import { tick } from "svelte";
  import { frozenTests, nudgeLine } from "../lib/review";
  import type { CodeComment, Reply, Review } from "../lib/types";

  interface Changes {
    branch: string | null;
    base: string | null;
    commits: { id: string; subject: string; task: string | null; verdict: string | null; date: string }[];
    files: { path: string; add: number | null; del: number | null }[];
    patch: string;
    cut: boolean;
  }

  // Mode and open file live in the app's state, so the address keeps them
  // (`?view=code&mode=files&file=src/lib.rs`).
  const mode = $derived(app.codeMode);
  // Line comments on a task's change, sent as one nudge for its next
  // attempt (spec 093); a frozen test's lines go to Review › Tests.
  let frozen = $state(new Set<string>());
  $effect(() => {
    void app.feature;
    get<Reply<Review>>(`/api/review${q({ feature: app.feature ?? undefined })}`)
      .then((r) => (frozen = frozenTests(r.data?.oracle ?? [])))
      .catch(() => (frozen = new Set()));
  });
  let notes = $state<CodeComment[]>([]);
  let writing = $state<{ path: string; line: number; quote: string } | null>(null);
  let noteWords = $state("");
  function commentOn(c: { path: string; line: number; quote: string }) {
    if (frozen.has(c.path)) {
      app.toast({ tone: "warn", text: "This is a frozen test. Comment on it in Review › Tests" });
      return;
    }
    writing = c;
    noteWords = "";
  }
  function keepNote() {
    if (!writing || !noteWords.trim()) return;
    notes = [...notes, { ...writing, text: noteWords.trim() }];
    writing = null;
  }
  async function sendNudge(task: string) {
    const all = notes;
    await app.perform(
      offer(
        { kind: "code_nudge", task, comments: all },
        { group: "task", label: `nudge ${task}`, why: "the next attempt reads it", command: `speccoder task nudge ${task} "${all.map(nudgeLine).join("; ").replaceAll('"', "'")}"` },
      ),
    );
    notes = [];
  }
  let changes = $state<Changes | null>(null);
  let commit = $state<string | null>(null);
  let commitPatch = $state("");
  let focus = $state<string | null>(null);
  interface WorktreeView { path: string | null; entries: { path: string; state: "untracked" | "ignored" | "modified" }[]; blocking: string[]; fix: string | null }
  let tree = $state<{ rev: string; files: string[]; worktree?: WorktreeView } | null>(null);
  // The worktree as it is on disk: untracked, ignored and modified paths
  // beside the committed tree (a per-viewer choice).
  const readDisk = () => { try { return localStorage.getItem("speccoder-worktree") === "on"; } catch { return false; } };
  let disk = $state(readDisk());
  function toggleDisk() {
    disk = !disk;
    try { localStorage.setItem("speccoder-worktree", disk ? "on" : "off"); } catch { /* storage off */ }
  }
  const wt = $derived(tree?.worktree ?? null);
  // While something on disk stops a run, the disk is shown: the culprit in view.
  const shown_disk = $derived(disk || !!wt?.blocking.length);
  const extra = $derived(shown_disk && wt ? wt.entries.filter((e) => e.state !== "modified") : []);
  const marks = $derived.by(() => {
    const m: Record<string, "untracked" | "ignored" | "ignored-dir" | "modified"> = {};
    if (!shown_disk || !wt) return m;
    for (const e of wt.entries) {
      const dir = e.path.endsWith("/");
      m[dir ? e.path.slice(0, -1) : e.path] = e.state === "ignored" && dir ? "ignored-dir" : e.state;
    }
    return m;
  });
  const counted = $derived({ untracked: wt?.entries.filter((e) => e.state === "untracked").length ?? 0, ignored: wt?.entries.filter((e) => e.state === "ignored").length ?? 0 });
  let filter = $state("");
  const open = $derived(app.codeFile);
  let text = $state<{ text?: string; html?: string; error?: string } | null>(null);
  // Markdown opens as its preview; the switch shows the source.
  let preview = $state(true);
  let loading = $state(false);

  const changesOf = latest();
  const treeOf = latest();
  const patchOf = latest();
  const fileOf = latest();
  // A feature switched: what was picked belonged to the other one.
  let shownFeature: string | null = null;
  $effect(() => {
    const f = app.feature;
    if (shownFeature !== null && f !== shownFeature) {
      commit = null;
      focus = null;
      tree = null;
    }
    shownFeature = f;
  });
  $effect(() => {
    void app.report;
    const feature = app.feature ?? undefined;
    loading = true;
    changesOf(get<Changes>(`/api/changes${q({ feature })}`), (c) => {
      changes = c;
      loading = false;
    }, () => (loading = false));
  });
  // The tree follows the feature and every new tick (a run adds files).
  $effect(() => {
    void app.report;
    const feature = app.feature ?? undefined;
    if (mode !== "files") return;
    treeOf(get<{ rev: string; files: string[]; worktree?: WorktreeView }>(`/api/tree${q({ feature })}`), (t) => (tree = t));
  });
  $effect(() => {
    const c = commit;
    commitPatch = "";
    if (c) patchOf(get<{ patch: string }>(`/api/diff${q({ commit: c })}`), (d) => (commitPatch = d.patch));
  });
  $effect(() => {
    const p = open;
    void app.report;
    const feature = app.feature ?? undefined;
    text = null;
    if (p) fileOf(get<{ text?: string; html?: string; error?: string }>(`/api/file${q({ feature, path: p })}`), (t) => (text = t), (e) => (text = { error: String(e) }));
  });

  const total = $derived((changes?.files ?? []).reduce((a, f) => ({ add: a.add + (f.add ?? 0), del: a.del + (f.del ?? 0) }), { add: 0, del: 0 }));
  // The whole tree (folders keep it navigable); a filter narrows it to the
  // matching paths, at most 400.
  const all = $derived([...(tree?.files ?? []), ...extra.map((e) => (e.path.endsWith("/") ? e.path.slice(0, -1) : e.path))]);
  const matching = $derived(all.filter((f) => !filter || f.toLowerCase().includes(filter.toLowerCase())));
  const matches = $derived(matching.length);
  const shown = $derived(filter ? matching.slice(0, 400) : matching);
  const changed = $derived(new Set((changes?.files ?? []).map((f) => f.path)));
  const taskOf = (t: string | null) => t?.split("/").pop() ?? null;
  const lines = $derived(text?.text?.split("\n") ?? []);

  // Go to a line: the address keeps it; the row is marked and scrolled to.
  let body: HTMLElement | undefined = $state();
  $effect(() => {
    const n = app.codeLine;
    void lines.length;
    if (!n || !body) return;
    void tick().then(() => body?.querySelector(`[data-line="${n}"]`)?.scrollIntoView({ block: "center" }));
  });
  function pickLine(n: number) {
    app.codeLine = app.codeLine === n ? null : n;
  }

  // Find in the file (⌘F): the window has no find of its own.
  let finding = $state(false);
  let needle = $state("");
  let at = $state(0);
  let findInput: HTMLInputElement | undefined = $state();
  const hits = $derived.by(() => {
    const q = needle.toLowerCase();
    return q ? lines.flatMap((l, i) => (l.toLowerCase().includes(q) ? [i + 1] : [])) : [];
  });
  $effect(() => {
    void needle;
    at = 0;
  });
  function jump(step: number) {
    if (!hits.length) return;
    at = (at + step + hits.length) % hits.length;
    body?.querySelector(`[data-line="${hits[at]}"]`)?.scrollIntoView({ block: "center" });
  }

  let quick = $state(false);
  function keys(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "p") {
      e.preventDefault();
      app.codeMode = "files";
      quick = true;
    } else if (mod && e.key.toLowerCase() === "f" && open && !(text?.html !== undefined && preview)) {
      e.preventDefault();
      finding = true;
      void tick().then(() => findInput?.select());
    }
  }

  // Links in a previewed Markdown file: a file of the branch opens here, a
  // web address in the browser, never this page away.
  function follow(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    const href = a?.getAttribute("href");
    if (!a || !href) return;
    e.preventDefault();
    if (/^https?:\/\//.test(href)) {
      window.open(href, "_blank", "noopener,noreferrer");
      return;
    }
    if (href.startsWith("#")) return;
    const base = (open ?? "").split("/").slice(0, -1);
    for (const part of href.split("#")[0]!.split("/")) {
      if (part === "..") base.pop();
      else if (part && part !== ".") base.push(decodeURIComponent(part));
    }
    const target = base.join("/");
    if (tree?.files.includes(target)) app.codeFile = target;
    else app.toast({ tone: "warn", text: "Not in this branch", detail: target });
  }
  function copyPath() {
    navigator.clipboard?.writeText(open ?? "");
    app.toast({ tone: "ok", text: "Copied the path", detail: open ?? "" });
  }
  const when = (d: string) => new Date(d).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
</script>

<svelte:window onkeydown={keys} />

{#if quick && tree}
  <QuickOpen files={tree.files} {changed} onpick={(p) => { app.codeFile = p; app.codeLine = null; quick = false; }} onclose={() => (quick = false)} />
{/if}

<section class="code">
  <header>
    <div class="seg" role="tablist" aria-label="Code">
      <button role="tab" aria-selected={mode === "changes"} class:on={mode === "changes"} onclick={() => (app.codeMode = "changes")}>Changes</button>
      <button role="tab" aria-selected={mode === "files"} class:on={mode === "files"} onclick={() => (app.codeMode = "files")}>Files</button>
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
                  <span class="subj"><Code text={c.subject.replace(/^speccoder: T\d+ /, "")} /></span>
                </button>
              </li>
            {/each}
          </ul>
          {#if !commit}
            <h3 class="section-title">Files {#if focus}<button class="clear" onclick={() => (focus = null)}>Show all</button>{/if}</h3>
            <FileTree
              paths={changes.files.map((f) => f.path)}
              selected={focus}
              counts={Object.fromEntries(changes.files.map((f) => [f.path, { add: f.add, del: f.del }]))}
              onpick={(p) => (focus = focus === p ? null : p)}
            />
          {/if}
        </aside>
        <div class="main">
          {#if changes.cut && !commit}<p class="muted small">The change is large; it is shown up to 2 MB. Pick a commit or a file.</p>{/if}
          {#if commit}
            {@const ofTask = taskOf(changes.commits.find((c) => c.id === commit)?.task ?? null)}
            <Diff patch={commitPatch} loading={!commitPatch} comment={ofTask ? commentOn : null} {frozen} />
            {#if writing && ofTask}
              <form class="note-form" onsubmit={(e) => { e.preventDefault(); keepNote(); }}>
                <p class="muted small mono">{writing.path}:{writing.line} “{writing.quote.trim().slice(0, 80)}”</p>
                <!-- svelte-ignore a11y_autofocus -->
                <textarea rows="2" bind:value={noteWords} autofocus aria-label="Comment on {writing.path}:{writing.line}" onkeydown={(e) => { if (e.key === "Escape") writing = null; if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); keepNote(); } }}></textarea>
                <div class="note-acts"><button class="btn small primary" type="submit" disabled={!noteWords.trim()}>Add Comment <kbd>⌘↵</kbd></button><button class="btn small ghost" type="button" onclick={() => (writing = null)}>Cancel</button></div>
              </form>
            {/if}
            {#if notes.length && ofTask}
              <div class="code-tray glass" aria-live="polite">
                <strong>{notes.length} {notes.length === 1 ? "comment" : "comments"} on {ofTask}</strong>
                <ul>{#each notes as n, i (i)}<li class="mono small">{nudgeLine(n)} <button class="btn small ghost" aria-label="Delete this comment" onclick={() => (notes = notes.filter((_, j) => j !== i))}>×</button></li>{/each}</ul>
                <button class="btn small primary" disabled={!!app.job?.running} title={`speccoder task nudge ${ofTask} "…"`} onclick={() => sendNudge(ofTask)}>Send {notes.length} {notes.length === 1 ? "Comment" : "Comments"} as a Nudge</button>
              </div>
            {/if}
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
        <div class="tools">
          <input class="search" placeholder="Filter files" bind:value={filter} aria-label="Filter files" />
          <button class="btn small" title="Go to file (⌘P)" onclick={() => (quick = true)}><Icon name="search" size={12} /> Go to <kbd>⌘P</kbd></button>
        </div>
        {#if wt?.path}
          <button class="disk" class:on={shown_disk} onclick={toggleDisk} aria-pressed={shown_disk} title="Show the worktree as it is on disk: what git does not track or ignores">
            <span class="sw" aria-hidden="true"><i></i></span>
            <span>Worktree on disk</span>
            <span class="muted small">{counted.untracked} untracked · {counted.ignored} ignored</span>
          </button>
        {/if}
        {#if wt?.blocking.length}
          <div class="blocker" role="note">
            <Icon name="exclaim" size={14} weight={2.2} />
            <div>
              <strong>{wt.blocking.join(", ")} {wt.blocking.length === 1 ? "is" : "are"} in the worktree, not in git</strong>
              <p>The checks write {wt.blocking.length === 1 ? "it" : "them"} on every run, so a run stops until {wt.blocking.length === 1 ? "it is" : "they are"} part of your checkout's setup.</p>
              <div class="row">
                <button class="btn small tinted" disabled={!!app.job?.running} onclick={() => app.perform(offer({ kind: "fix_setup" }, { group: "work", label: `commit the setup: ${wt.blocking.join(", ")}`, why: "copied into your checkout and committed; the next run takes it", command: wt.fix ?? "" }))}><Icon name="check" size={11} weight={2.4} /> {wt.blocking.some((b) => b.endsWith("/")) ? "Fix the Setup" : `Commit ${wt.blocking.join(", ")}`}</button>
                {#if wt.fix}<button class="btn small ghost" onclick={() => { void navigator.clipboard?.writeText(wt.fix ?? ""); app.toast({ tone: "ok", text: "Copied the command", detail: "Run it in your checkout." }); }}><Icon name="copy" size={11} /> Copy the Command</button>{/if}
              </div>
            </div>
          </div>
        {/if}
        {#key filter}
          <FileTree paths={shown} selected={open} {changed} {marks} onpick={(p) => (app.codeFile = p)} />
        {/key}
        {#if tree && filter && matches > shown.length}<p class="muted small">{matches - shown.length} more match — type more to narrow.</p>{/if}
      </aside>
      <div class="main">
        {#if !open}
          <div class="state"><Icon name="doc" size={24} /><p class="muted">Pick a file to read it{#if tree}{" at "}<span class="mono">{tree.rev}</span>{/if}.</p></div>
        {:else if !text}
          <div class="state"><span class="spinner"></span></div>
        {:else if text.error}
          <div class="state"><p class="muted">{text.error}</p></div>
        {:else}
          <div class="viewer">
            <div class="vhead">
              <button class="path mono" title="Copy the path" onclick={copyPath}>{open}<Icon name="copy" size={11} /></button>
              {#if text.html !== undefined}
                <div class="seg small" role="tablist" aria-label="Show">
                  <button role="tab" aria-selected={preview} class:on={preview} onclick={() => (preview = true)}>Preview</button>
                  <button role="tab" aria-selected={!preview} class:on={!preview} onclick={() => (preview = false)}>Source</button>
                </div>
              {/if}
              <span class="muted">{lines.length} lines</span>
              {#if !(text.html !== undefined && preview)}
                <button class="btn small icon" title="Find (⌘F)" aria-label="Find in file" onclick={() => { finding = true; void tick().then(() => findInput?.focus()); }}><Icon name="search" size={12} /></button>
              {/if}
            </div>
            {#if finding && !(text.html !== undefined && preview)}
              <div class="find">
                <Icon name="search" size={12} />
                <input bind:this={findInput} bind:value={needle} placeholder="Find in {open.split('/').pop()}" aria-label="Find"
                  onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); jump(e.shiftKey ? -1 : 1); } else if (e.key === "Escape") { finding = false; needle = ""; } }} />
                <span class="count">{hits.length ? `${at + 1} of ${hits.length}` : needle ? "no match" : ""}</span>
                <button class="btn small icon" aria-label="Previous" onclick={() => jump(-1)} disabled={!hits.length}><Icon name="arrowup" size={11} /></button>
                <button class="btn small icon" aria-label="Next" onclick={() => jump(1)} disabled={!hits.length}><Icon name="down" size={11} /></button>
                <button class="btn small icon" aria-label="Close find" onclick={() => { finding = false; needle = ""; }}><Icon name="xmark" size={10} weight={2.4} /></button>
              </div>
            {/if}
            {#if text.html !== undefined && preview}
              <!-- Rendered by the host from Markdown; raw HTML in the file is shown as text. -->
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
              <article class="md" use:diagrams={text.html} onclick={follow}>{@html text.html}</article>
            {:else}
            <div class="vbody" bind:this={body}>
              <table>
                <tbody>
                  {#each lines as l, i}
                    {@const n = i + 1}
                    <tr data-line={n} class:sel={app.codeLine === n} class:hit={hits.includes(n)} class:cur={hits[at] === n}>
                      <td class="ln"><button tabindex="-1" title="Link to line {n}" onclick={() => pickLine(n)}>{n}</button></td>
                      <td class="src"><Source line={l} lang={language(open)} /></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .disk { display: flex; align-items: center; gap: 0.5rem; width: 100%; border: 0; background: none; padding: 0.35rem 0.25rem; margin: 0.15rem 0 0.25rem; cursor: pointer; font: inherit; font-size: 0.8rem; color: var(--text-2); border-radius: 8px; }
  .disk:hover { background: var(--fill); }
  .disk .muted { margin-left: auto; }
  .sw { width: 1.75rem; height: 1rem; border-radius: 999px; background: var(--fill-2); position: relative; transition: background var(--speed) var(--ease); flex: none; }
  .sw i { position: absolute; top: 2px; left: 2px; width: 0.75rem; height: 0.75rem; border-radius: 50%; background: #fff; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25); transition: transform 220ms var(--spring); }
  .disk.on .sw { background: var(--verified-fill, #30d158); }
  .disk.on .sw i { transform: translateX(0.75rem); }
  .blocker { display: grid; grid-template-columns: auto 1fr; gap: 0.5rem; padding: 0.65rem 0.7rem; margin: 0.2rem 0 0.5rem; border-radius: 10px; color: var(--text-2); font-size: 0.8rem;
    background: color-mix(in srgb, var(--attention, #ff9f0a) 10%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--attention, #ff9f0a) 30%, transparent); }
  .blocker :global(svg) { color: var(--attention, #ff9f0a); margin-top: 0.1rem; }
  .blocker strong { color: var(--text); font-size: 0.82rem; }
  .blocker p { margin: 0.2rem 0 0.35rem; }
  .blocker .row { display: flex; flex-wrap: wrap; gap: 0.35rem; }
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
  .side { position: sticky; top: 0; display: grid; grid-template-columns: minmax(0, 1fr); gap: 0.4rem; max-height: calc(100vh - 9rem); overflow: auto; padding-right: 0.25rem; }
  ul { list-style: none; margin: 0 0 0.6rem; padding: 0; display: grid; gap: 1px; }
  .commits button { width: 100%; text-align: left; border: 0; background: none; padding: 0.4rem 0.55rem; border-radius: 8px; cursor: pointer; display: grid; gap: 0.1rem; }
  .commits button:hover { background: var(--fill); }
  .commits button.on { background: var(--accent); color: #fff; }
  .commits button.on .muted, .commits button.on .when { color: rgba(255, 255, 255, 0.8); }
  .all { font-weight: 600; }
  .small { font-size: 0.78rem; }
  .row1 { display: flex; align-items: center; gap: 0.4rem; font-size: 0.76rem; }
  .v { width: 16px; height: 16px; border-radius: 50%; display: grid; place-items: center; background: var(--fill-c); color: #fff; }
  .task { font-weight: 600; }
  .note-form { display: grid; gap: 0.35rem; margin-top: 0.6rem; padding: 0.6rem; border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow); }
  .note-form textarea { width: 100%; box-sizing: border-box; border-radius: 8px; border: 0.5px solid var(--separator); background: var(--surface-2); color: var(--text); padding: 0.4rem 0.55rem; font: inherit; }
  .note-form p { margin: 0; }
  .note-acts { display: flex; gap: 0.4rem; }
  .code-tray { position: fixed; right: 1.25rem; bottom: 2.4rem; z-index: 15; width: min(420px, calc(100vw - 2.5rem)); display: grid; gap: 0.4rem; padding: 0.7rem; border-radius: var(--radius-lg); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); }
  .code-tray ul { margin: 0; padding: 0; list-style: none; display: grid; gap: 0.2rem; }
  .when { margin-left: auto; color: var(--muted); }
  .subj { font-size: 0.84rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fname { font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dir { color: var(--muted); font-size: 0.92em; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--attention-fill); }
  .clear { border: 0; background: none; color: var(--accent-ink); font-weight: 600; cursor: pointer; font-size: 0.78rem; padding: 0; }
  .section-title { display: flex; justify-content: space-between; align-items: center; }
  .search { height: 30px; border: 0; border-radius: 8px; background: var(--fill); padding: 0 0.7rem; outline: none; }
  .search:focus { box-shadow: 0 0 0 3.5px var(--focus); }
  .main { min-width: 0; }
  .state { display: grid; justify-items: center; gap: 0.3rem; padding: 3rem 1rem; color: var(--muted); }
  .state p { margin: 0; }
  .state strong { color: var(--text); }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .viewer { border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow); overflow: hidden; }
  .seg.small button { height: 22px; padding: 0 0.7rem; font-size: 0.8rem; }
  .md { padding: 1.5rem 2rem 2rem; max-width: 80ch; line-height: 1.65; font-size: 15px; max-height: calc(100vh - 12rem); overflow: auto; }
  .md :global(h1) { font: 700 1.7rem/1.2 var(--display); letter-spacing: -0.024em; margin: 0 0 1rem; }
  .md :global(h2) { font: 650 1.25rem/1.25 var(--display); margin: 2rem 0 0.6rem; }
  .md :global(h3) { font: 650 1.05rem/1.3 var(--display); margin: 1.5rem 0 0.5rem; }
  .md :global(p), .md :global(li) { color: var(--text-2); }
  .md :global(code) { background: var(--fill); padding: 0.1em 0.35em; border-radius: 5px; font-size: 0.86em; }
  .md :global(pre) { background: var(--surface-2); padding: 0.85rem 1rem; border-radius: var(--radius); overflow-x: auto; font-size: 0.85em; }
  .md :global(pre code) { background: none; padding: 0; }
  .md :global(table) { border-collapse: collapse; width: 100%; font-size: 0.88rem; }
  .md :global(th), .md :global(td) { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 0.5px solid var(--separator); }
  .md :global(blockquote) { margin: 1rem 0; padding: 0.25rem 1rem; border-left: 3px solid var(--fill-2); color: var(--muted); }
  .md :global(figure.diagram) { margin: 1.25rem 0; padding: 1rem; border-radius: var(--radius); background: var(--surface-2); overflow-x: auto; text-align: center; }
  .md :global(figure.diagram svg) { max-width: 100%; height: auto; }
  .md :global(.diagram-error) { color: var(--danger); font-size: 0.82rem; text-align: left; }
  .tools { display: flex; gap: 0.4rem; align-items: center; }
  .tools .search { flex: 1; min-width: 0; }
  .path { border: 0; background: none; padding: 0; display: inline-flex; align-items: center; gap: 0.35rem; cursor: copy; color: var(--text); font-size: 0.82rem; }
  .path :global(svg) { color: var(--muted); opacity: 0; transition: opacity var(--speed); }
  .path:hover :global(svg) { opacity: 1; }
  .find { display: flex; align-items: center; gap: 0.4rem; padding: 0.35rem 0.6rem; border-bottom: 0.5px solid var(--separator); background: var(--surface); color: var(--muted); }
  .find input { flex: 1; border: 0; background: var(--fill); border-radius: 6px; height: 24px; padding: 0 0.5rem; outline: none; font-size: 0.84rem; color: var(--text); }
  .find input:focus { box-shadow: 0 0 0 3px var(--focus); }
  .count { font-size: 0.76rem; min-width: 5rem; text-align: right; font-variant-numeric: tabular-nums; }
  .ln button { all: unset; cursor: pointer; display: block; text-align: right; }
  .ln button:hover { color: var(--accent-ink); text-decoration: underline; }
  tr.sel { background: color-mix(in srgb, #ffcc00 22%, transparent); }
  tr.sel .ln { color: var(--text); font-weight: 600; }
  tr.hit .src { background: color-mix(in srgb, #ffcc00 14%, transparent); }
  tr.cur .src { background: color-mix(in srgb, #ff9500 30%, transparent); }
  .vhead { display: flex; justify-content: space-between; align-items: center; gap: 1rem; padding: 0.5rem 0.8rem; background: var(--surface-2); border-bottom: 0.5px solid var(--separator); font-size: 0.82rem; }
  .vbody { overflow: auto; max-height: calc(100vh - 12rem); font: 0.8rem/1.6 var(--mono); }
  table { border-collapse: collapse; width: 100%; }
  td { padding: 0; vertical-align: top; }
  .ln { width: 1%; min-width: 4ch; padding: 0 0.75rem; text-align: right; color: var(--faint); user-select: none; font-variant-numeric: tabular-nums; }
  .src { white-space: pre; padding-right: 1rem; }
  @media (max-width: 900px) { .split { grid-template-columns: 1fr; } .side { position: static; max-height: 40vh; } }
</style>
