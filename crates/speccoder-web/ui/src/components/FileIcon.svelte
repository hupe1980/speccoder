<script lang="ts">
  // A file's type at a glance: a tile in its language's colour with a
  // short label, as editors mark them. Decorative: the name says the rest.
  import { fileType } from "../lib/filetypes";
  let { path, size = 16 }: { path: string; size?: number } = $props();
  const t = $derived(fileType(path));
  const fit = $derived(t.label.length >= 3 ? 6.2 : t.label.length === 2 ? 7.6 : 9.5);
  // Light tiles (JavaScript's yellow) take dark letters: readable either way.
  const ink = $derived.by(() => {
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(t.color.slice(i, i + 2), 16) / 255);
    return 0.2126 * r! + 0.7152 * g! + 0.0722 * b! > 0.62 ? "#1d1d1f" : "#fff";
  });
</script>

<svg class="fi" width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" style="--c:{t.color}">
  <title>{t.name}</title>
  {#if t.label}
    <rect x="0.5" y="0.5" width="15" height="15" rx="3.6" class="tile" />
    <text x="8" y="8.4" text-anchor="middle" dominant-baseline="middle" style="font-size:{fit}px; fill:{ink}">{t.label}</text>
  {:else}
    <path class="page" d="M3.5 1.5h6l3 3V14a.5.5 0 0 1-.5.5h-8.5a.5.5 0 0 1-.5-.5V2a.5.5 0 0 1 .5-.5z" />
    <path class="page" d="M9.5 1.5v3h3" />
  {/if}
</svg>

<style>
  .fi { flex: none; display: block; }
  .tile { fill: var(--c); }
  text { font-family: var(--rounded, ui-rounded, -apple-system, system-ui, sans-serif); font-weight: 800; letter-spacing: -0.3px; }
  .page { fill: none; stroke: var(--muted, #8e8e93); stroke-width: 1.2; stroke-linejoin: round; }
</style>
