<script lang="ts">
  import { flushSync } from "svelte";
  import { app } from "./lib/app.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Hero from "./components/Hero.svelte";
  import Board from "./components/Board.svelte";
  import Documents from "./components/Documents.svelte";
  import ReportView from "./components/ReportView.svelte";
  import Drawer from "./components/Drawer.svelte";
  import Island from "./components/Island.svelte";
  import AskSheet from "./components/AskSheet.svelte";
  import InputSheet from "./components/InputSheet.svelte";
  import Palette from "./components/Palette.svelte";
  import Toasts from "./components/Toasts.svelte";
  import Icon from "./components/Icon.svelte";
  import Overview from "./components/Overview.svelte";
  import Inbox from "./components/Inbox.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import ContextMenu from "./components/ContextMenu.svelte";
  import Settings from "./components/Settings.svelte";
  import CodeView from "./components/CodeView.svelte";
  import Setup from "./components/Setup.svelte";
  import ConfirmSheet from "./components/ConfirmSheet.svelte";

  let theme = $state<string>(localStorage.getItem("speccoder-theme") ?? "auto");
  $effect(() => {
    if (theme === "auto") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("speccoder-theme", theme);
  });

  const themes: [string, string, string][] = [["light", "sun", "Light"], ["auto", "halfcircle", "Automatic"], ["dark", "moon", "Dark"]];
  let canAsk = $state(typeof Notification !== "undefined" && Notification.permission === "default");

  const views = [
    { v: "inbox", label: "Needs You", icon: "tray", key: "1" },
    { v: "overview", label: "Overview", icon: "list", key: "2" },
    { v: "board", label: "Board", icon: "grid", key: "3" },
    { v: "code", label: "Code", icon: "terminal", key: "4" },
    { v: "documents", label: "Spec & Plan", icon: "doc", key: "5" },
    { v: "report", label: "Report", icon: "chart", key: "6" },
  ] as const;

  /** Changes the view with a cross-fade where the browser can. */
  function go(v: typeof app.view) {
    if (v === app.view) return;
    const doc = document as Document & { startViewTransition?: (f: () => void) => unknown };
    if (doc.startViewTransition && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
      doc.startViewTransition(() => flushSync(() => (app.view = v)));
    } else app.view = v;
  }

  function keydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.closest("input, textarea, select");
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.palette = !app.palette;
      return;
    }
    if (typing || app.waiting || app.input || app.palette) return;
    if (e.key === ",") go("settings");
    const v = views.find((x) => x.key === e.key);
    if (v) go(v.v);
    if (e.key === "n" && app.primary) app.perform(app.primary);
    if (e.key === "Escape") app.selectTask(null);
  }

  // The menu bar (F-088): the window's native menus send a command id to
  // the page; the page does what the same button or key would.
  function command(id: string) {
    const find = (f: (a: (typeof app.actions)[number]) => boolean) => app.actions.find(f);
    const run = (a: (typeof app.actions)[number] | undefined, missing: string) => (a ? void app.perform(a) : app.toast({ tone: "warn", text: missing }));
    const v = views.find((x) => `view.${x.v}` === id);
    if (v) return go(v.v);
    switch (id) {
      case "app.settings": return go("settings");
      case "view.sidebar": app.sidebar = !app.sidebar; return;
      case "view.statusbar": app.statusbar = !app.statusbar; return;
      case "view.actions": app.palette = true; return;
      case "theme.light": case "theme.dark": case "theme.auto": theme = id.slice(6); return;
      case "file.new": return run(find((a) => a.run?.kind === "skill" && !!a.input), "No skill writes a new feature here — set up the spec tool first");
      case "file.issues": return go("overview");
      case "file.commit": return run(find((a) => a.run?.kind === "commit"), "Nothing to commit: the planning files are committed");
      case "run.feature": return run(find((a) => a.run?.kind === "run" && a.run.task === null), "No task is ready to run");
      case "run.task": return run(find((a) => a.run?.kind === "run" && a.run.task !== null), "Select a ready task first");
      case "run.converge": return run(find((a) => a.run?.kind === "converge"), "Nothing to re-check: no tick was proven against an earlier wording");
      case "task.nudge": case "task.skip": case "task.accept": {
        const verb = id.slice(5);
        if (!app.selected) return app.toast({ tone: "warn", text: "Select a task first (click its card on the board)" });
        return run(find((a) => a.group === "task" && a.label === `${verb} ${app.selected}`), `${app.selected} cannot be ${verb === "accept" ? "accepted" : verb === "skip" ? "skipped" : "nudged"} now`);
      }
      case "task.evidence":
        if (!app.selected) return app.toast({ tone: "warn", text: "Select a task first (click its card on the board)" });
        return go("board");
      case "run.stop": return app.job?.running ? void app.stop() : app.toast({ tone: "warn", text: "Nothing is running" });
      case "review.spec": app.doc = "review"; return go("documents");
      case "review.inbox": return go("inbox");
      case "review.pr": return run(find((a) => a.run?.kind === "pull_request"), "No pull request can be opened from here");
      case "review.report": return go("report");
    }
  }
  $effect(() => {
    const on = (e: Event) => command((e as CustomEvent<string>).detail);
    window.addEventListener("speccoder:menu", on);
    return () => window.removeEventListener("speccoder:menu", on);
  });

  // The splash (index.html) gives way once the first data is in: a fade
  // into the page it was standing in for, then it is gone.
  $effect(() => {
    if (app.loading) return;
    const splash = document.getElementById("splash");
    if (!splash) return;
    splash.classList.add("out");
    setTimeout(() => splash.remove(), 450);
  });

  const report = $derived(app.report?.data ?? null);
  // A project without speccoder.toml (or not yet a spec project) starts in
  // the setup assistant, until it is set up or the person leaves it.
  const needsSetup = $derived(
    app.view !== "settings" &&
      app.view !== "overview" &&
      app.view !== "inbox" &&
      // Only a project with no configuration at all: an invalid file or an
      // uncommitted change is Settings' to show, not a reason to start over.
      (app.report?.code === "NO-PROJECT" || app.preflight.some((c) => c.code === "CONFIG-MISSING")),
  );
  const at = $derived(views.findIndex((x) => x.v === app.view));
  // The thumb sits under the selected tab, measured: tabs are as wide as
  // their names, and lose the names when the window narrows.
  let bar = $state<HTMLElement>();
  const tabs: HTMLButtonElement[] = $state([]);
  let thumb = $state({ x: 0, w: 0 });
  const measure = () => {
    const t = tabs[Math.max(at, 0)];
    if (t) thumb = { x: t.offsetLeft, w: t.offsetWidth };
  };
  $effect(() => {
    void at;
    void app.inbox?.data?.items.length;
    measure();
    // Once the badge and the fonts have laid out.
    requestAnimationFrame(measure);
    void document.fonts?.ready.then(measure);
    if (!bar) return;
    const ro = new ResizeObserver(measure);
    ro.observe(bar);
    for (const t of tabs) if (t) ro.observe(t);
    return () => ro.disconnect();
  });

  // The tab says when a run needs you, and what is running.
  $effect(() => {
    const name = app.report?.data?.feature ?? app.project?.name ?? "speccoder";
    const waiting = app.inbox?.data?.items.length ?? 0;
    document.title = app.waiting ? `● Needs you · ${name}` : waiting && !app.job?.running ? `(${waiting}) ${name} · speccoder` : app.job?.running ? `▶ ${app.task?.id ?? "Working"} · ${name}` : `${name} · speccoder`;
  });

  // The address follows the page: a new entry per view or task, so Back works.
  $effect(() => {
    const next = app.location;
    if (next === location.pathname + location.search) return;
    // The feature filled in on arrival is no step of the person's.
    if (!location.search.includes("feature=")) history.replaceState(null, "", next);
    else history.pushState(null, "", next);
  });
</script>

<svelte:window onkeydown={keydown} />

<div class="shell" class:nosidebar={!app.sidebar} class:nostatus={!app.statusbar}>
  <header class="toolbar glass">
    <div class="brand">
      <svg viewBox="0 0 64 64" width="24" height="24" aria-hidden="true">
        <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b9bff" /><stop offset="1" stop-color="#0060df" /></linearGradient></defs>
        <rect width="64" height="64" rx="15" fill="url(#g)" />
        <path d="M19 33.5 28.5 43 46 22" fill="none" stroke="#fff" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <div class="names">
        <strong>speccoder</strong>
        <span class="project">{app.project?.name ?? ""}</span>
      </div>
    </div>

    <div class="segmented" class:none={at < 0} role="tablist" aria-label="Views" bind:this={bar} style="--x:{thumb.x}px; --w:{thumb.w}px">
      <span class="thumb" aria-hidden="true"></span>
      {#each views as x, i (x.v)}
        <button bind:this={tabs[i]} role="tab" aria-selected={app.view === x.v} class:on={app.view === x.v} onclick={() => go(x.v)} title="{x.label} ({x.key})" aria-label={x.label}>
          <Icon name={x.icon} size={15} /><span class="label">{x.label}</span>{#if x.v === "inbox" && app.inbox?.data?.items.length}<span class="badge" aria-label="{app.inbox.data.items.length} waiting">{app.inbox.data.items.length}</span>{/if}
        </button>
      {/each}
    </div>

    <div class="right">
      <button class="search" onclick={() => (app.palette = true)} aria-label="Actions">
        <Icon name="search" size={15} /><span>Actions</span><kbd>⌘K</kbd>
      </button>
      {#if canAsk}
        <button class="btn icon ghost" title="Notify me when a run needs me while I look elsewhere" aria-label="Notify me" onclick={async () => { await app.allowNotifications(); canAsk = false; }}><Icon name="bell" size={17} /></button>
      {/if}
      <button class="btn icon ghost" class:onv={app.view === "settings"} title="Settings (,)" aria-label="Settings" onclick={() => go("settings")}><Icon name="gear" size={17} /></button>
      <div class="themes" role="radiogroup" aria-label="Appearance">
        {#each themes as [t, icon, label] (t)}
          <button role="radio" aria-checked={theme === t} class:on={theme === t} title={label} aria-label={label} onclick={() => (theme = t)}><Icon name={icon} size={14} /></button>
        {/each}
      </div>
    </div>
  </header>

  {#if app.sidebar}<Sidebar />{/if}

  <main>
    {#if app.loading}
      <div class="loading" aria-busy="true"><span class="spinner"></span></div>
    {:else}
      {#if app.view === "setup" || needsSetup}<Setup />{:else if app.view === "inbox"}<Inbox />{:else if app.view === "overview"}<Overview />{:else if app.view === "settings"}<Settings />{:else}<Hero compact={app.view !== "board"} />{/if}
      {#if report && !needsSetup && app.view !== "setup" && app.view !== "overview" && app.view !== "inbox" && app.view !== "settings"}
        {#if app.view === "board"}<Board />{/if}
        {#if app.view === "code"}<CodeView />{/if}
        {#if app.view === "documents"}<Documents />{/if}
        {#if app.view === "report"}<ReportView />{/if}
      {/if}
    {/if}
  </main>

  {#if app.selected}<Drawer />{/if}
  {#if app.statusbar}<StatusBar />{/if}
</div>

<Island />
{#if app.waiting}<AskSheet />{/if}
{#if app.input}<InputSheet />{/if}
{#if app.confirming}<ConfirmSheet />{/if}
{#if app.palette}<Palette />{/if}
{#if app.context}<ContextMenu />{/if}
<Toasts />

<style>
  .badge { min-width: 1.15rem; height: 1.15rem; padding: 0 0.32rem; margin-left: 0.15rem; border-radius: 999px; display: inline-grid; place-items: center; background: var(--attention-fill); color: #fff; font: 700 0.68rem/1 var(--rounded); }
  .shell {
    display: grid;
    grid-template-columns: 260px 1fr;
    grid-template-rows: 56px 1fr 26px;
    grid-template-areas: "head head" "side main" "foot foot";
    height: 100vh;
  }
  .shell.nostatus { grid-template-rows: 56px 1fr; grid-template-areas: "head head" "side main"; }
  .shell.nosidebar { grid-template-columns: 1fr; grid-template-areas: "head" "main" "foot"; }
  .shell.nosidebar.nostatus { grid-template-areas: "head" "main"; }
  .toolbar {
    grid-area: head; position: relative; z-index: 10;
    display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 1rem; padding: 0 0.9rem 0 1rem;
    border-bottom: 0.5px solid var(--separator);
  }
  .brand { display: flex; align-items: center; gap: 0.6rem; min-width: 0; }
  .brand svg { filter: drop-shadow(0 1px 1.5px rgba(0, 0, 0, 0.18)); }
  .names { display: grid; line-height: 1.15; min-width: 0; }
  .names strong { font: 650 0.95rem/1.15 var(--display); letter-spacing: -0.01em; }
  .project { font-size: 0.78rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* A segmented control whose thumb slides to the selection. */
  .segmented {
    position: relative; display: flex; min-width: 0;
    padding: 2px; border-radius: 10px; background: var(--fill);
  }
  .thumb {
    position: absolute; top: 2px; bottom: 2px; left: 0; width: var(--w);
    transform: translateX(var(--x)); border-radius: 8px;
    background: var(--surface); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12), 0 0 0 0.5px rgba(0, 0, 0, 0.04);
    transition: transform 320ms var(--spring), width 320ms var(--spring);
  }
  @media (prefers-reduced-motion: reduce) { .thumb { transition: none; } }
  :global(:root[data-theme="dark"]) .thumb { background: #636366; }
  @media (prefers-color-scheme: dark) { :global(:root:not([data-theme="light"])) .thumb { background: #636366; } }
  .segmented.none .thumb { opacity: 0; }
  .segmented button {
    position: relative; display: inline-flex; align-items: center; justify-content: center; gap: 0.4rem;
    height: 28px; padding: 0 0.95rem; border: 0; background: none; border-radius: 8px; cursor: pointer;
    color: var(--text-2); font-weight: 500; font-size: 0.88rem; white-space: nowrap;
  }
  .segmented button.on { color: var(--text); font-weight: 600; }

  .right { justify-self: end; display: flex; align-items: center; gap: 0.5rem; min-width: 0; }
  .search {
    display: inline-flex; align-items: center; gap: 0.45rem; height: 30px; width: 13rem; padding: 0 0.4rem 0 0.65rem;
    border: 0; border-radius: 8px; background: var(--fill); color: var(--muted); cursor: pointer; font-size: 0.88rem;
    transition: background var(--speed) var(--ease);
  }
  .search:hover { background: var(--fill-2); }
  .search span { flex: 1; text-align: left; }
  .onv { background: var(--accent-soft); }
  .themes { display: flex; padding: 2px; border-radius: 8px; background: var(--fill); }
  .themes button { display: grid; place-items: center; width: 26px; height: 24px; border: 0; background: none; border-radius: 6px; color: var(--muted); cursor: pointer; }
  .themes button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12); }
  @keyframes blink { 50% { opacity: 0.35; } }

  main { grid-area: main; overflow: auto; padding: 2rem 2.5rem 7rem; scroll-padding-top: 1rem; }
  .loading { display: grid; place-items: center; height: 60vh; }
  .spinner { width: 22px; height: 22px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* The toolbar gives way as the window narrows, as a Mac toolbar does:
     the search field becomes its icon, then the views their icons (the
     names stay as tooltips and accessible names), then the appearance
     switch and the project's name go. Nothing overflows or overlaps. */
  @media (max-width: 1440px) { .search { width: auto; } .search span { display: none; } }
  @media (max-width: 1180px) {
    .segmented button { padding: 0 0.8rem; }
    .segmented .label { display: none; }
    .search kbd { display: none; }
  }
  @media (max-width: 1040px) { .themes, .names { display: none; } }
  @media (max-width: 900px) {
    .shell { grid-template-columns: 1fr; grid-template-areas: "head" "main" "foot"; }
    .shell.nostatus { grid-template-areas: "head" "main"; }
    .segmented button { padding: 0 0.6rem; }
    main { padding: 1.25rem 1rem 7rem; }
  }
</style>
