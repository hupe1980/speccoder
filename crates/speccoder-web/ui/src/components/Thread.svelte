<script lang="ts">
  // An attempt as one column: the model's thoughts and words (muted, its
  // claims), each call one line, reads folded, and the evidence as the only
  // bold rows. Built by the host from the record — the same live and after
  // a reload; while the attempt runs it is fetched again as events arrive.
  import { app } from "../lib/app.svelte";
  import { get, latest, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import type { Reply, Thread, ThreadRow } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { feature, task, live = false }: { feature: string; task: string; live?: boolean } = $props();

  let thread = $state<Thread | null>(null);
  let error = $state<string | null>(null);
  let pick = $state<string | null>(null);
  let open = $state(new Set<number>());
  let note = $state("");
  const fetchOf = latest();

  function load() {
    fetchOf(
      get<Reply<Thread>>(`/api/thread${q({ feature, task, run: pick ?? undefined })}`),
      (r) => {
        error = r.data ? null : r.message || "The thread cannot be read";
        if (r.data) thread = r.data;
      },
      (e) => (error = String(e)),
    );
  }
  // Again on another task or attempt, and when a job ends.
  $effect(() => {
    void feature;
    void task;
    void pick;
    void app.job?.running;
    void (app.elsewhere ? app.live : null); // a run in another process: as it is polled
    load();
  });
  // While it runs: again as its events arrive, at most every 400 ms (a
  // pending fetch is never pushed back by the next event).
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void app.pulse;
    if (!live || timer) return;
    timer = setTimeout(() => {
      timer = null;
      load();
    }, 400);
  });
  $effect(() => () => {
    if (timer) clearTimeout(timer);
  });

  // Follow the end while the person is there; else count what arrives.
  let box = $state<HTMLElement | null>(null);
  let atEnd = $state(true);
  let seen = $state(0);
  // What the thread holds, counting folded calls and rows dropped at the
  // cap: a read joining a fold is new too.
  const count = $derived((thread?.more ?? 0) + (thread?.rows ?? []).reduce((n, r) => n + (r.items?.length || 1), 0));
  const fresh = $derived(atEnd ? 0 : Math.max(0, count - seen));
  $effect(() => {
    const n = count;
    if (!box || !atEnd) return;
    seen = n;
    queueMicrotask(() => box?.scrollTo?.({ top: box.scrollHeight }));
  });
  function scrolled() {
    if (!box) return;
    atEnd = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
    if (atEnd) seen = count;
  }
  function toEnd() {
    atEnd = true;
    seen = count;
    box?.scrollTo?.({ top: box.scrollHeight, behavior: "smooth" });
  }

  const toggle = (i: number) => {
    const s = new Set(open);
    if (s.has(i)) s.delete(i);
    else s.add(i);
    open = s;
  };
  const mark = (r: ThreadRow) =>
    r.outcome === "failed" ? "✗" : r.outcome === "refused" ? "⊘" : r.outcome === "waiting" ? "…" : "✓";
  const lines = (r: ThreadRow) =>
    r.added != null || r.removed != null ? ` +${r.added ?? 0} −${r.removed ?? 0}` : "";
  const label = (o: string) => o.replace(/_/g, " ");
  function nudge() {
    const text = note.trim();
    if (!text) return;
    void app.perform(
      offer({ kind: "nudge", task }, { group: "task", label: `nudge ${task}`, why: "a note the next attempt reads", command: `speccoder task nudge ${task} --note ${JSON.stringify(text)}` }),
      text,
    );
    note = "";
  }
</script>

<section class="thread" aria-label="The attempt's thread">
  <header>
    <span class="title"><Icon name="sparkle" size={12} weight={2.2} /> Thread</span>
    {#if thread && thread.attempts.length > 1}
      <select aria-label="Attempt" value={pick ?? thread.attempt?.run ?? ""} onchange={(e) => (pick = (e.currentTarget as HTMLSelectElement).value)}>
        {#each thread.attempts as a, i (a.run)}<option value={a.run}>attempt {i + 1} · {label(a.outcome)}</option>{/each}
      </select>
    {:else if thread?.attempt}
      <span class="muted small">{label(thread.attempt.outcome)}</span>
    {/if}
    <span class="legend muted small">muted: the model's words · <strong>bold</strong>: evidence</span>
  </header>
  {#if error && !thread}
    <p class="muted small">{error}</p>
  {:else if !thread}
    <p class="muted small">Reading the record…</p>
  {:else if !thread.rows.length}
    <p class="muted small">{thread.attempts.length ? "Nothing recorded for this attempt yet." : "No attempt yet: the thread starts when the task runs."}</p>
  {:else}
    <div class="rows" bind:this={box} onscroll={scrolled}>
      {#if thread.more}<p class="muted small more">{thread.more} earlier rows — <code>speccoder task show {task} --thread</code> lists them all</p>{/if}
      <ol>
        {#each thread.rows as r, n (thread.more + n)}
          {@const i = thread.more + n}
          <li class="r {r.kind}" class:info={r.tone === "info"} data-tone={r.tone}>
            {#if r.kind === "thought"}
              <button class="line" aria-expanded={open.has(i)} onclick={() => toggle(i)}>
                <span class="glyph">∴</span><span class="claim">{r.conclusion ?? r.text}</span>
              </button>
              {#if open.has(i) && r.conclusion && r.conclusion !== r.text}<p class="body claim">{(r.text ?? "").slice(0, (r.text ?? "").lastIndexOf(r.conclusion)).trim() || r.text}</p>{/if}
            {:else if r.kind === "words"}
              <p class="line words"><span class="glyph">“</span><span class="claim">{r.text}</span></p>
            {:else if r.kind === "fold"}
              <button class="line" aria-expanded={open.has(i)} onclick={() => toggle(i)}>
                <Icon name={open.has(i) ? "down" : "chevron"} size={10} /><span>{r.text}</span>
              </button>
              {#if open.has(i)}
                <ul class="items">{#each r.items ?? [] as it, j (j)}<li><span class="verb">{it.verb}</span> <span class="mono obj">{it.object}</span>{#if it.outcome && it.outcome !== "ok"} <span class="o-{it.outcome}">{label(it.outcome)}</span>{/if}</li>{/each}</ul>
              {/if}
            {:else if r.kind === "call" || r.kind === "approval"}
              <button class="line" aria-expanded={r.detail ? open.has(i) : undefined} disabled={!r.detail && r.kind !== "approval"} onclick={() => toggle(i)}>
                <span class="m o-{r.outcome ?? 'ok'}" aria-label={r.outcome ?? "ok"}>{mark(r)}</span>
                <span class="verb">{r.verb}</span>
                <span class="mono obj" title={r.object ?? ""}>{r.object}{lines(r)}</span>
                {#if r.decided}<span class="tag" class:you={r.decided === "you"} class:no={r.decided === "refused"}>{r.decided === "you" ? "you approved" : r.decided}</span>{/if}
                {#if r.outcome === "waiting"}<span class="tag wait">waits for you</span>{/if}
              </button>
              {#if r.kind === "approval" && r.call}
                <div class="acts"><button class="btn small" onclick={() => (app.explain = { task: `${feature}/${task}`, call: r.call! })}><Icon name="info" size={11} /> Explain</button><button class="btn small ghost" onclick={() => app.open("approvals")}>Decide in Needs You</button></div>
              {/if}
              {#if open.has(i) && r.detail}<pre class="detail">{r.detail}</pre>{/if}
            {:else if r.kind === "divider"}
              <p class="divider t-{r.tone ?? 'info'}" role="separator" aria-label={r.text}>{r.text}</p>
            {/if}
          </li>
        {/each}
        {#if thread.live}<li class="r now muted small"><span class="spinner"></span> working — the thread grows as it goes</li>{/if}
      </ol>
    </div>
    {#if fresh > 0}<button class="pill" onclick={toEnd}>↓ {fresh} new</button>{/if}
  {/if}
  <form class="nudge" onsubmit={(e) => { e.preventDefault(); nudge(); }}>
    <input aria-label="Nudge the next attempt" placeholder="A note the next attempt reads…" bind:value={note} />
    <button class="btn small" disabled={!note.trim()} title="Not sent to the running model: the task's next attempt reads it">Nudge</button>
  </form>
</section>

<style>
  .thread { min-width: 0; position: relative; border-radius: var(--radius); background: var(--surface); padding: 0.6rem 0.75rem; display: flex; flex-direction: column; gap: 0.45rem; }
  header { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  .title { font-weight: 600; font-size: 0.88rem; display: inline-flex; gap: 0.35rem; align-items: center; }
  .legend { margin-left: auto; }
  select { font: inherit; font-size: 0.78rem; }
  .rows { max-height: 26rem; overflow-y: auto; }
  li { min-width: 0; }
  ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.1rem; }
  .line { all: unset; box-sizing: border-box; width: 100%; display: flex; gap: 0.4rem; align-items: baseline; padding: 0.15rem 0.25rem; border-radius: 6px; font-size: 0.82rem; cursor: pointer; min-width: 0; }
  .line:disabled { cursor: default; }
  .line:not(:disabled):hover { background: var(--fill); }
  .line:focus-visible { outline: 2px solid var(--focus); }
  .glyph { color: var(--muted); width: 1em; flex: none; text-align: center; }
  .claim { color: var(--muted); font-style: italic; }
  .words .claim::after { content: "”"; }
  .body { margin: 0.1rem 0 0.3rem 1.65rem; font-size: 0.8rem; white-space: pre-wrap; }
  .verb { font-weight: 500; flex: none; }
  .obj { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; font-size: 0.78rem; }
  .m { width: 1em; flex: none; text-align: center; }
  .o-ok { color: var(--verified); }
  .o-failed, .o-refused { color: var(--danger); }
  .o-waiting { color: var(--attention); }
  .tag { margin-left: auto; flex: none; font-size: 0.72rem; color: var(--muted); border: 1px solid var(--border); border-radius: 999px; padding: 0 0.4rem; }
  .tag.you { color: var(--accent); }
  .tag.no, .tag.wait { color: var(--attention); }
  .items { margin: 0.1rem 0 0.3rem 1.65rem; padding: 0; list-style: none; font-size: 0.78rem; }
  .detail { margin: 0.1rem 0 0.35rem 1.65rem; font-size: 0.74rem; max-height: 14rem; overflow: auto; white-space: pre-wrap; background: var(--fill); padding: 0.4rem 0.5rem; border-radius: 6px; }
  .acts { display: flex; gap: 0.35rem; margin: 0.15rem 0 0.3rem 1.65rem; flex-wrap: wrap; }
  .divider { margin: 0.35rem 0; padding: 0.3rem 0.5rem; font-weight: 700; font-size: 0.82rem; border-top: 1px solid currentColor; border-radius: 0; }
  .t-verified { color: var(--verified); }
  .t-regression { color: var(--regression); }
  .t-evidence { color: var(--text); }
  .t-failed, .t-blocked { color: var(--danger); }
  .t-stopped { color: var(--attention); }
  .t-info { color: var(--muted); font-weight: 500; border-top-style: dashed; }
  .now { display: flex; gap: 0.4rem; align-items: center; padding: 0.2rem 0.25rem; }
  .more { margin: 0 0 0.3rem; }
  .pill { position: absolute; bottom: 3.2rem; left: 50%; transform: translateX(-50%); border: none; border-radius: 999px; padding: 0.2rem 0.7rem; background: var(--accent); color: var(--accent-text-on); font: inherit; font-size: 0.78rem; cursor: pointer; box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2); }
  .nudge { display: flex; gap: 0.35rem; }
  .nudge input { flex: 1; min-width: 0; font: inherit; font-size: 0.8rem; padding: 0.25rem 0.45rem; border-radius: 6px; border: 1px solid var(--border); background: transparent; color: inherit; }
  .muted { color: var(--muted); }
  .small { font-size: 0.78rem; }
</style>
