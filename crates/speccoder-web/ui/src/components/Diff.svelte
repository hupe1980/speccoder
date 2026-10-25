<script lang="ts">
  let { patch }: { patch: string } = $props();
  interface File { name: string; lines: { kind: string; text: string }[] }
  const files = $derived.by(() => {
    const out: File[] = [];
    let cur: File | null = null;
    for (const line of patch.split("\n")) {
      if (line.startsWith("diff --git ")) {
        cur = { name: line.split(" b/").pop() ?? line, lines: [] };
        out.push(cur);
      } else if (cur && !line.startsWith("index ") && !line.startsWith("--- ") && !line.startsWith("+++ ")) {
        const kind = line.startsWith("@@") ? "hunk" : line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
        cur.lines.push({ kind, text: line });
      }
    }
    return out;
  });
</script>

{#if files.length === 0}
  <p class="muted">No change to show.</p>
{/if}
{#each files as f (f.name)}
  <details open class="file">
    <summary class="mono">{f.name}<span class="muted"> +{f.lines.filter((l) => l.kind === "add").length} −{f.lines.filter((l) => l.kind === "del").length}</span></summary>
    <pre>{#each f.lines as l}<span class={l.kind}>{l.text}
</span>{/each}</pre>
  </details>
{/each}

<style>
  .file { border: 1px solid var(--border); border-radius: 8px; margin-bottom: 0.75rem; overflow: hidden; background: var(--surface); }
  summary { padding: 0.45rem 0.7rem; cursor: pointer; background: var(--surface-2); font-size: 0.85rem; }
  pre { margin: 0; padding: 0.4rem 0; overflow-x: auto; font-size: 0.82rem; line-height: 1.45; }
  pre span { display: block; padding: 0 0.8rem; white-space: pre; }
  .add { background: var(--add-bg); color: var(--add); }
  .del { background: var(--del-bg); color: var(--del); }
  .hunk { color: var(--accent); background: var(--accent-soft); }
</style>
