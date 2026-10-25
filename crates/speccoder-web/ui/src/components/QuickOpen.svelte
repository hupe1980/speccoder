<script lang="ts">
  // Go to a file (⌘P): type a few letters of its path, in order; the best
  // matches first (a file name match beats a folder match, earlier and
  // closer letters beat later and scattered ones). Enter opens.
  import Icon from "./Icon.svelte";
  let { files, changed = new Set<string>(), onpick, onclose }: { files: string[]; changed?: Set<string>; onpick: (p: string) => void; onclose: () => void } = $props();
  let query = $state("");
  let at = $state(0);

  /** A score for `path` against `q` (higher is better), or null when the
   *  letters do not all appear in order. */
  function score(path: string, q: string): { s: number; marks: number[] } | null {
    if (!q) return { s: 0, marks: [] };
    const p = path.toLowerCase();
    const name = p.lastIndexOf("/") + 1;
    let i = 0;
    let s = 0;
    let last = -2;
    const marks: number[] = [];
    for (const ch of q.toLowerCase()) {
      const j = p.indexOf(ch, i);
      if (j < 0) return null;
      s += j === last + 1 ? 6 : 1; // runs of letters
      if (j >= name) s += 3; // in the file name
      if (j === 0 || "/-_.".includes(p[j - 1]!)) s += 4; // at a word start
      marks.push(j);
      last = j;
      i = j + 1;
    }
    return { s: s - path.length * 0.01, marks };
  }
  const results = $derived(
    files
      .map((f) => ({ f, m: score(f, query.trim()) }))
      .filter((x): x is { f: string; m: { s: number; marks: number[] } } => x.m !== null)
      .sort((a, b) => b.m.s - a.m.s)
      .slice(0, 50),
  );
  $effect(() => {
    void query;
    at = 0;
  });
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") { at = Math.min(at + 1, results.length - 1); e.preventDefault(); }
    else if (e.key === "ArrowUp") { at = Math.max(at - 1, 0); e.preventDefault(); }
    else if (e.key === "Enter" && results[at]) { onpick(results[at]!.f); e.preventDefault(); }
    else if (e.key === "Escape") onclose();
  }
  const parts = (f: string, marks: number[]) => [...f].map((c, i) => ({ c, m: marks.includes(i) }));
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="box glass" role="dialog" aria-modal="true" aria-label="Go to file">
    <label class="field">
      <Icon name="search" size={18} />
      <!-- svelte-ignore a11y_autofocus -->
      <input autofocus placeholder="Go to file…" bind:value={query} onkeydown={key} role="combobox" aria-expanded="true" aria-controls="qo-list" aria-activedescendant="qo-{at}" />
    </label>
    <ul id="qo-list" role="listbox">
      {#each results as r, i (r.f)}
        <li id="qo-{i}" role="option" aria-selected={i === at}>
          <button class:on={i === at} tabindex="-1" onmouseenter={() => (at = i)} onclick={() => onpick(r.f)}>
            <span class="name">{#each parts(r.f.split("/").pop() ?? r.f, r.m.marks.map((m) => m - (r.f.length - (r.f.split("/").pop() ?? r.f).length))) as p}{#if p.m}<b>{p.c}</b>{:else}{p.c}{/if}{/each}</span>
            <span class="dir">{r.f.includes("/") ? r.f.slice(0, r.f.lastIndexOf("/")) : ""}</span>
            {#if changed.has(r.f)}<span class="dot" title="Changed by this feature"></span>{/if}
          </button>
        </li>
      {:else}<li class="none">No file matches “{query}”.</li>{/each}
    </ul>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); z-index: 30; display: grid; justify-items: center; align-items: start; padding-top: 14vh; }
  .box { width: min(620px, 94vw); border-radius: 18px; overflow: hidden; background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); animation: pop 320ms var(--spring); }
  @keyframes pop { from { transform: scale(0.96); opacity: 0; } }
  .field { display: flex; align-items: center; gap: 0.6rem; padding: 0 1rem; height: 52px; color: var(--muted); border-bottom: 0.5px solid var(--separator); }
  input { flex: 1; border: 0; background: transparent; outline: none; font: 400 1.15rem var(--display); color: var(--text); }
  ul { list-style: none; margin: 0; padding: 0.35rem; max-height: 50vh; overflow: auto; }
  button { width: 100%; display: flex; align-items: baseline; gap: 0.6rem; border: 0; background: none; padding: 0.4rem 0.6rem; border-radius: 9px; cursor: pointer; text-align: left; }
  button.on { background: var(--accent); color: #fff; }
  button.on .dir { color: rgba(255, 255, 255, 0.75); }
  .name { font: 500 0.88rem var(--mono); white-space: nowrap; }
  .name b { color: var(--accent-ink); font-weight: 700; }
  button.on .name b { color: #fff; text-decoration: underline; }
  .dir { font-size: 0.78rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--attention-fill); align-self: center; }
  .none { padding: 1rem; color: var(--muted); text-align: center; }
</style>
