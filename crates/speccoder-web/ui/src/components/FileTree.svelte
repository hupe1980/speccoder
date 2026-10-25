<script lang="ts" module>
  export interface Node {
    name: string;
    path: string;
    dirs: Node[];
    files: { name: string; path: string }[];
  }

  /** Paths into a tree of folders; one-child folder chains are joined
   *  (`src/models`), as a navigator shows them. */
  export function build(paths: string[]): Node {
    const root: Node = { name: "", path: "", dirs: [], files: [] };
    for (const p of paths) {
      const parts = p.split("/");
      let at = root;
      for (let i = 0; i < parts.length - 1; i++) {
        const path = parts.slice(0, i + 1).join("/");
        let next = at.dirs.find((d) => d.path === path);
        if (!next) {
          next = { name: parts[i]!, path, dirs: [], files: [] };
          at.dirs.push(next);
        }
        at = next;
      }
      at.files.push({ name: parts[parts.length - 1]!, path: p });
    }
    const tidy = (n: Node): Node => {
      n.dirs = n.dirs.map(tidy).sort((a, b) => a.name.localeCompare(b.name));
      n.files.sort((a, b) => a.name.localeCompare(b.name));
      // A folder holding one folder and nothing else reads as one.
      if (n.path && n.files.length === 0 && n.dirs.length === 1) {
        const only = n.dirs[0]!;
        return { ...only, name: `${n.name}/${only.name}` };
      }
      return n;
    };
    return tidy(root);
  }
</script>

<script lang="ts">
  // A navigator: folders first, collapsible, files beneath; a dot on what
  // the feature changed, counts when given; the selection in the accent.
  import Icon from "./Icon.svelte";

  let {
    paths,
    selected = null,
    changed = new Set<string>(),
    counts = {},
    onpick,
  }: {
    paths: string[];
    selected?: string | null;
    changed?: Set<string>;
    counts?: Record<string, { add: number | null; del: number | null }>;
    onpick: (path: string) => void;
  } = $props();

  const tree = $derived(build(paths));
  // Folders on the way to a change start open; the rest closed (all open for
  // a small tree).
  let open = $state<Record<string, boolean>>({});
  const touched = $derived.by(() => {
    const s = new Set<string>();
    for (const c of changed) {
      const parts = c.split("/");
      for (let i = 1; i < parts.length; i++) s.add(parts.slice(0, i).join("/"));
    }
    return s;
  });
  const isOpen = (n: Node) => open[n.path] ?? (paths.length <= 40 || touched.has(n.path));
  const holdsChange = (n: Node) => [...changed].some((c) => c.startsWith(n.path + "/"));
  const ext = (name: string) => name.split(".").pop()?.toLowerCase() ?? "";
  const tint: Record<string, string> = { rs: "#ff6b35", ts: "#3178c6", js: "#e5b800", svelte: "#ff3e00", py: "#3572a5", go: "#00add8", md: "#8e8e93", toml: "#9c6b4e", json: "#8e8e93", yml: "#cb171e", yaml: "#cb171e", lock: "#8e8e93" };
</script>

{#snippet folder(n: Node, depth: number)}
  {#each n.dirs as d (d.path)}
    {@const o = isOpen(d)}
    <li>
      <button class="row dir" style="--d:{depth}" aria-expanded={o} onclick={() => (open = { ...open, [d.path]: !o })}>
        <span class="chev" class:o><Icon name="chevron" size={10} weight={2.6} /></span>
        <svg class="folder" width="16" height="14" viewBox="0 0 16 14" aria-hidden="true"><path d="M1 3.2C1 2 1.9 1 3 1h3.1c.5 0 .9.2 1.2.6L8.2 3H13c1.1 0 2 .9 2 2v6c0 1.1-.9 2-2 2H3c-1.1 0-2-.9-2-2z" fill="currentColor" /></svg>
        <span class="name">{d.name}</span>
        {#if holdsChange(d)}<span class="dot" title="Holds changes"></span>{/if}
      </button>
      {#if o}<ul role="group">{@render folder(d, depth + 1)}</ul>{/if}
    </li>
  {/each}
  {#each n.files as f (f.path)}
    {@const c = counts[f.path]}
    <li>
      <button class="row file" class:on={selected === f.path} style="--d:{depth}; --t:{tint[ext(f.name)] ?? 'var(--muted)'}" title={f.path} onclick={() => onpick(f.path)} aria-current={selected === f.path ? "true" : undefined}>
        <span class="chev"></span>
        <svg class="doc" width="12" height="15" viewBox="0 0 12 15" aria-hidden="true"><path d="M1.5 1h6l3.5 3.5V14h-9.5z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /><path d="M7.5 1v3.5H11" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /></svg>
        <span class="name mono">{f.name}</span>
        {#if c}
          <span class="n a">{c.add == null ? "bin" : `+${c.add}`}</span>{#if c.del}<span class="n d">−{c.del}</span>{/if}
        {:else if changed.has(f.path)}<span class="dot" title="Changed by this feature"></span>{/if}
      </button>
    </li>
  {/each}
{/snippet}

<ul class="tree" role="tree">{@render folder(tree, 0)}</ul>

<style>
  ul { list-style: none; margin: 0; padding: 0; }
  .row {
    width: 100%; display: flex; align-items: center; gap: 0.35rem; border: 0; background: none; cursor: pointer; text-align: left;
    height: 26px; padding: 0 0.5rem 0 calc(0.35rem + var(--d) * 0.95rem); border-radius: 6px; font-size: 0.84rem; color: var(--text);
  }
  .row:hover { background: var(--fill); }
  .row:focus-visible { box-shadow: inset 0 0 0 2px var(--focus); }
  .row.on { background: var(--accent); color: #fff; }
  .row.on .doc, .row.on .n { color: rgba(255, 255, 255, 0.85); }
  .chev { width: 12px; display: grid; place-items: center; color: var(--muted); transition: transform 180ms var(--ease); flex: none; }
  .chev.o { transform: rotate(90deg); }
  .folder { color: #5aa7f5; flex: none; }
  .doc { color: var(--t); flex: none; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file .name { font-size: 0.8rem; }
  .dir .name { font-weight: 500; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--attention-fill); flex: none; }
  .n { font-size: 0.72rem; font-weight: 600; font-variant-numeric: tabular-nums; }
  .a { color: var(--add); }
  .d { color: var(--del); }
</style>
