<script lang="ts">
  // Feedback between projects: what this project asks of the projects it
  // depends on — each item with where it stands, and for a project only on
  // GitHub, its prepared issue — and what other projects ask of this one,
  // to take up as a spec, record as a bug or decline. Another project's
  // text is shown as theirs; nothing is written into another project.
  import { app } from "../lib/app.svelte";
  import { get, post } from "../lib/api";
  import type { FeedbackReport, FeedbackStatus, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";

  let r = $state<FeedbackReport | null>(null);
  let open = $state(new Set<string>());
  let declining = $state<string | null>(null);
  let reason = $state("");
  let confirm = $state<{ upstream: string; id: string; repo: string; title: string; body: string } | null>(null);
  const load = () => get<Reply<FeedbackReport>>("/api/feedback").then((x) => (r = x.data ?? null)).catch(() => {});
  $effect(() => {
    void app.job?.running;
    void load();
  });

  const word = (s: FeedbackStatus): string => {
    switch (s.state) {
      case "not_seen": return "not seen yet";
      case "taken_up": return s.unit ? `taken up as ${s.unit}` : "taken up as a spec";
      case "bug": return s.task ? `recorded as bug ${s.task}` : "recorded as a bug";
      case "fixed": return `fixed in ${s.commit}${s.by_agent ? " (by an agent)" : ""}`;
      case "declined": return `declined — ${s.reason}`;
      case "filed": return `issue #${s.number}${s.issue_state ? ` · ${s.issue_state}` : ""}`;
      case "unknown": return s.why;
    }
  };
  const tone = (s: FeedbackStatus) => (s.state === "fixed" ? "ok" : s.state === "declined" || s.state === "unknown" ? "warn" : s.state === "not_seen" ? "" : "on");
  const toggle = (k: string) => {
    const n = new Set(open);
    if (n.has(k)) n.delete(k);
    else n.add(k);
    open = n;
  };

  async function decide(source: string, id: string, decision: "taken_up" | "bug" | "declined", why?: string) {
    const x = await post<{ ok: boolean; error?: string }>("/api/feedback/decide", { source, id, decision, reason: why });
    if (!x.ok) app.toast({ tone: "warn", text: x.error ?? "Not recorded" });
    await load();
  }
  // The asking project's words, quoted as theirs; the decision is recorded
  // only when the sheet is submitted.
  const quoted = (t: string) => t.split("\n").map((l) => `> ${l}`).join("\n");
  function takeUp(name: string, path: string, id: string, title: string, text: string) {
    app.pendingFeedback = { source: path, id, decision: "taken_up" };
    app.newFeature = { text: `${title}\n\n${name} asks (${id}), in its own words:\n\n${quoted(text)}`, through: "tasks" };
  }
  function asBug(name: string, path: string, id: string, title: string, text: string) {
    app.pendingFeedback = { source: path, id, decision: "bug" };
    app.bugSheet = { symptom: `From ${name} (${id}): ${title} — ${text}`.slice(0, 2000) };
  }
  async function withdraw(upstream: string, id: string) {
    const x = await post<{ ok: boolean; error?: string }>("/api/feedback/withdraw", { upstream, id });
    if (!x.ok) app.toast({ tone: "warn", text: x.error ?? "Not withdrawn" });
    await load();
  }
  async function decline(source: string, id: string) {
    if (!reason.trim()) return;
    await decide(source, id, "declined", reason.trim());
    declining = null;
    reason = "";
  }
  async function openIssue(upstream: string, id: string) {
    const x = await get<{ ok: boolean; error?: string; prepared?: { url: string } }>(`/api/feedback/prepared?upstream=${encodeURIComponent(upstream)}&id=${encodeURIComponent(id)}`);
    if (x.prepared) window.open(x.prepared.url, "_blank", "noopener");
    else app.toast({ tone: "warn", text: x.error ?? "Not prepared" });
  }
  async function askToFile(upstream: string, id: string) {
    const x = await get<{ ok: boolean; error?: string; prepared?: { repo: string; title: string; body: string } }>(`/api/feedback/prepared?upstream=${encodeURIComponent(upstream)}&id=${encodeURIComponent(id)}`);
    if (x.prepared) confirm = { upstream, id, ...x.prepared };
    else app.toast({ tone: "warn", text: x.error ?? "Not prepared" });
  }
  async function file() {
    if (!confirm) return;
    const c = confirm;
    confirm = null;
    const x = await post<{ ok: boolean; error?: string; number?: number; url?: string }>("/api/feedback/issue", { upstream: c.upstream, id: c.id });
    if (x.ok) app.toast({ tone: "ok", text: `Filed as ${c.repo}#${x.number}`, detail: x.url });
    else app.toast({ tone: "warn", text: x.error ?? "Not filed" });
    await load();
  }
</script>

<section class="feedback" aria-labelledby="fb-title">
  <h2 id="fb-title" class="section-title">Feedback Between Projects</h2>
  {#if !r}
    <p class="quiet">Reading…</p>
  {:else if !r.upstreams.length && !r.requests.length}
    <p class="quiet">This project asks nothing of others yet. Declare a project it depends on as <code>[upstream.&lt;name&gt;]</code> with <code>path</code> or <code>repo</code> in speccoder.toml; your agent reports to it through <code>speccoder mcp</code>.</p>
  {/if}
  {#each r?.upstreams ?? [] as u (u.name)}
    <div class="block">
      <h3>Asked of {u.name} <span class="quiet mono">{u.outbox}</span></h3>
      {#if !u.items.length}<p class="quiet">Nothing open{u.exists ? "" : " — the outbox is written by your agent or by you"}.</p>{/if}
      <ul>
        {#each u.items as i (i.id)}
          {@const k = `${u.name}#${i.id}`}
          <li>
            <button class="line" aria-expanded={open.has(k)} onclick={() => toggle(k)}>
              <span class="mono id">{i.id}</span><span class="t">{i.title}</span>
              <span class="chip {tone(i.status)}">{!u.path && i.status.state === "not_seen" ? "not filed yet" : word(i.status)}</span>{#if i.changed}<span class="chip warn">changed since</span>{/if}
            </button>
            {#if open.has(k)}<p class="text">{i.text}</p>{/if}
            <div class="acts">
              {#if u.repo && i.status.state !== "filed"}
                <button class="btn small" onclick={() => openIssue(u.name, i.id)} title="GitHub's new-issue page, filled in: you file it there"><Icon name="arrowup" size={11} /> Open on GitHub</button>
                <button class="btn small ghost" onclick={() => askToFile(u.name, i.id)} title="Filed with your GitHub sign-in, after you saw exactly what is sent">File Issue…</button>
              {/if}
              {#if i.status.state === "fixed" || i.status.state === "declined"}
                <button class="btn small ghost" onclick={() => withdraw(u.name, i.id)} title="Remove it from the outbox: adopted, or no longer needed">{i.status.state === "fixed" ? "Adopted — Withdraw" : "Withdraw"}</button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </div>
  {/each}
  {#if r?.requests.length}
    <div class="block">
      <h3>Asked of this project</h3>
      <ul>
        {#each r.requests as q (q.path + q.id)}
          {@const k = `req:${q.path}#${q.id}`}
          <li>
            <button class="line" aria-expanded={open.has(k)} onclick={() => toggle(k)}>
              <span class="mono id">{q.source} · {q.id}</span><span class="t">{q.title}</span>
              <span class="chip {tone(q.status)}">{word(q.status)}</span>{#if q.changed}<span class="chip warn">changed since</span>{/if}
            </button>
            {#if open.has(k)}<p class="text"><span class="quiet">{q.source}'s words:</span> {q.text}</p>{/if}
            {#if declining === k}
              <form class="decline" onsubmit={(e) => { e.preventDefault(); void decline(q.path, q.id); }}>
                <input aria-label="Why it is declined" placeholder="Why — {q.source} reads it" bind:value={reason} />
                <button class="btn small danger" disabled={!reason.trim()}>Decline</button>
                <button type="button" class="btn small ghost" onclick={() => (declining = null)}>Cancel</button>
              </form>
            {:else if q.status.state === "not_seen" || q.changed}
              <div class="acts">
                <button class="btn small primary" onclick={() => takeUp(q.source, q.path, q.id, q.title, q.text)}>Start a Spec</button>
                <button class="btn small" onclick={() => asBug(q.source, q.path, q.id, q.title, q.text)}>Record as Bug</button>
                <button class="btn small ghost" onclick={() => (declining = k)}>Decline…</button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</section>

<svelte:window onkeydowncapture={(e) => { if (e.key === "Escape" && confirm) { confirm = null; e.stopImmediatePropagation(); } }} />

{#if confirm}
  {@const c = confirm}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (confirm = null)}>
    <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="fi-title" tabindex="-1" {@attach (el) => el.focus()}>
      <h2 id="fi-title">File this issue in {c.repo}?</h2>
      <p class="quiet">Sent with your GitHub sign-in, exactly as shown; filed once.</p>
      <p class="title">{c.title}</p>
      <pre class="body">{c.body}</pre>
      <div class="row">
        <button class="btn" onclick={() => (confirm = null)}>Cancel</button>
        <button class="btn primary" onclick={file}>File Issue</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .feedback { display: grid; gap: 0.6rem; }
  .section-title { margin: 0; font: 650 1rem/1.2 var(--display); }
  .block { border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow); padding: 0.6rem 0.9rem; }
  h3 { margin: 0.2rem 0 0.4rem; font-size: 0.84rem; font-weight: 650; display: flex; gap: 0.5rem; align-items: baseline; flex-wrap: wrap; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.15rem; }
  li { min-width: 0; }
  .line { all: unset; box-sizing: border-box; cursor: pointer; width: 100%; display: flex; flex-wrap: wrap; gap: 0.45rem; align-items: baseline; padding: 0.3rem 0.25rem; border-radius: 6px; font-size: 0.86rem; }
  .line:hover { background: var(--fill); }
  .line:focus-visible { outline: 2px solid var(--focus); }
  .id { font-size: 0.76rem; color: var(--muted); }
  .t { flex: 1; min-width: 8rem; font-weight: 550; }
  .chip { font-size: 0.72rem; border-radius: 999px; padding: 0.05rem 0.5rem; background: var(--fill); color: var(--text-2); }
  .chip.on { color: var(--accent-ink); background: var(--accent-soft); }
  .chip.ok { color: var(--verified); background: color-mix(in srgb, var(--verified) 12%, transparent); }
  .chip.warn { color: var(--attention); background: color-mix(in srgb, var(--attention) 12%, transparent); }
  .text { margin: 0.2rem 0.25rem 0.4rem; font-size: 0.8rem; white-space: pre-wrap; color: var(--text-2); }
  .acts, .decline { display: flex; flex-wrap: wrap; gap: 0.35rem; margin: 0.15rem 0.25rem 0.45rem; }
  .decline input { flex: 1; min-width: 10rem; font: inherit; font-size: 0.82rem; padding: 0.3rem 0.5rem; border-radius: 7px; border: 1px solid var(--border); background: var(--surface); color: var(--text); }
  .quiet { color: var(--muted); font-size: 0.8rem; font-weight: 400; }
  .mono { font-family: var(--mono); }
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 24; padding: 1rem; }
  .sheet:focus { outline: none; }
  .sheet { width: min(600px, 100%); max-height: 88vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.2rem 1.3rem; display: grid; gap: 0.6rem; }
  .sheet h2 { margin: 0; font: 650 1.1rem/1.3 var(--display); overflow-wrap: anywhere; }
  .title { margin: 0; font-weight: 650; }
  .body { margin: 0; max-height: 40vh; overflow: auto; padding: 0.6rem 0.8rem; border-radius: var(--radius); background: var(--surface-2); font-size: 0.78rem; white-space: pre-wrap; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
