<script lang="ts">
  // Choose Editor (spec 128): asked once, the first time something opens
  // with no editor chosen; the editors found on this machine, the one VISUAL
  // or EDITOR names first. The choice is saved in the person's settings, so
  // the window and the terminal use it too; then the open goes on.
  import { app } from "../lib/app.svelte";
  import { get, post } from "../lib/api";
  import { editor, openIn } from "../lib/editor.svelte";
  import type { EditorsReport, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";

  /** `inline`: Settings' pane — the list alone, the choice marked. */
  let { inline = false }: { inline?: boolean } = $props();
  let report = $state<EditorsReport | null>(null);
  let failed = $state<string | null>(null);
  let saving = $state(false);
  $effect(() => {
    void get<Reply<EditorsReport>>("/api/editor").then(
      (r) => (report = r.data ?? null),
      (e) => (failed = String((e as Error).message ?? e)),
    );
  });
  // Window editors only: the host has no terminal to run vim in.
  const shown = $derived((report?.editors ?? []).filter((e) => !e.terminal));
  const found = $derived(shown.filter((e) => e.found).sort((a, b) => Number(b.id === report?.detected) - Number(a.id === report?.detected)));
  const mine = $derived(report?.chosen ?? report?.detected ?? null);
  const close = () => (editor.choosing = null);
  async function choose(id: string) {
    saving = true;
    const r = await post<Reply<EditorsReport>>("/api/editor", { editor: id }).catch((e) => ({ status: "error", message: String(e) }) as Reply<EditorsReport>);
    saving = false;
    if (r.status !== "ok") {
      failed = r.message;
      return;
    }
    const pending = editor.choosing?.pending ?? null;
    report = r.data ?? report;
    if (!inline) close();
    if (pending) void openIn(pending);
    else app.toast({ tone: "ok", text: `Files open in ${shown.find((e) => e.id === id)?.name ?? id}` });
  }
</script>

<svelte:window onkeydown={(e) => !inline && e.key === "Escape" && close()} />

{#snippet list()}
    {#if report?.chosen === "custom"}<p class="muted">Yours now: a command of your own, <code>[ui] editor</code> in your settings. Choosing one below replaces it.</p>{/if}
    {#if failed}
      <p class="bad"><Icon name="exclaim" size={14} /> {failed}</p>
    {:else if !report}
      <div class="center"><span class="spinner"></span></div>
    {:else if found.length}
      <ul>
        {#each found as e (e.id)}
          <li>
            <button class="pick" class:on={e.id === mine} aria-pressed={e.id === mine} disabled={saving} onclick={() => choose(e.id)}>
              <strong>{e.name}</strong>
              {#if e.id === mine}<span class="tag">{report.chosen ? "yours" : "from VISUAL"}</span>{:else if e.id === report.detected}<span class="tag">from VISUAL</span>{/if}
              <code>{e.found}</code>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">No editor found here. Install one — VS Code, Cursor, Zed, a JetBrains IDE — or name yours as <code>[ui] editor = ["my-editor", "{"{file}"}:{"{line}"}"]</code> in <code>~/.config/speccoder/config.toml</code>.</p>
    {/if}
    {#if report && shown.length > found.length}
      <p class="muted small">Also known: {shown.filter((e) => !e.found).map((e) => e.name).join(", ")}.</p>
    {/if}
{/snippet}

{#if inline}
  <div class="inline">{@render list()}</div>
{:else}

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="ce-title">
    <header>
      <span class="badge" aria-hidden="true"><Icon name="openin" size={20} weight={2.2} /></span>
      <div>
        <h2 id="ce-title">Choose Your Editor</h2>
        <p>Files and lines open there. Saved in your settings, so the terminal uses it too.</p>
      </div>
    </header>
    {@render list()}
    <div class="acts"><button class="btn" onclick={close}>Not Now</button></div>
  </div>
</div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(480px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.9rem; }
  header { display: flex; gap: 0.85rem; align-items: flex-start; }
  .badge { width: 40px; height: 40px; border-radius: 12px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent-ink); flex: none; }
  h2 { margin: 0; font: 650 1.12rem/1.3 var(--display); }
  header p { margin: 0.2rem 0 0; color: var(--text-2); font-size: 0.88rem; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.4rem; }
  .pick { width: 100%; display: grid; grid-template-columns: auto auto 1fr; align-items: baseline; gap: 0.5rem; text-align: left; border: 1px solid var(--border); background: var(--surface); color: var(--text); border-radius: var(--radius); padding: 0.6rem 0.8rem; cursor: pointer; font: inherit; }
  .pick:hover, .pick:focus-visible { border-color: var(--accent); background: var(--accent-soft); }
  .pick code { justify-self: end; color: var(--muted); font-size: 0.74rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .tag { font-size: 0.72rem; color: var(--accent-ink); }
  .bad { color: var(--danger); margin: 0; }
  .muted { color: var(--muted); margin: 0; font-size: 0.86rem; }
  .small { font-size: 0.78rem; }
  .center { display: grid; place-items: center; padding: 1rem; }
  .acts { display: flex; justify-content: flex-end; }
  .inline { display: grid; gap: 0.7rem; }
  .pick.on { border-color: var(--accent); box-shadow: inset 3px 0 0 var(--accent); }
</style>
