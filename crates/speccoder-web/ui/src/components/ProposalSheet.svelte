<script lang="ts">
  // A change Ask suggested, as the action it would be: every word is the
  // person's to edit, code checks it again as they do, and only Confirm
  // performs it — the same action the palette and the CLI take.
  import { app } from "../lib/app.svelte";
  import { ask } from "../lib/ask.svelte";
  import { offer } from "../lib/actions";
  import { post } from "../lib/api";
  import type { Proposal, ProposalFields } from "../lib/schema.gen";
  import Icon from "./Icon.svelte";
  import WritingHelp from "./WritingHelp.svelte";

  const start = ask.sheet!.proposal;
  let f = $state<ProposalFields>(structuredClone($state.snapshot(start.fields)) as ProposalFields);
  let p = $state<Proposal>(start);
  let checking = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const TITLE: Record<string, [string, string, string]> = {
    quick_change: ["Quick change", "sparkle", "Recorded as a story, a requirement and a task, then run and judged like any other change."],
    bug_fix: ["Bug fix", "exclaim", "Recorded as a bug: speccoder reproduces it first, then fixes it and proves the fix."],
    nudge: ["Nudge", "hand", "A note the task's next attempt reads."],
    answer: ["Answer an open question", "check", "Your answer replaces the question's marker in the spec."],
  };
  const [title, icon, about] = TITLE[start.kind] ?? [start.kind, "sparkle", ""];

  // Code checks the edited words again (the target may have moved on).
  $effect(() => {
    const fields = $state.snapshot(f) as ProposalFields;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      checking = true;
      try {
        const r = await post<{ proposal: Proposal }>("/api/ask/proposal", { proposal: { ...start, fields }, feature: app.report?.data?.feature ?? null });
        p = r.proposal;
      } finally {
        checking = false;
      }
    }, 300);
    return () => clearTimeout(timer);
  });

  const close = () => (ask.sheet = null);
  async function confirm() {
    if (!p.runnable || p.blocked || checking) return;
    const run = p.runnable;
    close();
    await app.perform(offer(run, { group: "work", label: title.toLowerCase(), why: "suggested in Ask, confirmed by you", command: p.command }), p.input ?? undefined);
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void confirm();
    }
  }
  const lists: [keyof ProposalFields, string, string][] = $derived(
    start.kind === "quick_change" ? [["checks", "Done when", "quick"]] : start.kind === "bug_fix" ? [["expected", "What should happen instead", "bug_expected"], ["unchanged", "What must keep working", "bug_unchanged"]] : [],
  );
  function setList(k: keyof ProposalFields, i: number, v: string) {
    const l = [...((f[k] as string[] | undefined) ?? [])];
    l[i] = v;
    (f[k] as string[]) = l;
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="ps-title">
    <header>
      <span class="badge" aria-hidden="true"><Icon name={icon} size={20} weight={2.2} /></span>
      <div>
        <span class="from"><Icon name="bubble" size={11} /> Suggested in Ask</span>
        <h2 id="ps-title">{title}{f.target ? ` · ${f.target}` : ""}</h2>
        <p>{about}</p>
      </div>
    </header>

    {#if start.kind === "quick_change"}
      <label class="field"><span>What should change?</span>
        <!-- svelte-ignore a11y_autofocus -->
        <textarea rows="3" bind:value={f.request} autofocus></textarea>
      </label>
      <WritingHelp field="quick" text={f.request ?? ""} onshaped={(t) => (f.request = t)} />
    {:else if start.kind === "bug_fix"}
      <label class="field"><span>What goes wrong?</span>
        <!-- svelte-ignore a11y_autofocus -->
        <textarea rows="2" bind:value={f.symptom} autofocus></textarea>
      </label>
      <WritingHelp field="bug_symptom" text={f.symptom ?? ""} onshaped={(t) => (f.symptom = t)} />
    {:else if start.kind === "nudge"}
      <label class="field"><span>Your note for the next attempt</span>
        <!-- svelte-ignore a11y_autofocus -->
        <textarea rows="3" bind:value={f.note} autofocus></textarea>
      </label>
      <WritingHelp field="note" text={f.note ?? ""} onshaped={(t) => (f.note = t)} />
    {:else if start.kind === "answer"}
      <label class="field"><span>Your answer</span>
        <!-- svelte-ignore a11y_autofocus -->
        <textarea rows="3" bind:value={f.answer} autofocus></textarea>
      </label>
      <WritingHelp field="answer" text={f.answer ?? ""} onshaped={(t) => (f.answer = t)} />
    {/if}

    {#each lists as [k, label] (k)}
      {@const items = (f[k] as string[] | undefined) ?? []}
      <div class="field">
        <span>{label}</span>
        {#each [...items, ""] as v, i (i)}
          {#if i < items.length || i === 0 || items[i - 1]?.trim()}
            <div class="check">
              <span class="n mono">{i + 1}.</span>
              <input value={v} aria-label="{label}, {i + 1}" placeholder={i ? "and…" : ""} oninput={(e) => setList(k, i, (e.target as HTMLInputElement).value)} />
              {#if i < items.length}<button class="x" aria-label="Remove" onclick={() => ((f[k] as string[]) = items.filter((_, j) => j !== i))}><Icon name="xmark" size={10} weight={2.6} /></button>{/if}
            </div>
          {/if}
        {/each}
      </div>
    {/each}

    <div class="record" aria-live="polite">
      <span class="rl">What will be recorded</span>
      {#if p.blocked}
        <span class="blocked"><Icon name="exclaim" size={13} /> {p.blocked}</span>
      {:else}
        <code>{p.command}</code>
      {/if}
    </div>

    <div class="row">
      <span class="hint muted">Nothing is recorded until you confirm · ⌘↩</span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={!p.runnable || !!p.blocked || checking || !!app.job?.running} onclick={confirm}>{start.kind === "quick_change" || start.kind === "bug_fix" ? "Record and Run" : "Confirm"}</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 24; backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); padding: 1rem; }
  .sheet { width: min(600px, 100%); max-height: calc(100vh - 2rem); overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.2rem; display: grid; gap: 1rem; animation: sheet 400ms var(--spring); }
  @keyframes sheet { from { transform: scale(0.95) translateY(8px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .sheet { animation: none; } }
  header { display: flex; gap: 0.9rem; align-items: flex-start; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(160deg, #bf5af2, #5e5ce6); }
  .from { display: inline-flex; align-items: center; gap: 0.3rem; font-size: 0.72rem; font-weight: 600; color: #8944ab; }
  h2 { margin: 0.1rem 0 0; font: 650 1.25rem/1.25 var(--display); letter-spacing: -0.016em; }
  header p { margin: 0.3rem 0 0; color: var(--text-2); font-size: 0.9rem; }
  .field { display: grid; gap: 0.4rem; }
  .field > span { font-weight: 600; font-size: 0.88rem; }
  textarea, input { padding: 0.5rem 0.7rem; border-radius: 9px; border: 0.5px solid var(--separator); background: var(--surface); color: var(--text); font: inherit; }
  textarea { resize: vertical; line-height: 1.5; font-size: 1rem; }
  textarea:focus, input:focus { outline: none; box-shadow: 0 0 0 3.5px var(--focus); }
  .check { display: flex; align-items: center; gap: 0.4rem; }
  .check input { flex: 1; min-width: 0; font-size: 0.9rem; }
  .n { width: 1.4rem; color: var(--muted); font-size: 0.8rem; text-align: right; }
  .x { width: 22px; height: 22px; border-radius: 50%; border: 0; background: var(--fill); color: var(--muted); display: grid; place-items: center; cursor: pointer; }
  .record { display: grid; gap: 0.3rem; padding: 0.65rem 0.75rem; border-radius: 10px; background: var(--fill); }
  .rl { font-size: 0.72rem; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--muted); }
  .record code { font: 0.78rem/1.45 var(--mono); overflow-wrap: anywhere; user-select: all; }
  .blocked { display: flex; gap: 0.35rem; align-items: center; color: var(--danger); font-size: 0.84rem; }
  .row { display: flex; justify-content: flex-end; gap: 0.5rem; align-items: center; }
  .hint { margin-right: auto; font-size: 0.76rem; }
  @media (prefers-color-scheme: dark) { .from { color: #da8fff; } }
</style>
