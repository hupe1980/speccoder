<script lang="ts">
  // Words selected anywhere outside Ask: a small pill above them adds them
  // to Ask, with where they are from (⌘⇧L does the same from the keyboard).
  import { ask, pointed, selected } from "../lib/ask.svelte";
  import Icon from "./Icon.svelte";

  let at = $state<{ x: number; y: number } | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function place() {
    clearTimeout(timer);
    // After the selection settles, never while a drag is still extending it.
    timer = setTimeout(() => {
      const s = selected();
      at = s?.rect && s.rect.width ? { x: Math.min(window.innerWidth - 90, Math.max(8, s.rect.left + s.rect.width / 2 - 36)), y: Math.max(8, s.rect.top - 38) } : null;
    }, 180);
  }

  function add(e: MouseEvent) {
    // The mouse acts on mousedown (keeping the selection); Enter or Space on
    // the focused pill is a click with no pointer (`detail` 0).
    if (e.type === "click" && e.detail !== 0) return;
    e.preventDefault();
    const chip = pointed();
    at = null;
    if (!chip) return;
    window.getSelection()?.removeAllRanges();
    ask.add(chip);
  }
</script>

<svelte:document onselectionchange={place} />
<svelte:window onscroll={() => (at = null)} onresize={() => (at = null)} />

{#if at}
  <!-- mousedown keeps the selection the click is about -->
  <button class="pill" style="left:{at.x}px; top:{at.y}px" title="Add the selection to Ask (⌘⇧L)" aria-label="Ask about the selection" onmousedown={add} onclick={add}>
    <Icon name="bubble" size={12} weight={2.2} /> Ask <kbd>⌘⇧L</kbd>
  </button>
{/if}

<style>
  .pill {
    position: fixed; z-index: 30; display: inline-flex; align-items: center; gap: 0.35rem;
    padding: 0.32rem 0.6rem 0.32rem 0.5rem; border: 0; border-radius: 999px; cursor: pointer;
    font: 600 0.76rem/1 var(--rounded, inherit); color: #fff;
    background: linear-gradient(160deg, #bf5af2, #5e5ce6); box-shadow: var(--shadow-lg);
    animation: pop 180ms var(--spring, ease-out);
  }
  .pill:hover { filter: brightness(1.08); }
  kbd { font: 600 0.64rem/1 var(--rounded, inherit); opacity: 0.75; }
  @keyframes pop { from { opacity: 0; transform: translateY(4px) scale(0.96); } }
  @media (prefers-reduced-motion: reduce) { .pill { animation: none; } }
</style>
