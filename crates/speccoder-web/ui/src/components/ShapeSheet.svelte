<script lang="ts">
  // Shape: the person's words made into a requirement (or a bug line, a
  // quick change, a note). The assistant proposes; code marks what it added
  // and what of the person's words it dropped; every value it introduced is
  // an assumption to keep, ask about or remove, every sentence that landed
  // nowhere is added back or dropped on purpose — and only then can the
  // words go into the field. Nothing is written here: the field's own sheet
  // saves.
  import { app } from "../lib/app.svelte";
  import { post } from "../lib/api";
  import { trap } from "../lib/sheet";
  import type { ShapeReport } from "../lib/schema.gen";
  import Icon from "./Icon.svelte";

  type Span = { text: string; origin: "yours" | "added" | "dropped" };
  type Finding = { rule: string; words: string; reason: string; fix: string };
  type Coverage = { sentence: string; lines: number[]; dropped: boolean };
  type Choice = "keep" | "ask" | "remove";

  const req = app.shape!;
  const labels: Record<string, string> = {
    requirement: "a requirement",
    feature: "a feature description",
    quick: "a quick change",
    bug_symptom: "what goes wrong",
    bug_expected: "what should happen",
    bug_unchanged: "what must keep working",
    answer: "an answer",
    note: "a note for the next attempt",
  };

  let report = $state<ShapeReport | null>(null);
  let failed = $state<string | null>(null);
  let spans = $state<Span[]>([]);
  let findings = $state<Finding[]>([]);
  let editing = $state(false);
  // Once the person edits, their text is the proposal until they go back.
  let edited = $state(false);
  let typed = $state("");
  let hover = $state<string | null>(null);
  // The person's decisions, by position in the first proposal's lists.
  let assumed = $state<(Choice | null)[]>([]);
  let unexplained = $state<(Choice | null)[]>([]);
  let dropped = $state<("add" | "drop" | null)[]>([]);
  let droppedSentences = $state<string[]>([]);

  const marker = (question: string) => `[NEEDS CLARIFICATION: ${question.replace(/\]/g, ")")}]`;
  // Words are found whole, without regard to case, and replaced literally
  // (a `$` in a question stays a `$`). Removing them takes the little word
  // that introduces them ("within 2 seconds", "up to 10 000 rows"), so the
  // sentence still reads.
  const escape = (t: string) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const whole = (words: string, lead = "") =>
    new RegExp(`${lead}(?<![\\p{L}\\p{N}])${escape(words)}(?![\\p{L}\\p{N}])`, "iu");
  const swap = (text: string, words: string, by: string) => text.replace(whole(words), () => by);
  const remove = (text: string, words: string) =>
    text.replace(whole(words, "(?:(?<![\\p{L}\\p{N}])(?:within|in|after|under|over|below|above|up to|at most|at least|for|of|by)\\s+)?"), () => "");

  /** The proposal with the decisions applied: asked words become an open
   *  question, removed ones go, sentences added back are appended. While the
   *  person edits, their text is used exactly as they write it. */
  const proposal = $derived.by(() => {
    if (!report) return "";
    if (edited) return typed;
    let text = report.proposal;
    report.assumptions.forEach((a, i) => {
      if (assumed[i] === "ask") text = swap(text, a.words, marker(a.question));
      if (assumed[i] === "remove") text = remove(text, a.words);
    });
    report.unexplained.forEach((u, i) => {
      if (unexplained[i] === "ask") text = swap(text, u, marker(`What should "${u}" be?`));
      if (unexplained[i] === "remove") text = remove(text, u);
    });
    const back = droppedSentences.filter((_, i) => dropped[i] === "add");
    if (back.length) text = `${text.trim()}\n${back.join("\n")}`;
    return text.replace(/[ \t]{2,}/g, " ").replace(/ ([.,;:])/g, "$1").trim();
  });
  const pending = $derived(
    edited ? 0 : assumed.filter((c) => !c).length + unexplained.filter((c) => !c).length + dropped.filter((c) => !c).length,
  );

  async function load() {
    try {
      const r = await post<{ status: string; message: string; data?: ShapeReport }>("/api/shape", { field: req.field, text: req.text });
      if (r.status !== "ok" || !r.data) {
        failed = r.message;
        return;
      }
      report = r.data;
      typed = r.data.proposal;
      spans = r.data.diff.proposal as Span[];
      findings = r.data.findings as Finding[];
      assumed = r.data.assumptions.map(() => null);
      unexplained = r.data.unexplained.map(() => null);
      droppedSentences = (r.data.coverage as Coverage[]).filter((c) => c.dropped).map((c) => c.sentence);
      dropped = droppedSentences.map(() => null);
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }
  void load();

  // The proposal as the person changed it, judged again by code — a
  // moment after the last change; only the newest answer counts.
  let judging = 0;
  $effect(() => {
    const text = proposal;
    if (!report) return;
    const n = ++judging;
    const kept = report.assumptions.filter((_, i) => assumed[i] !== "remove");
    const translated = !!report.back_translation;
    const original = report.original;
    const t = setTimeout(() => {
      void post<{ diff: { proposal: Span[] }; findings: Finding[] }>("/api/shape/judge", { field: req.field, original, proposal: text, assumptions: kept })
        .then((j) => {
          if (n !== judging) return;
          // In another language the inked text is the back translation;
          // the writing check is always the proposal's.
          if (!translated) spans = j.diff.proposal;
          findings = j.findings;
        })
        .catch(() => {});
    }, 300);
    return () => clearTimeout(t);
  });

  const keepAll = () => {
    assumed = assumed.map((c) => c ?? "keep");
    unexplained = unexplained.map((c) => c ?? "keep");
  };
  const close = () => (app.shape = null);
  function use() {
    if (pending) return;
    req.accept(proposal);
    close();
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      use();
    }
  }
  // A stretch of the proposal split around the words being hovered.
  const pieces = (text: string) => {
    if (!hover || !text.includes(hover)) return [{ text, lit: false }];
    const at = text.indexOf(hover);
    return [
      { text: text.slice(0, at), lit: false },
      { text: hover, lit: true },
      { text: text.slice(at + hover.length), lit: false },
    ];
  };
</script>

<div class="d-scrim shape-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={key}>
  <div class="d-sheet shape" role="dialog" aria-modal="true" aria-labelledby="sh-title" aria-describedby="sh-lead" use:trap>
    <header class="d-head">
      <span class="d-badge violet" aria-hidden="true"><Icon name="sparkle" size={20} weight={2.2} /></span>
      <div>
        <h2 id="sh-title">Shape {labels[req.field] ?? "your words"}</h2>
        <p id="sh-lead">The assistant proposes. Code marks every word it added — you decide each change before it is used.</p>
      </div>
    </header>

    {#if failed}
      <div class="d-warn" role="alert">{failed}</div>
      <div class="foot"><span></span><button class="btn" onclick={close}>Close</button></div>
    {:else if !report}
      <div class="thinking" aria-live="polite"><span class="spinner"></span> Reading your words…</div>
    {:else}
      <div class="cols">
        <section class="col mine" aria-label="Your words">
          <h3>Your words</h3>
          <p class="ink">{#each report.diff.original as s, i (i)}<span class={s.origin}>{s.text}</span>{/each}</p>
          {#if report.diff.original.some((s) => s.origin === "dropped")}<span class="legend"><s>struck</s> = not in the proposal</span>{/if}
        </section>
        <section class="col proposed" aria-label="Proposed">
          <h3>Proposed {#if edited && !editing}<button class="link" onclick={() => (edited = false)}>Back to the proposal</button>{/if}<button class="link" onclick={() => { if (!editing && !edited) { typed = proposal; edited = true; } editing = !editing; }}>{editing ? "Done" : "Edit"}</button></h3>
          {#if editing}
            <!-- svelte-ignore a11y_autofocus -->
            <textarea rows="5" bind:value={typed} autofocus aria-label="The proposal"></textarea>
          {:else if report.back_translation}
            <p class="ink plain">{proposal}</p>
            <h3 class="back">In your words ({report.language})</h3>
            <p class="ink">{#each spans as s, i (i)}<span class={s.origin}>{s.text}</span>{/each}</p>
          {:else}
            <p class="ink">{#each spans as s, i (i)}<span class={s.origin}>{#each pieces(s.text) as p, k (k)}{#if p.lit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}</span>{/each}</p>
          {/if}
          {#if spans.some((s) => s.origin === "added")}<span class="legend"><span class="added">tinted</span> = added by the assistant</span>{/if}
        </section>
      </div>

      {#if report.assumptions.length || report.unexplained.length || droppedSentences.length}
        <section class="decide" class:off={edited} aria-label="To decide" inert={edited}>
          {#if edited}<p class="editing">You edited the proposal: your text is used as you wrote it, and these no longer apply.</p>{/if}
          <h3>To decide {#if pending}<span class="count">{pending}</span>{:else}<span class="done"><Icon name="check" size={10} weight={2.8} /> all decided</span>{/if}</h3>
          <ul>
            {#each report.assumptions as a, i (i)}
              <li class="pill" class:decided={assumed[i]} onmouseenter={() => (hover = a.words)} onmouseleave={() => (hover = null)}>
                <span class="what"><span class="tag">Assumed</span> <q>{a.words}</q>{#if a.from} <span class="muted">from “{a.from}”</span>{/if}</span>
                <span class="seg" role="radiogroup" aria-label="Assumed {a.words}">
                  <button role="radio" aria-checked={assumed[i] === "keep"} class:on={assumed[i] === "keep"} onclick={() => (assumed[i] = "keep")}>Keep</button>
                  <button role="radio" aria-checked={assumed[i] === "ask"} class:on={assumed[i] === "ask"} title={a.question} onclick={() => (assumed[i] = "ask")}>Ask instead</button>
                  <button role="radio" aria-checked={assumed[i] === "remove"} class:on={assumed[i] === "remove"} onclick={() => (assumed[i] = "remove")}>Remove</button>
                </span>
                {#if assumed[i] === "ask"}<span class="q">The spec will ask: {a.question}</span>{/if}
              </li>
            {/each}
            {#each report.unexplained as u, i (u)}
              <li class="pill warn" class:decided={unexplained[i]} onmouseenter={() => (hover = u)} onmouseleave={() => (hover = null)}>
                <span class="what"><span class="tag">Added without saying why</span> <q>{u}</q></span>
                <span class="seg" role="radiogroup" aria-label="Added {u}">
                  <button role="radio" aria-checked={unexplained[i] === "keep"} class:on={unexplained[i] === "keep"} onclick={() => (unexplained[i] = "keep")}>Keep</button>
                  <button role="radio" aria-checked={unexplained[i] === "ask"} class:on={unexplained[i] === "ask"} onclick={() => (unexplained[i] = "ask")}>Ask instead</button>
                  <button role="radio" aria-checked={unexplained[i] === "remove"} class:on={unexplained[i] === "remove"} onclick={() => (unexplained[i] = "remove")}>Remove</button>
                </span>
              </li>
            {/each}
            {#each droppedSentences as d, i (i)}
              <li class="pill warn" class:decided={dropped[i]}>
                <span class="what"><span class="tag">Not carried over</span> “{d}”</span>
                <span class="seg" role="radiogroup" aria-label="Your sentence">
                  <button role="radio" aria-checked={dropped[i] === "add"} class:on={dropped[i] === "add"} onclick={() => (dropped[i] = "add")}>Add it back</button>
                  <button role="radio" aria-checked={dropped[i] === "drop"} class:on={dropped[i] === "drop"} onclick={() => (dropped[i] = "drop")}>Drop it</button>
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if report.questions.length}
        <section class="open" aria-label="Left open">
          <h3>Left open — the spec review will ask</h3>
          <ul>{#each report.questions as q, i (i)}<li>{q}</li>{/each}</ul>
        </section>
      {/if}

      {#if findings.length}
        <section class="check" aria-label="Writing check">
          <h3>Writing check</h3>
          <ul>{#each findings as f (f.rule + f.words)}<li>{#if f.words}<q>{f.words}</q>{" "}{/if}{f.reason} — <span class="fix">{f.fix}</span></li>{/each}</ul>
        </section>
      {/if}

      <div class="foot">
        <span class="muted">{#if report.cut}Your words were cut at 20 000 characters. {/if}Nothing is saved until you save the field.</span>
        <button class="btn ghost" onclick={close}>Keep Mine</button>
        {#if assumed.some((c) => !c) || unexplained.some((c) => !c)}<button class="btn" onclick={keepAll}>Keep All Assumptions</button>{/if}
        <button class="btn primary" disabled={pending > 0} title={pending ? `${pending} still to decide` : "Put it in the field (⌘⏎)"} onclick={use}>{pending ? `${pending} to Decide` : "Use It"}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Over the field's own sheet: solid, so that sheet does not show through. */
  .shape-scrim { z-index: 60; background: rgb(0 0 0 / 38%); }
  .shape { width: min(960px, 100%); background: var(--bg); }
  .thinking { display: flex; align-items: center; gap: 0.6rem; padding: 2rem 0; color: var(--text-2); justify-content: center; }
  .spinner { width: 18px; height: 18px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr); gap: 0.8rem; }
  .col { display: grid; align-content: start; gap: 0.4rem; padding: 0.8rem 0.95rem; border-radius: 14px; background: var(--fill); }
  .proposed { background: color-mix(in srgb, var(--accent) 6%, var(--fill)); }
  h3 { display: flex; align-items: center; gap: 0.45rem; margin: 0; font-size: 0.78rem; font-weight: 650; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  h3 .link { text-transform: none; letter-spacing: 0; }
  h3 .link:first-of-type { margin-left: auto; }
  h3.back { margin-top: 0.4rem; }
  .link { border: 0; background: none; color: var(--accent); font: 600 0.8rem/1 var(--rounded); cursor: pointer; padding: 0; }
  .ink { margin: 0; white-space: pre-wrap; line-height: 1.6; font-size: 0.98rem; color: var(--text); }
  .ink .added { background: color-mix(in srgb, var(--accent) 18%, transparent); border-radius: 3px; text-decoration: underline dotted color-mix(in srgb, var(--accent) 70%, transparent); text-underline-offset: 3px; }
  .ink .dropped { color: var(--muted); text-decoration: line-through; text-decoration-color: var(--attention); }
  .ink.plain { font-weight: 550; }
  mark { background: color-mix(in srgb, var(--attention) 35%, transparent); color: inherit; border-radius: 3px; }
  .legend { font-size: 0.72rem; color: var(--muted); }
  .legend .added { background: color-mix(in srgb, var(--accent) 18%, transparent); border-radius: 3px; padding: 0 0.2rem; }
  textarea { width: 100%; padding: 0.5rem 0.7rem; border-radius: 9px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); font: inherit; line-height: 1.55; resize: vertical; }
  .decide.off { opacity: 0.45; }
  .editing { margin: 0 0 0.3rem; font-size: 0.82rem; color: var(--text-2); }
  .decide ul, .open ul, .check ul { list-style: none; margin: 0.35rem 0 0; padding: 0; display: grid; gap: 0.4rem; }
  .pill { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem 0.8rem; padding: 0.55rem 0.75rem; border-radius: 12px; background: var(--fill); border: 1px solid transparent; transition: background 160ms; }
  .pill.warn { border-color: color-mix(in srgb, var(--attention) 40%, transparent); }
  .pill.decided { background: transparent; border-color: var(--separator); }
  .what { flex: 1; min-width: 14rem; font-size: 0.9rem; }
  .tag { font: 650 0.68rem/1 var(--rounded); text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); margin-right: 0.2rem; }
  .warn .tag { color: var(--attention); }
  q { font-weight: 600; quotes: "“" "”"; }
  .seg { display: inline-flex; border-radius: 9px; background: var(--fill-2); padding: 2px; gap: 2px; }
  .seg button { border: 0; background: none; padding: 0.25rem 0.6rem; border-radius: 7px; font: 600 0.78rem/1.2 var(--rounded); color: var(--text-2); cursor: pointer; }
  .seg button.on { background: var(--surface, var(--bg)); color: var(--text); box-shadow: 0 1px 3px rgba(0,0,0,0.12); }
  .q { flex-basis: 100%; font-size: 0.8rem; color: var(--text-2); }
  .count { min-width: 1.3rem; height: 1.2rem; padding: 0 0.4rem; border-radius: 999px; display: inline-grid; place-items: center; background: var(--attention-fill); color: #fff; font: 700 0.7rem/1 var(--rounded); text-transform: none; }
  .done { display: inline-flex; align-items: center; gap: 0.2rem; color: var(--verified); text-transform: none; letter-spacing: 0; }
  .open li, .check li { font-size: 0.88rem; color: var(--text-2); }
  .open li::before { content: "? "; color: var(--accent); font-weight: 700; }
  .fix { color: var(--text); }
  .foot { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .foot .muted { flex: 1; font-size: 0.8rem; min-width: 12rem; }
  @media (max-width: 760px) { .cols { grid-template-columns: 1fr; } }
</style>
