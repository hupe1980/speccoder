<script lang="ts">
  import { app } from "../lib/app.svelte";
  import type { Action } from "../lib/types";
  let filter = $state("");
  let at = $state(0);
  const groups: Record<string, string> = { next: "Next", plan: "Plan", work: "Work", task: "Task", feature: "Feature", review: "Review" };
  const items = $derived(
    app.actions.filter((a) => {
      const f = filter.toLowerCase();
      return !f || a.label.toLowerCase().includes(f) || a.command.toLowerCase().includes(f) || a.why.toLowerCase().includes(f);
    }),
  );
  const pick = (a?: Action) => a && app.perform(a);
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") { at = Math.min(at + 1, items.length - 1); e.preventDefault(); }
    else if (e.key === "ArrowUp") { at = Math.max(at - 1, 0); e.preventDefault(); }
    else if (e.key === "Enter") { pick(items[at]); e.preventDefault(); }
    else if (e.key === "Escape") app.palette = false;
  }
  $effect(() => { void filter; at = 0; });
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.palette = false)}>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Actions">
    <!-- svelte-ignore a11y_autofocus -->
    <input autofocus placeholder="What do you want to do?" bind:value={filter} onkeydown={key} aria-controls="pal-list" />
    <ul id="pal-list" role="listbox">
      {#each items as a, i (a.group + a.label)}
        {#if i === 0 || items[i - 1]?.group !== a.group}<li class="group" role="presentation">{groups[a.group] ?? a.group}</li>{/if}
        <li role="option" aria-selected={i === at}>
          <button class:on={i === at} disabled={!!a.blocked} onmouseenter={() => (at = i)} onclick={() => pick(a)}>
            <span class="label">{a.label}{#if a.input}…{/if}</span>
            <span class="why muted">{a.blocked ?? a.why}</span>
            <span class="tag">{a.run ? (a.blocked ? "blocked" : "run") : "copy"}</span>
          </button>
        </li>
      {:else}<li class="muted none">Nothing matches.</li>{/each}
    </ul>
    <footer class="mono muted">{items[at]?.command ?? ""}</footer>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(10, 12, 18, 0.35); z-index: 30; display: grid; justify-items: center; align-items: start; padding-top: 12vh; }
  .palette { width: min(680px, 94vw); background: var(--surface); border-radius: 14px; box-shadow: var(--shadow-lg); overflow: hidden; border: 1px solid var(--border); }
  input { width: 100%; border: 0; border-bottom: 1px solid var(--border); padding: 1rem 1.2rem; font-size: 1.05rem; background: transparent; outline: none; }
  ul { list-style: none; margin: 0; padding: 0.4rem; max-height: 52vh; overflow: auto; }
  .group { font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); font-weight: 700; padding: 0.6rem 0.7rem 0.25rem; }
  button { width: 100%; display: grid; grid-template-columns: auto 1fr auto; gap: 0.9rem; align-items: center; text-align: left; border: 0; background: none; padding: 0.5rem 0.7rem; border-radius: 8px; cursor: pointer; }
  button.on { background: var(--accent-soft); }
  button[disabled] { opacity: 0.55; cursor: not-allowed; }
  .label { font-weight: 600; white-space: nowrap; }
  .why { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.88rem; }
  .tag { font-size: 0.72rem; color: var(--muted); border: 1px solid var(--border); border-radius: 999px; padding: 0 0.5rem; }
  footer { border-top: 1px solid var(--border); padding: 0.55rem 1.2rem; font-size: 0.8rem; min-height: 2.2rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .none { padding: 1rem; }
</style>
