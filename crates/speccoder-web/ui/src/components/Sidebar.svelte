<script lang="ts">
  import { app } from "../lib/app.svelte";
  const features = $derived(app.project?.features?.features ?? []);
  const unit = $derived(app.project?.unit ?? "feature");
  const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  // A ring's arc for `done` of `all` on a 20-point ring.
  const C = 2 * Math.PI * 8;
  const arc = (done: number, all: number) => (all ? (done / all) * C : 0);
</script>

<aside class="glass" aria-label="{cap(unit)}s">
  <h2 class="section-title">{cap(unit)}s <span class="count">{features.length}</span></h2>
  {#if features.length === 0}
    <p class="none">No {unit}s yet. The next step writes the first one.</p>
  {/if}
  <ul>
    {#each features as f (f.name)}
      {@const all = f.tasks}
      <li>
        <button class:on={app.feature === f.name} onclick={() => app.selectFeature(f.name)} aria-current={app.feature === f.name ? "page" : undefined}>
          <svg class="mini" width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
            <circle cx="10" cy="10" r="8" fill="none" stroke="var(--fill-2)" stroke-width="3" />
            {#if all && f.done > 0}
              <circle cx="10" cy="10" r="8" fill="none" stroke={f.done === all ? "var(--verified-fill)" : "var(--accent)"} stroke-width="3" stroke-linecap="round" stroke-dasharray="{arc(f.done, all)} {C}" transform="rotate(-90 10 10)" />
            {/if}
          </svg>
          <span class="text">
            <span class="name">{app.titleOf(f.name)}</span>
            <span class="meta">
              <span class="mono">{String(f.number).padStart(3, "0")}</span>
              {#if all}· {f.done} of {all}{:else if f.artifacts.length}· {f.artifacts.join(", ")}{:else}· empty{/if}
            </span>
          </span>
          {#if f.current}<span class="cur" title="The current {unit}">●</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
  {#if app.project}
    <footer>
      <span>{app.project.format}</span>
      <span class="mono">v{app.project.version}</span>
    </footer>
  {/if}
</aside>

<style>
  aside {
    grid-area: side; display: flex; flex-direction: column; overflow: auto; padding: 1.1rem 0.6rem 0.75rem;
    border-right: 0.5px solid var(--separator); box-shadow: none;
  }
  .section-title { padding: 0 0.5rem; display: flex; justify-content: space-between; }
  .count { font-weight: 500; color: var(--faint); font-variant-numeric: tabular-nums; }
  .none { padding: 0 0.6rem; font-size: 0.86rem; color: var(--muted); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; }
  li button {
    width: 100%; display: flex; align-items: center; gap: 0.65rem; text-align: left;
    border: 0; background: none; padding: 0.5rem 0.55rem; border-radius: 8px; cursor: pointer;
    transition: background var(--speed) var(--ease);
  }
  li button:hover { background: var(--fill); }
  li button.on { background: var(--accent); color: var(--accent-text-on); }
  li button.on .meta, li button.on .cur { color: rgba(255, 255, 255, 0.78); }
  li button.on .mini circle:first-child { stroke: rgba(255, 255, 255, 0.3); }
  li button.on .mini circle + circle { stroke: #fff; }
  .mini { flex: none; }
  .mini circle + circle { transition: stroke-dasharray 600ms var(--ease); }
  .text { display: grid; min-width: 0; flex: 1; }
  .name { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name::first-letter { text-transform: uppercase; }
  .meta { font-size: 0.76rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .meta .mono { font-size: 0.95em; }
  .cur { color: var(--accent); font-size: 0.55rem; }
  footer { margin-top: auto; padding: 0.75rem 0.55rem 0; display: flex; justify-content: space-between; font-size: 0.74rem; color: var(--faint); }
  @media (max-width: 900px) { aside { display: none; } }
</style>
