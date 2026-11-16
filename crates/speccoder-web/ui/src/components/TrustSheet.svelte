<script lang="ts">
  // A checkout's agent files that wait for trust (spec 126): each one new or
  // changed, with what changed; one action trusts exactly what is shown —
  // a file that changes meanwhile is not trusted, and the sheet says so.
  import { app } from "../lib/app.svelte";
  import { get, post } from "../lib/api";
  import type { AgentFile, Reply, TrustReport } from "../lib/types";
  import { READERS } from "../lib/kinds";
  import Icon from "./Icon.svelte";

  let reply = $state<Reply<TrustReport> | null>(null);
  let open = $state<string | null>(null);
  let busy = $state(false);
  let note = $state<string | null>(null);
  async function load() {
    try {
      reply = await get<Reply<TrustReport>>("/api/trust");
    } catch (e) {
      reply = { message: String(e) } as Reply<TrustReport>;
    }
    open ??= waiting[0]?.path ?? null;
  }
  void load();
  const files = $derived(reply?.data?.files ?? []);
  const waiting = $derived(files.filter((f) => f.state !== "trusted"));
  const trusted = $derived(files.length - waiting.length);
  const close = () => (app.trustSheet = false);
  let sheet = $state<HTMLElement | null>(null);
  $effect(() => sheet?.focus());
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }
  const STATE: Record<string, string> = { untrusted: "New", changed: "Changed", gone: "Gone" };
  const readers = (f: AgentFile) => f.readers.map((r) => READERS[r] ?? r).join(", ");
  const runs = (f: AgentFile) => f.kind === "settings";
  const line = (l: string) => (l.startsWith("+") && !l.startsWith("+++") ? "add" : l.startsWith("-") && !l.startsWith("---") ? "del" : l.startsWith("@@") ? "hunk" : "");
  async function trust() {
    busy = true;
    note = null;
    try {
      const x = await post<{ ok: boolean; error?: string; changed?: string[]; trusted?: string[] }>("/api/trust", {
        files: waiting.map((f) => ({ path: f.path, digest: f.state === "gone" ? "gone" : f.digest })),
      });
      if (x.ok) {
        const n = x.trusted?.length ?? 0;
        app.toast({ tone: "ok", text: `Trusted ${n} agent ${n === 1 ? "file" : "files"}`, detail: "Agents read them as you saw them." });
        close();
        void app.refresh();
        return;
      }
      note = x.changed?.length ? `${x.changed.join(", ")} changed while you looked — here is what is there now.` : (x.error ?? "Not trusted.");
      await load();
    } catch (e) {
      note = String((e as Error).message ?? e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="t-title" tabindex="-1" bind:this={sheet}>
    <header>
      <span class="badge" aria-hidden="true"><Icon name="shield" size={20} weight={2.2} /></span>
      <div>
        <h2 id="t-title">{reply?.message || "Agent files"}</h2>
        <p class="sub">What a checkout tells agents can steer them. Agents read only what you trusted, as you saw it — until then speccoder's model gets the trusted version, and an external agent does not start.</p>
      </div>
    </header>
    {#if !reply}
      <div class="center"><span class="spinner"></span></div>
    {:else if !reply.data}
      <p class="bad"><Icon name="exclaim" size={14} /> {reply.message}</p>
    {:else if !waiting.length}
      <p class="ok"><Icon name="check" size={14} /> Every agent file is trusted as it is now.</p>
    {:else}
      <ul class="files">
        {#each waiting as f (f.path)}
          <li class:open={open === f.path}>
            <button class="file" aria-expanded={open === f.path} onclick={() => (open = open === f.path ? null : f.path)}>
              <span class="state {f.state}">{STATE[f.state] ?? f.state}</span>
              <span class="path mono">{f.path}{#if f.tree}<span class="tree">{` · in ${f.tree}'s worktree`}</span>{/if}</span>
              <span class="who">{f.kind} · {readers(f)}</span>
              <Icon name={open === f.path ? "down" : "chevron"} size={12} />
            </button>
            {#if open === f.path}
              {#if runs(f)}<p class="warn"><Icon name="exclaim" size={12} /> Settings: {readers(f)} may start what this file names — hooks, servers — on your machine.</p>{/if}
              {#if f.state === "gone"}
                <p class="muted">Deleted since you trusted it. Trusting records that it is gone.</p>
              {:else if f.diff}
                <pre class="diff" aria-label={`${f.state === "changed" ? "What changed in" : "The text of"} ${f.path}`}>{#each f.diff.split("\n") as l, i (i)}<span class={f.state === "changed" ? line(l) : ""}>{l}
</span>{/each}</pre>
              {:else}
                <p class="muted">Not shown: the text you trusted before was too large to keep; compare it in your editor.</p>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
      {#if trusted}<p class="muted foot">{trusted} other agent {trusted === 1 ? "file is" : "files are"} trusted as {trusted === 1 ? "it is" : "they are"}.</p>{/if}
    {/if}
    {#if note}<p class="bad" role="alert"><Icon name="exclaim" size={14} /> {note}</p>{/if}
    <div class="row">
      <button class="btn" onclick={close}>Not Now</button>
      {#if waiting.length}
        <button class="btn primary" disabled={busy} onclick={trust}><Icon name="shield" size={12} /> Trust {waiting.length} {waiting.length === 1 ? "File" : "Files"}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 24; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet:focus { outline: none; }
  .sheet { width: min(720px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.3rem 1.4rem 1.1rem; display: grid; gap: 0.85rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: var(--attention-fill, var(--attention)); }
  h2 { margin: 0; font: 650 1.15rem/1.25 var(--display); letter-spacing: -0.016em; overflow-wrap: anywhere; }
  .sub { margin: 0.3rem 0 0; color: var(--text-2); font-size: 0.86rem; }
  .files { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.4rem; }
  .files li { border-radius: var(--radius); background: var(--surface-2); overflow: hidden; }
  .file { width: 100%; display: grid; grid-template-columns: auto minmax(0, 1fr) auto auto; gap: 0.6rem; align-items: center; padding: 0.55rem 0.8rem; background: none; border: 0; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .file:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .state { font-size: 0.72rem; font-weight: 650; padding: 0.1rem 0.5rem; border-radius: 999px; background: var(--fill-2); }
  .state.changed { color: var(--attention); background: var(--attention-bg); }
  .state.untrusted { color: var(--accent); background: var(--accent-soft); }
  .path { overflow-wrap: anywhere; font-size: 0.85rem; }
  .tree { color: var(--muted); font-family: var(--font); font-size: 0.78rem; }
  .who { color: var(--muted); font-size: 0.78rem; white-space: nowrap; }
  .warn { margin: 0 0.8rem 0.45rem; display: flex; gap: 0.35rem; align-items: center; font-size: 0.8rem; color: var(--attention); }
  .diff { margin: 0; max-height: 18rem; overflow: auto; padding: 0.55rem 0.8rem; font: 0.78rem/1.45 var(--mono); background: var(--surface); white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff .add { color: var(--add); background: var(--add-bg); display: block; }
  .diff .del { color: var(--del); background: var(--del-bg); display: block; }
  .diff .hunk { color: var(--muted); display: block; }
  .muted { color: var(--muted); font-size: 0.82rem; margin: 0 0.8rem 0.5rem; }
  .foot { margin: 0; }
  .mono { font-family: var(--mono); }
  .ok { margin: 0; display: flex; gap: 0.4rem; align-items: center; color: var(--verified); font-weight: 600; }
  .bad { margin: 0; display: flex; gap: 0.4rem; align-items: center; color: var(--danger); font-weight: 600; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; flex-wrap: wrap; }
  .center { display: grid; place-items: center; padding: 1.5rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 600px) { .file { grid-template-columns: auto minmax(0, 1fr) auto; } .who { display: none; } }
</style>
