<script lang="ts">
  // Every action, after Spotlight: one field, results grouped, the
  // selection in the accent, the command it runs shown below. Enter runs
  // it (or copies a command that would leave the machine).
  import { app } from "../lib/app.svelte";
  import { key as keyOf, offer, type Offer } from "../lib/actions";
  import type { Action } from "../lib/types";
  import Icon from "./Icon.svelte";
  let filter = $state("");
  let at = $state(0);
  const groups: Record<string, string> = { page: "View", next: "Up Next", plan: "Plan", work: "Work", task: "Tasks", feature: "Features", review: "Review" };
  const icons: Record<string, string> = { next: "sparkle", plan: "doc", work: "play", task: "list", feature: "grid", review: "chart" };
  // The page's own commands, as the menu bar has them (by their menu id).
  type Entry = (Action | Offer) & { menu?: string };
  const page: Entry[] = ([
    ["theme.light", "appearance: light"],
    ["theme.dark", "appearance: dark"],
    ["theme.auto", "appearance: automatic", "follow the system"],
    ["view.sidebar", "toggle the sidebar"],
    ["view.statusbar", "toggle the status bar"],
  ] as [string, string, string?][]).map(([id, label, why]) => ({ ...offer(null, { group: "page", label, why: why ?? "", command: "" }), menu: id }));
  const isMenu = (a?: Entry) => !!a?.menu;
  const matched = $derived(
    ([...app.actions, ...page] as Entry[]).filter((a) => {
      const f = filter.toLowerCase().trim();
      // The page's commands only when asked for: they would crowd the work.
      if (isMenu(a)) return f.length >= 2 && a.label.includes(f);
      return !f || a.label.toLowerCase().includes(f) || a.command.toLowerCase().includes(f) || a.why.toLowerCase().includes(f);
    }),
  );
  const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  // Typed words are also a change to make: offered last, as Spotlight
  // offers a search of the web.
  const quick = $derived.by((): Entry | null => {
    const f = filter.trim();
    if (f.length < 6) return null;
    return offer(null, { group: "work", label: `quick change: "${f}"`, why: "recorded as a story, a requirement and a task; judged on evidence", command: `speccoder do "${f.replaceAll('"', '\\"')}"` });
  });
  const items = $derived(quick ? [...matched, quick] : matched);
  const pick = (a?: Entry) => {
    if (!a || a.blocked) return;
    if (isMenu(a)) {
      app.palette = false;
      window.dispatchEvent(new CustomEvent("speccoder:menu", { detail: a.menu }));
      return;
    }
    if (a === quick) {
      app.quickDraft = filter.trim();
      app.palette = false;
      app.quick = true;
      return;
    }
    void app.perform(a);
  };
  let list: HTMLElement | undefined = $state();
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") { at = Math.min(at + 1, items.length - 1); e.preventDefault(); }
    else if (e.key === "ArrowUp") { at = Math.max(at - 1, 0); e.preventDefault(); }
    else if (e.key === "Enter") { pick(items[at]); e.preventDefault(); }
    else if (e.key === "Escape") app.palette = false;
  }
  $effect(() => { void filter; at = 0; });
  $effect(() => { list?.querySelector(`[data-i="${at}"]`)?.scrollIntoView({ block: "nearest" }); });
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.palette = false)}>
  <div class="spotlight glass" role="dialog" aria-modal="true" aria-label="Actions">
    <label class="field">
      <Icon name="search" size={20} />
      <!-- svelte-ignore a11y_autofocus -->
      <input autofocus placeholder="What would you like to do?" bind:value={filter} onkeydown={key} aria-controls="pal-list" aria-activedescendant="pal-{at}" />
    </label>
    <ul id="pal-list" role="listbox" bind:this={list}>
      {#each items as a, i (keyOf(a) + (a.menu ?? ""))}
        {#if i === 0 || items[i - 1]?.group !== a.group}<li class="group" role="presentation">{groups[a.group] ?? cap(a.group)}</li>{/if}
        <li role="option" id="pal-{i}" aria-selected={i === at} aria-disabled={!!a.blocked} data-i={i}>
          <button class:on={i === at} disabled={!!a.blocked} tabindex="-1" onmouseenter={() => (at = i)} onclick={() => pick(a)}>
            <span class="ic"><Icon name={isMenu(a) ? "halfcircle" : a.run ? (icons[a.group] ?? "play") : "terminal"} size={14} weight={2} /></span>
            <span class="label">{cap(a.label)}{#if a.input}…{/if}</span>
            <span class="why">{a.blocked ?? a.why}</span>
            <span class="tag">{isMenu(a) ? "Apply" : a.run ? (a.blocked ? "Blocked" : "Run") : "Copy"}</span>
          </button>
        </li>
      {:else}<li class="none">No action matches “{filter}”.</li>{/each}
    </ul>
    <footer>
      <span class="mono cmd"><Icon name="terminal" size={12} />{isMenu(items[at]) ? "" : (items[at]?.command ?? "")}</span>
      <span class="keys"><kbd>↑</kbd><kbd>↓</kbd> move <kbd>↵</kbd> run <kbd>esc</kbd> close</span>
    </footer>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); z-index: 30; display: grid; justify-items: center; align-items: start; padding-top: 14vh; animation: fade 160ms var(--ease); }
  @keyframes fade { from { opacity: 0; } }
  .spotlight {
    width: min(680px, 94vw); border-radius: 22px; overflow: hidden; background: var(--glass-strong);
    box-shadow: var(--shadow-lg), var(--glass-edge); animation: pop 360ms var(--spring);
  }
  @keyframes pop { from { transform: scale(0.96) translateY(-6px); opacity: 0; } }
  .field { display: flex; align-items: center; gap: 0.7rem; padding: 0 1.2rem; height: 58px; color: var(--muted); border-bottom: 0.5px solid var(--separator); }
  input { flex: 1; border: 0; background: transparent; outline: none; font: 400 1.3rem/1 var(--display); letter-spacing: -0.015em; color: var(--text); }
  input::placeholder { color: var(--faint); }
  ul { list-style: none; margin: 0; padding: 0.4rem; max-height: 50vh; overflow: auto; }
  .group { font-size: 0.74rem; color: var(--muted); font-weight: 650; padding: 0.65rem 0.75rem 0.3rem; }
  button {
    width: 100%; display: grid; grid-template-columns: auto auto 1fr auto; gap: 0.7rem; align-items: center; text-align: left;
    border: 0; background: none; padding: 0.45rem 0.6rem; border-radius: 10px; cursor: pointer; min-height: 38px;
  }
  button.on { background: var(--accent); color: #fff; }
  button.on .why, button.on .tag { color: rgba(255, 255, 255, 0.8); }
  button.on .ic { background: rgba(255, 255, 255, 0.22); color: #fff; }
  button[disabled] { opacity: 0.45; cursor: default; }
  .ic { width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; background: var(--fill); color: var(--text-2); }
  .label { font-weight: 550; white-space: nowrap; }
  .why { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.86rem; color: var(--muted); }
  .tag { font-size: 0.72rem; color: var(--muted); font-weight: 600; }
  footer { display: flex; justify-content: space-between; gap: 1rem; align-items: center; border-top: 0.5px solid var(--separator); padding: 0.55rem 1.1rem; font-size: 0.76rem; color: var(--muted); }
  .cmd { display: inline-flex; align-items: center; gap: 0.4rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .keys { display: inline-flex; align-items: center; gap: 0.25rem; white-space: nowrap; }
  .none { padding: 1.25rem; color: var(--muted); text-align: center; }
  @media (max-width: 640px) { .keys { display: none; } }
</style>
