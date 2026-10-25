<script lang="ts">
  // Text with `code` spans, as task titles and reasons are written.
  let { text }: { text: string } = $props();
  const parts = $derived(text.split(/(`[^`]+`)/g).filter(Boolean));
</script>

{#each parts as p}{#if p.startsWith("`") && p.endsWith("`")}<code>{p.slice(1, -1)}</code>{:else}{p}{/if}{/each}

<style>
  code { background: var(--surface-2); border: 1px solid var(--border); padding: 0 0.3em; border-radius: 4px; font-size: 0.86em; }
</style>
