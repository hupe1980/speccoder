<script lang="ts">
  import { app } from "../lib/app.svelte";
  import Code from "./Code.svelte";
  import Icon from "./Icon.svelte";
  import { post } from "../lib/api";

  const w = $derived(app.waiting!);
  const p = $derived(w.presentation);
  let full = $state(false);
  let note = $state("");
  let reason = $state("");
  let rejecting = $state(false);
  let text = $state("");
  const offered = $derived(new Set(p?.request.options ?? []));
  const body = $derived((full ? p?.full : p?.body) ?? []);
  const kind = (l: string) => (l.trimStart().startsWith("+") && !l.includes("+++") ? "add" : l.trimStart().startsWith("-") && !l.includes("---") ? "del" : "");

  const approve = (scope: "once" | "task" | "run") => app.answer({ answer: { decision: "approve", scope, note: note.trim() || null, reason: null, by: null } });
  const reject = () => {
    if (!reason.trim()) return;
    app.answer({ answer: { decision: "reject", scope: "once", note: null, reason: reason.trim(), by: null } });
  };
  // An exact command can be allowed for good — not as a grant in memory,
  // but as a line in the project's policy, committed and reviewed like code.
  const argv = $derived.by(() => {
    const a = p?.request.arguments?.argv;
    return (p?.request.kind === "run" || p?.request.kind === "network") && Array.isArray(a) && a.length && a.every((x) => typeof x === "string") ? (a as string[]) : null;
  });
  const policyKey = $derived(p?.request.kind === "network" ? "sandbox.network_allow" : "commands.allow");
  let always = $state(false);
  async function allowAlways() {
    if (!argv) return;
    await approve("once");
    try {
      const r = await post<{ status: string; message: string }>("/api/config", { changes: [{ key: policyKey, append: argv }] });
      app.toast(r.status === "ok"
        ? { tone: "ok", text: `Added to [${policyKey.split(".")[0]}] ${policyKey.split(".")[1]}`, detail: "Commit speccoder.toml in Settings to put it in force" }
        : { tone: "warn", text: "Allowed once; the policy was not changed", detail: r.message });
    } catch (e) {
      app.toast({ tone: "warn", text: "Allowed once; the policy was not changed", detail: String(e) });
    }
  }
  const stopTask = () => app.answer({ answer: { decision: "stop", scope: "once", note: null, reason: null, by: null } });

  function keydown(e: KeyboardEvent) {
    if ((e.target as HTMLElement).closest("input, textarea")) return;
    if (!p) return;
    const k = e.key.toLowerCase();
    if (k === "y" || k === "1") approve("once");
    else if ((k === "t" || k === "2") && offered.has("task")) approve("task");
    else if ((k === "r" || k === "3") && offered.has("run")) approve("run");
    else if (k === "n" || k === "4") rejecting = true;
    else if (k === "s" || k === "5") stopTask();
    else if (k === "v") full = !full;
    else if (k === "a" && argv) always = true;
    else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="scrim">
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="ask-title">
    {#if p}
      <header>
        <span class="badge" aria-hidden="true"><Icon name="hand" size={22} weight={2} /></span>
        <div class="heads">
          <span class="eyebrow">{p.request.task}{p.request.turn ? ` · turn ${p.request.turn}` : ""}{#if p.request.task_title}{" · "}<Code text={p.request.task_title} />{/if}</span>
          <h2 id="ask-title">{p.title} <span class="mono subject">{p.subject}</span></h2>
        </div>
        <span class="risk r-{p.request.risk}">{p.request.risk} risk</span>
      </header>
      <pre class="body">{#each body as l}<span class={kind(l)}>{l}
</span>{/each}</pre>
      {#if p.full.length > p.body.length}
        <button class="btn small ghost more" onclick={() => (full = !full)}>{full ? "Show Less" : `Show All ${p.full.length} Lines`} <kbd>V</kbd></button>
      {/if}
      <dl class="grouped">
        <div><dt>Why it stopped</dt><dd><Code text={p.request.reason} /></dd></div>
        <div><dt>Sandbox</dt><dd>{p.sandbox}</dd></div>
        {#if p.request.claim}<div><dt>The model says</dt><dd class="claim">“{p.request.claim}” <span class="muted">— a claim, not evidence</span></dd></div>{/if}
        <div><dt>“For this task” covers</dt><dd><Code text={p.scope} /></dd></div>
      </dl>
      <input class="note" placeholder="Add a note for the model with your yes (optional)" bind:value={note} />
      {#if rejecting}
        <div class="reject">
          <!-- svelte-ignore a11y_autofocus -->
          <input autofocus placeholder="Why not? The model is told." bind:value={reason} onkeydown={(e) => e.key === "Enter" && reject()} />
          <button class="btn danger" disabled={!reason.trim()} onclick={reject}>Don't Allow</button>
          <button class="btn" onclick={() => (rejecting = false)}>Cancel</button>
        </div>
      {/if}
      {#if always && argv}
        <div class="always">
          <Icon name="lock" size={14} weight={2} />
          <span>Adds <code>{JSON.stringify(argv)}</code> to <code>[{policyKey.split(".")[0]}] {policyKey.split(".")[1]}</code> in speccoder.toml — it runs without asking once you commit it. Anything after this prefix runs too.</span>
          <button class="btn small" onclick={() => (always = false)}>Cancel</button>
          <button class="btn small primary" onclick={allowAlways}>Allow and Add</button>
        </div>
      {/if}
      <div class="choices">
        <button class="btn plain" onclick={stopTask} title="The call keeps waiting; decide later with speccoder task approve">Stop Task <kbd>S</kbd></button>
        <span class="spacer"></span>
        <button class="btn danger" onclick={() => (rejecting = true)}>Don't Allow… <kbd>N</kbd></button>
        {#if argv}<button class="btn" onclick={() => (always = true)} title="Add this exact command to the project's policy">Always Allow… <kbd>A</kbd></button>{/if}
        <button class="btn" disabled={!offered.has("run")} onclick={() => approve("run")} title={offered.has("run") ? "This class of call, in every task of this run" : "Never for high-risk calls"}>Allow for Run <kbd>R</kbd></button>
        <button class="btn" disabled={!offered.has("task")} onclick={() => approve("task")} title={offered.has("task") ? "This class of call, for the rest of the task" : "The call's class is not shown in full"}>Allow for Task <kbd>T</kbd></button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn primary" autofocus onclick={() => approve("once")}>Allow Once <kbd>Y</kbd></button>
      </div>
    {:else}
      <header>
        <span class="badge q" aria-hidden="true"><Icon name="info" size={22} weight={2} /></span>
        <div class="heads">
          <span class="eyebrow">A question from the planning skill</span>
          <h2 id="ask-title">{w.question}</h2>
        </div>
      </header>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea autofocus rows="4" bind:value={text} placeholder="Your answer"></textarea>
      <div class="choices">
        <span class="spacer"></span>
        <button class="btn" onclick={() => app.answer({ text: "" })}>Skip</button>
        <button class="btn primary" onclick={() => app.answer({ text })}>Answer</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 20; backdrop-filter: blur(3px); animation: fade 200ms var(--ease); padding: 1rem; }
  @keyframes fade { from { opacity: 0; } }
  .sheet {
    width: min(820px, 100%); max-height: 92vh; overflow: auto; border-radius: var(--radius-xl); background: var(--glass-strong);
    box-shadow: var(--shadow-lg), var(--glass-edge); padding: 1.4rem 1.5rem 1.25rem; display: grid; gap: 1rem;
    animation: sheet 420ms var(--spring);
  }
  @keyframes sheet { from { transform: scale(0.94) translateY(10px); opacity: 0; } }
  header { display: flex; align-items: flex-start; gap: 0.9rem; }
  .badge { width: 44px; height: 44px; border-radius: 12px; flex: none; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #ffb340, var(--attention-fill)); box-shadow: inset 0 -1px 0 rgba(0, 0, 0, 0.12), 0 2px 8px color-mix(in srgb, var(--attention-fill) 35%, transparent); }
  .badge.q { background: linear-gradient(180deg, #4aa3ff, var(--accent)); box-shadow: inset 0 -1px 0 rgba(0, 0, 0, 0.12); }
  .heads { flex: 1; min-width: 0; display: grid; gap: 0.15rem; }
  .eyebrow { color: var(--muted); font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  h2 { margin: 0; font: 650 1.3rem/1.25 var(--display); letter-spacing: -0.018em; }
  .subject { font-weight: 500; color: var(--text-2); font-size: 0.85em; }
  .risk { flex: none; font-size: 0.74rem; font-weight: 650; padding: 0.15rem 0.6rem; border-radius: 999px; text-transform: capitalize; }
  .r-low { background: var(--verified-bg); color: var(--verified); }
  .r-medium { background: var(--regression-bg); color: var(--regression); }
  .r-high { background: var(--danger-bg); color: var(--danger); }
  .body { margin: 0; max-height: 38vh; overflow: auto; background: var(--surface); border-radius: var(--radius); padding: 0.6rem 0; font-size: 0.8rem; line-height: 1.55; box-shadow: var(--shadow); }
  .body span { display: block; padding: 0 0.9rem; white-space: pre; }
  .add { background: var(--add-bg); color: var(--add); }
  .del { background: var(--del-bg); color: var(--del); }
  .more { justify-self: start; margin-top: -0.5rem; }
  dl { margin: 0; }
  dl > div { display: grid; grid-template-columns: 11rem 1fr; gap: 1rem; padding: 0.55rem 0.85rem; font-size: 0.88rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .claim { font-style: italic; color: var(--text-2); }
  .note, .reject input, textarea { width: 100%; border: 0; border-radius: 10px; padding: 0.6rem 0.8rem; background: var(--fill); outline: none; }
  .note:focus, .reject input:focus, textarea:focus { box-shadow: 0 0 0 3.5px var(--focus); }
  .reject { display: flex; gap: 0.5rem; align-items: center; }
  .choices { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; }
  .spacer { flex: 1; }
  .always { display: flex; align-items: center; gap: 0.6rem; padding: 0.65rem 0.8rem; border-radius: var(--radius); background: var(--accent-soft); font-size: 0.86rem; }
  .always span { flex: 1; }
  .btn.plain { background: none; color: var(--muted); }
  .btn.plain:hover { background: var(--fill); color: var(--text); }
  .choices .btn { height: 32px; }
  @media (max-width: 720px) { dl > div { grid-template-columns: 1fr; gap: 0.1rem; } .spacer { display: none; } }
</style>
