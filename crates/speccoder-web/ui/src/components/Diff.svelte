<script lang="ts">
  // A change as code: the commit's message, then each file as a card with
  // its additions and removals, old and new line numbers in the gutter, and
  // hunks as dividers. Long lines scroll; wrapping is a toggle.
  import Icon from "./Icon.svelte";
  import Source from "./Source.svelte";
  import { language } from "../lib/highlight";
  /** `only`: show this file alone (the Code view picks one). */
  let { patch, loading = false, only = null, header = true }: { patch: string; loading?: boolean; only?: string | null; header?: boolean } = $props();

  interface Line { kind: "add" | "del" | "ctx" | "hunk" | "note"; text: string; old: number | null; new: number | null }
  interface File { name: string; status: string; lines: Line[]; add: number; del: number }

  let wrap = $state(false);
  let folded = $state<Record<string, boolean>>({});

  const parsed = $derived.by(() => {
    const files: File[] = [];
    const head: string[] = [];
    let cur: File | null = null;
    let o = 0;
    let n = 0;
    for (const line of patch.split("\n")) {
      if (line.startsWith("diff --git ")) {
        cur = { name: line.split(" b/").pop() ?? line, status: "modified", lines: [], add: 0, del: 0 };
        files.push(cur);
        continue;
      }
      if (!cur) {
        head.push(line);
        continue;
      }
      if (line.startsWith("new file")) cur.status = "added";
      else if (line.startsWith("deleted file")) cur.status = "deleted";
      else if (line.startsWith("rename to ")) { cur.status = "renamed"; cur.name = line.slice(10); }
      else if (line.startsWith("index ") || line.startsWith("--- ") || line.startsWith("+++ ") || line.startsWith("similarity") || line.startsWith("rename from") || /^(old|new) mode/.test(line)) continue;
      else if (line.startsWith("@@")) {
        const m = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/);
        o = m ? Number(m[1]) : 0;
        n = m ? Number(m[2]) : 0;
        cur.lines.push({ kind: "hunk", text: m?.[3]?.trim() ?? "", old: null, new: null });
      } else if (line.startsWith("+")) { cur.lines.push({ kind: "add", text: line.slice(1), old: null, new: n++ }); cur.add++; }
      else if (line.startsWith("-")) { cur.lines.push({ kind: "del", text: line.slice(1), old: o++, new: null }); cur.del++; }
      else if (line.startsWith("\\")) cur.lines.push({ kind: "note", text: line.slice(2), old: null, new: null });
      else if (line.length || cur.lines.length) cur.lines.push({ kind: "ctx", text: line.slice(1), old: o++, new: n++ });
    }
    // The commit header: "<hash> <subject>", a blank line, the body (trailers).
    const [first = "", , ...body] = head;
    const hash = first.split(" ")[0] ?? "";
    const subject = first.slice(hash.length + 1);
    const trailers = body.filter((l) => /^[A-Z][\w-]+: /.test(l));
    return { files, hash, subject, trailers };
  });
  const totals = $derived(parsed.files.reduce((a, f) => ({ add: a.add + f.add, del: a.del + f.del }), { add: 0, del: 0 }));
  const blocks = (f: File) => Math.min(5, Math.round((5 * f.add) / Math.max(f.add + f.del, 1)));
</script>

{#if loading}
  <div class="state"><span class="spinner"></span></div>
{:else if parsed.files.length === 0}
  <div class="state muted"><Icon name="doc" size={22} /><span>No change to show.</span></div>
{:else}
  <div class="change">
    {#if parsed.subject && header}
      <header class="commit">
        <span class="hash mono">{parsed.hash}</span>
        <strong>{parsed.subject}</strong>
        {#if parsed.trailers.length}
          <dl>
            {#each parsed.trailers as t}
              {@const [k, ...v] = t.split(": ")}
              <dt>{k}</dt><dd class="mono">{v.join(": ")}</dd>
            {/each}
          </dl>
        {/if}
      </header>
    {/if}
    <div class="summary">
      <span>{parsed.files.length} {parsed.files.length === 1 ? "file" : "files"}</span>
      <span class="a">+{totals.add}</span><span class="d">−{totals.del}</span>
      <label class="wrap"><input type="checkbox" bind:checked={wrap} /> Wrap lines</label>
    </div>
    {#each only ? parsed.files.filter((f) => f.name === only) : parsed.files as f (f.name)}
      <section class="file">
        <button class="fhead" onclick={() => (folded = { ...folded, [f.name]: !folded[f.name] })} aria-expanded={!folded[f.name]}>
          <Icon name={folded[f.name] ? "chevron" : "down"} size={12} weight={2.2} />
          <span class="fname mono" title={f.name}>{f.name}</span>
          {#if f.status !== "modified"}<span class="badge {f.status}">{f.status}</span>{/if}
          <span class="stat"><span class="a">+{f.add}</span><span class="d">−{f.del}</span>
            <span class="blocks" aria-hidden="true">{#each Array(5) as _, i}<i class:ga={i < blocks(f)} class:gd={i >= blocks(f) && i < (f.del ? 5 : blocks(f))}></i>{/each}</span>
          </span>
        </button>
        {#if !folded[f.name]}
          <div class="code" class:wrap>
            <table>
              <tbody>
                {#each f.lines as l}
                  {#if l.kind === "hunk"}
                    <tr class="hunk"><td colspan="3"><span class:named={!!l.text}>{l.text}</span></td></tr>
                  {:else if l.kind === "note"}
                    <tr class="note"><td colspan="3">{l.text}</td></tr>
                  {:else}
                    <tr class={l.kind}>
                      <td class="ln">{l.old ?? ""}</td>
                      <td class="ln">{l.new ?? ""}</td>
                      <td class="src"><span class="sign">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : " "}</span><Source line={l.text} lang={language(f.name)} /></td>
                    </tr>
                  {/if}
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    {/each}
  </div>
{/if}

<style>
  .change { display: grid; gap: 0.75rem; }
  .state { display: grid; justify-items: center; gap: 0.4rem; padding: 2.5rem 1rem; color: var(--muted); }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .commit { display: grid; gap: 0.2rem; padding: 0.75rem 0.9rem; border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow); }
  .hash { font-size: 0.74rem; color: var(--muted); }
  .commit strong { font-weight: 600; line-height: 1.35; }
  .commit dl { margin: 0.35rem 0 0; display: grid; grid-template-columns: auto 1fr; gap: 0.1rem 0.75rem; font-size: 0.76rem; }
  .commit dt { color: var(--muted); }
  .commit dd { margin: 0; overflow-wrap: anywhere; color: var(--text-2); }
  .summary { display: flex; align-items: center; gap: 0.5rem; font-size: 0.82rem; color: var(--text-2); padding: 0 0.25rem; }
  .a { color: var(--add); font-weight: 600; font-variant-numeric: tabular-nums; }
  .d { color: var(--del); font-weight: 600; font-variant-numeric: tabular-nums; }
  .wrap { margin-left: auto; display: inline-flex; align-items: center; gap: 0.35rem; color: var(--muted); cursor: pointer; }
  .file { border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow); overflow: hidden; }
  .fhead {
    all: unset; box-sizing: border-box; width: 100%; display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.75rem;
    cursor: pointer; background: var(--surface-2); border-bottom: 0.5px solid var(--separator); font-size: 0.82rem;
  }
  .fhead:focus-visible { box-shadow: inset 0 0 0 2px var(--focus); }
  .fname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .badge { font-size: 0.7rem; font-weight: 650; padding: 0 0.45rem; border-radius: 999px; text-transform: capitalize; }
  .badge.added { color: var(--add); background: var(--add-bg); }
  .badge.deleted { color: var(--del); background: var(--del-bg); }
  .badge.renamed { color: var(--ready); background: var(--ready-bg); }
  .stat { display: inline-flex; align-items: center; gap: 0.35rem; font-size: 0.76rem; }
  .blocks { display: inline-flex; gap: 1.5px; }
  .blocks i { width: 7px; height: 7px; border-radius: 1.5px; background: var(--fill-2); }
  .blocks i.ga { background: var(--verified-fill); }
  .blocks i.gd { background: var(--danger-fill); }
  .code { overflow-x: auto; font: 0.78rem/1.6 var(--mono); }
  table { border-collapse: collapse; width: 100%; }
  td { padding: 0; vertical-align: top; }
  .ln { width: 1%; min-width: 3ch; padding: 0 0.5rem; text-align: right; color: var(--faint); user-select: none; font-variant-numeric: tabular-nums; }
  .src { padding: 0 0.9rem 0 0.25rem; white-space: pre; color: var(--text); }
  .wrap .src { white-space: pre-wrap; overflow-wrap: anywhere; }
  .sign { display: inline-block; width: 1.3ch; color: var(--faint); user-select: none; }
  tr.add { background: var(--add-bg); }
  tr.add .sign, tr.add .ln { color: var(--add); }
  tr.del { background: var(--del-bg); }
  tr.del .sign, tr.del .ln { color: var(--del); }
  tr.hunk td { padding: 0.25rem 0.75rem; background: var(--accent-soft); color: var(--accent-ink); font-size: 0.74rem; }
  tr.hunk span::before { content: "⋯"; opacity: 0.6; }
  tr.hunk span.named::before { content: "⋯  "; }
  tr.note td { padding: 0 0.75rem; color: var(--muted); font-style: italic; }
</style>
