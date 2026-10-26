<script lang="ts">
  // A bug, in the person's words — what happens, what should, what must
  // stay — reproduced by a test-blind author, confirmed by the person once
  // here, then fixed and judged with the revert. The sheet records and
  // decides through the host's actions; it resumes from the host's state.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { STAGES, at, blocked, command, outcomeWords } from "../lib/bug";
  import type { BugReport, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";

  const opened = app.bugSheet ?? {};
  let symptom = $state(opened.symptom ?? "");
  let expected = $state<string[]>([""]);
  let unchanged = $state<string[]>([]);
  let issue = $state<number | null>(opened.issue ?? null);
  let tests = $state<string[]>([]);
  let candidate = $state("");
  let bug = $state<BugReport | null>(null);
  let key = $state<string | null>(opened.key ?? null);
  let loadError = $state<string | null>(null);

  const why = $derived(blocked(symptom, expected, tests));
  const ready = $derived(!why && !app.job?.running);

  async function load() {
    if (!key) return;
    try {
      const r = await get<Reply<BugReport>>(`/api/bug${q({ task: key })}`);
      bug = r.data ?? null;
      loadError = r.data ? null : r.message;
    } catch (e) {
      loadError = String((e as Error).message ?? e);
    }
  }
  $effect(() => { void load(); });
  // A finished job that answered with this (or a new) bug: show it.
  $effect(() => {
    const r = app.job?.result;
    if (r?.payload === "bug" && r.data) {
      const b = r.data as BugReport;
      bug = b;
      key = `${b.unit}/${b.task}`;
    }
  });

  async function record(run: boolean) {
    if (!ready) return;
    const f = { symptom, expected, unchanged, issue, tests, candidate, run };
    await app.perform(
      offer(
        {
          kind: "bug_fix",
          symptom: symptom.trim(),
          expected: expected.map((x) => x.trim()).filter(Boolean),
          unchanged: unchanged.map((x) => x.trim()).filter(Boolean),
          issue,
          tests: tests.map((x) => x.trim()).filter(Boolean),
          candidate: candidate.trim() || null,
          slug: null,
          run,
        },
        { group: "work", label: run ? "record and reproduce the bug" : "record the bug", why: "your words become the triad; nothing is fixed before you confirm a failing reproduction", command: command(f) },
      ),
    );
  }

  async function decide(reject: boolean) {
    if (!bug?.confirm_digest || !key) return;
    await app.perform(
      offer(
        { kind: "bug_confirm", task: key, digest: bug.confirm_digest, reject },
        {
          group: "next",
          label: reject ? "reject the reproductions" : "confirm the reproductions",
          why: reject ? "the author tries again, told your reason" : "they are frozen and the fix runs",
          command: reject ? `speccoder bug confirm ${key} --reject --reason "…"` : `speccoder bug confirm ${key} --digest ${bug.confirm_digest}`,
          input: reject ? "Why they do not show the bug" : null,
        },
      ),
    );
  }

  const close = () => (app.bugSheet = null);
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !bug) {
      e.preventDefault();
      void record(true);
    }
  }
  const list = (xs: string[], i: number, v: string) => xs.map((x, j) => (j === i ? v : x));
</script>

<svelte:window onkeydown={keydown} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="bug-title">
    <header>
      <span class="badge" aria-hidden="true"><Icon name="exclaim" size={20} weight={2.2} /></span>
      <div>
        <h2 id="bug-title">{bug ? bug.title : "Report a Bug"}</h2>
        <p>{bug ? outcomeWords(bug) : "Say what happens and what should. A test-blind author writes one reproduction per expected behaviour; it must fail on the untouched code by an assertion before anything is fixed, and you confirm it once."}</p>
      </div>
    </header>

    <ol class="strip" aria-label="Stages">
      {#each STAGES as s, i (s.stage)}
        <li class:done={i < at(bug)} class:now={i === at(bug)}>{s.label}</li>
      {/each}
    </ol>

    {#if loadError}<p class="warn">{loadError}</p>{/if}

    {#if bug}
      {#if bug.reproductions.length}
        <ul class="repros">
          {#each bug.reproductions as r (r.file)}
            <li>
              <span class="st mono">{r.statement}</span>
              <span class="w"><strong>{bug.triad.expected.find((s) => s.id === r.statement)?.text ?? bug.triad.unchanged.find((s) => s.id === r.statement)?.text}</strong><span class="d mono">{r.file} :: {r.test}</span><span class="d mono" title="the exact command you confirm">$ {r.argv.join(" ")}</span></span>
              <span class="cls mono">{r.baseline.join(" · ")}{r.final !== "not_run" ? ` → ${r.final}` : ""}{r.revert !== "not_run" ? ` · revert ${r.revert}` : ""}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if bug.held_out_reason}
        <p class="warn">No held-out variant could be kept ({bug.held_out_reason}): the fix cannot be verified until one is.</p>
      {/if}
      {#if bug.handoff}
        <p class="muted">{bug.handoff.tries.length} tries · reproduced: {bug.handoff.statements.reproduced.join(", ") || "none"} · not: {bug.handoff.statements.not_reproduced.join(", ") || "none"} · already passing: {bug.handoff.statements.satisfied.join(", ") || "none"}</p>
      {/if}
      <div class="row">
        {#if bug.outcome === "awaiting_confirmation"}
          <span class="hint muted mono">digest {bug.confirm_digest?.slice(0, 19)}…</span>
          <button class="btn" onclick={close}>Later</button>
          <button class="btn" disabled={!!app.job?.running} onclick={() => decide(true)}>Reject with Reason</button>
          <button class="btn primary" disabled={!!app.job?.running} onclick={() => decide(false)}>Confirm and Fix</button>
        {:else}
          <button class="btn primary" onclick={close}>Close</button>
        {/if}
      </div>
    {:else}
      <label class="field">
        <span>What happens?</span>
        <!-- svelte-ignore a11y_autofocus -->
        <textarea rows="2" bind:value={symptom} autofocus placeholder="Saving an empty title crashes"></textarea>
      </label>
      <div class="field">
        <span>What should happen instead <em>— one per behaviour; each gets its own reproduction</em></span>
        {#each expected as v, i (i)}
          <div class="item"><input value={v} oninput={(e) => (expected = list(expected, i, e.currentTarget.value))} placeholder="An empty title is refused with a message" aria-label="Expected, {i + 1}" />{#if expected.length > 1}<button class="x" aria-label="Remove" onclick={() => (expected = expected.filter((_, j) => j !== i))}><Icon name="xmark" size={10} weight={2.6} /></button>{/if}</div>
        {/each}
        <button class="add" onclick={() => (expected = [...expected, ""])}><Icon name="plus" size={10} weight={2.6} /> Another expected behaviour</button>
      </div>
      <div class="field">
        <span>What must keep working <em>— optional</em></span>
        {#each unchanged as v, i (i)}
          <div class="item"><input value={v} oninput={(e) => (unchanged = list(unchanged, i, e.currentTarget.value))} placeholder="A title is saved as typed" aria-label="Unchanged, {i + 1}" /><button class="x" aria-label="Remove" onclick={() => (unchanged = unchanged.filter((_, j) => j !== i))}><Icon name="xmark" size={10} weight={2.6} /></button></div>
        {/each}
        <button class="add" onclick={() => (unchanged = [...unchanged, ""])}><Icon name="plus" size={10} weight={2.6} /> Something that must not change</button>
      </div>
      <details>
        <summary>Issue, your own test, someone else's fix</summary>
        <label class="field"><span>Issue number <em>— context for the author, never the triad</em></span><input type="number" min="1" value={issue ?? ""} oninput={(e) => (issue = e.currentTarget.value ? Number(e.currentTarget.value) : null)} /></label>
        <div class="field">
          <span>Your failing test <em>— PATH::NAME, one per expected behaviour; naming it confirms it</em></span>
          {#each tests as v, i (i)}
            <div class="item"><input class="mono" value={v} oninput={(e) => (tests = list(tests, i, e.currentTarget.value))} placeholder="tests/title.rs::empty_title_is_refused" aria-label="Test, {i + 1}" /><button class="x" aria-label="Remove" onclick={() => (tests = tests.filter((_, j) => j !== i))}><Icon name="xmark" size={10} weight={2.6} /></button></div>
          {/each}
          <button class="add" onclick={() => (tests = [...tests, ""])}><Icon name="plus" size={10} weight={2.6} /> A test</button>
        </div>
        <label class="field"><span>A fix to judge <em>— a patch file or pr:N; judged, never retried</em></span><input class="mono" bind:value={candidate} placeholder="pr:123" /></label>
      </details>
      <div class="row">
        <span class="hint muted">{why ?? "⌘↩ records and reproduces"}</span>
        <button class="btn" onclick={close}>Cancel</button>
        <button class="btn" disabled={!ready} title={why ?? ""} onclick={() => record(false)}>Record</button>
        <button class="btn primary" disabled={!ready} title={why ?? ""} onclick={() => record(true)}>Record and Reproduce</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 22; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(640px, 100%); max-height: calc(100vh - 2rem); overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 0.9rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .sheet { animation: none; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #ff6961, #d70015); }
  h2 { margin: 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); font-size: 0.9rem; }
  .strip { display: flex; gap: 0.25rem; list-style: none; margin: 0; padding: 0; }
  .strip li { flex: 1; text-align: center; font-size: 0.74rem; padding: 0.25rem 0; border-radius: 6px; background: var(--fill); color: var(--muted); }
  .strip li.done { background: var(--accent-fill, var(--fill)); color: var(--text); }
  .strip li.now { background: var(--accent); color: #fff; font-weight: 650; }
  .field { display: grid; gap: 0.4rem; }
  .field > span { font-weight: 600; font-size: 0.88rem; }
  em { font-style: normal; font-weight: 500; color: var(--muted); }
  textarea, input { padding: 0.5rem 0.7rem; border-radius: 9px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); font: inherit; }
  textarea { resize: vertical; line-height: 1.5; font-size: 1rem; }
  textarea:focus, input:focus { outline: none; box-shadow: 0 0 0 3.5px var(--focus); }
  .item { display: flex; gap: 0.4rem; align-items: center; }
  .item input { flex: 1; min-width: 0; font-size: 0.9rem; }
  .x { width: 22px; height: 22px; border-radius: 50%; border: 0; background: var(--fill); color: var(--muted); display: grid; place-items: center; cursor: pointer; }
  .add { justify-self: start; display: inline-flex; gap: 0.3rem; align-items: center; border: 0; background: none; color: var(--accent-ink); font-weight: 600; font-size: 0.82rem; cursor: pointer; padding: 0.2rem 0; }
  details { display: grid; gap: 0.6rem; }
  summary { cursor: pointer; font-size: 0.86rem; color: var(--text-2); }
  .repros { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.4rem; }
  .repros li { display: flex; gap: 0.6rem; align-items: baseline; padding: 0.45rem 0.6rem; border-radius: 9px; background: var(--fill); }
  .st { font-weight: 700; }
  .w { flex: 1; display: grid; }
  .d, .cls { font-size: 0.76rem; color: var(--muted); }
  .warn { color: var(--attention, #c93400); margin: 0; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; align-items: center; }
  .hint { margin-right: auto; font-size: 0.76rem; }
</style>
