<script lang="ts">
  // A task here stopped because of another project: Report to ‹project›…
  // asks the one thing only the person knows — what should happen instead —
  // and records the bug there through that project's own command (one
  // commit of its spec files; nothing runs until someone runs it there).
  // Or: Edit There First opens its bug sheet filled from this stop.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  let { feature, task }: { feature: string; task: string } = $props();
  let open = $state(false);
  let to = $state<{ path: string; name: string } | null>(null);
  let expected = $state("");
  let busy = $state(false);
  $effect(() => {
    if (!app.projects) void app.loadProjects();
  });
  const others = $derived((app.projects?.projects ?? []).filter((p) => !p.current && p.state !== "missing"));
  let trigger = $state<HTMLButtonElement | null>(null);
  const close = () => {
    open = false;
    to = null;
    expected = "";
  };
  // Escape closes the form or the list, before the page's own Escape
  // closes the inspector, and gives the focus back.
  function key(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      close();
      e.stopImmediatePropagation();
      trigger?.focus();
    }
  }
  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (!to || !expected.trim() || busy) return;
    busy = true;
    const ok = await app.reportTo(to.path, to.name, feature, task, expected.trim());
    busy = false;
    if (ok) close();
  }
</script>

<svelte:window onkeydowncapture={key} />

{#if others.length}
  <div class="report">
    <button class="btn small ghost" bind:this={trigger} aria-haspopup="menu" aria-expanded={open} onclick={() => (open ? close() : (open = true))} title="Caused by another project? Report it there as a bug">
      <Icon name="arrowup" size={11} weight={2.2} /> Report to…
    </button>
    {#if open}
      <div class="scrim" role="presentation" onclick={close}></div>
      {#if !to}
        <div class="menu glass" role="menu" aria-label="Report this stop to">
          {#each others as p (p.path)}
            <button role="menuitem" onclick={() => (to = { path: p.path, name: p.name })}>{p.name}</button>
          {/each}
        </div>
      {:else}
        <form class="menu glass form" onsubmit={send} aria-label="Report to {to.name}">
          <label for="rt-expected">What should happen instead, in {to.name}?</label>
          <!-- svelte-ignore a11y_autofocus -->
          <input id="rt-expected" autofocus bind:value={expected} maxlength="500" placeholder="The orders endpoint returns every field the contract names" />
          <p class="quiet">Recorded in {to.name} as a bug with this stop — one commit of its spec files; nothing runs there until someone reproduces it.</p>
          <div class="row">
            <button type="button" class="btn small ghost" onclick={() => { const t = to!; close(); void app.reportTo(t.path, t.name, feature, task); }}>Edit There First</button>
            <button class="btn small primary" disabled={!expected.trim() || busy}>{busy ? "Reporting…" : `Report to ${to.name}`}</button>
          </div>
        </form>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .report { position: relative; display: inline-block; }
  .scrim { position: fixed; inset: 0; z-index: 30; }
  .menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 31; min-width: 14rem; padding: 0.3rem; border-radius: 12px; background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); display: grid; gap: 1px; color: var(--text); font-weight: 400; text-align: left; }
  /* A phone: a sheet across the screen, never cut at its edge. */
  @media (max-width: 600px) { .menu { position: fixed; left: 16px; right: 16px; top: 22vh; width: auto; } }
  .menu button[role="menuitem"] { border: 0; background: none; padding: 0.4rem 0.55rem; border-radius: 7px; color: var(--text); font: inherit; font-size: 0.86rem; cursor: pointer; text-align: left; }
  .menu button[role="menuitem"]:hover { background: var(--accent); color: var(--accent-text-on); }
  .menu button:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .form { width: min(22rem, calc(100vw - 2rem)); padding: 0.7rem; gap: 0.45rem; }
  label { font-size: 0.82rem; font-weight: 600; }
  input { font: inherit; font-size: 0.84rem; padding: 0.35rem 0.5rem; border-radius: 7px; border: 1px solid var(--border); background: var(--surface); color: var(--text); }
  .row { display: flex; justify-content: flex-end; gap: 0.4rem; }
  .quiet { margin: 0; font-size: 0.74rem; color: var(--muted); }
</style>
