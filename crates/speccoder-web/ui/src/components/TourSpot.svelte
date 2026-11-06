<script lang="ts">
  // One stop of the sample's tour: a ring around the part of the screen it
  // is about, the rest dimmed, and a caption beside it — what this is and
  // why it matters. It only shows; the playback decides what comes next.
  import Code from "./Code.svelte";

  let {
    anchor,
    title,
    text,
    n,
    of,
    playing,
    onprev,
    onnext,
    ontoggle,
    onend,
  }: {
    anchor: string;
    title: string;
    text: string;
    n: number;
    of: number;
    playing: boolean;
    onprev: () => void;
    onnext: () => void;
    ontoggle: () => void;
    onend: () => void;
  } = $props();

  let rect = $state<DOMRect | null>(null);
  const PAD = 6;

  // Where the anchor is now; it moves with scrolling, resizing and layout.
  function measure() {
    const el = document.querySelector(anchor);
    rect = el ? el.getBoundingClientRect() : null;
  }
  $effect(() => {
    void anchor;
    const el = document.querySelector(anchor);
    el?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    measure();
    const t = setInterval(measure, 250);
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      clearInterval(t);
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  });

  const tall = () => (typeof window === "undefined" ? 800 : window.innerHeight);
  // The caption below the anchor when there is room, else above it; a part
  // that fills most of the screen gets it at the bottom, centred.
  const big = $derived(!!rect && rect.height > tall() * 0.5);
  const below = $derived(!rect || rect.bottom + 190 < tall());
</script>

{#if rect}
  <div
    class="ring"
    aria-hidden="true"
    style:left="{rect.left - PAD}px"
    style:top="{rect.top - PAD}px"
    style:width="{rect.width + PAD * 2}px"
    style:height="{rect.height + PAD * 2}px"
  ></div>
{/if}
<div
  class="caption glass"
  role="dialog"
  aria-label="Tour, stop {n} of {of}"
  style:left={rect ? `${Math.max(12, Math.min(rect.left, (typeof window === "undefined" ? 1200 : window.innerWidth) - 412))}px` : undefined}
  style:top={rect && !big && below ? `${rect.bottom + PAD + 12}px` : undefined}
  style:bottom={rect && !big && !below ? `${tall() - rect.top + PAD + 12}px` : undefined}
>
  <p class="kicker">Tour · {n} of {of}</p>
  <h3>{title}</h3>
  <p class="text" aria-live="polite"><Code {text} /></p>
  <div class="ctl">
    <button class="btn small ghost" onclick={onprev} disabled={n <= 1} aria-label="Previous stop" title="Previous (←)">←</button>
    <button class="btn small" onclick={ontoggle} aria-label={playing ? "Pause the tour" : "Play the tour"} title="{playing ? 'Pause' : 'Play'} (Space)">{playing ? "Pause" : "Play"}</button>
    <button class="btn small primary" onclick={onnext} aria-label="Next stop" title="Next (→)">{n < of ? "Next →" : "Finish"}</button>
    <button class="btn small ghost end" onclick={onend} title="End the tour (Esc)">End tour</button>
  </div>
</div>

<style>
  .ring { position: fixed; z-index: 40; border-radius: 12px; pointer-events: none; box-shadow: 0 0 0 3px var(--accent), 0 0 0 100vmax rgba(0, 0, 0, 0.38); transition: all 260ms var(--spring, ease); }
  .caption { position: fixed; z-index: 41; width: min(400px, calc(100vw - 24px)); padding: 0.85rem 1rem; border-radius: var(--radius-lg, 14px); box-shadow: var(--shadow-lg); display: grid; gap: 0.35rem; animation: rise 260ms var(--spring, ease); }
  .caption:not([style*="top"]):not([style*="bottom"]) { left: 50% !important; bottom: 1.5rem; transform: translateX(-50%); }
  .kicker { margin: 0; font-size: 0.72rem; letter-spacing: 0.04em; text-transform: uppercase; color: var(--accent-ink); font-weight: 600; }
  h3 { margin: 0; font-size: 1.02rem; }
  .text { margin: 0; color: var(--text-2); font-size: 0.88rem; line-height: 1.45; }
  .ctl { display: flex; gap: 0.35rem; align-items: center; margin-top: 0.3rem; }
  .ctl .end { margin-left: auto; }
  @keyframes rise { from { transform: translateY(6px); opacity: 0; } }
  @media (max-width: 640px) {
    .caption, .caption:not([style*="top"]):not([style*="bottom"]) { left: 12px !important; right: 12px; top: auto !important; bottom: 12px !important; width: auto; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring, .caption { transition: none; animation: none; }
  }
</style>
