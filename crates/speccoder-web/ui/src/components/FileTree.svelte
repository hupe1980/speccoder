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
  // A navigator, after the W3C tree pattern: folders first and collapsible,
  // one row in the tab order at a time; ↑ ↓ move, → opens or enters, ←
  // closes or goes to the parent, Home/End, Enter opens, typing jumps to a
  // name. A dot marks what the feature changed; counts when given.
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    paths,
    selected = null,
    changed = new Set<string>(),
    counts = {},
    label = "Files",
    onpick,
  }: {
    paths: string[];
    selected?: string | null;
    changed?: Set<string>;
    counts?: Record<string, { add: number | null; del: number | null }>;
    label?: string;
    onpick: (path: string) => void;
  } = $props();

  const tree = $derived(build(paths));
  let open = $state<Record<string, boolean>>({});
  // Folders on the way to a change start open; everything when the tree is small.
  const touched = $derived.by(() => {
    const s = new Set<string>();
    for (const c of changed) {
      const parts = c.split("/");
      for (let i = 1; i < parts.length; i++) s.add(parts.slice(0, i).join("/"));
    }
    return s;
  });
  const isOpen = (path: string) => open[path] ?? (paths.length <= 40 || touched.has(path));

  interface Row { key: string; kind: "dir" | "file"; name: string; path: string; depth: number; parent: string | null; expanded?: boolean; holds?: boolean }
  // The visible rows, in order: what the keys move through.
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (n: Node, depth: number, parent: string | null) => {
      for (const d of n.dirs) {
        const o = isOpen(d.path);
        out.push({ key: "d:" + d.path, kind: "dir", name: d.name, path: d.path, depth, parent, expanded: o, holds: [...changed].some((c) => c.startsWith(d.path + "/")) });
        if (o) walk(d, depth + 1, "d:" + d.path);
      }
      for (const f of n.files) out.push({ key: "f:" + f.path, kind: "file", name: f.name, path: f.path, depth, parent });
    };
    walk(tree, 0, null);
    return out;
  });

  let focus = $state<string | null>(null);
  // The row in the tab order: the focused one, else the selection, else the first.
  const active = $derived(rows.find((r) => r.key === focus)?.key ?? (selected ? "f:" + selected : null) ?? rows[0]?.key ?? null);
  let list: HTMLElement | undefined = $state();

  async function move(key: string | undefined) {
    if (!key) return;
    focus = key;
    await tick();
    list?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)?.focus();
  }
  function toggle(r: Row, to?: boolean) {
    open = { ...open, [r.path]: to ?? !r.expanded };
  }
  let typed = "";
  let typedAt = 0;
  function key(e: KeyboardEvent, r: Row) {
    const i = rows.findIndex((x) => x.key === r.key);
    switch (e.key) {
      case "ArrowDown": void move(rows[i + 1]?.key); break;
      case "ArrowUp": void move(rows[i - 1]?.key); break;
      case "Home": void move(rows[0]?.key); break;
      case "End": void move(rows[rows.length - 1]?.key); break;
      case "ArrowRight":
        if (r.kind === "dir" && !r.expanded) toggle(r, true);
        else if (r.kind === "dir") void move(rows[i + 1]?.key);
        break;
      case "ArrowLeft":
        if (r.kind === "dir" && r.expanded) toggle(r, false);
        else if (r.parent) void move(r.parent);
        break;
      case "Enter":
      case " ":
        if (r.kind === "dir") toggle(r);
        else onpick(r.path);
        break;
      default:
        if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
          // Typeahead: letters typed in quick succession name a row.
          const now = Date.now();
          typed = now - typedAt < 700 ? typed + e.key.toLowerCase() : e.key.toLowerCase();
          typedAt = now;
          const after = [...rows.slice(i + 1), ...rows.slice(0, i + 1)];
          void move(after.find((x) => x.name.toLowerCase().startsWith(typed))?.key);
          break;
        }
        return;
    }
    e.preventDefault();
  }
  const ext = (name: string) => name.split(".").pop()?.toLowerCase() ?? "";
  const tint: Record<string, string> = { rs: "#ff6b35", ts: "#3178c6", js: "#e5b800", svelte: "#ff3e00", py: "#3572a5", go: "#00add8", md: "#8e8e93", toml: "#9c6b4e", json: "#8e8e93", yml: "#cb171e", yaml: "#cb171e", lock: "#8e8e93" };
</script>

<ul class="tree" role="tree" aria-label={label} bind:this={list}>
  {#each rows as r (r.key)}
    {@const c = counts[r.path]}
    <li
      role="treeitem"
      class="row {r.kind}"
      class:on={r.kind === "file" && selected === r.path}
      style="--d:{r.depth}; --t:{tint[ext(r.name)] ?? 'var(--muted)'}"
      data-key={r.key}
      tabindex={active === r.key ? 0 : -1}
      aria-level={r.depth + 1}
      aria-expanded={r.kind === "dir" ? r.expanded : undefined}
      aria-selected={r.kind === "file" ? selected === r.path : undefined}
      title={r.path}
      onclick={() => { focus = r.key; if (r.kind === "dir") toggle(r); else onpick(r.path); }}
      onkeydown={(e) => key(e, r)}
    >
      {#if r.kind === "dir"}
        <span class="chev" class:o={r.expanded} aria-hidden="true"><Icon name="chevron" size={10} weight={2.6} /></span>
        <svg class="folder" width="16" height="14" viewBox="0 0 16 14" aria-hidden="true"><path d="M1 3.2C1 2 1.9 1 3 1h3.1c.5 0 .9.2 1.2.6L8.2 3H13c1.1 0 2 .9 2 2v6c0 1.1-.9 2-2 2H3c-1.1 0-2-.9-2-2z" fill="currentColor" /></svg>
        <span class="name">{r.name}</span>
        {#if r.holds}<span class="dot" title="Holds changes"></span>{/if}
      {:else}
        <span class="chev" aria-hidden="true"></span>
        <svg class="doc" width="12" height="15" viewBox="0 0 12 15" aria-hidden="true"><path d="M1.5 1h6l3.5 3.5V14h-9.5z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /><path d="M7.5 1v3.5H11" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /></svg>
        <span class="name mono">{r.name}</span>
        {#if c}
          <span class="n a">{c.add == null ? "bin" : `+${c.add}`}</span>{#if c.del}<span class="n d">−{c.del}</span>{/if}
        {:else if changed.has(r.path)}<span class="dot" title="Changed by this feature"></span>{/if}
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding: 0; }
  .row {
    display: flex; align-items: center; gap: 0.35rem; cursor: pointer; user-select: none;
    height: 26px; padding: 0 0.5rem 0 calc(0.35rem + var(--d) * 0.95rem); border-radius: 6px; font-size: 0.84rem; color: var(--text);
  }
  .row:hover { background: var(--fill); }
  .row:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--focus); }
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
