<script lang="ts">
  // The project's name at the top of the sidebar, a switcher behind it:
  // the recent projects with what waits in each (? when it could not be
  // read), All Projects…, and how to open another. Choosing one opens it
  // in its own host — its own window or tab, its own key.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  let open = $state(false);
  let hint = $state(false);
  // Counts for the badge: read once, then whenever the list is opened.
  $effect(() => {
    if (!app.projects) void app.loadProjects();
  });
  const list = $derived(app.projects?.projects.filter((p) => p.state !== "missing") ?? []);
  const name = $derived(app.project?.name ?? "Project");
  const elsewhere = $derived(list.filter((p) => !p.current).reduce((n, p) => n + (p.waiting ?? 0), 0));
  const unknown = $derived(list.some((p) => !p.current && p.waiting == null));

  function toggle() {
    open = !open;
    hint = false;
    if (open) void app.loadProjects();
  }
  let trigger = $state<HTMLButtonElement | null>(null);
  // Escape closes the list, before the page's own Escape closes more.
  function key(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      open = false;
      e.stopImmediatePropagation();
      trigger?.focus();
    }
  }
</script>

<svelte:window onkeydowncapture={key} />

<div class="switch">
  <button class="current" bind:this={trigger} aria-haspopup="menu" aria-expanded={open} onclick={toggle} title="Switch project">
    <span class="name">{name}</span>
    {#if elsewhere || unknown}<span class="badge" title={unknown ? "Waiting for you in other projects — some could not be read" : "Waiting for you in other projects"}>{elsewhere ? `${elsewhere}${unknown ? "+" : ""}` : "?"}</span>{/if}
    <Icon name="down" size={11} weight={2.4} />
  </button>
  {#if open}
    <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
    <div class="menu glass" role="menu" aria-label="Projects">
      {#each list as p (p.path)}
        <button role="menuitemradio" aria-checked={p.current} onclick={() => { open = false; if (!p.current) void app.openProject(p.path, p.name); }}>
          <span class="tick" aria-hidden="true">{#if p.current}<Icon name="check" size={11} weight={2.6} />{/if}</span>
          <span class="n">{p.name}</span>
          {#if p.waiting == null}<span class="c unknown" title={p.unknown ?? "could not be read"}>?</span>{:else if p.waiting}<span class="c">{p.waiting}</span>{/if}
        </button>
      {/each}
      {#if !list.length}<p class="quiet">No other projects yet.</p>{/if}
      <hr />
      <button role="menuitem" onclick={() => { open = false; app.view = "projects"; }}><span class="tick"></span><span class="n">All Projects…</span></button>
      <button role="menuitem" onclick={() => (hint = !hint)}><span class="tick"></span><span class="n">Open Project…</span></button>
      {#if hint}<p class="quiet">{#if app.project?.window}File ▸ Open Project… (⌘O){:else}Run <code>speccoder ui</code> in its folder{/if} — it then appears here.</p>{/if}
    </div>
  {/if}
</div>

<style>
  .switch { position: relative; padding: 0 0.25rem 0.6rem; }
  .current { width: 100%; display: flex; gap: 0.4rem; align-items: center; border: 0; background: none; padding: 0.35rem 0.5rem; border-radius: 8px; cursor: pointer; color: var(--text); font: 650 0.95rem/1.2 var(--display); text-align: left; }
  .current:hover { background: var(--fill); }
  .current:focus-visible, .menu button:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { flex: none; min-width: 1.15rem; height: 1.15rem; padding: 0 0.32rem; border-radius: 999px; display: inline-grid; place-items: center; background: var(--attention-fill); color: #fff; font: 700 0.68rem/1 var(--rounded); }
  .scrim { position: fixed; inset: 0; z-index: 30; }
  .menu { position: absolute; left: 0.25rem; right: 0.25rem; top: calc(100% - 0.4rem); z-index: 31; padding: 0.3rem; border-radius: 12px; background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); display: grid; gap: 1px; }
  .menu button { display: flex; gap: 0.4rem; align-items: center; border: 0; background: none; padding: 0.4rem 0.5rem; border-radius: 7px; color: var(--text); font: inherit; font-size: 0.86rem; cursor: pointer; text-align: left; }
  .menu button:hover { background: var(--accent); color: var(--accent-text-on); }
  .tick { width: 12px; flex: none; display: inline-grid; place-items: center; }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .c { flex: none; font-variant-numeric: tabular-nums; font-weight: 650; color: var(--attention); }
  .menu button:hover .c { color: inherit; }
  .unknown { color: var(--muted); }
  hr { border: 0; border-top: 0.5px solid var(--separator); margin: 0.25rem 0.3rem; }
  .quiet { margin: 0.2rem 0.5rem; font-size: 0.78rem; color: var(--muted); }
</style>
