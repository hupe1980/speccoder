<script lang="ts">
  // Explore: an idea thought through before any spec. The model reads the
  // project and proposes approaches (claims); the person picks one, keeps
  // or adds constraints and open questions, and the brief is made from
  // that — its digest checked when the spec starts from it. Nothing is
  // written in the checkout until then.
  import { tick } from "svelte";
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { trap } from "../lib/sheet";
  import type { ExplorePayload, Item, Reply, SessionEntry } from "../lib/types";
  import Icon from "./Icon.svelte";

  const opened = app.explore ?? {};
  let idea = $state(opened.idea ?? "");
  let p = $state<ExplorePayload | null>(null);
  let sessions = $state<SessionEntry[]>([]);
  let picked = $state(0);
  let constraints = $state<Item[]>([]);
  let questions = $state<Item[]>([]);
  let adding = $state({ constraint: "", question: "" });
  let waiting = $state<"explore" | "choose" | null>(null);

  let more = $state("");
  let cards = $state<HTMLElement>();
  /** Why the last run failed, in its own words. */
  let failed = $state<string | null>(null);
  /** What the last run noticed besides its approaches (files someone else
   *  changed meanwhile). */
  let note = $state<string | null>(null);

  const session = $derived(p?.session ?? null);
  const approaches = $derived(p?.approaches ?? []);
  const brief = $derived(p?.brief && p.choice ? { text: p.brief, digest: p.choice.digest } : null);
  // Edits since the brief was made: choosing again makes a new one.
  const edited = $derived(
    !!p?.choice &&
      (p.choice.approach !== picked + 1 ||
        JSON.stringify(p.choice.constraints.map((i) => i.text)) !== JSON.stringify(constraints.map((i) => i.text)) ||
        JSON.stringify(p.choice.open_questions.map((i) => i.text)) !== JSON.stringify(questions.map((i) => i.text))),
  );
  const why = $derived(
    session
      ? session.selected_because === "format"
        ? `${session.skill} — the format's own`
        : session.selected_because === "flag"
          ? session.skill
          : "speccoder's explorer — the format has none of its own"
      : "",
  );

  function take(next: ExplorePayload) {
    p = next;
    const c = next.choice;
    picked = c ? c.approach - 1 : 0;
    constraints = c ? c.constraints : next.constraints;
    questions = c ? c.open_questions : next.open_questions;
  }

  async function load(id: string) {
    const r = await get<Reply<ExplorePayload>>(`/api/explore/session${q({ id })}`).catch(() => null);
    if (r?.data) take(r.data);
  }
  $effect(() => {
    if (opened.id) void load(opened.id);
    else void get<Reply<ExplorePayload>>("/api/explore").then((r) => (sessions = r.data?.sessions ?? [])).catch(() => {});
  });

  /** A run's answer: the session as it stands, or why there is nothing
   *  to choose from — said here, not in a toast. */
  function answered(r: Reply<ExplorePayload> | null) {
    if (r?.data?.session) take(r.data);
    // A failed "Explore further" still carries the earlier approaches: the
    // status says it failed, not an empty list.
    failed = r && (r.status !== "ok" || !r.data?.approaches?.length) ? r.message : null;
    note = r?.status === "ok" && r.code ? r.message : null;
    waiting = null;
  }

  // The approaches arrive with the focus on the picked card: 1–9 and ↑↓
  // work at once.
  let shownFor = "";
  $effect(() => {
    const key = `${session?.id ?? ""}:${p?.session?.runs.length ?? 0}`;
    if (!approaches.length || key === shownFor) return;
    shownFor = key;
    void tick().then(() => cards?.querySelectorAll<HTMLElement>("[role=radio]")[picked]?.focus());
  });

  function close() {
    app.explore = null;
  }

  async function explore(resume?: string) {
    const words = resume ? more.trim() : idea.trim();
    if (!resume && !words) return;
    failed = null;
    waiting = "explore";
    const r = await app.run(
      offer(
        { kind: "explore", idea: words, skill: null, resume: resume ?? null },
        { group: "plan", label: resume ? "explore further" : "explore an idea", why: "read-only: approaches to choose from", command: resume ? `speccoder spec explore resume ${resume} "${words}"` : `speccoder spec explore "${words.replaceAll('"', '\\"')}"` },
      ),
    );
    answered(r as Reply<ExplorePayload> | null);
    more = "";
  }

  async function choose() {
    if (!session) return;
    waiting = "choose";
    const r = await app.run(
      offer(
        { kind: "explore_choose", session: session.id, approach: picked + 1, constraints: constraints.map((i) => i.text), questions: questions.map((i) => i.text), flag: false },
        { group: "plan", label: `choose approach ${picked + 1}`, why: "makes the brief; no model call", command: `speccoder spec explore choose ${picked + 1} ${session.id}` },
      ),
    );
    answered(r as Reply<ExplorePayload> | null);
  }

  function start() {
    if (!brief || !session || edited) return;
    app.newFeature = { text: brief.text, source: { kind: "explore", session: session.id, digest: brief.digest }, through: "tasks" };
    close();
  }

  function add(kind: "constraint" | "question") {
    const text = adding[kind].trim();
    if (!text) return;
    if (kind === "constraint") constraints = [...constraints, { text, source: "person" }];
    else questions = [...questions, { text, source: "person" }];
    adding[kind] = "";
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      return close();
    }
    const field = (e.target as HTMLElement).closest("textarea, input");
    // 1–9 pick an approach; ↑↓ move between them.
    if (approaches.length && !field && /^[1-9]$/.test(e.key) && Number(e.key) <= approaches.length) {
      picked = Number(e.key) - 1;
      cards?.querySelectorAll<HTMLElement>("[role=radio]")[picked]?.focus();
    } else if (approaches.length && (e.key === "ArrowDown" || e.key === "ArrowUp") && (e.target as HTMLElement).getAttribute("role") === "radio") {
      e.preventDefault();
      picked = (picked + (e.key === "ArrowDown" ? 1 : approaches.length - 1)) % approaches.length;
      cards?.querySelectorAll<HTMLElement>("[role=radio]")[picked]?.focus();
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      // As the buttons: nothing while a run is on.
      if (waiting || app.job?.running) return;
      if (!session) void explore();
      else if (brief && !edited) start();
      else if (approaches.length) void choose();
    }
  }
  const when = (at: number) => new Date(at * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
</script>

<div class="d-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={key}>
  <div class="d-sheet wide explore" role="dialog" aria-modal="true" aria-labelledby="ex-title" aria-describedby="ex-lead" use:trap={"textarea"}>
    <header class="d-head">
      <span class="d-badge violet" aria-hidden="true"><Icon name="sparkle" size={20} weight={2.2} /></span>
      <div>
        <h2 id="ex-title">Explore</h2>
        <p id="ex-lead">Think it through before a spec: approaches to choose from, then a brief. Nothing is written until you start the spec.</p>
      </div>
    </header>

    {#if !session}
      <label class="d-field">
        <span class="d-label">What are you thinking about?</span>
        <textarea rows="3" bind:value={idea} placeholder="Reports people can take with them — CSV? a PDF? a link that stays fresh?" disabled={!!waiting}></textarea>
      </label>
      {#if failed}
        <p class="d-warn attention" role="alert"><Icon name="exclaim" size={13} /> {failed}</p>
      {/if}
      {#if waiting}
        <p class="working" role="status"><span class="spinner" aria-hidden="true"></span> Reading the project and weighing approaches… questions show up for you to answer.</p>
      {:else if sessions.length}
        <div class="d-field">
          <span class="d-label">Pick up an exploration</span>
          <ul class="sessions">
            {#each sessions.slice(0, 5) as s (s.id)}
              <li><button type="button" onclick={() => void load(s.id)}><span class="st {s.state}">{s.state}</span><span class="si">{s.idea}</span><span class="muted">{when(s.updated_at)}</span></button></li>
            {/each}
          </ul>
        </div>
      {/if}
    {:else}
      <div class="idea"><Icon name="bubble" size={13} /><div><p>{session.idea}</p><span class="muted">{why}</span></div></div>
      {#if waiting}
        <p class="working" role="status"><span class="spinner" aria-hidden="true"></span> {waiting === "choose" ? "Making the brief…" : "Exploring further…"}</p>
      {/if}
      {#if failed}
        <p class="d-warn attention" role="alert"><Icon name="exclaim" size={13} /> {failed}</p>
      {:else if note}
        <p class="d-warn" role="status"><Icon name="exclaim" size={13} /> {note}</p>
      {/if}

      <div class="d-field">
        <span class="d-label" id="ex-approaches">Approaches <span class="muted">— the model's; you choose</span></span>
        <div class="cards" role="radiogroup" aria-labelledby="ex-approaches" bind:this={cards}>
          {#each approaches as a, i (`${i}:${a.title}`)}
            <div class="card" class:on={picked === i} role="radio" aria-checked={picked === i} tabindex={picked === i ? 0 : -1} onclick={() => (picked = i)} onkeydown={(e) => (e.key === " " || e.key === "Enter") && !e.metaKey && !e.ctrlKey && (e.preventDefault(), (picked = i))}>
              <span class="n" aria-hidden="true">{i + 1}</span>
              <strong>{a.title}</strong>
              <p>{a.summary}</p>
              {#each a.tradeoffs as t, j (j)}<span class="tag give"><Icon name="minus" size={10} /> {t}</span>{/each}
              {#each a.risks as r, j (j)}<span class="tag risk"><Icon name="exclaim" size={10} /> {r}</span>{/each}
            </div>
          {/each}
        </div>
      </div>

      <div class="lists">
        {#each [["constraint", "Constraints", constraints], ["question", "Open questions", questions]] as [kind, title, items] (kind)}
          {@const k = kind as "constraint" | "question"}
          <div class="d-field">
            <span class="d-label">{title}</span>
            <ul class="items">
              {#each items as Item[] as it, i (it.text + i)}
                <li class={it.source}>
                  <span>{it.text}</span>
                  {#if it.source !== "model"}<span class="src">{it.source === "person" ? "you" : "unanswered"}</span>{/if}
                  <button type="button" aria-label="Drop “{it.text}”" onclick={() => (k === "constraint" ? (constraints = constraints.filter((_, j) => j !== i)) : (questions = questions.filter((_, j) => j !== i)))}><Icon name="xmark" size={9} weight={2.4} /></button>
                </li>
              {/each}
            </ul>
            <input aria-label="Add a {k}" placeholder={k === "constraint" ? "Add a constraint…" : "Add a question…"} bind:value={adding[k]} onkeydown={(e) => e.key === "Enter" && !e.metaKey && (e.preventDefault(), add(k))} />
          </div>
        {/each}
      </div>

      {#if brief}
        <div class="brief" class:stale={edited}>
          <div class="bh"><Icon name="doc" size={13} /> <strong>The brief</strong> <code>{brief.digest.slice(0, 12)}</code>{#if edited}<span class="muted"> · changed — choose again to make it anew</span>{/if}</div>
          <pre>{brief.text.split("\n").slice(1).join("\n").trim()}</pre>
        </div>
      {/if}

      <details class="further">
        <summary>Explore further</summary>
        <div class="row">
          <input aria-label="What should it also consider?" placeholder="What should it also consider?" bind:value={more} />
          <button type="button" class="btn small" disabled={!more.trim() || !!waiting} onclick={() => void explore(session.id)}>Explore further</button>
        </div>
      </details>
    {/if}

    <footer class="d-row">
      <span class="d-hint">{session ? "1–9 choose · ⌘↩ " + (brief && !edited ? "start the spec" : "make the brief") : "⌘↩ explore · read-only"}</span>
      <button type="button" class="btn" onclick={close}>Close</button>
      {#if !session}
        <button type="button" class="btn primary" disabled={!idea.trim() || !!waiting || !!app.job?.running} onclick={() => void explore()}>Explore</button>
      {:else if !approaches.length}
        <button type="button" class="btn primary" disabled={!!waiting || !!app.job?.running} onclick={() => { more = session?.idea ?? ""; void explore(session!.id); }}>Explore again</button>
      {:else if brief && !edited}
        <button type="button" class="btn primary" onclick={start}>Start spec from this brief</button>
      {:else}
        <button type="button" class="btn primary" disabled={!approaches.length || !!waiting || !!app.job?.running} onclick={() => void choose()}>{p?.choice ? "Make the brief again" : "Make the brief"}</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .explore { gap: 1rem; }
  .idea { display: grid; grid-template-columns: auto 1fr; gap: 0.5rem; align-items: start; font-size: 0.92rem; }
  .idea p { margin: 0; font-weight: 600; }
  .idea span { font-size: 0.78rem; }
  .working { margin: 0; display: flex; align-items: center; gap: 0.5rem; color: var(--text-2); font-size: 0.88rem; }
  .sessions { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.2rem; }
  .sessions button { width: 100%; display: grid; grid-template-columns: auto 1fr auto; gap: 0.6rem; align-items: center; padding: 0.45rem 0.6rem; border: 0; border-radius: 9px; background: var(--fill); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .sessions button:hover { background: var(--fill-2); }
  .si { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st { font-size: 0.7rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  .st.chosen { color: #a646d8; }
  .st.started { color: var(--verified); }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr)); gap: 0.6rem; }
  .card { position: relative; display: grid; align-content: start; gap: 0.35rem; padding: 0.8rem 0.85rem 0.75rem; border-radius: 14px; background: var(--surface); box-shadow: inset 0 0 0 1px var(--separator); cursor: pointer; transition: box-shadow var(--speed) var(--ease), transform 120ms var(--ease); }
  .card:hover { box-shadow: inset 0 0 0 1px color-mix(in srgb, #bf5af2 40%, var(--separator)); }
  .card.on { box-shadow: inset 0 0 0 2px #bf5af2, 0 6px 18px color-mix(in srgb, #bf5af2 16%, transparent); }
  .card:focus-visible { outline: none; box-shadow: inset 0 0 0 2px #bf5af2, 0 0 0 4px var(--focus); }
  .card .n { position: absolute; top: 0.6rem; right: 0.7rem; width: 1.3rem; height: 1.3rem; border-radius: 50%; display: grid; place-items: center; font: 700 0.7rem/1 var(--rounded); background: var(--fill); color: var(--muted); }
  .card.on .n { background: #bf5af2; color: #fff; }
  .card strong { padding-right: 1.6rem; font-size: 0.92rem; }
  .card p { margin: 0 0 0.2rem; font-size: 0.84rem; color: var(--text-2); line-height: 1.45; }
  .tag { display: flex; gap: 0.35rem; align-items: baseline; font-size: 0.76rem; line-height: 1.4; }
  .tag.give { color: var(--text-2); }
  .tag.risk { color: var(--attention); }
  .lists { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
  .items { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.25rem; }
  .items li { display: grid; grid-template-columns: 1fr auto auto; gap: 0.4rem; align-items: center; padding: 0.3rem 0.3rem 0.3rem 0.6rem; border-radius: 8px; background: var(--fill); font-size: 0.84rem; }
  .items li.person { background: color-mix(in srgb, #bf5af2 10%, transparent); }
  .items li.nobody { background: var(--attention-bg); }
  .src { font-size: 0.68rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  .items button { width: 18px; height: 18px; border: 0; border-radius: 50%; display: grid; place-items: center; background: none; color: var(--muted); cursor: pointer; }
  .items button:hover { background: var(--fill-2); color: var(--text); }
  .brief { display: grid; gap: 0.4rem; padding: 0.7rem 0.8rem; border-radius: 12px; background: color-mix(in srgb, #bf5af2 7%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, #bf5af2 22%, transparent); }
  .brief.stale { opacity: 0.6; }
  .bh { display: flex; gap: 0.4rem; align-items: center; font-size: 0.84rem; }
  .bh code { font-size: 0.76rem; color: var(--muted); }
  .brief pre { margin: 0; max-height: 14rem; overflow: auto; font: 0.8rem/1.5 var(--mono); white-space: pre-wrap; }
  .further summary { cursor: pointer; font-size: 0.84rem; color: var(--accent-ink); font-weight: 600; }
  .further .row { display: flex; gap: 0.5rem; margin-top: 0.5rem; }
  .further input { flex: 1; }
  @media (max-width: 640px) { .lists { grid-template-columns: 1fr; } }
</style>
