<script lang="ts">
  // What `speccoder doctor` and `speccoder explain CODE` show, in the app:
  // every check of the setup with its fixes (taken the way the host says),
  // Copy Bug Report; or one code — what it means and how to resolve it.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import type { Reply } from "../lib/types";
  import FixSteps from "./FixSteps.svelte";
  import Icon from "./Icon.svelte";

  interface Code { code: string; title: string; explain: string; fix: string[]; docs: string }
  const h = $derived(app.help!);
  let code = $state<Reply<{ codes: Code[] }> | null>(null);
  $effect(() => {
    if (h.kind !== "explain") return;
    void get<Reply<{ codes: Code[] }>>(`/api/explain${q({ code: h.code })}`).then((r) => (code = r));
  });
  const entry = $derived(code?.data?.codes?.[0] ?? null);
  const mark: Record<string, string> = { pass: "check", info: "info", warn: "exclaim", fail: "xoctagon" };
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") app.help = null;
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.help = null)}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="h-title">
    {#if h.kind === "doctor"}
      <header>
        <h2 id="h-title">Check the Setup</h2>
        <p class="muted">git, the project, the configuration, the model key, the sandbox — what a run depends on.</p>
      </header>
      <ul class="grouped checks">
        {#each app.doctor as c (c.check + (c.code ?? ""))}
          <li class={c.level}>
            <span class="m" aria-hidden="true"><Icon name={mark[c.level] ?? "info"} size={13} weight={2.2} /></span>
            <span class="what">
              <strong>{c.check}<span class="sr-only">: {c.level}</span></strong>
              <span class="d">{c.detail}</span>
              {#if c.level !== "pass" && c.fix_steps?.length}<FixSteps steps={c.fix_steps} limit={2} />{/if}
            </span>
            {#if c.level === "warn" || c.level === "fail"}
              <button class="btn icon small ghost" title="Copy what went wrong" aria-label="Copy what went wrong: {c.check}" onclick={() => app.copyError({ message: c.detail, code: c.code, fix: c.fix })}><Icon name="copy" size={12} /></button>
            {/if}
          </li>
        {:else}
          <li><span class="d">Checking…</span></li>
        {/each}
      </ul>
      <p class="cli mono">$ speccoder doctor</p>
    {:else}
      <header>
        <h2 id="h-title" class="mono">{h.code}</h2>
        {#if entry}<p>{entry.title}</p>{:else if code}<p class="muted">{code.message}</p>{/if}
      </header>
      {#if entry}
        <p>{entry.explain}</p>
        <p class="cli mono">$ speccoder explain {entry.code}</p>
      {/if}
    {/if}
    <div class="row">
      <button class="btn ghost" title="speccoder doctor --bug-report: the setup in Markdown, secrets taken out" onclick={() => app.copyBugReport()}><Icon name="copy" size={11} /> Copy Bug Report</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn primary" autofocus onclick={() => (app.help = null)}>Done</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 21; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(720px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.9rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; }
  p { margin: 0; }
  .checks { margin: 0; padding: 0; }
  .checks li { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 0.7rem; align-items: start; padding: 0.55rem 0.85rem; list-style: none; }
  .m { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; color: #fff; background: var(--verified-fill); margin-top: 0.1rem; }
  .info .m { background: var(--fill-2); color: var(--text-2); }
  .warn .m { background: var(--attention-fill); }
  .fail .m { background: var(--danger-fill); }
  .what { display: grid; gap: 0.2rem; min-width: 0; }
  .d { color: var(--muted); font-size: 0.86rem; overflow-wrap: anywhere; }
  .cli { font-size: 0.76rem; color: var(--muted); }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
