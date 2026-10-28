<script lang="ts">
  // The person's own step, before and while it runs: the exact command
  // speccoder printed, the folder, that it runs as them outside the
  // sandbox, and Run or Copy. Running, its output streams here (links and
  // a login's device code are clickable) with Cancel; at the end, how it
  // ended and a copy of what went wrong. A command that asks questions
  // goes to a terminal instead; `export` belongs to the shell speccoder
  // starts from, and the card says why.
  import { app } from "../lib/app.svelte";
  import { segments } from "../lib/steps";
  import Icon from "./Icon.svelte";

  const s = $derived(app.personStep!);
  const run = $derived(app.fixRun);
  const running = $derived(!!run && !run.result);
  const ended = $derived(run?.result ?? null);
  let out: HTMLElement | undefined = $state();
  $effect(() => {
    void run?.lines.length;
    if (out) out.scrollTop = out.scrollHeight;
  });
  function close() {
    if (running) return;
    app.personStep = null;
    app.fixRun = null;
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
  const copyCommand = () => app.copy(s.command, "Copied the command", s.command);
  function settings() {
    app.personStep = null;
    app.view = "settings";
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="p-title" aria-describedby="p-what">
    <header>
      <span class="badge" aria-hidden="true"><Icon name="terminal" size={20} weight={2.2} /></span>
      <div>
        {#if s.how.how === "shell"}
          <h2 id="p-title">This one is for your shell</h2>
          <p id="p-what">{s.how.why}.</p>
        {:else if s.how.interactive}
          <h2 id="p-title">Run it in a terminal</h2>
          <p id="p-what">It asks you questions as it runs (a sign-in), so it needs a terminal of your own{app.project?.window ? "" : ": copy it and run it at the project"}.</p>
        {:else}
          <h2 id="p-title">{running ? "Running…" : ended ? (ended.status === "ok" ? "Done" : "It did not succeed") : "Run this command?"}</h2>
          <p id="p-what">speccoder printed it as a fix. It runs as you, outside the sandbox, in the project folder — what you would type yourself.</p>
        {/if}
      </div>
    </header>
    <dl class="grouped">
      <div><dt>Command</dt><dd class="mono">{s.command}</dd></div>
      <div><dt>Folder</dt><dd class="mono">{app.project?.root ?? ""}</dd></div>
    </dl>
    {#if run}
      <pre class="out mono" bind:this={out} aria-live="polite" aria-label="Output">{#each run.lines as l, i (i)}<span class:err={l.stream === "stderr"}>{#each segments(l.line) as seg, j (j)}{#if seg.kind === "url"}<a href={seg.text} target="_blank" rel="noreferrer" onclick={() => app.copy(seg.text, "Copied the link", seg.text)}>{seg.text}</a>{:else if seg.kind === "code"}<button class="code" title="Copy the code" onclick={() => app.copy(seg.text, "Copied the code", seg.text)}>{seg.text}</button>{:else}{seg.text}{/if}{/each}
</span>{:else}{running ? "…" : "(no output)"}{/each}</pre>
      {#if ended}
        <p class:bad={ended.status !== "ok"} class="end"><Icon name={ended.status === "ok" ? "check" : "exclaim"} size={13} /> {ended.message}</p>
      {/if}
    {/if}
    <div class="row">
      {#if running}
        <button class="btn" onclick={() => app.stop()}><Icon name="stop" size={11} /> Cancel</button>
      {:else}
        {#if ended && ended.status !== "ok"}
          <button class="btn ghost" onclick={() => ended && app.copyError(ended)}><Icon name="copy" size={11} /> Copy What Went Wrong</button>
        {/if}
        <button class="btn" onclick={close}>{ended ? "Close" : "Cancel"}</button>
        {#if s.how.how === "shell"}
          {#if s.how.setting === "model"}<button class="btn" onclick={settings}><Icon name="gear" size={11} /> Settings › Model</button>{/if}
          <!-- svelte-ignore a11y_autofocus -->
          <button class="btn primary" autofocus onclick={copyCommand}><Icon name="copy" size={11} /> Copy</button>
        {:else if s.how.interactive}
          <button class="btn" onclick={copyCommand}><Icon name="copy" size={11} /> Copy</button>
          {#if app.project?.window}
            <!-- svelte-ignore a11y_autofocus -->
            <button class="btn primary" autofocus onclick={() => app.openTerminal()}><Icon name="terminal" size={11} /> Open Terminal Here</button>
          {/if}
        {:else}
          <button class="btn" onclick={copyCommand}><Icon name="copy" size={11} /> Copy</button>
          <!-- svelte-ignore a11y_autofocus -->
          <button class="btn primary" autofocus disabled={!!app.job?.running} onclick={() => app.runFix()}><Icon name="play" size={11} /> {ended ? "Run Again" : "Run"}</button>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(680px, 100%); max-height: 90vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.9rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #8e8e93, #636366); }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); }
  dl { margin: 0; }
  dl > div { display: grid; grid-template-columns: 5.5rem 1fr; gap: 1rem; padding: 0.5rem 0.85rem; font-size: 0.88rem; }
  dt { color: var(--muted); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .out { margin: 0; max-height: 40vh; min-height: 4rem; overflow: auto; padding: 0.7rem 0.9rem; border-radius: var(--radius); background: var(--surface-2); font-size: 0.76rem; white-space: pre-wrap; overflow-wrap: anywhere; }
  .out .err { color: var(--danger); }
  .out a { color: var(--accent); }
  .code { border: 0; padding: 0 0.25rem; border-radius: 4px; background: var(--fill-2); font: inherit; font-weight: 700; cursor: copy; color: inherit; }
  .end { margin: 0; display: flex; gap: 0.4rem; align-items: center; font-weight: 600; }
  .end.bad { color: var(--danger); }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; flex-wrap: wrap; }
</style>
