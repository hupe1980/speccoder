<script lang="ts">
  // The feature rung's own sheet: the description, where it came from, how
  // far to plan it — and what will happen: the format's own skills, in its
  // order, stopping for questions and the person's review; the folder they
  // write; what it cost before; the exact command. Start performs plan
  // through; the spec is written by the format's skill, never by the page.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { chainSkills, debounce, featureCommands, plural, short, throughChoices } from "../lib/start";
  import { trap } from "../lib/sheet";
  import type { Reply, SimilarMatch, SimilarReport, Through } from "../lib/types";
  import Icon from "./Icon.svelte";

  const opened = app.newFeature ?? {};
  let text = $state(opened.text ?? "");
  // The hand-off's source, while its text is unchanged.
  const handed = opened.source ?? null;
  let source = $state<"description" | "issue">(handed ? "issue" : "description");
  let through = $state<Through>(opened.through ?? "tasks");
  let commands = $state(false);
  let invalid = $state(false);
  let starting = $state(false);
  let matches = $state<SimilarMatch[]>([]);
  let limited = $state(false);
  let cost = $state<{ cost: { tokens: number; calls: number | null; usd: number | null; runs: number } | null; model: string | null } | null>(null);

  const planning = $derived(app.project?.planning);
  const create = $derived(planning?.creates ?? null);
  const unit = $derived(app.project?.unit ?? "feature");
  const format = $derived(app.project?.format ?? "the spec tool");
  const choices = $derived(throughChoices(!!planning?.checks));
  const skills = $derived(chainSkills(planning, through));
  const issue = $derived(source === "issue" && handed ? handed.number : null);
  const keyCheck = $derived(app.doctor.find((c) => c.check === "model key"));
  const noKey = $derived(keyCheck?.level === "fail" ? (keyCheck.detail.split(" ")[0] ?? "a model key") : null);
  const running = $derived(app.job?.running ? app.job.command : null);
  const writes = $derived.by(() => {
    const features = app.project?.features?.features ?? [];
    if (unit === "change") return "openspec/changes/<id>/";
    const next = Math.max(0, ...features.map((f) => f.number)) + 1;
    return `specs/${String(next).padStart(3, "0")}-…/`;
  });
  const blocked = $derived(
    !create
      ? `${format} has no skill that creates a ${unit}`
      : noKey
        ? `No model key: set ${noKey} where you start speccoder`
        : running
          ? `A job is running — ${running}`
          : null,
  );

  // The similarity hint: computed by the host, before any model call.
  const lookUp = debounce(async (t: string) => {
    if (t.trim().length < 8) return void (matches = []);
    try {
      const r = await get<Reply<SimilarReport>>(`/api/similar${q({ text: t })}`);
      matches = r.data?.matches ?? [];
      limited = !!r.data?.limited;
    } catch {
      matches = [];
    }
  }, 300);
  $effect(() => {
    void lookUp(text);
    return () => lookUp.cancel();
  });

  // What these skills cost before here; nothing on record: nothing made up.
  $effect(() => {
    const names = [create, ...(through === "spec" ? [] : planning?.toward ?? []), ...(through === "analyze" && planning?.checks ? [planning.checks] : [])].filter(Boolean) as string[];
    if (!names.length) return;
    void get<typeof cost>(`/api/skill-cost${q({ skills: names.join(",") })}`)
      .then((c) => (cost = c))
      .catch(() => (cost = null));
  });
  const costLine = $derived.by(() => {
    const c = cost?.cost;
    if (!c) return `cost: unknown until it runs${cost?.model ? ` — ${cost.model}` : ""}`;
    const tokens = c.tokens >= 1000 ? `${Math.round(c.tokens / 1000)}k tokens` : `${c.tokens} tokens`;
    const calls = c.calls != null ? `about ${plural(c.calls, "model call")} · ` : "about ";
    return `${calls}${tokens}${c.usd != null ? ` · ~$${c.usd.toFixed(2)}` : ""} (from your last ${plural(c.runs, "run")} of these skills)`;
  });

  function edited() {
    invalid = false;
    // The text is no longer the issue's.
    if (handed && source === "issue" && text !== opened.text) source = "description";
  }

  function close() {
    app.newFeature = null;
  }

  async function start() {
    if (blocked || starting) return;
    const description = source === "issue" && opened.text ? opened.text : text;
    if (!description.trim()) {
      invalid = true;
      return;
    }
    starting = true;
    const cmd = featureCommands(create, description, through, issue)[issue ? 1 : 0] ?? "";
    app.following = true;
    app.view = "documents";
    app.doc = "spec";
    close();
    await app.perform(
      offer(
        { kind: "plan_through", from: { kind: "create", description, source: issue ? { kind: "issue", number: issue } : { kind: "description" } }, through },
        { group: "plan", label: `new ${unit}`, why: `${format}'s own skills write and plan it`, command: cmd },
      ),
    );
  }

  /** Extend that unit instead: make it current, draft the description as
   *  a comment on its requirement — not sent; the person sends it. */
  async function extend(m: SimilarMatch) {
    const t = text.trim();
    if (!planning?.revises || !m.at) {
      await navigator.clipboard?.writeText(t).catch(() => {});
      close();
      await app.selectFeature(m.unit);
      app.view = "documents";
      app.doc = "review";
      app.toast({ tone: "ok", text: `${m.unit} is open; your description is on the clipboard` });
      return;
    }
    close();
    const switched = await app.run(offer({ kind: "feature" }, { group: "feature", label: `switch to ${m.unit}`, why: "", command: `speccoder feature ${m.unit}` }), m.unit);
    if (switched?.status !== "ok") return;
    await app.selectFeature(m.unit);
    const added = await app.run(
      offer(
        { kind: "comment", op: "add", anchor: { kind: "passage", path: m.at.path, start_line: m.at.line, end_line: m.at.line, quote: m.at.quote, quote_sha256: "", section: "" }, id: null },
        { group: "review", label: `comment on ${m.requirement}`, why: "", command: "speccoder review comments" },
      ),
      t,
    );
    app.view = "documents";
    app.doc = "review";
    if (added?.status === "ok") app.toast({ tone: "ok", text: `A comment on ${m.unit} ${m.requirement} waits in its tray`, detail: "Send it to revise the spec with the format's own skill" });
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void start();
    }
  }
</script>

<div class="d-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={key}>
  <div class="d-sheet wide" role="dialog" aria-modal="true" aria-labelledby="nf-title" aria-describedby="nf-lead" use:trap={"textarea"}>
    <header class="d-head">
      <span class="d-badge" aria-hidden="true"><Icon name="sparkle" size={20} weight={2.2} /></span>
      <div>
        <h2 id="nf-title">New {unit}</h2>
        <p id="nf-lead">{format}'s own skill writes the spec; you review it before anything is planned.</p>
      </div>
    </header>

    <label class="d-field">
      <span class="d-label">What should it do?</span>
      <textarea rows="3" bind:value={text} oninput={edited} placeholder="Users can export a report as CSV, with the columns they chose" aria-invalid={invalid} aria-describedby={invalid ? "nf-need" : undefined} readonly={!create}></textarea>
      {#if invalid}<span id="nf-need" class="d-error">Describe what it should do</span>{/if}
    </label>

    {#if handed}
      <div class="d-field" role="radiogroup" aria-label="Start from">
        <span class="d-label">Start from</span>
        <div class="d-radios">
          <label><input type="radio" name="nf-source" value="issue" bind:group={source} onchange={() => (text = opened.text ?? text)} /> issue #{handed.number} “{handed.title}”</label>
          <label><input type="radio" name="nf-source" value="description" bind:group={source} /> this description</label>
        </div>
      </div>
    {/if}

    {#if matches.length}
      <div class="similar" role="status" aria-live="polite">
        {#each matches as m (m.unit + m.requirement)}
          <div class="d-warn" class:attention={m.level === "very_close"}>
            <span>⚠ {m.level === "very_close" ? "Very close" : "Close"} to <strong>{m.unit} {m.requirement}</strong> “{m.text}” <span class="muted">— shares: {m.shared.join(", ")}</span></span>
            <span><button class="btn small tinted" onclick={() => extend(m)}>{planning?.revises ? `Extend ${m.unit.split("-")[0]} instead` : `Open ${m.unit.split("-")[0]}`}</button></span>
          </div>
        {/each}
        {#if limited}<span class="d-note">compared with the 2 000 most recently changed {unit}s</span>{/if}
      </div>
    {:else}
      <div class="sr-only" role="status" aria-live="polite"></div>
    {/if}

    <dl class="d-grid">
      <dt id="nf-plan">Plan it</dt>
      <dd>
        <div class="d-radios" role="radiogroup" aria-labelledby="nf-plan" title={planning?.checks ? undefined : `${format} has no analyze step`}>
          {#each choices as c (c.through)}
            <label><input type="radio" name="nf-through" value={c.through} bind:group={through} /> {c.label}</label>
          {/each}
        </div>
        <div class="d-note">stops for: questions (in place){through === "spec" ? "" : " · your review of the spec"}</div>
      </dd>
      <dt>Runs</dt>
      <dd>{#if create}{format} {skills.join(" → ")}{:else}<span class="muted">nothing: {format} has no creating skill</span>{/if}</dd>
      <dt>Writes</dt>
      <dd><code>{writes}</code> · planning files are left for you to commit</dd>
      <dt>Cost</dt>
      <dd class="muted">{costLine}</dd>
    </dl>

    <details bind:open={commands}>
      <summary>Show commands</summary>
      <div class="d-mono">{featureCommands(create, text, through, issue).join("\n") || "speccoder spec"}</div>
    </details>

    {#if blocked}
      <p class="d-warn attention" role="note">
        {blocked}{#if noKey} — <button class="btn small ghost" onclick={() => { close(); app.view = "settings"; }}>Settings › Model</button>{/if}
      </p>
    {/if}

    <div class="d-row">
      <span class="d-hint">⌘⏎ starts</span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={!!blocked || starting} onclick={start}>
        {#if starting}Starting…{:else}Start <kbd>⌘⏎</kbd>{/if}
      </button>
    </div>
  </div>
</div>

<style>
  .similar { display: grid; gap: 0.35rem; }
  .similar .d-warn { grid-template-columns: 1fr auto; align-items: center; }
  details summary { cursor: pointer; font-size: 0.86rem; color: var(--accent-ink); font-weight: 600; }
  details .d-mono { margin-top: 0.4rem; }
  code { font: 0.82rem var(--mono); }
</style>
