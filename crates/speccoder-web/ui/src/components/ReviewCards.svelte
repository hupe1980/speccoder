<script lang="ts">
  // The spec, reviewed requirement by requirement: each card is one
  // requirement's wording and the person's decision on exactly that wording.
  // A wording changed since its decision shows what was decided, word by
  // word. Change requests go to the format's revising skill as one request.
  // It is a conversation: the documents' open questions come first,
  // answered in place; a requirement is reworded here; what changed since
  // the last commit and how the spec evolved are beside it.
  import { app } from "../lib/app.svelte";
  import { byKind, offer, type Offer } from "../lib/actions";
  import { get, latest, q } from "../lib/api";
  import type { Action, Reply, Review, ReviewItem } from "../lib/types";
  import { diffWords, questions as asks, tokenised } from "../lib/words";
  import Icon from "./Icon.svelte";
  import WritingHelp from "./WritingHelp.svelte";
  import Code from "./Code.svelte";
  import OracleTests from "./OracleTests.svelte";
  import BugTests from "./BugTests.svelte";
  import ApproveSheet from "./ApproveSheet.svelte";
  import { comments } from "../lib/comments.svelte";
  import { driftAnchor, testsCommand } from "../lib/review";

  let review = $state<Reply<Review> | null>(null);
  const fetchOf = latest();
  $effect(() => {
    void app.report;
    void app.job?.running;
    fetchOf(get<Reply<Review>>(`/api/review${q({ feature: app.feature ?? undefined })}`), (r) => (review = r), () => (review = null));
  });
  const r = $derived(review?.data ?? null);
  const total = $derived(r?.requirements.length ?? 0);
  // Approvable: not approved yet, and not still asking a question.
  const pending = $derived((r?.requirements ?? []).filter((i) => i.state !== "approved" && !asks(i.text).some((p) => p.question)));
  const words: Record<ReviewItem["state"], string> = { open: "To Review", approved: "Approved", changes_requested: "Changes Requested", lapsed: "Changed Since Decided" };
  const tone: Record<ReviewItem["state"], string> = { open: "ready", approved: "verified", changes_requested: "needs_approval", lapsed: "needs_approval" };

  const act = (ids: string[], verdict: "approve" | "request" | "clear", label: string): Offer =>
    offer(
      { kind: "review", ids, verdict },
      {
        group: "plan",
        label,
        why: verdict === "request" ? "the comment says what the requirement should say" : "recorded with your name, bound to this wording",
        command: `speccoder review ${verdict}${ids.length ? " " + ids.join(" ") : ""}${verdict === "request" ? ' --comment "…"' : ""}`,
        input: verdict === "request" ? `What should ${ids[0]} say?` : null,
      },
    );
  const reviseNow = () => {
    if (!r?.revise_with || !r.revision) return;
    void app.perform(
      offer(
        { kind: "skill", name: r.revise_with, arguments: r.revision },
        {
          group: "plan",
          label: `revise the spec (${r.revise_with})`,
          why: "the format's skill rewrites what you asked; you review the result",
          command: `speccoder spec ${r.revise_with} "…"`,
        },
      ),
    );
  };
  // Answers and edits, kept per card while typed.
  let answers = $state<Record<string, string>>({});
  let editing = $state<string | null>(null);
  let draft = $state("");
  const words_ = (id: string, verdict: "answer" | "edit" | "edit-approve", label: string, command: string): Offer =>
    offer(
      { kind: "review", ids: [id], verdict },
      { group: "plan", label, why: verdict === "answer" ? "your words replace the question where it stands" : "the line keeps its identifier", command },
    );
  async function answer(id: string) {
    const text = (answers[id] ?? "").trim();
    if (!text) return;
    await app.perform(words_(id, "answer", `answer ${id}`, `speccoder review answer ${id} "…"`), text);
    answers = { ...answers, [id]: "" };
  }
  async function save(id: string, approve: boolean) {
    const text = draft.trim();
    if (!text) return;
    await app.perform(words_(id, approve ? "edit-approve" : "edit", `reword ${id}`, `speccoder review edit ${id} "…"${approve ? " --approve" : ""}`), text);
    editing = null;
  }
  const clarify = () => {
    if (!r?.revise_with) return;
    void app.perform(offer({ kind: "skill", name: r.revise_with, arguments: "" }, { group: "plan", label: `clarify with ${r.revise_with}`, why: "the format's skill asks you the questions and writes the answers in", command: `speccoder spec ${r.revise_with}` }));
  };
  const commit = $derived(byKind(app.actions, "commit")[0]);
  const how = (then: string | null, now: string | null) => (!then ? "Added" : !now ? "Removed" : "Reworded");
  const plain = (l: string | null) => (l ?? "").replace(/^\s*-\s*\*\*[A-Z]+-\d+\*\*:?\s*/, "");
  // Approve and Run: the sheet with the three choices.
  let approving = $state(false);
  let menu = $state(false);
  const blockedBy = $derived(r?.sheet?.blocked ?? []);
  // Keep Refining: the comment tray, open and focused.
  function refine() {
    comments.open = true;
    requestAnimationFrame(() => document.getElementById("comment-tray")?.focus());
  }
  // A finding the review shows, as a comment in the tray.
  let finding = $state<{ id: string; text: string } | null>(null);
  let findingWords = $state("");
  async function addFinding() {
    if (!finding || !findingWords.trim()) return;
    await comments.add({ kind: "finding", source: "review", id: finding.id, text: finding.text }, findingWords.trim());
    finding = null;
    findingWords = "";
  }
  // A line changed since the last commit, commented on from its gutter +.
  let drift = $state<string | null>(null);
  let driftWords = $state("");
  async function addDrift(c: NonNullable<typeof r>["changes"][number]) {
    const anchor = driftAnchor(c);
    if (!anchor || !driftWords.trim()) return;
    await comments.add(anchor, driftWords.trim());
    drift = null;
    driftWords = "";
  }
  // The menu bar's Review › Write Tests: the first requirement without tests.
  $effect(() => {
    const on = (e: Event) => {
      if ((e as CustomEvent<string>).detail !== "review.tests") return;
      const v = r?.oracle.find((o) => o.tests.length === 0);
      if (!v) return app.toast({ tone: "warn", text: "Every requirement has its tests" });
      void app.perform(offer({ kind: "oracle_tests", req: v.req, op: "write", path: null, scenario: null, reading: null }, { group: "review", label: `write tests for ${v.req}`, why: "red-checked on the current code", command: testsCommand(v.req, "write") }));
    };
    window.addEventListener("speccoder:review", on);
    return () => window.removeEventListener("speccoder:review", on);
  });
  const when = (s: number | null) => (s ? new Date(s * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "");
</script>

{#if !review}
  <div class="center"><span class="spinner"></span></div>
{:else if !r || !total}
  <div class="none muted">{review.message}</div>
{:else}
  <section class="review" aria-label="Review the spec">
    <header>
      <div>
        <h2>Review the Spec</h2>
        <p class="sub">The result will be judged against these words. Approve each one, or say what it should say.</p>
      </div>
      <div class="progress" role="img" aria-label="{r.totals.approved} of {total} approved">
        <span class="bar"><span style="width:{(r.totals.approved / total) * 100}%"></span></span>
        <span class="mono">{r.totals.approved}/{total}</span>
      </div>
    </header>
    <div class="bulk">
      <span class="split">
        <button class="btn small primary" disabled={!!app.job?.running} title={blockedBy.length ? blockedBy.join("; ") : "speccoder review approve --run ask"} onclick={() => (approving = true)}><Icon name="check" size={11} weight={2.4} /> Approve and Run</button>
        <button class="btn small primary more" aria-label="More ways to approve" aria-expanded={menu} disabled={!!app.job?.running} onclick={() => (menu = !menu)}>▾</button>
        {#if menu}
          <span class="menu" role="menu">
            {#if pending.length}
              <button role="menuitem" onclick={() => { menu = false; void app.perform(act([], "approve", `approve ${pending.length} requirement(s)`)); }}>Approve {pending.length === total ? "All Lines" : `${pending.length} More Lines`} Only</button>
            {/if}
            <button role="menuitem" onclick={() => { menu = false; approving = true; }}>Approve and Run…</button>
          </span>
        {/if}
      </span>
      {#if r.totals.ambiguous}<span class="amb"><Icon name="exclaim" size={11} /> {r.totals.ambiguous} read two ways</span>{/if}
      {#if r.totals.changes_requested && r.revise_with}
        <button class="btn small tinted" disabled={!!app.job?.running} onclick={reviseNow}><Icon name="sparkle" size={11} weight={2.2} /> Revise the Spec ({r.totals.changes_requested} {r.totals.changes_requested === 1 ? "request" : "requests"})</button>
      {/if}
      <span class="file muted">Recorded in <code>{r.file}</code> — commit it with the spec.</span>
    </div>
    {#if r.questions.length}
      <section class="questions" aria-label="Open questions">
        <h3><Icon name="exclaim" size={14} weight={2.2} /> {r.questions.length === 1 ? "One question is open" : `${r.questions.length} questions are open`} <span class="muted">— answer {r.questions.length === 1 ? "it" : "them"} before the spec is built on</span>
          {#if r.revise_with}<button class="btn small ghost" disabled={!!app.job?.running} onclick={clarify}><Icon name="sparkle" size={10} weight={2.2} /> Ask Me with {r.revise_with}</button>{/if}
        </h3>
        {#each r.questions as q (q.id)}
          <div class="q">
            <div class="qtop"><span class="id mono">{q.id}</span><span class="where muted">{q.near ?? ""} · {q.file.split("/").pop()}:{q.line}</span></div>
            <p class="qtext">{q.text}</p>
            <button class="btn small ghost addc" onclick={() => { finding = { id: q.id, text: q.text }; findingWords = ""; }}>Add to Comments</button>
            {#if finding?.id === q.id}
              <form class="qform" onsubmit={(e) => { e.preventDefault(); void addFinding(); }}>
                <input placeholder="Your comment on {q.id}" bind:value={findingWords} aria-label="Comment on {q.id}" />
                <button class="btn small primary" type="submit" disabled={!findingWords.trim()}>Add Comment</button>
              </form>
            {/if}
            <form class="qform" onsubmit={(e) => { e.preventDefault(); void answer(q.id); }}>
              <input placeholder="Your answer — it takes the question's place" bind:value={answers[q.id]} aria-label="Answer to {q.id}" />
              <button class="btn small primary" type="submit" disabled={!!app.job?.running || !(answers[q.id] ?? "").trim()}>Write In</button>
            </form>
          </div>
        {/each}
      </section>
    {/if}
    <ol class="cards">
      {#each r.requirements as i (i.id)}
        <li class="card s-{tone[i.state]}-edge">
          <div class="top">
            <span class="id mono">{i.id}</span>
            <span class="chip s-{tone[i.state]}">{words[i.state]}</span>
          </div>
          {#if editing === i.id}
            <form class="edit" onsubmit={(e) => { e.preventDefault(); void save(i.id, false); }}>
              <!-- svelte-ignore a11y_autofocus -->
              <textarea rows="2" bind:value={draft} autofocus aria-label="{i.id} wording" onkeydown={(e) => { if (e.key === "Escape") editing = null; if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void save(i.id, true); } }}></textarea>
              <WritingHelp field="requirement" text={draft} onshaped={(t) => (draft = t)} />
              {#if draft.trim() && draft.trim() !== i.text}<p class="preview">{#each diffWords(i.text, draft.trim()) as s, n (n)}<span class={s.kind}>{s.text}</span>{/each}</p>{/if}
              <div class="acts">
                <button class="btn small primary" type="button" disabled={!draft.trim() || !!app.job?.running} onclick={() => save(i.id, true)} title="⌘↩">Save and Approve</button>
                <button class="btn small" type="submit" disabled={!draft.trim() || !!app.job?.running}>Save</button>
                <button class="btn small ghost" type="button" onclick={() => (editing = null)}>Cancel</button>
              </div>
            </form>
          {:else}
            <p class="text">
              {#if i.state === "lapsed" && i.decided_on}
                {#each diffWords(tokenised(i.decided_on), tokenised(i.text)) as s, n (n)}<span class={s.kind}>{s.text}</span>{/each}
              {:else}{#each asks(i.text) as part, n (n)}{#if part.question}<span class="ask" title="An open question — answer it above">? {part.text}</span>{:else}<Code text={part.text} />{/if}{/each}{/if}
            </p>
          {/if}
          {#if i.by}
            <p class="who muted">{i.state === "lapsed" ? "Decided on the earlier wording by" : i.state === "approved" ? "Approved by" : "Requested by"} {i.by}{#if i.at}{" · "}{when(i.at)}{/if}{#if i.comment}<span class="comment">“{i.comment}”</span>{/if}</p>
          {/if}
          <div class="acts">
            {#if i.state !== "approved"}
              {@const open = asks(i.text).some((p) => p.question)}
              <button class="btn small primary" disabled={!!app.job?.running || open} title={open ? "It still asks a question: answer it, or reword it" : ""} onclick={() => app.perform(act([i.id], "approve", `approve ${i.id}`))}><Icon name="check" size={11} weight={2.4} /> Approve</button>
            {/if}
            <button class="btn small" disabled={!!app.job?.running || editing === i.id} onclick={() => { editing = i.id; draft = i.text; }}><Icon name="doc" size={11} weight={2} /> Edit</button>
            <button class="btn small" disabled={!!app.job?.running} onclick={() => app.perform(act([i.id], "request", `request changes to ${i.id}`))}><Icon name="sparkle" size={11} weight={2.2} /> Request Changes</button>
            {#if i.state !== "open"}
              <button class="btn small ghost" disabled={!!app.job?.running} onclick={() => app.perform(act([i.id], "clear", `clear ${i.id}`))}>Clear</button>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
    {#if r.tests_notice}
      <section class="tests" aria-label="Tests">
        <h3><Icon name="shield" size={13} weight={2} /> Tests</h3>
        <p class="muted">{r.tests_notice}</p>
        {#if r.bug_tests?.length}<BugTests bugs={r.bug_tests} />{/if}
      </section>
    {:else if r.oracle.length}
      <section class="tests" aria-label="Tests">
        <h3><Icon name="shield" size={13} weight={2} /> Tests <span class="muted">— what the run would use, written now and frozen when you approve</span></h3>
        {#each r.oracle as v (v.req)}<OracleTests {v} />{/each}
      </section>
    {/if}
    {#if r.changes.length}
      <section class="since" aria-label="Since the last commit">
        <h3><Icon name="branch" size={13} weight={2} /> Since the last commit <span class="muted">— what a commit of the spec would record</span>
          {#if commit}<button class="btn small primary" disabled={!!commit.blocked || !!app.job?.running} title={`$ ${commit.command}`} onclick={() => commit && app.perform(commit)}>Commit</button>{/if}
        </h3>
        {#each r.changes as c (c.id)}
          <div class="change"><span class="id mono">{#if driftAnchor(c)}<button class="add" aria-label="Comment on {c.id} as changed" title="Comment on this line" onclick={() => { drift = c.id; driftWords = ""; }}>+</button>{/if}{c.id}</span><span class="how">{how(c.then, c.now)}</span>
            <p class="text small">{#each diffWords(tokenised(plain(c.then)), tokenised(plain(c.now))) as s, n (n)}<span class={s.kind}>{s.text}</span>{/each}</p>
          </div>
          {#if drift === c.id}
            <form class="qform driftc" onsubmit={(e) => { e.preventDefault(); void addDrift(c); }}>
              <!-- svelte-ignore a11y_autofocus -->
              <input placeholder="Your comment on {c.id} as it reads now" bind:value={driftWords} autofocus aria-label="Comment on the change to {c.id}" onkeydown={(e) => { if (e.key === "Escape") drift = null; }} />
              <button class="btn small primary" type="submit" disabled={!driftWords.trim()}>Add Comment</button>
              <button class="btn small ghost" type="button" onclick={() => (drift = null)}>Cancel</button>
            </form>
          {/if}
        {/each}
      </section>
    {/if}
    {#if r.history.length}
      <section class="history" aria-label="How the spec evolved">
        <h3><Icon name="hourglass" size={13} weight={2} /> How the spec evolved</h3>
        <ol>
          {#each r.history as v (v.commit)}
            <li>
              <span class="dot" aria-hidden="true"></span>
              <span class="when muted mono">{v.date}</span>
              <span class="subj">{v.subject}</span>
              <span class="what">
                {#if v.added.length}<span class="add" title={v.added.join(", ")}>+{v.added.length}</span>{/if}
                {#if v.reworded.length}<span class="re" title={v.reworded.join(", ")}>~{v.reworded.length}</span>{/if}
                {#if v.removed.length}<span class="rm" title={v.removed.join(", ")}>−{v.removed.length}</span>{/if}
              </span>
              <span class="by muted">{v.author} · <code>{v.commit.slice(0, 7)}</code></span>
            </li>
          {/each}
        </ol>
      </section>
    {/if}
  </section>
  {#if approving && r}<ApproveSheet review={r} onclose={() => (approving = false)} onrefine={refine} />{/if}
{/if}

<style>
  .review { display: grid; gap: 1rem; max-width: 78ch; }
  header { display: flex; justify-content: space-between; align-items: flex-end; gap: 1rem; flex-wrap: wrap; }
  h2 { margin: 0; font: 700 1.5rem/1.2 var(--display); letter-spacing: -0.02em; }
  .sub { margin: 0.25rem 0 0; color: var(--text-2); }
  .progress { display: flex; align-items: center; gap: 0.5rem; font-size: 0.82rem; }
  .bar { width: 9rem; height: 6px; border-radius: 6px; background: var(--fill); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--verified-fill); border-radius: 6px; transition: width 400ms var(--spring); }
  .bulk { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .file { font-size: 0.78rem; margin-left: auto; }
  .cards { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.7rem; }
  .card { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 1rem 1.2rem 1.05rem; display: grid; gap: 0.5rem; border-left: 3px solid var(--fill-c, var(--fill)); }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .id { font-weight: 650; font-size: 0.82rem; }
  .chip { font-size: 0.72rem; font-weight: 650; padding: 0.08rem 0.55rem; border-radius: 999px; }
  .text { margin: 0; font-size: 1rem; line-height: 1.55; color: var(--text); }
  .gone { color: var(--danger); text-decoration: line-through; opacity: 0.8; }
  .new { background: color-mix(in srgb, var(--verified-fill) 22%, transparent); border-radius: 4px; padding: 0 0.12em; font-weight: 550; }
  .who { margin: 0; font-size: 0.8rem; }
  .comment { display: block; margin-top: 0.2rem; color: var(--text-2); font-style: italic; }
  .acts { display: flex; gap: 0.4rem; flex-wrap: wrap; }
  .s-verified-edge { --fill-c: var(--verified-fill); }
  .s-needs_approval-edge { --fill-c: var(--attention-fill); }
  .s-ready-edge { --fill-c: var(--fill-2); }
  h3 { margin: 0; display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; font: 650 0.95rem/1.3 var(--display); }
  h3 .muted { font-weight: 500; font-size: 0.85rem; }
  h3 .btn { margin-left: auto; }
  .questions { display: grid; gap: 0.6rem; padding: 0.9rem 1rem; border-radius: var(--radius-lg); background: var(--attention-bg); }
  .q { display: grid; gap: 0.35rem; padding: 0.7rem 0.85rem; border-radius: var(--radius); background: var(--surface); }
  .qtop { display: flex; gap: 0.5rem; align-items: baseline; }
  .where { font-size: 0.76rem; }
  .qtext { margin: 0; font-weight: 600; }
  .qform { display: flex; gap: 0.4rem; }
  .qform input, .edit textarea { flex: 1; min-width: 0; padding: 0.4rem 0.6rem; border-radius: 8px; border: 0.5px solid var(--separator); background: var(--surface-2); color: var(--text); font: inherit; font-size: 0.92rem; }
  .qform input:focus, .edit textarea:focus { outline: none; box-shadow: 0 0 0 3.5px var(--focus); }
  .edit { display: grid; gap: 0.45rem; }
  .edit textarea { resize: vertical; line-height: 1.5; }
  .ask { display: inline-block; margin: 0 0.15em; padding: 0 0.5em; border-radius: 999px; background: var(--attention-bg); color: var(--attention); font-weight: 600; font-size: 0.88em; }
  .preview { margin: 0; font-size: 0.88rem; color: var(--text-2); }
  .since, .history { display: grid; gap: 0.5rem; padding: 0.9rem 1rem; border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow); }
  .change { display: grid; grid-template-columns: 4.5rem 5.5rem 1fr; gap: 0.5rem; align-items: baseline; padding: 0.35rem 0; border-top: 0.5px solid var(--separator); }
  .change:first-of-type { border-top: 0; }
  .how { font-size: 0.76rem; color: var(--muted); }
  .small { font-size: 0.88rem; }
  .history ol { list-style: none; margin: 0; padding: 0 0 0 0.4rem; border-left: 1.5px solid var(--separator); display: grid; gap: 0.55rem; }
  .history li { position: relative; display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: baseline; padding-left: 0.8rem; font-size: 0.86rem; }
  .history .dot { position: absolute; left: -0.48rem; top: 0.42rem; width: 7px; height: 7px; border-radius: 50%; background: var(--accent); }
  .when { font-size: 0.74rem; }
  .subj { font-weight: 550; }
  .what { display: inline-flex; gap: 0.25rem; }
  .what span { font: 600 0.7rem/1 var(--rounded); padding: 0.15rem 0.35rem; border-radius: 999px; }
  .add { background: var(--verified-bg); color: var(--verified); }
  .re { background: var(--attention-bg); color: var(--attention); }
  .rm { background: var(--danger-bg, var(--fill)); color: var(--danger); }
  .by { margin-left: auto; font-size: 0.74rem; }
  .none { padding: 2rem; }
  .split { position: relative; display: inline-flex; gap: 1px; }
  .split .more { padding-inline: 0.45rem; }
  .menu { position: absolute; top: calc(100% + 4px); left: 0; z-index: 5; display: grid; min-width: 14rem; padding: 0.3rem; border-radius: var(--radius); background: var(--surface); box-shadow: var(--shadow-lg); }
  .menu button { border: 0; background: none; text-align: left; padding: 0.4rem 0.6rem; border-radius: 6px; cursor: pointer; color: var(--text); font: inherit; font-size: 0.86rem; }
  .menu button:hover, .menu button:focus-visible { background: var(--fill); }
  .amb { display: inline-flex; gap: 0.3rem; align-items: center; font-size: 0.8rem; font-weight: 600; color: var(--attention); }
  .tests { display: grid; gap: 0.6rem; }
  .tests > p { margin: 0; }
  .addc { justify-self: start; }
  .change .add { border: 0; background: none; color: var(--faint); cursor: pointer; padding: 0 0.3rem 0 0; font: inherit; }
  .change:hover .add, .change .add:focus-visible { color: var(--accent-ink); }
  .driftc { display: flex; gap: 0.4rem; align-items: center; }
  .driftc input { flex: 1; }
  .center { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
