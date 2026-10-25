<script lang="ts">
  import { app } from "../lib/app.svelte";
  const features = $derived(app.project?.features?.features ?? []);
  const unit = $derived(app.project?.unit ?? "feature");
</script>

<aside aria-label="{unit}s">
  <div class="head">
    <span class="label">{unit}s</span>
    <span class="muted">{features.length}</span>
  </div>
  {#if features.length === 0}
    <p class="muted none">No {unit}s yet. <br />The next step on the right writes the first one.</p>
  {/if}
  <ul>
    {#each features as f (f.name)}
      {@const pct = f.tasks ? Math.round((f.done / f.tasks) * 100) : 0}
      <li>
        <button class:on={app.feature === f.name} onclick={() => app.selectFeature(f.name)} aria-current={app.feature === f.name}>
          <span class="name"><span class="num mono">{String(f.number).padStart(2, "0")}</span>{f.name.replace(/^\d+-/, "")}</span>
          <span class="bar" aria-hidden="true"><span style="width:{pct}%"></span></span>
          <span class="meta muted">
            {#if f.tasks}{f.done}/{f.tasks} tasks{:else}{f.artifacts.join(" · ") || "empty"}{/if}
            {#if f.current}<span class="cur">current</span>{/if}
          </span>
        </button>
      </li>
    {/each}
  </ul>
  {#if app.project}
    <footer class="muted">
      <span>{app.project.format}</span>
      <span class="mono">v{app.project.version}</span>
    </footer>
  {/if}
</aside>

<style>
  aside { grid-area: side; border-right: 1px solid var(--border); background: var(--surface); display: flex; flex-direction: column; overflow: auto; }
  .head { display: flex; justify-content: space-between; padding: 1rem 1rem 0.5rem; }
  .label { text-transform: uppercase; font-size: 0.72rem; letter-spacing: 0.08em; color: var(--muted); font-weight: 700; }
  .none { padding: 0 1rem; font-size: 0.9rem; }
  ul { list-style: none; margin: 0; padding: 0 0.5rem; display: grid; gap: 2px; }
  li button {
    width: 100%; text-align: left; border: 0; background: none; padding: 0.6rem 0.65rem; border-radius: 8px;
    display: grid; gap: 0.35rem; cursor: pointer; transition: background var(--speed);
  }
  li button:hover { background: var(--surface-2); }
  li button.on { background: var(--accent-soft); }
  .name { display: flex; gap: 0.5rem; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .num { color: var(--muted); font-weight: 500; }
  .bar { height: 4px; border-radius: 4px; background: var(--surface-3); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--verified); border-radius: 4px; transition: width 400ms; }
  .meta { font-size: 0.8rem; display: flex; justify-content: space-between; }
  .cur { color: var(--accent); font-weight: 600; }
  footer { margin-top: auto; padding: 1rem; display: flex; justify-content: space-between; font-size: 0.78rem; }
</style>
