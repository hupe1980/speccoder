<script lang="ts">
  // Sign in to GitHub (F-089, F-092): through speccoder's GitHub App with a
  // code — shown big, copied, GitHub's page a click away, the sheet waiting
  // until it is entered — or with a token pasted once. Either way the token
  // goes to the system keychain, never into the page, a file or a log.
  import { app } from "../lib/app.svelte";
  import { post } from "../lib/api";
  import type { Envelope } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { host, onclose }: { host: string; onclose: () => void } = $props();
  type Mode = "choose" | "code" | "token";
  let mode = $state<Mode>("choose");
  let code = $state<{ user_code: string; verification_uri: string } | null>(null);
  let token = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let copied = $state(false);
  const make = $derived(`https://${host}/settings/personal-access-tokens/new`);

  function done(r: Envelope<unknown>) {
    if (r.status !== "ok") {
      error = r.message;
      return;
    }
    app.toast({ tone: r.message.includes("install the") ? "warn" : "ok", text: `Signed in to ${host}`, detail: r.message });
    onclose();
    void app.refresh();
  }

  async function withCode() {
    mode = "code";
    error = null;
    busy = true;
    try {
      const r = await post<{ user_code?: string; verification_uri?: string; error?: string }>("/api/github/device", {});
      if (r.error || !r.user_code || !r.verification_uri) {
        error = r.error ?? "GitHub did not start the sign-in";
        return;
      }
      code = { user_code: r.user_code, verification_uri: r.verification_uri };
      copied = await navigator.clipboard?.writeText(r.user_code).then(() => true, () => false) ?? false;
      done(await post<Envelope<unknown>>("/api/github/device/wait", {}));
    } catch (e) {
      error = String((e as Error).message ?? e);
    } finally {
      busy = false;
    }
  }

  async function withToken(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const r = await post<Envelope<unknown>>("/api/github/login", { token });
      token = "";
      done(r);
    } catch (err) {
      error = String((err as Error).message ?? err);
    } finally {
      busy = false;
    }
  }
  const copy = async () => {
    if (code) copied = await navigator.clipboard?.writeText(code.user_code).then(() => true, () => false) ?? false;
  };
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="si-title">
    <header>
      <span class="badge" aria-hidden="true"><svg width="22" height="22" viewBox="0 0 16 16"><path fill="currentColor" d="M8 0a8 8 0 0 0-2.5 15.6c.4 0 .5-.2.5-.4v-1.5c-2.2.5-2.7-1-2.7-1-.4-.9-.9-1.2-.9-1.2-.7-.5.1-.5.1-.5.8.1 1.2.8 1.2.8.7 1.3 1.9.9 2.4.7 0-.5.3-.9.5-1.1-1.8-.2-3.6-.9-3.6-4 0-.9.3-1.6.8-2.1-.1-.2-.4-1 .1-2.1 0 0 .7-.2 2.2.8a7.5 7.5 0 0 1 4 0c1.5-1 2.2-.8 2.2-.8.4 1.1.2 1.9.1 2.1.5.6.8 1.3.8 2.1 0 3.1-1.9 3.7-3.6 3.9.3.3.6.8.6 1.5v2.2c0 .2.1.5.6.4A8 8 0 0 0 8 0z" /></svg></span>
      <div>
        <h2 id="si-title">Sign In to GitHub</h2>
        <p>speccoder reads issues and opens draft pull requests as you. Your token goes to the system keychain — never into a file.</p>
      </div>
    </header>

    {#if mode === "choose"}
      <button class="choice primary-choice" onclick={withCode}>
        <span class="ci"><Icon name="key" size={16} weight={2.2} /></span>
        <span class="cw"><strong>Continue with GitHub</strong><span>Approve speccoder on GitHub with a short code — only the permissions it needs, on the repositories you choose.</span></span>
        <Icon name="chevron" size={11} weight={2.6} />
      </button>
      <button class="choice" onclick={() => (mode = "token")}>
        <span class="ci"><Icon name="lock" size={16} weight={2} /></span>
        <span class="cw"><strong>Use a Token</strong><span>Paste a fine-grained personal access token you created.</span></span>
        <Icon name="chevron" size={11} weight={2.6} />
      </button>
      <p class="cli mono">$ speccoder auth login</p>
    {:else if mode === "code"}
      {#if code}
        <div class="code-box">
          <span class="label">Your code</span>
          <span class="code mono" aria-live="polite">{code.user_code}</span>
          <button class="btn small ghost" onclick={copy}><Icon name={copied ? "check" : "copy"} size={11} weight={2.4} /> {copied ? "Copied" : "Copy"}</button>
        </div>
        <a class="btn primary go" href={code.verification_uri} target="_blank" rel="noopener noreferrer">Open GitHub and Enter the Code</a>
        <p class="wait"><span class="spin" aria-hidden="true"></span> Waiting for you to approve speccoder on GitHub…</p>
      {:else if busy}
        <p class="wait"><span class="spin" aria-hidden="true"></span> Asking GitHub for a code…</p>
      {/if}
      {#if error}<p class="bad"><Icon name="exclaim" size={13} /> {error}</p>{/if}
      <div class="row"><button class="btn" onclick={() => { mode = "choose"; error = null; }}>Back</button></div>
    {:else}
      <form class="inner" onsubmit={withToken}>
        <ol class="steps">
          <li><a href={make} target="_blank" rel="noopener noreferrer">Create a fine-grained token on {host}</a> for this repository: <strong>Issues</strong> and <strong>Pull requests: read and write</strong>; <strong>Contents</strong>, <strong>Metadata</strong>, <strong>Checks</strong>, <strong>Commit statuses: read</strong>.</li>
          <li>Paste it here.</li>
        </ol>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="password" autocomplete="off" spellcheck="false" placeholder="github_pat_…" bind:value={token} autofocus aria-label="Token" />
        {#if error}<p class="bad"><Icon name="exclaim" size={13} /> {error}</p>{/if}
        <p class="cli mono">$ speccoder auth login --with-token &lt; token.txt</p>
        <div class="row">
          <button type="button" class="btn" onclick={() => { mode = "choose"; error = null; }}>Back</button>
          <button type="submit" class="btn primary" disabled={busy || !token.trim()}>{busy ? "Checking…" : "Sign In"}</button>
        </div>
      </form>
    {/if}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(560px, 100%); border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.85rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .sheet { animation: none; } }
  .inner { display: grid; gap: 0.85rem; }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #4a4a4f, #1f1f22); }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); font-size: 0.9rem; }
  .choice { display: flex; align-items: center; gap: 0.8rem; text-align: left; padding: 0.8rem 0.9rem; border-radius: var(--radius); border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); cursor: pointer; font: inherit; }
  .choice:hover { background: var(--surface-2); }
  .choice:focus-visible { box-shadow: 0 0 0 3.5px var(--focus); outline: none; }
  .primary-choice { border-color: color-mix(in srgb, var(--accent) 45%, transparent); }
  .ci { width: 34px; height: 34px; border-radius: 9px; flex: none; display: grid; place-items: center; background: var(--fill); color: var(--accent-ink); }
  .cw { flex: 1; display: grid; gap: 0.15rem; }
  .cw span { color: var(--text-2); font-size: 0.84rem; }
  .code-box { display: flex; align-items: center; gap: 0.9rem; justify-content: center; padding: 1rem; border-radius: var(--radius); background: var(--surface); }
  .label { color: var(--muted); font-size: 0.8rem; }
  .code { font-size: 1.9rem; font-weight: 700; letter-spacing: 0.12em; }
  .go { justify-self: center; text-decoration: none; }
  .wait { margin: 0; display: flex; gap: 0.5rem; align-items: center; justify-content: center; color: var(--text-2); font-size: 0.86rem; }
  .spin { width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--fill-2); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .steps { margin: 0; padding-left: 1.2rem; color: var(--text-2); font-size: 0.88rem; display: grid; gap: 0.3rem; }
  .steps a { color: var(--accent-ink); }
  input { height: 34px; padding: 0 0.7rem; border-radius: 8px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); font: 0.92rem var(--mono); }
  input:focus { outline: none; box-shadow: 0 0 0 3.5px var(--focus); }
  .bad { margin: 0; display: flex; gap: 0.4rem; align-items: center; color: var(--danger); font-size: 0.86rem; }
  .cli { margin: 0; font-size: 0.76rem; color: var(--muted); }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
