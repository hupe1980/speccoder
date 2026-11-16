<script lang="ts">
  // Settings › Hooks: a card per hook — what it guards or hears, whether
  // you trusted its exact bytes, its last runs as marks — with Trust… (the
  // confirm sheet shows what it runs) and Test. None yet: two to copy.
  import { app } from "../lib/app.svelte";
  import { get } from "../lib/api";
  import { offer } from "../lib/actions";
  import type { HookEntry, HooksReport, Reply, TrustReport } from "../lib/types";
  import { READERS } from "../lib/kinds";
  import Icon from "./Icon.svelte";

  let reply = $state<Reply<HooksReport> | null>(null);
  const load = () => get<Reply<HooksReport>>("/api/hooks").then((r) => (reply = r)).catch(() => {});
  // Again whenever a job ends: a trust or a test changes what is shown.
  $effect(() => {
    void app.job?.running;
    void load();
  });
  const hooks = $derived(reply?.data?.hooks ?? []);
  // The checkout's agent files, read again when the sheet closes.
  let agent = $state<Reply<TrustReport> | null>(null);
  $effect(() => {
    if (!app.trustSheet) void get<Reply<TrustReport>>("/api/trust").then((r) => (agent = r)).catch(() => {});
  });
  const files = $derived(agent?.data?.files ?? []);
  const WORD: Record<string, string> = { trusted: "Trusted", changed: "Changed", untrusted: "New", gone: "Gone" };
  const STARTERS = [
    { title: "A gate the tick waits for", text: '[hooks.licence]\nat = "tick"\nargv = ["cargo", "deny", "check", "licenses"]\nnetwork = true' },
    { title: "An observer for your team chat", text: '[hooks.team-chat]\non = ["task.ticked", "task.stopped"]\nurl = "https://hooks.example.com/speccoder"\nsecret_env = "SPECCODER_HOOK_SECRET"' },
  ];
  const STATE = {
    trusted: { label: "Trusted", tone: "verified", icon: "check" },
    untrusted: { label: "Waits for your trust", tone: "needs_approval", icon: "hand" },
    changed: { label: "Changed — trust it again", tone: "needs_approval", icon: "exclaim" },
  } as const;
  const short = (d: string) => d.replace("sha256:", "").slice(0, 12);
  const mark = (o: string) => (o === "passed" || o === "delivered" ? "ok" : o === "not_run" || o === "skipped_untrusted" ? "skip" : "bad");
  const trust = (h: HookEntry) =>
    app.perform(
      offer({ kind: "hook_trust", name: h.name, digest: h.digest }, {
        group: "hooks",
        label: `trust ${h.name}`,
        why: "",
        command: `speccoder hooks trust ${h.name}`,
        confirm: h.role === "gate" ? `It stands ${h.listens} from now on; only exit 0 lets the point through.` : `It ${h.listens} from now on.`,
      }),
    );
  const test = (h: HookEntry) =>
    app.perform(offer({ kind: "hook_test", name: h.name }, { group: "hooks", label: `test ${h.name}`, why: "", command: `speccoder hooks test ${h.name}` }));
</script>

<section class="hooks" aria-label="Hooks">
  {#if reply?.data?.problem}
    <p class="bad"><Icon name="exclaim" size={14} /> {reply.data.problem}</p>
  {:else if reply && !hooks.length}
    <p class="muted small">No hooks yet. Copy one into speccoder.toml, commit it, then trust it here.</p>
    <ul class="starters">
      {#each STARTERS as s (s.title)}
        <li>
          <header><strong>{s.title}</strong><button class="btn small tinted" onclick={() => app.copy(s.text, "Copied", "Paste it into speccoder.toml")}><Icon name="copy" size={11} /> Copy</button></header>
          <pre class="mono">{s.text}</pre>
        </li>
      {/each}
    </ul>
  {:else}
    <ul class="cards">
      {#each hooks as h (h.name)}
        {@const st = STATE[h.state]}
        <li class:waiting={h.state !== "trusted"}>
          <header>
            <span class="glyph" aria-hidden="true"><Icon name={h.role === "gate" ? "shield" : "bubble"} size={14} weight={2.2} /></span>
            <strong class="mono">{h.name}</strong>
            <span class="chip">{h.role === "gate" ? `Guards the ${h.at === "pr" ? "pull request" : "tick"}` : h.listens.replace("hears", "Hears")}</span>
            <span class="state {st.tone}"><Icon name={st.icon} size={11} weight={2.4} /> {st.label}</span>
          </header>
          <p class="runs mono">{h.runs}</p>
          {#if h.problem}<p class="bad small"><Icon name="exclaim" size={12} /> {h.problem}</p>{/if}
          <footer>
            {#if h.recent.length}
              <span class="marks" aria-label="Last runs">
                {#each h.recent.slice(-10) as r (r.at + r.what)}<span class="m {mark(r.outcome)}" title="{r.what}: {r.outcome}{r.error ? ` — ${r.error}` : ''}"></span>{/each}
              </span>
              {@const last = h.recent.at(-1)}
              {#if last?.error}<span class="last small">{last.outcome} — {last.error}</span>{/if}
            {:else}
              <span class="muted small">Not run yet</span>
            {/if}
            <span class="digest mono small" title={h.digest}>{short(h.digest)}</span>
            {#if h.state !== "trusted"}
              <button class="btn small primary" disabled={!!app.job?.running} onclick={() => trust(h)}>Trust…</button>
            {:else}
              <button class="btn small" disabled={!!app.job?.running} onclick={() => test(h)}><Icon name="play" size={10} /> Test</button>
            {/if}
          </footer>
        </li>
      {/each}
    </ul>
  {/if}
  {#if files.length}
    <section class="agent" aria-labelledby="agent-files">
      <header>
        <h3 id="agent-files">Agent files</h3>
        <span class="muted small">What this checkout tells agents. They read only what you trusted, as you saw it.</span>
        {#if agent?.data?.waiting}<button class="btn small primary" onclick={() => (app.trustSheet = true)}>Review {agent.data.waiting}…</button>{/if}
      </header>
      <ul class="files">
        {#each files as f (f.path)}
          <li class="file" class:waiting={f.state !== "trusted"}>
            <span class="state {f.state === 'trusted' ? 'verified' : 'needs_approval'}">{WORD[f.state] ?? f.state}</span>
            <span class="mono path">{f.path}</span>
            <span class="muted small">{f.kind} · {f.readers.map((r) => READERS[r] ?? r).join(", ")}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</section>

<style>
  .agent { display: grid; gap: 0.5rem; }
  .agent header h3 { margin: 0; font-size: 0.95rem; }
  .agent .files { gap: 0.25rem; }
  .agent .file { display: grid; grid-template-columns: 6.5rem minmax(0, 1fr) auto; gap: 0.6rem; align-items: center; padding: 0.4rem 0.7rem; border-radius: 8px; }
  .agent .path { overflow-wrap: anywhere; font-size: 0.82rem; }
  .agent .file .state { margin-left: 0; }
  @media (max-width: 600px) { .agent .file { grid-template-columns: 5.5rem minmax(0, 1fr); } .agent .file > :last-child { display: none; } }
  .hooks { display: grid; gap: 0.75rem; margin-bottom: 1rem; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.6rem; }
  li { border: 1px solid var(--line); border-radius: 12px; padding: 0.75rem 0.9rem; background: var(--surface); }
  li.waiting { border-color: color-mix(in srgb, var(--needs_approval, #ff9f0a) 45%, var(--line)); }
  header { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .glyph { display: inline-grid; place-items: center; width: 22px; height: 22px; border-radius: 6px; background: color-mix(in srgb, #ff9f0a 18%, transparent); color: #ff9f0a; }
  .chip { font-size: 0.75rem; padding: 0.1rem 0.5rem; border-radius: 999px; background: var(--fill, rgba(127, 127, 127, 0.12)); }
  .state { margin-left: auto; font-size: 0.75rem; display: inline-flex; gap: 0.25rem; align-items: center; }
  .state.verified { color: var(--verified); }
  .state.needs_approval { color: var(--needs_approval, #ff9f0a); }
  .runs { margin: 0.4rem 0 0; font-size: 0.8rem; color: var(--muted); overflow-wrap: anywhere; }
  footer { display: flex; align-items: center; gap: 0.6rem; margin-top: 0.55rem; flex-wrap: wrap; }
  .marks { display: inline-flex; gap: 3px; }
  .m { width: 8px; height: 8px; border-radius: 2px; background: var(--muted); }
  .m.ok { background: var(--verified); }
  .m.bad { background: var(--failed, #ff3b30); }
  .m.skip { opacity: 0.4; }
  .last { color: var(--muted); overflow-wrap: anywhere; flex: 1 1 12rem; }
  .digest { color: var(--muted); margin-left: auto; }
  .starters pre { margin: 0.5rem 0 0; font-size: 0.78rem; white-space: pre-wrap; overflow-wrap: anywhere; }
  .starters header { justify-content: space-between; }
  .bad { color: var(--failed, #ff3b30); }
</style>
