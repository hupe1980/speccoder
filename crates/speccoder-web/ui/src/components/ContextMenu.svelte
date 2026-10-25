<script lang="ts">
  // A context menu, as the system draws one: a material panel at
  // the pointer, kept on screen, with the item's own commands — each of
  // which also lives elsewhere (the inspector, the menu bar), as the HIG
  // asks. ↑ ↓ move, Return chooses, Esc closes.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  const m = $derived(app.context!);
  let panel = $state<HTMLElement>();
  let at = $state(0);
  const pos = $derived.by(() => {
    const w = panel?.offsetWidth ?? 240;
    const h = panel?.offsetHeight ?? 200;
    return { x: Math.min(m.x, innerWidth - w - 8), y: Math.min(m.y, innerHeight - h - 8) };
  });
  const enabled = $derived(m.items.map((i, n) => (i.disabled ? -1 : n)).filter((n) => n >= 0));
  $effect(() => {
    at = enabled[0] ?? 0;
    panel?.focus();
  });
  function choose(n: number) {
    const i = m.items[n];
    if (!i || i.disabled) return;
    app.context = null;
    i.act();
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") app.context = null;
    else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const k = enabled.indexOf(at);
      at = enabled[(k + (e.key === "ArrowDown" ? 1 : enabled.length - 1)) % enabled.length] ?? at;
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(at);
    }
  }
</script>

<svelte:window onblur={() => (app.context = null)} onresize={() => (app.context = null)} />

<div class="veil" role="presentation" onclick={() => (app.context = null)} oncontextmenu={(e) => { e.preventDefault(); app.context = null; }}></div>
<div bind:this={panel} class="menu glass" role="menu" tabindex="-1" aria-label={m.title} style="left:{pos.x}px; top:{pos.y}px" onkeydown={key}>
  <div class="title">{m.title}</div>
  {#each m.items as i, n (i.label)}
    <button role="menuitem" class:on={n === at} disabled={!!i.disabled} title={i.disabled ?? i.hint ?? ""} onmouseenter={() => !i.disabled && (at = n)} onclick={() => choose(n)}>
      <span class="ic">{#if i.icon}<Icon name={i.icon} size={13} weight={2} />{/if}</span>{i.label}
    </button>
  {/each}
</div>

<style>
  .veil { position: fixed; inset: 0; z-index: 40; }
  .menu {
    position: fixed; z-index: 41; min-width: 220px; padding: 5px; border-radius: 10px; outline: none;
    background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge);
    display: grid; animation: pop 120ms var(--ease);
  }
  @keyframes pop { from { opacity: 0; transform: scale(0.97); } }
  @media (prefers-reduced-motion: reduce) { .menu { animation: none; } }
  .title { padding: 0.25rem 0.6rem 0.3rem; font-size: 0.72rem; font-weight: 650; color: var(--muted); }
  button {
    display: flex; align-items: center; gap: 0.45rem; height: 24px; padding: 0 0.6rem; border: 0; border-radius: 5px;
    background: none; color: var(--text); font: inherit; font-size: 0.86rem; text-align: left; cursor: default;
  }
  button.on { background: var(--accent); color: #fff; }
  button:disabled { color: var(--faint); }
  .ic { width: 14px; display: grid; place-items: center; }
</style>
