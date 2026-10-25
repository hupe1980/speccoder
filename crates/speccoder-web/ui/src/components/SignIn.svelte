<script lang="ts">
  // Sign in to GitHub (F-089): a token, pasted once, kept in the system
  // keychain by the host — never in the page's storage, the address or a
  // log. Where to make one, with exactly the permissions it needs.
  import { app } from "../lib/app.svelte";
  import { post } from "../lib/api";
  import type { Envelope } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { host, onclose }: { host: string; onclose: () => void } = $props();
  let token = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  const make = $derived(`https://${host}/settings/personal-access-tokens/new`);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const r = await post<Envelope<{ login: string | null }>>("/api/github/login", { token });
      token = "";
      if (r.status !== "ok") {
        error = r.message;
        return;
      }
      app.toast({ tone: "ok", text: `Signed in to ${host}`, detail: r.message });
      onclose();
      void app.refresh();
    } catch (err) {
      error = String((err as Error).message ?? err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="si-title"><form class="inner" onsubmit={submit}>
    <header>
      <span class="badge" aria-hidden="true"><Icon name="key" size={20} weight={2.2} /></span>
      <div>
        <h2 id="si-title">Sign In to GitHub</h2>
        <p>speccoder reads issues and opens draft pull requests as you, through GitHub's API. The token goes to your system keychain — never into a file.</p>
      </div>
    </header>
    <ol class="steps">
      <li><a href={make} target="_blank" rel="noopener noreferrer">Create a fine-grained token on {host}</a> for this repository with <strong>Issues: read</strong>, <strong>Pull requests: read and write</strong>, <strong>Contents</strong> and <strong>Metadata: read</strong>.</li>
      <li>Paste it here.</li>
    </ol>
    <!-- svelte-ignore a11y_autofocus -->
    <input type="password" autocomplete="off" spellcheck="false" placeholder="github_pat_…" bind:value={token} autofocus aria-label="Token" />
    {#if error}<p class="bad"><Icon name="exclaim" size={13} /> {error}</p>{/if}
    <p class="cli mono">$ speccoder auth login --with-token &lt; token.txt</p>
    <div class="row">
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy || !token.trim()}>{busy ? "Checking…" : "Sign In"}</button>
    </div>
  </form></div>
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
  .steps { margin: 0; padding-left: 1.2rem; color: var(--text-2); font-size: 0.88rem; display: grid; gap: 0.3rem; }
  .steps a { color: var(--accent-ink); }
  input { height: 34px; padding: 0 0.7rem; border-radius: 8px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); font: 0.92rem var(--mono); }
  input:focus { outline: none; box-shadow: 0 0 0 3.5px var(--focus); }
  .bad { margin: 0; display: flex; gap: 0.4rem; align-items: center; color: var(--danger); font-size: 0.86rem; }
  .cli { margin: 0; font-size: 0.76rem; color: var(--muted); }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
