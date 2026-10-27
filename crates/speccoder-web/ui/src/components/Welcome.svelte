<script lang="ts">
  // A host started outside a project: four doors — New Project, Open,
  // Clone, Try the Sample — and the projects opened before. Choosing one
  // binds this host to it, once; the page reloads into the project.
  import { app } from "../lib/app.svelte";
  import { get, post, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { debounce } from "../lib/start";
  import type { RecentProject } from "../lib/types";
  import Icon from "./Icon.svelte";

  const w = $derived(app.welcome);
  let opening = $state(false);
  let path = $state(app.welcome?.start_shown ?? "");
  let error = $state<string | null>(null);
  let suggestions = $state<string[]>([]);
  let busy = $state(false);
  let startOver = $state(false);

  const suggest = debounce(async (p: string) => {
    try {
      const f = await get<{ folders: string[] }>(`/api/folders${q({ under: p })}`);
      suggestions = f.folders ?? [];
    } catch {
      suggestions = [];
    }
  }, 200);
  $effect(() => {
    if (opening) void suggest(path);
  });

  async function open(p = path) {
    error = null;
    busy = true;
    try {
      const r = await post<{ ok?: boolean; error?: string }>("/api/open", { path: p });
      if (r.ok) location.reload();
      else error = r.error ?? "Not a folder";
    } catch (e) {
      error = String((e as Error).message ?? e);
    } finally {
      busy = false;
    }
  }

  async function sample(fresh: boolean) {
    busy = true;
    const r = await app.run(offer({ kind: "open_sample", fresh }, { group: "work", label: "try the sample", why: "a recorded run, replayed strictly", command: fresh ? "speccoder sample --fresh --yes" : "speccoder sample" }));
    busy = false;
    startOver = false;
    if (r && r.status !== "ok") app.toast({ tone: "error", text: r.message, detail: r.code ?? undefined });
  }

  async function forget(r: RecentProject) {
    await post("/api/recent/forget", { path: r.path });
    app.welcome = await get("/api/welcome");
  }

  async function undo(r: RecentProject) {
    if (!confirmUndo || confirmUndo !== r.path) {
      confirmUndo = r.path;
      return;
    }
    confirmUndo = null;
    const commit = "";
    const res = await app.run(offer({ kind: "undo_new_project", commit }, { group: "work", label: `undo ${r.name}`, why: "", command: `speccoder new --undo ${r.path} --yes` }), undefined, r.path);
    if (res) app.toast({ tone: res.status === "ok" ? "ok" : "error", text: res.message });
    app.welcome = await get("/api/welcome");
  }
  let confirmUndo = $state<string | null>(null);

  const ago = (s: number) => {
    const d = Math.floor((Date.now() / 1000 - s) / 86400);
    return d <= 0 ? "today" : d === 1 ? "yesterday" : `${d} days ago`;
  };

  function key(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "o") {
      e.preventDefault();
      opening = true;
    }
  }
</script>

<svelte:window onkeydown={key} />

<section class="welcome" aria-labelledby="w-title">
  <header>
    <svg viewBox="0 0 64 64" width="44" height="44" aria-hidden="true"><rect width="64" height="64" rx="15" fill="#0a7cff" /><path d="M19 33.5 28.5 43 46 22" fill="none" stroke="#fff" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
    <h1 id="w-title">Welcome to speccoder</h1>
    <p class="muted">Specs in, verified code out — every task proven on evidence.</p>
  </header>

  <div class="cards">
    <button class="card" onclick={() => (app.newProject = {})}>
      <span class="t"><Icon name="plus" size={16} /> New Project <kbd>⌘N</kbd></span>
      <span class="d">From one line of intent to a project speccoder can judge</span>
    </button>
    <button class="card" aria-expanded={opening} onclick={() => (opening = !opening)}>
      <span class="t"><Icon name="doc" size={16} /> Open Project… <kbd>⌘O</kbd></span>
      <span class="d">A folder with code or a spec project</span>
    </button>
    <button class="card" onclick={() => (app.clone = true)}>
      <span class="t"><Icon name="branch" size={16} /> Clone from GitHub…</span>
      <span class="d">A repository of yours, then set up</span>
    </button>
    {#if w?.sample}
      <button class="card" disabled={busy} onclick={() => (w?.sample_copy ? (startOver = true) : sample(false))}>
        <span class="t"><Icon name="play" size={16} /> Try the Sample</span>
        <span class="d">A recorded run, replayed — no key, offline, 2 minutes</span>
      </button>
    {/if}
  </div>

  {#if startOver}
    <div class="panel" role="group" aria-label="The sample">
      <p>The sample was opened before.</p>
      <div class="row"><button class="btn" onclick={() => sample(false)}>Open it as it is</button><button class="btn danger" onclick={() => sample(true)}>Start over — your changes to the sample are discarded</button></div>
    </div>
  {/if}

  {#if opening}
    <form class="panel" onsubmit={(e) => { e.preventDefault(); void open(); }}>
      <label for="w-path">Folder</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="w-path" role="combobox" aria-expanded={suggestions.length > 0} aria-controls="w-folders" aria-autocomplete="list" list="w-folders" bind:value={path} autofocus spellcheck="false" aria-invalid={!!error} aria-describedby={error ? "w-err" : undefined} />
      <datalist id="w-folders">{#each suggestions as s (s)}<option value={s}></option>{/each}</datalist>
      <button class="btn primary" type="submit" disabled={busy || !path.trim()}>Open</button>
      {#if error}<span id="w-err" class="err">{error}</span>{/if}
      <span class="hint muted">In the window, File ▸ Open Project… (⌘O) opens the system's folder picker.</span>
    </form>
  {/if}

  {#if w?.recents.length}
    <h2 class="section-title">Recent</h2>
    <ul class="recents grouped">
      {#each w.recents as r (r.path)}
        <li class:gone={!r.exists}>
          {#if r.exists}
            <button class="link" onclick={() => open(r.path)}><strong>{r.name}</strong> <span class="mono muted">{r.shown}</span></button>
          {:else}
            <span><strong>{r.name}</strong> <span class="mono muted">{r.shown}</span></span>
          {/if}
          <span class="when muted">{r.exists ? ago(r.opened_at) : "Folder not found"}</span>
          {#if r.undo?.available}
            <button class="btn small ghost" onclick={() => undo(r)}>{confirmUndo === r.path ? (r.undo.kind === "trash" ? "Move to the Trash?" : "Revert speccoder's commit?") : "Undo"}</button>
          {/if}
          {#if !r.exists}<button class="btn small ghost" onclick={() => forget(r)}>Forget</button>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .welcome { max-width: 980px; margin: 4rem auto 0; display: grid; gap: 1.4rem; }
  header { text-align: center; display: grid; justify-items: center; gap: 0.4rem; }
  h1 { margin: 0.4rem 0 0; font: 700 1.9rem/1.2 var(--display); letter-spacing: -0.02em; }
  header p { margin: 0; }
  .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 0.8rem; }
  .card { display: grid; gap: 0.4rem; text-align: left; padding: 1rem; border-radius: var(--radius); border: 0; background: var(--surface); box-shadow: var(--shadow); cursor: pointer; color: var(--text); font: inherit; }
  .card:hover { box-shadow: var(--shadow-md); }
  .card:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .card[disabled] { opacity: 0.5; cursor: default; }
  .t { display: flex; align-items: center; gap: 0.45rem; font-weight: 650; }
  .t kbd { margin-left: auto; }
  .d { color: var(--text-2); font-size: 0.86rem; }
  .panel { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; padding: 0.8rem 1rem; border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow); }
  .panel p { margin: 0; width: 100%; }
  .panel .row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  .panel input { flex: 1; min-width: 14rem; padding: 0.45rem 0.6rem; border-radius: 8px; border: 0.5px solid var(--separator); background: var(--bg); color: var(--text); font: 0.9rem var(--mono); }
  .panel input:focus { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .err { color: var(--danger); font-size: 0.84rem; width: 100%; }
  .hint { font-size: 0.78rem; width: 100%; }
  .recents { list-style: none; margin: 0; padding: 0; }
  .recents li { display: flex; align-items: center; gap: 0.6rem; padding: 0.55rem 0.8rem; }
  .recents li.gone { opacity: 0.6; }
  .link { flex: 1; text-align: left; border: 0; background: none; color: var(--text); cursor: pointer; font: inherit; padding: 0; }
  .link:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); border-radius: 6px; }
  .when { font-size: 0.8rem; margin-left: auto; }
</style>
