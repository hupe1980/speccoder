<script lang="ts">
  // One requirement's tests at review (spec 093): the visible acceptance
  // tests the run would use, written on the person's request, red-checked
  // on the current code, shaped here — reject, ask again, edit — and frozen
  // when the spec is approved. Of the held-out tests only their number is
  // shown: they are kept from the person and the implementer alike.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import { comments } from "../lib/comments.svelte";
  import { language } from "../lib/highlight";
  import { calls, coverage, discardWord, stageLabel, testsCommand } from "../lib/review";
  import type { OracleView, Runnable } from "../lib/types";
  import Icon from "./Icon.svelte";
  import Source from "./Source.svelte";

  let { v }: { v: OracleView } = $props();

  const running = $derived(!!app.job?.running);
  const stage = $derived(app.oracleStage?.requirement === v.req ? app.oracleStage : null);
  const scenario = (id: string) => v.scenarios.find((s) => s.id === id)?.text ?? id;
  const when = (s: number | null) => (s ? new Date(s * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "");

  const pill = (red: boolean, cls: string) => (red ? `red: ${cls.replaceAll("_", " ")}` : cls === "passed" ? "already passes" : cls.replaceAll("_", " "));

  function act(op: "write" | "reject" | "again" | "edit" | "reading", extra: Partial<Extract<Runnable, { kind: "oracle_tests" }>>, label: string, command: string, input: string | null = null) {
    return offer(
      { kind: "oracle_tests", req: v.req, op, path: null, scenario: null, reading: null, ...extra },
      { group: "review", label, why: "red-checked on the current code; nothing is frozen until you approve", command, input },
    );
  }
  const write = () => app.perform(act("write", {}, `write tests for ${v.req}`, testsCommand(v.req, "write")));
  const reject = (path: string) => app.perform(act("reject", { path }, `reject ${path}`, testsCommand(v.req, "reject", path), "Why is this test wrong?"));
  const again = (path: string) => app.perform(act("again", { path }, `ask again for ${path}`, testsCommand(v.req, "again", path), "What should it check instead?"));
  const reading = (scenario_: string, r: "A" | "B") => app.perform(act("reading", { scenario: scenario_, reading: r }, `reading ${r} of ${scenario_}`, testsCommand(v.req, "reading", r, scenario_)));
  function reword() {
    app.doc = "spec";
    app.view = "documents";
    app.toast({ tone: "ok", text: `Reword the scenario in the spec, then Write Tests for ${v.req} again` });
  }

  let open = $state<Record<string, boolean>>({});
  let editing = $state<string | null>(null);
  let draft = $state("");
  async function save(path: string) {
    if (!draft.trim()) return;
    await app.perform(act("edit", { path }, `edit ${path}`, testsCommand(v.req, "edit", path)), draft);
    editing = null;
  }
  // A test line's comment: a small form under the line.
  let commenting = $state<{ path: string; line: number; quote: string } | null>(null);
  let words = $state("");
  async function addComment() {
    if (!commenting || !words.trim()) return;
    await comments.add({ kind: "test_line", req: v.req, path: commenting.path, line: commenting.line, quote: commenting.quote }, words.trim());
    commenting = null;
    words = "";
  }
  const openAmbiguities = $derived(v.ambiguities.filter((a) => !a.resolution));
  // Frozen with the approval: the run uses exactly these, so they are read
  // only — rewording the requirement lapses the freeze.
  const frozen = $derived(v.state === "frozen");
</script>

<article class="card" aria-labelledby="t-{v.req}">
  <header>
    <span class="id mono" id="t-{v.req}">{v.req}</span>
    <span class="title">{v.title}</span>
    <span class="count" title="{v.tests.filter((t) => t.red).length} red tests">{v.tests.length || "—"}</span>
    {#if v.state === "frozen"}
      <span class="chip s-ready"><Icon name="lock" size={10} weight={2.2} /> {`Frozen${v.frozen_by ? ` by ${v.frozen_by}` : ""}${v.frozen_at ? ` · ${when(v.frozen_at)}` : ""}`}</span>
    {:else if v.state === "lapsed"}
      <span class="chip s-needs_approval"><Icon name="exclaim" size={10} weight={2.2} /> Lapsed</span>
    {:else if v.state === "frozen_elsewhere"}
      <span class="chip s-neutral">Frozen on another machine</span>
    {/if}
  </header>
  <ol class="scenarios">
    {#each v.scenarios as s (s.id)}<li><span class="mono sid">{s.id}</span> <span class="muted">{s.text.replace(/\*\*/g, "")}</span></li>{/each}
  </ol>

  {#if v.state === "lapsed"}
    <p class="note">Frozen on the earlier wording — the run writes tests anew unless you write and approve them again.{#if v.frozen_on}<span class="was mono">{v.frozen_on}</span>{/if}</p>
  {:else if v.state === "frozen_elsewhere"}
    <p class="note muted">Frozen on another machine: the run will write tests anew here.</p>
  {:else if frozen}
    <p class="note muted">Frozen with the approval: these are the tests the run uses, as they were frozen. Reword the requirement to lapse the freeze and write them again.</p>
  {/if}

  {#if openAmbiguities.length}
    <div class="ambiguous" role="group" aria-label="Two readings">
      {#each openAmbiguities as a (a.scenario)}
        <p><Icon name="exclaim" size={13} weight={2.2} /> <strong>Two readings of {a.scenario}.</strong>
          A: given <code>{a.a.given || "—"}</code>, expects <code>{a.a.expect}</code> · B: given <code>{a.b.given || "—"}</code>, expects <code>{a.b.expect}</code>.</p>
        <div class="acts">
          <button class="btn small" disabled={running} title={testsCommand(v.req, "reading", "A", a.scenario)} onclick={() => reading(a.scenario, "A")}>A Is Meant</button>
          <button class="btn small" disabled={running} title={testsCommand(v.req, "reading", "B", a.scenario)} onclick={() => reading(a.scenario, "B")}>B Is Meant</button>
          <button class="btn small ghost" onclick={reword}>Reword…</button>
        </div>
      {/each}
    </div>
  {/if}

  {#if stage}
    <p class="stage" aria-live="polite"><span class="spinner" aria-hidden="true"></span> {stageLabel(stage.stage, stage.n, stage.of)}</p>
  {/if}

  {#if v.tests.length === 0}
    <p class="before">No tests yet. Write the tests the run would use, check they fail now, and freeze them when you approve.</p>
    {#if v.error}<p class="bad"><Icon name="exclaim" size={12} /> {v.error}</p>{/if}
    <div class="acts">
      <button class="btn small primary" disabled={running} title={testsCommand(v.req, "write")} onclick={write}><Icon name="sparkle" size={11} weight={2.2} /> Write Tests</button>
      <span class="muted small">{calls(v)}</span>
    </div>
  {:else}
    <p class="summary">
      <span title="Kept from you and the implementer; shown per task after the tick"><Icon name="lock" size={11} weight={2.2} /> Held out: {v.held_out.state === "off" ? "off" : v.held_out.count}</span>
      <span>· Strength target: {v.strength_target != null ? `${v.strength_target}%` : "none set"}</span>
      <span>· {coverage(v)}</span>
    </p>
    {#if v.uncovered.length}
      <p class="muted small">Not covered: {#each v.uncovered as u, n (u)}{@const why_ = v.untestable.find((x) => x.scenario === u)}{n ? ", " : ""}{u}{#if why_} ({why_.why}){/if}{/each}</p>
    {/if}
    {#if v.error}<p class="bad"><Icon name="exclaim" size={12} /> {v.error}</p>{/if}
    {#each v.tests as t (t.path)}
      <section class="test">
        <button class="thead" aria-expanded={!!open[t.path]} onclick={() => (open = { ...open, [t.path]: !open[t.path] })}>
          <Icon name={open[t.path] ? "down" : "chevron"} size={11} weight={2.2} />
          <span class="mono path">{t.path}</span>
          {#each t.scenarios as s (s)}<span class="sc mono" title={scenario(s)}>{s}</span>{/each}
          <span class="pill" class:ev={t.red} class:no={!t.red}>{pill(t.red, t.red_class)} — {t.red ? "evidence" : "no evidence"}</span>
          {#if t.edited_by}<span class="edited">edited by {t.edited_by}</span>{/if}
        </button>
        {#if open[t.path]}
          {#if editing === t.path}
            <!-- svelte-ignore a11y_autofocus -->
            <textarea class="editor mono" rows="10" bind:value={draft} autofocus aria-label="{t.path} content" onkeydown={(e) => { if (e.key === "Escape") editing = null; if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void save(t.path); } }}></textarea>
            <div class="acts">
              <button class="btn small primary" disabled={running || !draft.trim()} onclick={() => save(t.path)} title={testsCommand(v.req, "edit", t.path)}>Save and Check</button>
              <button class="btn small ghost" onclick={() => (editing = null)}>Cancel</button>
            </div>
          {:else}
            <table class="code"><tbody>
              {#each t.content.replace(/\n$/, "").split("\n") as line, n (n)}
                <tr>
                  <td class="ln">{n + 1}</td>
                  <td class="plus">{#if !frozen}<button class="add" aria-label="Comment on line {n + 1} of {t.path}" onclick={() => { commenting = { path: t.path, line: n + 1, quote: line }; words = ""; }}>+</button>{/if}</td>
                  <td class="src"><Source {line} lang={language(t.path)} /></td>
                </tr>
                {#if commenting?.path === t.path && commenting.line === n + 1}
                  <tr><td colspan="3">
                    <form class="popover" onsubmit={(e) => { e.preventDefault(); void addComment(); }}>
                      <!-- svelte-ignore a11y_autofocus -->
                      <textarea rows="2" bind:value={words} autofocus aria-label="Comment on line {n + 1}" onkeydown={(e) => { if (e.key === "Escape") commenting = null; if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void addComment(); } }}></textarea>
                      <div class="acts"><button class="btn small primary" type="submit" disabled={!words.trim()}>Add Comment <kbd>⌘↵</kbd></button><button class="btn small ghost" type="button" onclick={() => (commenting = null)}>Cancel</button></div>
                    </form>
                  </td></tr>
                {/if}
              {/each}
            </tbody></table>
            {#if !frozen}
              <div class="acts">
                <button class="btn small" disabled={running} title={testsCommand(v.req, "reject", t.path)} onclick={() => reject(t.path)}><Icon name="xmark" size={10} weight={2.4} /> Reject…</button>
                <button class="btn small" disabled={running} title={testsCommand(v.req, "again", t.path)} onclick={() => again(t.path)}><Icon name="sparkle" size={10} weight={2.2} /> Ask Again…</button>
                <button class="btn small ghost" disabled={running} onclick={() => { editing = t.path; draft = t.content; }}><Icon name="doc" size={10} weight={2} /> Edit</button>
              </div>
            {/if}
          {/if}
        {/if}
      </section>
    {/each}
    {#if v.discarded.length}
      <details class="discarded"><summary>{v.discarded.length} discarded</summary>
        <ul>{#each v.discarded as d (d.path)}<li><span class="mono">{d.path}</span> — {discardWord(d.why, v.req)}</li>{/each}</ul>
      </details>
    {/if}
    {#if v.rejected.length}
      <details class="discarded"><summary>{v.rejected.length} rejected by you</summary>
        <ul>{#each v.rejected as r (r.path + r.at)}<li><span class="mono">{r.path}</span> — “{r.reason}” · {r.by}</li>{/each}</ul>
      </details>
    {/if}
    {#if !frozen}
      <div class="acts">
        <button class="btn small ghost" disabled={running} title={testsCommand(v.req, "write")} onclick={write}><Icon name="loop" size={10} weight={2.2} /> Write Again</button>
        <span class="muted small">{calls(v)}</span>
      </div>
    {/if}
  {/if}
</article>

<style>
  .card { background: var(--surface); border-radius: var(--radius-lg); box-shadow: var(--shadow); padding: 1rem 1.2rem; display: grid; gap: 0.55rem; }
  header { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .id { font-weight: 650; font-size: 0.82rem; }
  .title { font-weight: 600; }
  .count { font: 650 0.7rem/1 var(--rounded); padding: 0.15rem 0.45rem; border-radius: 999px; background: var(--fill); color: var(--muted); }
  .chip { margin-left: auto; display: inline-flex; gap: 0.25rem; align-items: center; font-size: 0.72rem; font-weight: 650; padding: 0.08rem 0.55rem; border-radius: 999px; }
  .s-neutral { background: var(--fill); color: var(--muted); }
  .scenarios { margin: 0; padding-left: 1.1rem; font-size: 0.82rem; display: grid; gap: 0.15rem; }
  .sid { font-size: 0.74rem; font-weight: 600; }
  .before, .note { margin: 0; color: var(--text-2); font-size: 0.9rem; }
  .was { display: block; margin-top: 0.3rem; white-space: pre-wrap; font-size: 0.76rem; color: var(--muted); text-decoration: line-through; }
  .summary { margin: 0; display: flex; gap: 0.35rem; flex-wrap: wrap; font-size: 0.84rem; color: var(--text-2); align-items: center; }
  .small { font-size: 0.8rem; }
  .bad { margin: 0; color: var(--danger); font-size: 0.84rem; display: flex; gap: 0.3rem; align-items: center; }
  .acts { display: flex; gap: 0.4rem; flex-wrap: wrap; align-items: center; }
  .ambiguous { display: grid; gap: 0.4rem; padding: 0.7rem 0.85rem; border-radius: var(--radius); background: var(--attention-bg); }
  .ambiguous p { margin: 0; font-size: 0.88rem; }
  .ambiguous code { background: var(--surface); padding: 0 0.3em; border-radius: 4px; }
  .stage { margin: 0; display: flex; gap: 0.4rem; align-items: center; font-size: 0.84rem; color: var(--accent-ink); }
  .spinner { width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--fill-2); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .test { border-radius: var(--radius); background: var(--surface-2); overflow: hidden; }
  .thead { all: unset; box-sizing: border-box; width: 100%; display: flex; flex-wrap: wrap; align-items: center; gap: 0.4rem; padding: 0.45rem 0.7rem; cursor: pointer; font-size: 0.8rem; }
  .thead:focus-visible { box-shadow: inset 0 0 0 2px var(--focus); }
  .path { font-size: 0.78rem; }
  .sc { font-size: 0.7rem; padding: 0.05rem 0.4rem; border-radius: 999px; background: var(--fill); }
  .pill { margin-left: auto; font-size: 0.72rem; font-weight: 650; padding: 0.05rem 0.5rem; border-radius: 999px; }
  .pill.ev { background: var(--verified-bg); color: var(--verified); }
  .pill.no { background: var(--attention-bg); color: var(--attention); }
  .edited { font-size: 0.72rem; color: var(--accent-ink); }
  .code { border-collapse: collapse; width: 100%; font: 0.76rem/1.55 var(--mono); }
  .code td { padding: 0; vertical-align: top; }
  .ln { width: 1%; min-width: 3ch; padding: 0 0.4rem; text-align: right; color: var(--faint); user-select: none; }
  .plus { width: 1%; }
  .add { border: 0; background: none; color: var(--faint); cursor: pointer; padding: 0 0.3rem; opacity: 0; }
  tr:hover .add, .add:focus-visible { opacity: 1; color: var(--accent-ink); }
  .src { white-space: pre; padding-right: 0.8rem; }
  .popover { display: grid; gap: 0.35rem; padding: 0.5rem 0.7rem; background: var(--surface); }
  .popover textarea, .editor { width: 100%; box-sizing: border-box; border-radius: 8px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); padding: 0.4rem 0.6rem; font: inherit; }
  .editor { font-family: var(--mono); font-size: 0.78rem; margin: 0.4rem 0.7rem; width: calc(100% - 1.4rem); }
  .discarded { font-size: 0.8rem; color: var(--muted); }
  .discarded ul { margin: 0.3rem 0 0; padding-left: 1.1rem; }
</style>
