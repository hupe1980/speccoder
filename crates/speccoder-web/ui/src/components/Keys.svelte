<script lang="ts">
  // Every key the page answers, in one place (`?`); each has a button or a
  // menu item too — none is the only way to do something.
  import { app } from "../lib/app.svelte";
  import { trap } from "../lib/sheet";
  const rows: [string, string, string][] = [
    ["Anywhere", "N", "the composer: a change, a bug, a feature (⌘N in the window)"],
    ["Anywhere", "⌘K", "every action"],
    ["Anywhere", "q · ⇧B", "quick change · report a bug"],
    ["Anywhere", "1–6 · ,", "the views · settings"],
    ["Anywhere", "n", "the next step"],
    ["Composer", "⏎ · ↑ ↓", "take the selected rung · move"],
    ["Composer", "⌥⏎ ⇧⏎ ⌃⏎ ⌘⏎", "quick change · feature · bug fix · ask"],
    ["Sheets", "⌘⏎ · Esc", "the primary button (Start, Create, Approve and plan) · cancel"],
    ["Welcome", "N · ⌘O", "New Project · Open Project"],
    ["Sample", "Space · → · End", "play or pause · one step · skip to the end"],
    ["Window", "⌘⇧N · ⌘⇧B · ⌘.", "quick change · bug · stop"],
  ];
  function key(e: KeyboardEvent) {
    if (e.key === "Escape" || e.key === "?") {
      e.preventDefault();
      app.keys = false;
    }
  }
</script>

<div class="d-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.keys = false)} onkeydown={key}>
  <div class="d-sheet" role="dialog" aria-modal="true" aria-labelledby="keys-title" use:trap={"button"}>
    <header class="d-head"><div><h2 id="keys-title">Keys</h2><p>Every key has a button or a menu item too.</p></div></header>
    <table>
      <thead><tr><th scope="col">Where</th><th scope="col">Key</th><th scope="col">Does</th></tr></thead>
      <tbody>
        {#each rows as [where, k, does] (where + k)}
          <tr><td class="muted">{where}</td><td><kbd>{k}</kbd></td><td>{does}</td></tr>
        {/each}
      </tbody>
    </table>
    <div class="d-row"><button class="btn primary" onclick={() => (app.keys = false)}>Done</button></div>
  </div>
</div>

<style>
  table { border-collapse: collapse; font-size: 0.88rem; }
  th { text-align: left; color: var(--muted); font-weight: 600; padding: 0.25rem 0.5rem; }
  td { padding: 0.3rem 0.5rem; border-top: 0.5px solid var(--separator); }
</style>
