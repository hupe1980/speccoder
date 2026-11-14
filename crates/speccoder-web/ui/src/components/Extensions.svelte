<script lang="ts">
  // Settings › Extensions: a card per extension — what it contributes and
  // can reach, in words, one line each; whether you trusted its one digest;
  // and for a changed one what changed, a widened reach first. Trust… opens
  // the sheet that shows exactly what would run. None yet: how to add one.
  import { app } from "../lib/app.svelte";
  import { get } from "../lib/api";
  import { offer } from "../lib/actions";
  import type { Extension, ExtensionsReport, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";

  let reply = $state<Reply<ExtensionsReport> | null>(null);
  const load = () => get<Reply<ExtensionsReport>>("/api/extensions").then((r) => (reply = r)).catch(() => {});
  // Again whenever a job ends: a trust changes what is shown.
  $effect(() => {
    void app.job?.running;
    void load();
  });
  const list = $derived(reply?.data?.extensions ?? []);
  const KIND: Record<string, string> = { gate: "shield", observer: "bubble", view: "chart", skill: "sparkle", mcp_server: "grid", command: "terminal", hook_offer: "list" };
  const STATE = {
    trusted: { label: "Trusted", tone: "verified", icon: "check" },
    untrusted: { label: "Waits for your trust", tone: "attention", icon: "hand" },
    changed: { label: "Changed — trust it again", tone: "attention", icon: "exclaim" },
  } as const;
  const short = (d: string) => d.replace("sha256:", "").slice(0, 12);
  // A steady colour per name, for the monogram.
  const hue = (name: string) => [...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  const ADD = [
    { title: "From a folder", text: "speccoder extensions add ./path/to/extension" },
    { title: "From a repository, at a commit", text: "speccoder extensions add https://github.com/you/extension@<40-character commit>" },
  ];
  const trust = (e: Extension) =>
    app.perform(
      offer({ kind: "extension_trust", name: e.name, digest: e.digest }, {
        group: "extensions",
        label: `trust ${e.name}`,
        why: "",
        command: `speccoder extensions trust ${e.name}`,
        confirm: `Every gate, view, skill and server of ${e.name} runs from now on, as shown.`,
      }),
    );
</script>

<section class="exts" aria-label="Extensions">
  {#if reply && !list.length}
    <div class="empty">
      <p><strong>No extensions yet.</strong> An extension is a folder — gates, views, skills and MCP servers — that runs only after you trusted one digest over every file in it. Agent Plugins folders made for other agents work as they are; Spec Kit extensions show here too.</p>
      <ul class="add">
        {#each ADD as a (a.title)}
          <li>
            <span class="muted small">{a.title}</span>
            <code class="mono">{a.text}</code>
            <button class="btn small tinted" onclick={() => app.copy(a.text, "Copied", "Run it in the project")}><Icon name="copy" size={11} /> Copy</button>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <ul class="cards">
      {#each list as e (e.name)}
        {@const st = e.problems?.length ? { label: "Cannot be loaded", tone: "danger", icon: "xoctagon" } : STATE[e.trust]}
        <li class:waiting={e.trust !== "trusted" || !!e.problems?.length}>
          <header>
            <span class="mono-gram" style="--h:{hue(e.name)}" aria-hidden="true">{e.name.charAt(0).toUpperCase()}</span>
            <div class="who">
              <strong>{e.name}{#if e.version}<span class="ver"> {e.version}</span>{/if}</strong>
              {#if e.description}<span class="desc">{e.description}</span>{/if}
            </div>
            <span class="chip">{e.source === "spec_kit" ? "Spec Kit" : "Agent Plugins"}</span>
            <span class="state {st.tone}"><Icon name={st.icon} size={11} weight={2.4} /> {st.label}</span>
          </header>

          {#each e.problems ?? [] as p (p)}<p class="bad small"><Icon name="exclaim" size={12} /> {p}</p>{/each}

          {#if e.changes}
            <div class="changed" role="group" aria-label="What changed">
              {#each e.changes.widened as w (w)}<p class="wide"><Icon name="arrowup" size={11} weight={2.4} /> {w}</p>{/each}
              {#each e.changes.gone ?? [] as g (g)}<p class="gone">gone — {g}</p>{/each}
              {#if e.changes.files.length}<p class="files mono">{e.changes.files.join(" · ")}</p>{/if}
            </div>
          {/if}

          <ul class="gives" aria-label="What it contributes">
            {#each e.contributions as c (c.kind + c.name)}
              <li><span class="k" title={c.kind.replace("_", " ")}><Icon name={KIND[c.kind] ?? "gear"} size={12} weight={2.2} /></span><span>{c.line}</span></li>
            {/each}
          </ul>

          <footer>
            <span class="where mono small" title={e.digest}>
              {#if e.origin}{e.origin.url.replace(/^https?:\/\//, "")}{e.origin.commit ? ` @ ${e.origin.commit.slice(0, 8)}` : ""}{:else}{e.folder}{/if}
              · {e.files.length} files · {short(e.digest)}
            </span>
            {#if !e.problems?.length && e.trust !== "trusted"}
              <button class="btn small primary" disabled={!!app.job?.running} onclick={() => trust(e)}>Trust…</button>
            {/if}
          </footer>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .exts { display: grid; gap: 0.75rem; margin-bottom: 1rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  .cards { display: grid; gap: 0.7rem; }
  .cards > li { border: 1px solid var(--line); border-radius: 14px; padding: 0.85rem 1rem; background: var(--surface); display: grid; gap: 0.55rem; }
  .cards > li.waiting { border-color: color-mix(in srgb, var(--attention) 45%, var(--line)); }
  header { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .mono-gram { width: 30px; height: 30px; border-radius: 8px; display: grid; place-items: center; font: 700 0.95rem/1 var(--display); color: #fff; background: linear-gradient(160deg, hsl(var(--h) 70% 58%), hsl(calc(var(--h) + 30) 65% 45%)); flex: none; }
  .who { display: grid; min-width: 0; flex: 1 1 10rem; }
  .who strong { font: 650 0.98rem/1.2 var(--display); }
  .ver { margin-left: 0.35rem; color: var(--muted); font-weight: 500; font-size: 0.82rem; }
  .desc { color: var(--muted); font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip { font-size: 0.72rem; padding: 0.1rem 0.5rem; border-radius: 999px; background: var(--fill, rgba(127, 127, 127, 0.12)); }
  .state { font-size: 0.75rem; display: inline-flex; gap: 0.25rem; align-items: center; }
  .state.verified { color: var(--verified); }
  .state.attention { color: var(--attention); }
  .state.danger { color: var(--danger); }
  .changed { border-radius: 10px; padding: 0.5rem 0.7rem; background: color-mix(in srgb, var(--attention) 10%, transparent); display: grid; gap: 0.2rem; }
  .changed p { margin: 0; font-size: 0.8rem; }
  .wide { color: var(--attention); font-weight: 600; display: flex; gap: 0.35rem; align-items: baseline; }
  .gone { color: var(--muted); text-decoration: line-through; }
  .files { color: var(--muted); font-size: 0.74rem !important; overflow-wrap: anywhere; }
  .gives { display: grid; gap: 0.25rem; }
  .gives li { display: grid; grid-template-columns: auto 1fr; gap: 0.5rem; align-items: baseline; font-size: 0.82rem; color: var(--text-2); overflow-wrap: anywhere; }
  .k { color: var(--muted); }
  footer { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .where { color: var(--muted); margin-right: auto; overflow-wrap: anywhere; }
  .empty p { margin: 0 0 0.6rem; color: var(--text-2); font-size: 0.88rem; }
  .add { display: grid; gap: 0.5rem; }
  .add li { display: grid; grid-template-columns: 1fr auto; gap: 0.2rem 0.6rem; align-items: center; padding: 0.55rem 0.75rem; border-radius: 10px; background: var(--surface); border: 1px solid var(--line); }
  .add .muted { grid-column: 1 / -1; }
  .add code { font-size: 0.78rem; overflow-wrap: anywhere; }
  .bad { margin: 0; color: var(--danger); display: flex; gap: 0.35rem; align-items: baseline; }
</style>
