<script lang="ts">
  import { app } from "./lib/app.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Hero from "./components/Hero.svelte";
  import Board from "./components/Board.svelte";
  import Documents from "./components/Documents.svelte";
  import ReportView from "./components/ReportView.svelte";
  import Drawer from "./components/Drawer.svelte";
  import Dock from "./components/Dock.svelte";
  import AskSheet from "./components/AskSheet.svelte";
  import InputSheet from "./components/InputSheet.svelte";
  import Palette from "./components/Palette.svelte";
  import Toasts from "./components/Toasts.svelte";

  let theme = $state<string>(localStorage.getItem("speccoder-theme") ?? "auto");
  $effect(() => {
    if (theme === "auto") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("speccoder-theme", theme);
  });

  let canAsk = $state(typeof Notification !== "undefined" && Notification.permission === "default");

  function keydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.closest("input, textarea");
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.palette = !app.palette;
      return;
    }
    if (typing || app.waiting || app.input || app.palette) return;
    if (e.key === "1") app.view = "board";
    if (e.key === "2") app.view = "documents";
    if (e.key === "3") app.view = "report";
    if (e.key === "n" && app.primary) app.perform(app.primary);
    if (e.key === "Escape") app.selectTask(null);
  }

  const report = $derived(app.report?.data ?? null);

  // The tab says when a run needs you, and what is running.
  $effect(() => {
    const name = app.report?.data?.feature ?? app.project?.name ?? "speccoder";
    document.title = app.waiting ? `● Needs you · ${name}` : app.job?.running ? `▶ ${app.task?.id ?? "Working"} · ${name}` : `${name} · speccoder`;
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

<div class="shell" class:drawer={!!app.selected}>
  <header>
    <div class="brand">
      <svg viewBox="0 0 64 64" width="22" height="22" aria-hidden="true"><rect width="64" height="64" rx="14" fill="var(--accent)" /><path d="M18 33.5 28 43l18-21" fill="none" stroke="var(--accent-text)" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" /></svg>
      <strong>speccoder</strong>
      <span class="muted project">{app.project?.name ?? ""}</span>
    </div>
    <nav aria-label="Views">
      {#each [["board", "Board", "1"], ["documents", "Spec & plan", "2"], ["report", "Report", "3"]] as [v, label, key]}
        <button class="tab" class:on={app.view === v} onclick={() => (app.view = v as typeof app.view)} title={`${label} (${key})`}>{label}</button>
      {/each}
    </nav>
    <div class="right">
      <button class="btn ghost palette" onclick={() => (app.palette = true)} aria-label="Actions">
        <span>Actions</span><kbd>⌘K</kbd>
      </button>
      {#if canAsk}
        <button class="btn ghost small" title="Tell me when a run needs me while I look elsewhere" onclick={async () => { await app.allowNotifications(); canAsk = false; }}>🔔 Notify me</button>
      {/if}
      <select bind:value={theme} aria-label="Theme" class="theme">
        <option value="auto">Auto</option><option value="light">Light</option><option value="dark">Dark</option>
      </select>
      <span class="conn" class:off={!app.connected} title={app.connected ? "connected to the host" : "reconnecting to the host"}></span>
    </div>
  </header>

  <Sidebar />

  <main>
    {#if app.loading}
      <div class="empty">Loading…</div>
    {:else}
      <Hero compact={app.view !== "board"} />
      {#if report}
        {#if app.view === "board"}<Board />{/if}
        {#if app.view === "documents"}<Documents />{/if}
        {#if app.view === "report"}<ReportView />{/if}
      {/if}
    {/if}
  </main>

  {#if app.selected}<Drawer />{/if}
  <Dock />
</div>

{#if app.waiting}<AskSheet />{/if}
{#if app.input}<InputSheet />{/if}
{#if app.palette}<Palette />{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-columns: 264px 1fr;
    grid-template-rows: 52px 1fr auto;
    grid-template-areas: "head head" "side main" "side dock";
    height: 100vh;
  }
  header {
    grid-area: head; display: flex; align-items: center; gap: 1.5rem; padding: 0 1rem;
    border-bottom: 1px solid var(--border); background: var(--surface);
  }
  .brand { display: flex; align-items: center; gap: 0.55rem; min-width: 232px; }
  .brand strong { letter-spacing: -0.01em; }
  .project { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 12rem; }
  nav { display: flex; gap: 0.25rem; }
  .tab { border: 0; background: none; padding: 0.4rem 0.8rem; border-radius: 6px; cursor: pointer; color: var(--text-2); }
  .tab:hover { background: var(--surface-2); }
  .tab.on { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .right { margin-left: auto; display: flex; align-items: center; gap: 0.6rem; }
  .theme { border: 1px solid var(--border); background: var(--surface); border-radius: 6px; padding: 0.25rem 0.4rem; }
  .conn { width: 8px; height: 8px; border-radius: 50%; background: var(--verified); box-shadow: 0 0 0 3px var(--verified-bg); }
  .conn.off { background: var(--attention); box-shadow: 0 0 0 3px var(--attention-bg); }
  main { grid-area: main; overflow: auto; padding: 1.5rem 2rem 2rem; }
  .empty { color: var(--muted); padding: 4rem; text-align: center; }
  @media (max-width: 900px) {
    .shell { grid-template-columns: 1fr; grid-template-areas: "head" "main" "dock"; }
  }
</style>
