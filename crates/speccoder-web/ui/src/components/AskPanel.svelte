<script lang="ts">
  // Ask: a read-only conversation beside the view. The answer is the
  // model's words; its sources are what code checked it read this turn;
  // a change it suggests is a card that opens a sheet — nothing here
  // changes the project.
  import { tick } from "svelte";
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { ask, stepWords, type ChipRef } from "../lib/ask.svelte";
  import { blocks, type Part } from "../lib/ask-text";
  import type { AskReport, AskResult, Citation, Proposal } from "../lib/schema.gen";
  import Account from "./Account.svelte";
  import Icon from "./Icon.svelte";
  import PromoteOffer from "./PromoteOffer.svelte";

  let text = $state("");
  let box = $state<HTMLTextAreaElement>();
  let log = $state<HTMLElement>();
  let announce = $state("");
  let openSteps = $state<Record<number, boolean>>({});

  const KIND: Record<string, [string, string]> = {
    quick_change: ["Quick change", "sparkle"],
    bug_fix: ["Bug fix", "exclaim"],
    nudge: ["Nudge", "hand"],
    answer: ["Answer", "check"],
    start_spec: ["Start a spec", "doc"],
  };

  // ⌘L and new conversations focus the composer.
  $effect(() => {
    void ask.focus;
    void tick().then(() => box?.focus());
  });
  // What a confirmed proposal created is recorded by its job: read the
  // results again when a job ends.
  let jobRan = false;
  $effect(() => {
    const running = !!app.job?.running;
    if (jobRan && !running) void ask.refresh();
    jobRan = running;
  });
  // An empty panel offers the conversations to go back to.
  $effect(() => {
    if (!ask.turns.length && !ask.list) void ask.loadRecent();
  });
  // The list takes ↑↓; the first conversation has focus when it opens.
  let listEl = $state<HTMLElement>();
  $effect(() => {
    if (ask.list?.length) void tick().then(() => listEl?.querySelector<HTMLElement>(".conv")?.focus());
  });
  function listKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const all = [...(listEl?.querySelectorAll<HTMLElement>(".conv") ?? [])];
    const i = all.indexOf(document.activeElement as HTMLElement);
    const next = all[Math.min(all.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)))];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  }
  const when = (at: number) => new Date(at * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });

  // The newest turn in view; one announcement per answer, never per word.
  $effect(() => {
    void ask.turns.length;
    void ask.pending?.steps.length;
    void ask.pending?.words.length;
    void tick().then(() => log?.scrollTo({ top: log.scrollHeight, behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" }));
  });
  $effect(() => {
    const last = ask.turns.at(-1);
    if (!last) return;
    const n = last.citations.filter((c) => c.state === "read").length;
    announce = last.end === "answered" ? `Answer ready, ${n} ${n === 1 ? "source" : "sources"}` : last.end === "stopped" ? "Stopped" : (last.reason ?? "The turn stopped");
  });

  async function send(q = text) {
    if (!q.trim()) return;
    if (ask.pending?.stopping) {
      // Stopped, the stop still landing: the question waits for it.
      ask.queued = q.trim();
      text = "";
      return;
    }
    if (ask.pending) return;
    text = "";
    await ask.send(q);
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      // While a turn is answered, Enter keeps the draft for the next one.
      if (!ask.pending || ask.pending.stopping) void send();
    } else if (e.key === "ArrowUp" && !text) {
      const last = ask.turns.at(-1)?.question ?? ask.failed?.question;
      if (last) {
        e.preventDefault();
        text = last;
      }
    } else if (e.key === "Backspace" && !text && box?.selectionStart === 0) {
      if (ask.chips.length) ask.remove(ask.chips.length - 1);
      else if (ask.viewChip) ask.remove("view");
    }
  }

  function panelKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      if (ask.sheet) return;
      // The first Escape stops a turn in flight; the next leaves.
      if (ask.pending && !ask.pending.stopping) return void ask.stop();
      if (e.target === box) (document.querySelector("main") as HTMLElement | null)?.focus();
      else ask.toggle(false);
    } else if (e.key === "n" && e.altKey && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      ask.fresh();
    } else if (e.key === "/" && !(e.target as HTMLElement).closest("textarea, input")) {
      e.preventDefault();
      box?.focus();
    }
  }

  // The divider: drag, or ←/→ (Home/End: narrowest, widest).
  function drag(e: PointerEvent) {
    const start = e.clientX;
    const from = ask.width;
    const move = (m: PointerEvent) => ask.resize(from + (start - m.clientX));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
  function gripKey(e: KeyboardEvent) {
    const by: Record<string, number> = { ArrowLeft: 24, ArrowRight: -24 };
    if (e.key in by) ask.resize(ask.width + (by[e.key] ?? 0));
    else if (e.key === "Home") ask.resize(320);
    else if (e.key === "End") ask.resize(640);
    else return;
    e.preventDefault();
  }

  /** A source opens where it lives: a file at its lines in Code, a chip's task in the inspector. */
  // A source opens where it lives: a file at its lines (marked, and said
  // when the file no longer says what was read), evidence in the task's
  // inspector, a run's account in place.
  function openSource(c: Citation, turn: AskReport) {
    if (c.state !== "read") return;
    const read = turn.reads.find((r) => r.id === c.id);
    const chip = turn.chips.find((x) => x.id === c.id);
    const path = c.path ?? read?.path ?? null;
    if (path) {
      const from = c.lines?.[0] ?? read?.start ?? 1;
      const to = c.lines?.[1] ?? read?.end ?? from;
      app.codeMode = "files";
      app.codeFile = path;
      app.codeLine = from;
      app.codeCited = { path, from, to, changed: null };
      app.view = "code";
      if (read?.content_sha256 && read.path) {
        void get<{ changed: boolean | null }>(`/api/ask/changed${q({ path: read.path, start: read.start?.toString(), end: read.end?.toString(), sha: read.content_sha256 })}`)
          .then((r) => {
            if (app.codeCited?.path === path) app.codeCited = { ...app.codeCited, changed: r.changed };
          })
          .catch(() => {});
      }
      return;
    }
    if (read?.tool === "task_evidence" && read.subject) return void app.selectTask(read.subject);
    if (read?.tool === "journal_account" && read.subject) {
      const from = c.lines?.[0] ?? read.start ?? null;
      return void ask.peekAccount(read.subject, from ? [from, c.lines?.[1] ?? read.end ?? from] : null);
    }
    const task = (chip?.reference as { task?: string } | undefined)?.task;
    if (task) void app.selectTask(task);
  }

  function chipName(c: ChipRef) {
    return c.label ?? String((c.reference as Record<string, unknown>).task ?? (c.reference as Record<string, unknown>).path ?? c.kind);
  }

  function review(p: Proposal, turn: number) {
    if (p.kind === "start_spec") {
      // A quick change or bug in context that grew: the same promotion path
      // as Needs You, carrying this conversation too (FR-025).
      const o = ask.offer;
      if (o) return void promote(o);
      // Every way to start a spec lands in the one New feature sheet.
      app.newFeature = { text: p.fields.description ?? p.title, through: "tasks" };
      return;
    }
    ask.sheet = { proposal: p, turn };
  }

  /** What a proposal created, opened where it lives. */
  async function openResult(r: AskResult) {
    await app.selectFeature(r.unit);
    app.view = "board";
    if (r.task) await app.selectTask(r.task);
  }
  const made = (r: AskResult) => (r.kind === "spec" ? `Started ${r.unit}` : `Recorded ${r.task ?? r.requirement ?? "it"} in ${r.unit}`);

  /** Promote a task in context: the New feature sheet, with this
   * conversation among those it carries. */
  function promote(o: NonNullable<typeof ask.offer>) {
    const a = o.actions[0];
    const r = a?.run;
    if (!a || !r || r.kind !== "plan_through" || r.from.kind !== "create" || r.from.source.kind !== "promotion") return void (a && app.perform(a));
    const carried = r.from.source.conversations ?? [];
    const conversations = ask.conversation && !carried.includes(ask.conversation) ? [ask.conversation, ...carried] : carried;
    const task = r.from.source.task;
    const command = `speccoder spec promote ${task}${conversations.map((c) => ` --conversation ${c}`).join("")}`;
    void app.perform(offer({ ...r, from: { ...r.from, source: { ...r.from.source, conversations } } }, { group: "review", label: a.label, why: o.detail, command }));
  }

  const sourcesOf = (t: AskReport) => t.citations.filter((c) => c.state === "read");
  const unread = (t: AskReport) => t.citations.filter((c) => c.state !== "read");
  const scope = $derived(app.report?.data?.feature ?? app.project?.name ?? "this project");
</script>

{#snippet line(parts: Part[], turn: AskReport | null)}
  {#each parts as p, i (i)}
    {#if p.t === "text"}{p.text}{:else if p.t === "code"}<code>{p.text}</code>{:else if p.t === "bold"}<strong>{p.text}</strong>{:else if p.t === "mark"}<span class="cite live" aria-hidden="true"></span>{:else if p.n !== null && turn}<button class="cite" title={p.citation.label} aria-label="Source {p.n}: {p.citation.label}" onclick={() => openSource(p.citation, turn)}>{p.n}</button>{:else}<span class="cite unread" title="Not among what was read this turn" aria-label="A citation to something not read this turn">?</span>{/if}
  {/each}
{/snippet}

{#snippet words(text: string, turn: AskReport | null, live = false)}
  {#each blocks(text, turn?.citations ?? [], live) as b, i (i)}
    {#if b.kind === "pre"}<pre>{b.text}</pre>{:else if b.kind === "h"}<h4>{@render line(b.parts, turn)}</h4>{:else if b.kind === "li" || b.kind === "ol"}<div class="li" class:ol={b.kind === "ol"}>{@render line(b.parts, turn)}</div>{:else}<p>{@render line(b.parts, turn)}</p>{/if}
  {/each}
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="ask glass" aria-label="Ask" style="--w:{ask.width}px" onkeydown={panelKey}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="grip"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize Ask"
    aria-valuemin={320}
    aria-valuemax={640}
    aria-valuenow={ask.width}
    tabindex="0"
    onpointerdown={drag}
    onkeydown={gripKey}
  ></div>

  <header>
    <span class="mark" aria-hidden="true"><Icon name="bubble" size={14} weight={2} /></span>
    <div class="title">
      <strong>Ask</strong>
      <span class="scope" title="What the questions are about">about {scope}</span>
    </div>
    <button class="hbtn" title="Conversations" aria-label="Conversations" aria-pressed={ask.list !== null} onclick={() => (ask.list ? (ask.list = null) : void ask.showList())}><Icon name="list" size={15} /></button>
    <button class="hbtn" title="New conversation (⌥⌘N)" aria-label="New conversation" onclick={() => ask.fresh()}><Icon name="plus" size={15} /></button>
    <button class="hbtn" title="Close (⌘L)" aria-label="Close Ask" onclick={() => ask.toggle(false)}><Icon name="xmark" size={14} /></button>
  </header>

  <div class="log" role="log" bind:this={log}>
    {#if ask.list}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="list" bind:this={listEl} onkeydown={listKey}>
        <h3>Conversations</h3>
        {#each ask.list as c (c.id)}
          <button class="conv" onclick={() => void ask.resume(c.id)}>
            <span class="ct">{c.title}</span>
            <span class="cm">{when(c.at)} · {c.turns} {c.turns === 1 ? "turn" : "turns"}</span>
          </button>
        {:else}
          <p class="quiet">No conversations yet. What you ask is kept here, for this project.</p>
        {/each}
      </div>
    {:else if !ask.turns.length && !ask.pending && !ask.failed}
      <div class="empty">
        <div class="orb" aria-hidden="true"><Icon name="bubble" size={26} weight={1.6} /></div>
        <h3>Ask about {scope}</h3>
        <p>Specs, code, runs and their evidence. Every source is checked against what was actually read — and Ask changes nothing.</p>
        {#if app.report || app.project}
          <div class="sugg">
            {#each ask.suggestions as s (s)}
              <button onclick={() => void send(s)}>{s}</button>
            {/each}
          </div>
        {/if}
        {#if ask.recent.length}
          <div class="recent" aria-label="Pick up where you left off">
            <span class="rh">Pick up where you left off</span>
            {#each ask.recent as c (c.id)}
              <button class="conv" onclick={() => void ask.resume(c.id)}>
                <Icon name="bubble" size={12} />
                <span class="ct">{c.title}</span>
                <span class="cm">{when(c.at)}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if !ask.list}
      {#each ask.turns as t (t.turn)}
        <div class="q">{t.question}</div>
        <article class="a" aria-label="Answer {t.turn}">
          {#if t.reads.length}
            <button class="steps-t" aria-expanded={!!openSteps[t.turn]} onclick={() => (openSteps[t.turn] = !openSteps[t.turn])}>
              <Icon name="chevron" size={11} /> {t.reads.length === 1 ? stepWords({ tool: t.reads[0]!.tool, target: t.reads[0]!.target, refused: false }) : `Read ${t.reads.length} sources`}
            </button>
            {#if openSteps[t.turn]}
              <ul class="steps">{#each t.reads as r (r.id)}<li><span class="rid">{r.id}</span> {stepWords({ tool: r.tool, target: r.target, refused: false })}</li>{/each}</ul>
            {/if}
          {/if}
          {#if t.answer}
            <div class="said">
              <span class="who">The model says</span>
              {@render words(t.answer, t)}
            </div>
            {#if sourcesOf(t).length}
              <div class="sources">
                {#each sourcesOf(t) as c, i (c.marker)}
                  <button class="src" onclick={() => openSource(c, t)} title="Open {c.label}"><span class="n">{i + 1}</span><span class="sl">{c.label}</span></button>
                {/each}
              </div>
            {:else if t.end !== "stopped"}
              <p class="nosrc">No sources — the model's words only.</p>
            {/if}
            {#if unread(t).length}
              <p class="unread-note"><Icon name="exclaim" size={12} /> {unread(t).length} {unread(t).length === 1 ? "citation names" : "citations name"} something not read this turn: {unread(t).map((c) => c.marker).join(", ")}</p>
            {/if}
          {/if}
          {#if t.end === "stopped"}
            <p class="halted"><Icon name="stop" size={11} /> Stopped{t.answer ? " — the words so far" : ""}</p>
          {:else if t.end !== "answered"}
            <div class="stopped" class:budget={t.end === "budget"}>
              <Icon name={t.end === "budget" ? "gauge" : "xoctagon"} size={14} />
              <span>{t.reason ?? "The turn stopped"}</span>
            </div>
          {/if}
          {#if t.proposal}
            {@const p = t.proposal}
            {@const k = KIND[p.kind] ?? [p.kind, "sparkle"]}
            <div class="card" class:blocked={!!p.blocked}>
              <span class="glyph" aria-hidden="true"><Icon name={k[1]} size={14} weight={2} /></span>
              <div class="cw">
                <span class="ck">{k[0]}{p.fields.target ? ` ${p.fields.target}` : ""}</span>
                <strong>{p.title}</strong>
                {#if p.blocked}<span class="why">{p.blocked}</span>{:else}<code class="cmd">{p.command}</code>{/if}
              </div>
              <button class="btn small" disabled={!!p.blocked} onclick={() => review(p, t.turn)}>Review…</button>
            </div>
          {/if}
          {#each ask.results.filter((r) => r.turn === t.turn) as r, i (i)}
            <button class="made" onclick={() => void openResult(r)}><Icon name="check" size={11} weight={2.6} /><span>{made(r)}</span>{" "}<span class="go">Open</span></button>
          {/each}
          {#if ask.confirmed[t.turn]}<p class="made"><Icon name="check" size={11} weight={2.6} /><span>{ask.confirmed[t.turn]}</span></p>{/if}
          <span class="meta">{t.usage.input + t.usage.output} tokens</span>
        </article>
      {/each}

      {#each ask.results.filter((r) => r.turn == null) as r, i (i)}
        <button class="made spec" onclick={() => void openResult(r)}><Icon name="sparkle" size={11} weight={2.2} /><span>{made(r)} from this conversation</span>{" "}<span class="go">Open</span></button>
      {/each}

      {#if ask.pending}
        <div class="q">{ask.pending.question}</div>
        <article class="a live" aria-busy="true">
          <ul class="steps live">
            {#each ask.pending.steps as s, i (i)}<li class:refused={s.refused}><Icon name={s.refused ? "xmark" : "check"} size={11} /> {stepWords(s)}</li>{/each}
            {#if ask.pending.stopping}{:else if !ask.pending.words}<li class="thinking"><span class="dots" aria-hidden="true"><i></i><i></i><i></i></span> {ask.pending.steps.length ? "Reading" : "Thinking"}</li>{/if}
          </ul>
          {#if ask.pending.words}<div class="said">{@render words(ask.pending.words, null, !ask.pending.stopping)}</div>{/if}
          {#if ask.pending.stopping}<p class="halted"><Icon name="stop" size={11} /> Stopped{ask.pending.words ? " — the words so far" : ""}</p>{/if}
        </article>
        {#if ask.queued}<div class="q queued" title="Sent as soon as the stopped turn has ended">{ask.queued}</div>{/if}
      {/if}

      {#if ask.failed}
        <div class="q">{ask.failed.question}</div>
        <div class="stopped">
          <Icon name="xoctagon" size={14} />
          <span>{ask.failed.message}</span>
          <button class="link" onclick={() => { const f = ask.failed!; ask.failed = null; void send(f.question); }}>Retry</button>
        </div>
      {/if}
    {/if}
  </div>
  <div class="sr" aria-live="polite">{announce}</div>

  {#if ask.peek}
    <div class="peek" role="dialog" aria-label="Account of run {ask.peek.run}">
      <div class="ph">
        <div><strong>The run's account</strong><span>the implementing model's words — a claim, never evidence</span></div>
        <button class="hbtn" aria-label="Close the account" onclick={() => (ask.peek = null)}><Icon name="xmark" size={12} weight={2.2} /></button>
      </div>
      <div class="pb">
        {#if !ask.peek.turns}<span class="spinner"></span>{:else if ask.peek.error}<p class="quiet">{ask.peek.error}</p>{:else if !ask.peek.turns.length}<p class="quiet">This run kept no words of its own.</p>{:else}<Account turns={ask.peek.turns} marked={ask.peek.marked ?? null} />{/if}
      </div>
    </div>
  {/if}

  <form class="composer" onsubmit={(e) => { e.preventDefault(); void send(); }}>
    {#if ask.offer}
      {@const o = ask.offer}
      <PromoteOffer item={o} compact onstart={() => promote(o)} />
    {/if}
    {#if ask.viewChip || ask.chips.length}
      <div class="chips" aria-label="Context">
        {#if ask.viewChip}
          <span class="chip view" title="From the view: the subject on screen">
            <span class="dot" aria-hidden="true"></span>{chipName(ask.viewChip)}
            <button type="button" aria-label="Remove {chipName(ask.viewChip)} from context" onclick={() => ask.remove("view")}><Icon name="xmark" size={9} weight={2.4} /></button>
          </span>
        {/if}
        {#each ask.chips as c, i (i)}
          <span class="chip">{chipName(c)}<button type="button" aria-label="Remove {chipName(c)} from context" onclick={() => ask.remove(i)}><Icon name="xmark" size={9} weight={2.4} /></button></span>
        {/each}
      </div>
    {/if}
    <div class="field">
      <textarea
        bind:this={box}
        bind:value={text}
        rows="1"
        placeholder={ask.turns.length ? "Ask a follow-up…" : "Ask about the specs, the code, a run…"}
        aria-label="Your question"
        onkeydown={keydown}
      ></textarea>
      {#if ask.pending && !ask.pending.stopping}
        <button class="send stop" type="button" aria-label="Stop" title="Stop (Esc)" onclick={() => void ask.stop()}>
          <span class="ring" aria-hidden="true"></span><span class="sq" aria-hidden="true"></span>
        </button>
      {:else}
        <button class="send" type="submit" aria-label="Send" disabled={!text.trim() || !!ask.queued}><Icon name="arrowup" size={15} weight={2.4} /></button>
      {/if}
    </div>
    <p class="hint">{ask.pending?.stopping ? (ask.queued ? "Asked — it goes as soon as the stop lands" : "Stopped · Enter asks next") : ask.pending ? "Esc to stop · keep typing, it waits for you" : "Enter to send · ⇧Enter for a new line · reads only, changes nothing"}</p>
  </form>
</aside>

<style>
  .ask {
    position: fixed; top: 64px; right: 10px; bottom: var(--panel-bottom); width: min(var(--w), calc(100vw - 20px)); z-index: 13;
    display: grid; grid-template-rows: auto 1fr auto; border-radius: var(--radius-lg);
    background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); overflow: hidden;
    animation: enter 360ms var(--spring);
  }
  @keyframes enter { from { transform: translateX(24px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .ask { animation: none; } }
  .grip { position: absolute; left: 0; top: 0; bottom: 0; width: 7px; cursor: col-resize; z-index: 1; }
  .grip:hover, .grip:focus-visible { background: linear-gradient(90deg, var(--accent-soft), transparent); outline: none; }

  header { display: flex; align-items: center; gap: 0.5rem; padding: 0.7rem 0.7rem 0.6rem 0.95rem; border-bottom: 0.5px solid var(--separator); }
  .mark { width: 24px; height: 24px; border-radius: 7px; display: grid; place-items: center; color: #fff; background: linear-gradient(160deg, #bf5af2, #5e5ce6); flex: none; }
  .title { flex: 1; display: grid; line-height: 1.15; min-width: 0; }
  .title strong { font: 650 0.92rem/1.2 var(--display); }
  .scope { font-size: 0.74rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hbtn { width: 26px; height: 26px; border: 0; border-radius: 7px; background: none; color: var(--muted); display: grid; place-items: center; cursor: pointer; }
  .hbtn:hover, .hbtn[aria-pressed="true"] { background: var(--fill); color: var(--text); }

  .log { overflow: auto; padding: 0.9rem 0.95rem 1rem; display: grid; align-content: start; gap: 0.7rem; }
  .empty { display: grid; justify-items: center; text-align: center; gap: 0.5rem; padding: 2.2rem 0.6rem 0.5rem; }
  .orb { width: 52px; height: 52px; border-radius: 16px; display: grid; place-items: center; color: #fff; background: radial-gradient(circle at 30% 25%, #da8fff, #5e5ce6 70%); box-shadow: 0 8px 22px color-mix(in srgb, #5e5ce6 35%, transparent); }
  .empty h3 { margin: 0.4rem 0 0; font: 650 1.02rem/1.3 var(--display); }
  .empty p { margin: 0; font-size: 0.83rem; color: var(--text-2); line-height: 1.45; max-width: 30ch; }
  .sugg { display: grid; gap: 0.4rem; width: 100%; margin-top: 0.8rem; }
  .sugg button { text-align: left; border: 0; border-radius: 10px; padding: 0.55rem 0.75rem; background: var(--fill); color: var(--text); font-size: 0.84rem; cursor: pointer; transition: background var(--speed) var(--ease); }
  .sugg button:hover { background: var(--fill-2); }

  .q { justify-self: end; max-width: 88%; padding: 0.5rem 0.75rem; border-radius: 14px 14px 4px 14px; background: var(--accent); color: #fff; font-size: 0.87rem; line-height: 1.4; white-space: pre-wrap; overflow-wrap: anywhere; }
  .a { display: grid; gap: 0.5rem; font-size: 0.87rem; line-height: 1.5; }
  .said { display: grid; gap: 0.45rem; }
  .who { font-size: 0.7rem; font-weight: 600; letter-spacing: 0.02em; text-transform: uppercase; color: var(--muted); }
  .said p, .said h4 { margin: 0; overflow-wrap: anywhere; }
  .said h4 { font: 650 0.88rem/1.35 var(--display); margin-top: 0.2rem; }
  .li { padding-left: 1rem; position: relative; overflow-wrap: anywhere; }
  .li::before { content: "•"; position: absolute; left: 0.25rem; color: var(--muted); }
  .said code { font: 0.8em var(--mono); padding: 0.05rem 0.3rem; border-radius: 5px; background: var(--fill); }
  .said pre { margin: 0; padding: 0.6rem 0.7rem; border-radius: 9px; background: var(--fill); font: 0.78rem/1.45 var(--mono); overflow: auto; }
  .cite { display: inline-grid; place-items: center; min-width: 1.05rem; height: 1.05rem; padding: 0 0.25rem; margin: 0 0.1rem; vertical-align: 0.15em; border: 0; border-radius: 5px; font: 700 0.64rem/1 var(--rounded); background: var(--accent-soft); color: var(--accent-ink); cursor: pointer; }
  .cite:hover { background: var(--accent); color: #fff; }
  .cite.live { min-width: 0.9rem; background: var(--fill-2); cursor: default; }
  .cite.unread { background: color-mix(in srgb, var(--danger) 14%, transparent); color: var(--danger); cursor: help; }

  .sources { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .src { display: inline-flex; align-items: center; gap: 0.35rem; max-width: 100%; border: 0; border-radius: 7px; padding: 0.2rem 0.5rem 0.2rem 0.25rem; background: var(--fill); color: var(--text-2); font-size: 0.76rem; cursor: pointer; }
  .src:hover { background: var(--fill-2); color: var(--text); }
  .src .n { display: grid; place-items: center; min-width: 1.05rem; height: 1.05rem; border-radius: 5px; background: var(--accent-soft); color: var(--accent-ink); font: 700 0.64rem/1 var(--rounded); }
  .sl { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: 0.72rem; }
  .nosrc, .meta, .quiet, .hint { margin: 0; font-size: 0.74rem; color: var(--muted); }
  .unread-note { margin: 0; font-size: 0.76rem; color: var(--danger); display: flex; gap: 0.3rem; align-items: center; }

  .steps-t { justify-self: start; display: inline-flex; align-items: center; gap: 0.3rem; border: 0; background: none; padding: 0; font-size: 0.75rem; color: var(--muted); cursor: pointer; }
  .steps-t[aria-expanded="true"] :global(.icon) { transform: rotate(90deg); }
  .steps { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.2rem; font-size: 0.76rem; color: var(--text-2); }
  .steps li { display: flex; align-items: center; gap: 0.35rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rid { font: 600 0.66rem var(--mono); color: var(--muted); }
  .steps.live li { animation: in 260ms var(--ease); }
  .steps li.refused { color: var(--danger); }
  @keyframes in { from { opacity: 0; transform: translateY(3px); } }
  .thinking { color: var(--muted); }
  .dots { display: inline-flex; gap: 3px; }
  .dots i { width: 4px; height: 4px; border-radius: 50%; background: currentColor; animation: pulse 1.1s infinite ease-in-out; }
  .dots i:nth-child(2) { animation-delay: 0.15s; } .dots i:nth-child(3) { animation-delay: 0.3s; }
  @keyframes pulse { 0%, 80%, 100% { opacity: 0.25; } 40% { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) { .dots i, .steps.live li { animation: none; } }

  .card { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 0.6rem; padding: 0.65rem 0.7rem; border-radius: 12px; background: color-mix(in srgb, #bf5af2 9%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, #bf5af2 26%, transparent); }
  .card.blocked { background: var(--fill); box-shadow: none; }
  .glyph { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: #fff; background: linear-gradient(160deg, #bf5af2, #5e5ce6); }
  .cw { display: grid; gap: 0.12rem; min-width: 0; }
  .ck { font-size: 0.68rem; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: #8944ab; }
  .cw strong { font-size: 0.86rem; font-weight: 600; }
  .cmd { font: 0.7rem var(--mono); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .why { font-size: 0.76rem; color: var(--text-2); }
  .stopped { display: flex; align-items: center; gap: 0.45rem; padding: 0.55rem 0.7rem; border-radius: 10px; font-size: 0.82rem; background: color-mix(in srgb, var(--danger) 9%, var(--surface)); color: var(--text-2); }
  .stopped.budget { background: color-mix(in srgb, var(--attention) 12%, var(--surface)); }
  .stopped span { flex: 1; }
  .link { border: 0; background: none; padding: 0; color: var(--accent-ink); font-weight: 600; cursor: pointer; }

  .list { display: grid; gap: 0.25rem; }
  .list h3 { margin: 0 0 0.3rem; font: 650 0.8rem var(--display); color: var(--muted); text-transform: uppercase; letter-spacing: 0.03em; }
  .conv { display: grid; text-align: left; gap: 0.1rem; border: 0; border-radius: 9px; padding: 0.5rem 0.6rem; background: none; cursor: pointer; }
  .conv:hover, .conv:focus-visible { background: var(--fill); }
  .ct { font-size: 0.85rem; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cm { font-size: 0.72rem; color: var(--muted); }

  .composer { display: grid; gap: 0.4rem; padding: 0.6rem 0.75rem 0.65rem; border-top: 0.5px solid var(--separator); }
  .chips { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.3rem; max-width: 100%; padding: 0.12rem 0.2rem 0.12rem 0.5rem; border-radius: 999px; background: var(--fill); font-size: 0.74rem; color: var(--text-2); }
  .chip.view { background: var(--accent-soft); color: var(--accent-ink); }
  .chip .dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .chip button { width: 16px; height: 16px; border: 0; border-radius: 50%; display: grid; place-items: center; background: none; color: inherit; cursor: pointer; opacity: 0.7; }
  .chip button:hover { opacity: 1; background: var(--fill-2); }
  .field { display: flex; align-items: flex-end; gap: 0.4rem; padding: 0.35rem 0.35rem 0.35rem 0.7rem; border-radius: 14px; background: var(--surface); box-shadow: inset 0 0 0 1px var(--separator); transition: box-shadow var(--speed) var(--ease); }
  .field:focus-within { box-shadow: inset 0 0 0 1px var(--accent), 0 0 0 3px var(--accent-soft); }
  textarea, textarea:focus-visible { box-shadow: none; }
  textarea { flex: 1; resize: none; border: 0; outline: none; background: none; color: var(--text); font: inherit; font-size: 0.88rem; line-height: 1.4; max-height: 10rem; field-sizing: content; padding: 0.25rem 0; }
  .send { width: 30px; height: 30px; flex: none; border: 0; border-radius: 10px; display: grid; place-items: center; background: var(--accent); color: #fff; cursor: pointer; transition: opacity var(--speed) var(--ease), transform 120ms var(--ease); }
  .send:disabled { opacity: 0.35; cursor: default; }
  .send:not(:disabled):active { transform: scale(0.94); }
  .send.stop { position: relative; background: var(--text); }
  .sq { width: 9px; height: 9px; border-radius: 2.5px; background: var(--surface); }
  .ring { position: absolute; inset: 3px; border-radius: 50%; border: 2px solid transparent; border-top-color: var(--surface); opacity: 0.55; animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .ring { animation: none; } }
  .q.queued { opacity: 0.6; }
  .made { justify-self: start; display: inline-flex; align-items: center; gap: 0.4rem; margin: 0; padding: 0.3rem 0.6rem 0.3rem 0.45rem; border: 0; border-radius: 999px; font: inherit; font-size: 0.76rem; color: var(--text-2, var(--text)); background: var(--fill); }
  button.made { cursor: pointer; }
  .made.spec { color: #a646d8; background: color-mix(in srgb, #bf5af2 12%, transparent); }
  .made .go { font-weight: 600; opacity: 0.75; }
  button.made:hover .go { opacity: 1; text-decoration: underline; }
  .recent { display: grid; gap: 0.15rem; width: 100%; margin-top: 0.9rem; text-align: left; }
  .rh { font-size: 0.68rem; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); padding: 0 0.5rem 0.2rem; }
  .recent .conv { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 0.5rem; color: var(--muted); }
  .recent .conv .ct { color: var(--text); }
  @keyframes rise { from { transform: translateY(6px); opacity: 0; } }
  .peek { position: absolute; inset: 56px 0 0; z-index: 2; display: grid; grid-template-rows: auto 1fr; background: var(--glass-strong); backdrop-filter: var(--blur); animation: rise 260ms var(--spring); }
  .ph { display: flex; align-items: flex-start; justify-content: space-between; gap: 0.5rem; padding: 0.7rem 0.85rem; border-bottom: 0.5px solid var(--separator); }
  .ph div { display: grid; }
  .ph strong { font-size: 0.86rem; }
  .ph span { font-size: 0.72rem; color: var(--muted); }
  .pb { overflow: auto; padding: 0.75rem 0.85rem; }
  .halted { margin: 0; display: inline-flex; align-items: center; gap: 0.3rem; font-size: 0.74rem; color: var(--muted); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
  @media (prefers-color-scheme: dark) { .ck { color: #da8fff; } }
  @media (max-width: 720px) { .ask { top: 56px; right: 0; bottom: 0; width: 100vw; border-radius: 0; } .grip { display: none; } }
</style>
