<script lang="ts">
  // Open in your editor (spec 128): a small button beside a path, a hunk or
  // a line that hands the person to their own editor at that line. Hidden
  // at phone width, where an editor on the host's desktop is not wanted.
  import Icon from "./Icon.svelte";
  import { openIn } from "../lib/editor.svelte";

  let { path, line = null, column = null, feature, label = null }: {
    path: string;
    line?: number | null;
    column?: number | null;
    feature?: string | null;
    label?: string | null;
  } = $props();
  const where = $derived(`${path}${line ? `:${line}` : ""}`);
</script>

<button
  type="button"
  class="open-in"
  class:labelled={!!label}
  title={`Open ${where} in your editor`}
  aria-label={`Open ${where} in your editor`}
  onclick={(e) => {
    e.stopPropagation();
    void openIn({ path, line, column, feature });
  }}
>
  <Icon name="openin" size={12} weight={2} />{#if label}<span>{label}</span>{/if}
</button>

<style>
  .open-in { display: inline-flex; align-items: center; gap: 0.3rem; border: 0; background: none; color: var(--muted); padding: 0.15rem 0.3rem; border-radius: var(--radius-xs); cursor: pointer; font: inherit; font-size: 0.78rem; line-height: 1; vertical-align: middle; }
  .open-in:hover, .open-in:focus-visible { color: var(--accent-ink); background: var(--accent-soft); }
  .labelled { color: var(--accent-ink); }
  @media (max-width: 600px) { .open-in { display: none; } }
</style>
