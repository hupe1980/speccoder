<script lang="ts">
  // A repository of the person's, cloned with their git into a folder of
  // their choice — after one confirmation naming github.com. The list is
  // read through speccoder's GitHub sign-in; cloning needs only git.
  import { app } from "../lib/app.svelte";
  import { get } from "../lib/api";
  import { offer } from "../lib/actions";
  import { trap } from "../lib/sheet";
  import Icon from "./Icon.svelte";

  interface Repo { slug: string; description: string | null; private: boolean; pushed: string | null }
  let login = $state<string | null>(null);
  let repos = $state<Repo[]>([]);
  let signedOut = $state<string | null>(null);
  let filter = $state("");
  let repo = $state("");
  let parent = $state(app.welcome?.parent_shown ?? "~/Workspaces");
  let dest = $state("");
  let destEdited = $state(false);
  let confirming = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    void get<{ login?: string; repos?: Repo[]; error?: string }>("/api/github/repos").then((r) => {
      if (r.error) signedOut = r.error;
      login = r.login ?? null;
      repos = r.repos ?? [];
    });
  });
  $effect(() => {
    if (!destEdited) dest = repo.includes("/") ? `${parent.replace(/\/$/, "")}/${repo.split("/")[1]}` : "";
  });
  const shown = $derived(repos.filter((r) => !filter || r.slug.toLowerCase().includes(filter.toLowerCase())).slice(0, 50));
  const valid = $derived(/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repo.trim()) && dest.trim().length > 0);

  function close() {
    if (!busy) app.clone = false;
  }

  async function clone() {
    if (!valid || busy) return;
    if (!confirming) {
      confirming = true;
      return;
    }
    busy = true;
    error = null;
    const home = dest.startsWith("~") ? dest : dest;
    const r = await app.run(offer({ kind: "clone_repository", repo: repo.trim(), target: home }, { group: "work", label: `clone ${repo}`, why: "this reaches github.com", command: `git clone https://github.com/${repo}.git ${dest}` }));
    busy = false;
    confirming = false;
    if (r && r.status !== "ok") error = r.message;
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (confirming) confirming = false;
      else close();
    }
  }
</script>

<div class="d-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={key}>
  <div class="d-sheet" role="dialog" aria-modal="true" aria-labelledby="cl-title" use:trap={"input"}>
    <header class="d-head">
      <span class="d-badge violet" aria-hidden="true"><Icon name="branch" size={20} weight={2.2} /></span>
      <div>
        <h2 id="cl-title">Clone from GitHub</h2>
        <p>{login ? `As ${login} on github.com.` : "A repository of yours."} Cloned with your git, then speccoder's setup takes over.</p>
      </div>
    </header>

    {#if signedOut}
      <p class="d-warn">Your repositories are listed once you sign in: <code>speccoder auth login</code>. A public repository can be named below anyway.</p>
    {:else if repos.length}
      <label class="d-field">
        <span class="d-label">Your repositories</span>
        <input type="text" bind:value={filter} placeholder="Filter" aria-controls="cl-list" />
      </label>
      <ul id="cl-list" class="list" aria-label="Repositories">
        {#each shown as r (r.slug)}
          <li>
            <button aria-pressed={repo === r.slug} onclick={() => (repo = r.slug)} class:on={repo === r.slug}><strong>{r.slug}</strong>{#if r.private} <span class="muted">private</span>{/if}{#if r.description}<span class="muted"> — {r.description}</span>{/if}</button>
          </li>
        {/each}
      </ul>
    {/if}

    <dl class="d-grid">
      <dt><label for="cl-repo">Repository</label></dt>
      <dd><input id="cl-repo" type="text" bind:value={repo} placeholder="owner/repo" spellcheck="false" /></dd>
      <dt><label for="cl-dest">Into</label></dt>
      <dd><input id="cl-dest" type="text" bind:value={dest} oninput={() => (destEdited = true)} spellcheck="false" /></dd>
    </dl>

    {#if confirming}
      <div class="d-warn attention" role="group" aria-label="Confirm">
        <span>Clone {repo} into {dest} — this reaches github.com</span>
        <div class="d-row"><button class="btn" onclick={() => (confirming = false)}>Cancel</button><button class="btn primary" onclick={clone} disabled={busy}>{busy ? "Cloning…" : "Clone"}</button></div>
      </div>
    {/if}
    {#if error}<p class="d-error" role="alert">{error}</p>{/if}

    {#if !confirming}
      <div class="d-row">
        <button class="btn" onclick={close}>Cancel</button>
        <button class="btn primary" disabled={!valid || busy} onclick={clone}>Clone</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .list { list-style: none; margin: 0; padding: 0; max-height: 14rem; overflow: auto; border-radius: 10px; background: var(--surface); }
  .list button { width: 100%; text-align: left; border: 0; background: none; padding: 0.4rem 0.6rem; cursor: pointer; color: var(--text); font: inherit; font-size: 0.86rem; }
  .list button:hover { background: var(--fill); }
  .list button.on { background: var(--accent-soft); }
  input[type="text"] { width: 100%; box-sizing: border-box; }
  code { font: 0.82rem var(--mono); }
</style>
