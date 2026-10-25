<script lang="ts">
  // The feature's documents: each rendered from Markdown, with its outline
  // in the side nav — the headings, the section being read, the
  // requirements each section names (cited or not), a phase's progress in
  // the task list; [ and ] step through the sections.
  import { app } from "../lib/app.svelte";
  import { diagrams } from "../lib/mermaid";
  import { outline, type Section } from "../lib/outline";
  import ReviewCards from "./ReviewCards.svelte";
  const docs = $derived(app.documents);
  // Review is offered when the spec names requirements to review.
  const reviewable = $derived(!!app.report?.data?.review);
  const reviewing = $derived(reviewable && app.doc === "review");
  const shown = $derived(docs.find((d) => d.name === app.doc) ?? docs[0]);
  const pendingReview = $derived.by(() => {
    const t = app.report?.data?.review;
    return t ? t.open + t.lapsed + t.changes_requested : 0;
  });
  const names: Record<string, string> = { spec: "Spec", plan: "Plan", tasks: "Tasks", research: "Research", "data-model": "Data model", quickstart: "Quickstart" };
  // Requirement IDs become chips that say which task cites them (the report's trace).
  const cites = $derived(new Map((app.report?.data?.trace ?? []).map((t) => [t.requirement, t.task])));
  const parsed = $derived(outline(shown?.html ?? ""));
  const html = $derived(
    parsed.html.replace(/\b((?:FR|SC|NFR)-\d{2,4})\b/g, (id) => {
      const task = cites.get(id);
      return `<span class="req ${task ? "cited" : "uncited"}" title="${task ? `cited by ${task}` : "no task cites it"}">${id}</span>`;
    }),
  );
  const sections = $derived(parsed.sections);

  // A phase of the task list: done of total, from the report's own states.
  const phases = $derived.by(() => {
    const m = new Map<string, { done: number; total: number }>();
    for (const t of app.report?.data?.tasks ?? []) {
      const p = m.get(t.phase) ?? { done: 0, total: 0 };
      p.total++;
      if (["verified", "regression_only", "attested", "ticked_without_evidence", "skipped"].includes(t.state)) p.done++;
      m.set(t.phase, p);
    }
    return m;
  });
  const progress = (s: Section) => (shown?.name === "tasks" ? phases.get(s.text) : undefined);

  // Where the reader is: the last heading above the top of the view.
  let article = $state<HTMLElement>();
  let reading = $state<string | null>(null);
  let read = $state(0);
  $effect(() => {
    void html;
    const main = article?.closest("main");
    if (!main || !article) return;
    const heads = Array.from(article.querySelectorAll<HTMLElement>("h2[id], h3[id]"));
    const update = () => {
      const top = main.getBoundingClientRect().top + 90;
      let at: string | null = heads[0]?.id ?? null;
      for (const h of heads) if (h.getBoundingClientRect().top <= top) at = h.id;
      reading = at;
      const max = main.scrollHeight - main.clientHeight;
      read = max > 0 ? Math.min(1, main.scrollTop / max) : 1;
    };
    update();
    main.addEventListener("scroll", update, { passive: true });
    return () => main.removeEventListener("scroll", update);
  });
  // Arriving at a section (a link, a reload): go there once, when it is
  // drawn — never again on a refresh, which would pull the reader back.
  let landed = "";
  $effect(() => {
    const id = app.section;
    const key = `${shown?.name}#${id}`;
    if (!id || !html || key === landed || !article?.querySelector(`#${CSS.escape(id)}`)) return;
    landed = key;
    requestAnimationFrame(() => article?.querySelector(`#${CSS.escape(id)}`)?.scrollIntoView({ block: "start" }));
  });
  function jump(id: string) {
    app.section = id;
    landed = `${shown?.name}#${id}`;
    const smooth = !matchMedia("(prefers-reduced-motion: reduce)").matches;
    article?.querySelector(`#${CSS.escape(id)}`)?.scrollIntoView({ block: "start", behavior: smooth ? "smooth" : "auto" });
  }
  function open(name: string) {
    app.doc = name;
    app.section = null;
    article?.closest("main")?.scrollTo({ top: 0 });
  }
  function key(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.closest("input, textarea, select") || reviewing || !sections.length) return;
    if (e.key !== "[" && e.key !== "]") return;
    e.preventDefault();
    const i = sections.findIndex((s) => s.id === reading);
    const next = sections[Math.max(0, Math.min(sections.length - 1, i + (e.key === "]" ? 1 : -1)))];
    if (next) jump(next.id);
  }
</script>

<svelte:window onkeydown={key} />

{#if docs.length === 0}
  <div class="empty muted">Nothing written yet.</div>
{:else}
  <div class="docs">
    <nav aria-label="Documents">
      {#if reviewable}
        <button class="doc" class:on={reviewing} onclick={() => (app.doc = "review")}>Review{#if pendingReview}<span class="count">{pendingReview}</span>{/if}</button>
      {/if}
      {#each docs as d (d.name)}
        {@const current = !reviewing && shown?.name === d.name}
        <button class="doc" class:on={current} aria-current={current ? "page" : undefined} onclick={() => open(d.name)}>{names[d.name] ?? d.name}</button>
        {#if current && sections.length}
          <ol class="outline" aria-label="{names[d.name] ?? d.name} outline">
            {#each sections as s (s.id)}
              {@const p = progress(s)}
              <li class="l{s.level}" class:at={reading === s.id}>
                <button onclick={() => jump(s.id)} title={s.text}>
                  <span class="t">{s.text.replace(/^Phase \d+:\s*/, "")}</span>
                  {#if p}<span class="phase" class:done={p.done === p.total}>{p.done}/{p.total}</span>{/if}
                </button>
                {#if s.reqs.length}
                  <span class="reqs">
                    {#each s.reqs.slice(0, 8) as r (r)}<span class="dot" class:cited={cites.get(r)} title="{r} — {cites.get(r) ? `cited by ${cites.get(r)}` : "no task cites it"}"></span>{/each}{#if s.reqs.length > 8}<span class="more">+{s.reqs.length - 8}</span>{/if}
                  </span>
                {/if}
              </li>
            {/each}
          </ol>
        {/if}
      {/each}
      {#if !reviewing && sections.length > 1}<p class="keys"><kbd>[</kbd> <kbd>]</kbd> previous, next section</p>{/if}
    </nav>
    {#if reviewing}
      <ReviewCards />
    {:else}
      <article class="prose" bind:this={article} use:diagrams={html}>
        <div class="bar" aria-hidden="true"><span style="width:{read * 100}%"></span></div>
        <p class="path mono muted">{shown?.path}</p>
        <!-- Rendered by the host from Markdown; raw HTML in the file is shown as text. -->
        {@html html}
      </article>
    {/if}
  </div>
{/if}

<style>
  .docs { display: grid; grid-template-columns: 15rem minmax(0, 1fr); gap: 2rem; max-width: 1240px; margin: 0 auto; }
  /* Sticky beside the page: a grid item stretches to its row and so never
     sticks; aligned to the start, it stays in view while the page scrolls. */
  nav { display: grid; gap: 1px; align-content: start; align-self: start; position: sticky; top: 0; max-height: calc(100vh - 56px - 26px - 2rem); overflow-y: auto; overscroll-behavior: contain; padding-bottom: 1rem; }
  nav .doc { text-align: left; border: 0; background: none; padding: 0.4rem 0.7rem; border-radius: 8px; cursor: pointer; color: var(--text-2); font-weight: 500; }
  nav .doc:hover { background: var(--fill); }
  nav .doc.on { background: var(--accent); color: #fff; font-weight: 600; }
  /* The outline: a quiet column under its document; the section being
     read is marked by an accent rail, as a reader's finger on the page. */
  .outline { list-style: none; margin: 0.25rem 0 0.6rem; padding: 0; border-left: 1.5px solid var(--separator); margin-left: 0.85rem; }
  .outline li { position: relative; display: flex; align-items: center; gap: 0.35rem; }
  .outline li::before { content: ""; position: absolute; left: -1.5px; top: 4px; bottom: 4px; width: 2.5px; border-radius: 2px; background: transparent; transition: background var(--speed) var(--ease); }
  .outline li.at::before { background: var(--accent); }
  .outline button { flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.35rem; border: 0; background: none; text-align: left; cursor: pointer; padding: 0.22rem 0.5rem 0.22rem 0.7rem; border-radius: 6px; color: var(--muted); font-size: 0.8rem; line-height: 1.3; }
  .outline .l3 button { padding-left: 1.45rem; font-size: 0.76rem; }
  .outline button:hover { color: var(--text); background: var(--fill); }
  .outline li.at button { color: var(--text); font-weight: 600; }
  .outline .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .phase { margin-left: auto; flex: none; font: 600 0.68rem/1 var(--rounded); padding: 0.15rem 0.35rem; border-radius: 999px; background: var(--fill); color: var(--muted); }
  .phase.done { background: var(--verified-bg); color: var(--verified); }
  .reqs { display: inline-flex; gap: 2px; padding-right: 0.4rem; flex: none; }
  .dot { width: 5px; height: 5px; border-radius: 50%; background: var(--regression-fill); }
  .dot.cited { background: var(--verified-fill); }
  .more { font-size: 0.62rem; color: var(--muted); }
  .keys { margin: 0.6rem 0.7rem 0; font-size: 0.7rem; color: var(--faint); }
  .keys kbd { font: 0.68rem var(--mono); padding: 0 0.3rem; border-radius: 4px; background: var(--fill); color: var(--muted); }
  .bar { position: sticky; top: 0; height: 3px; margin: -2.25rem -2.75rem 2rem; border-radius: var(--radius-lg) var(--radius-lg) 0 0; overflow: hidden; background: transparent; z-index: 1; }
  .bar span { display: block; height: 100%; background: var(--accent); transition: width 120ms linear; }
  .prose :global(h2[id]), .prose :global(h3[id]) { scroll-margin-top: 1.25rem; }
  nav .count { float: right; min-width: 1.2rem; padding: 0 0.35rem; border-radius: 999px; background: var(--attention-fill); color: #fff; font: 700 0.68rem/1.2rem var(--rounded); text-align: center; }
  .prose {
    background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 2.25rem 2.75rem 2.75rem;
    max-width: 78ch; font-size: 15px; line-height: 1.65; letter-spacing: -0.008em;
  }
  .path { font-size: 0.75rem; margin: 0 0 1.25rem; }
  .prose :global(h1) { font: 700 1.85rem/1.15 var(--display); letter-spacing: -0.026em; margin: 0 0 1rem; }
  .prose :global(h2) { font: 650 1.3rem/1.25 var(--display); letter-spacing: -0.018em; margin: 2.25rem 0 0.75rem; }
  .prose :global(h3) { font: 650 1.05rem/1.3 var(--display); margin: 1.6rem 0 0.5rem; }
  .prose :global(p), .prose :global(li) { color: var(--text-2); }
  .prose :global(strong) { color: var(--text); font-weight: 600; }
  .prose :global(a) { color: var(--accent-ink); text-decoration: none; }
  .prose :global(code) { background: var(--fill); padding: 0.12em 0.38em; border-radius: 5px; font-size: 0.86em; }
  .prose :global(pre) { background: var(--surface-2); padding: 0.9rem 1.1rem; border-radius: var(--radius); overflow-x: auto; font-size: 0.85em; line-height: 1.55; }
  .prose :global(pre code) { background: none; padding: 0; }
  .prose :global(blockquote) { margin: 1rem 0; padding: 0.25rem 1rem; border-left: 3px solid var(--fill-2); color: var(--muted); }
  .prose :global(hr) { border: 0; border-top: 0.5px solid var(--separator); margin: 2rem 0; }
  .prose :global(table) { border-collapse: collapse; width: 100%; font-size: 0.88rem; margin: 1rem 0; }
  .prose :global(th) { text-align: left; font-weight: 600; color: var(--muted); font-size: 0.8rem; padding: 0.4rem 0.6rem; border-bottom: 0.5px solid var(--separator); }
  .prose :global(td) { padding: 0.45rem 0.6rem; border-bottom: 0.5px solid var(--separator); vertical-align: top; }
  .prose :global(ul), .prose :global(ol) { padding-left: 1.3rem; }
  .prose :global(li) { margin: 0.2rem 0; }
  .prose :global(li input[type="checkbox"]) { margin-right: 0.45rem; accent-color: var(--verified-fill); }
  .prose :global(.req) { font-family: var(--mono); font-size: 0.78em; padding: 0.1em 0.5em; border-radius: 999px; font-weight: 600; white-space: nowrap; }
  .prose :global(.req.cited) { background: var(--verified-bg); color: var(--verified); }
  .prose :global(figure.diagram) { margin: 1.25rem 0; padding: 1rem; border-radius: var(--radius); background: var(--surface-2); overflow-x: auto; text-align: center; }
  .prose :global(figure.diagram svg) { max-width: 100%; height: auto; }
  .prose :global(.diagram-error) { color: var(--danger); font-size: 0.82rem; text-align: left; margin: 0.5rem 0 0; }
  .prose :global(.req.uncited) { background: var(--regression-bg); color: var(--regression); }
  .empty { padding: 3rem; text-align: center; }
  @media (max-width: 1040px) { .docs { grid-template-columns: 12rem minmax(0, 1fr); gap: 1.25rem; } }
  @media (max-width: 900px) { .docs { grid-template-columns: 1fr; } nav { grid-auto-flow: column; overflow-x: auto; position: static; max-height: none; } .outline, .keys { display: none; } .prose { padding: 1.5rem 1.25rem; } .bar { margin: -1.5rem -1.25rem 1.25rem; } }
</style>
