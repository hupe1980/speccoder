<script lang="ts">
  // From one line of intent to a project speccoder can judge: what you are
  // building, where, the stack and the spec format — and below, every
  // command that will run, in order, local or network, from the host's one
  // plan (`speccoder new --dry-run`'s). Create performs exactly that plan,
  // bound to its digest; the ecosystems' own tools write every file.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { offer } from "../lib/actions";
  import { checklist, debounce, networkSteps, shell, slug, stepMark } from "../lib/start";
  import { trap } from "../lib/sheet";
  import type { NewChoices, NewProjectReport, ProjectPlan, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";

  const opened = app.newProject ?? {};
  let idea = $state(opened.idea ?? "");
  let parent = $state(app.welcome?.parent_shown ?? "~/Workspaces");
  let where = $state(opened.path ?? "");
  let whereEdited = $state(!!opened.path);
  let stack = $state<string | null>(null);
  let format = $state<string>("speckit");
  let bugExtension = $state(false);
  let more = $state<string[]>([]);
  let preset = $state(true);
  let plan = $state<ProjectPlan | null>(null);
  let choices = $state<NewChoices | null>(null);
  let loading = $state(false);
  let confirming = $state(false);
  let running = $state(false);
  let result = $state<Reply<NewProjectReport> | null>(null);
  let folders = $state<string[]>([]);
  /** Undo asked about, inline (not a second modal). */
  let undoAsk = $state<string | null>(null);

  // Where: the parent and the idea's slug, until the person types a path.
  $effect(() => {
    if (!whereEdited) where = idea.trim() ? `${parent.replace(/\/$/, "")}/${slug(idea)}` : "";
  });

  const fetchPlan = debounce(async (args: { where: string; idea: string; stack: string | null; format: string; ext: string; preset: boolean }) => {
    if (!args.where.trim()) {
      plan = null;
      return;
    }
    loading = true;
    try {
      const r = await get<{ plan?: ProjectPlan; choices?: NewChoices; error?: string }>(
        `/api/new/plan${q({ path: args.where, idea: args.idea, stack: args.stack, format: args.format, extensions: args.ext, no_preset: args.preset ? null : "true" })}`,
      );
      plan = r.plan ?? null;
      choices = r.choices ?? null;
      if (!stack && r.plan) stack = r.plan.stack.id;
    } catch (e) {
      plan = null;
      app.toast({ tone: "error", text: "The plan could not be made", detail: String((e as Error).message ?? e) });
    } finally {
      loading = false;
    }
  }, 300);
  $effect(() => {
    const ext = [bugExtension ? "bug" : "", ...more].filter((x) => x.trim()).join("\n");
    confirming = false;
    void fetchPlan({ where, idea, stack, format, ext, preset });
    return () => fetchPlan.cancel();
  });

  // Folder names to choose from, under the parent.
  $effect(() => {
    const dir = where.includes("/") ? where.slice(0, where.lastIndexOf("/") + 1) : parent;
    void get<{ folders: string[] }>(`/api/folders${q({ under: dir })}`)
      .then((f) => (folders = f.folders ?? []))
      .catch(() => (folders = []));
  });

  const net = $derived(plan ? networkSteps(plan) : { steps: [], hosts: [] });
  const refusal = $derived(plan?.refusal ?? null);
  const ready = $derived(!!plan && !refusal && !loading && !app.job?.running && !running);
  const rows = $derived(plan ? checklist(plan, result?.data?.results?.length ? result.data.results : app.steps, app.stepRunning) : []);
  const failed = $derived(rows.find((r) => r.state === "failed") ?? null);
  const done = $derived(result?.status === "ok" ? result.data ?? null : null);
  // A host serving another project opens the new one in its own host.
  const elsewhere = $derived(!!app.project && !app.welcome && app.project.root !== plan?.target);

  function close() {
    if (running) return;
    app.newProject = null;
  }

  async function create() {
    if (!ready || !plan || !choices) return;
    if (net.steps.length && !confirming) {
      confirming = true;
      return;
    }
    confirming = false;
    running = true;
    result = null;
    const r = await app.run(
      offer(
        { kind: "new_project", target: plan.target, choices, plan_digest: plan.digest },
        { group: "work", label: `create ${plan.name}`, why: "the plan on screen, as shown", command: `speccoder new ${shell([plan.target])} --yes` },
      ),
    );
    running = false;
    result = r as Reply<NewProjectReport> | null;
    // A changed plan: the new one is shown, nothing ran.
    if (r?.code === "NEW-PLAN-CHANGED" && r.data) plan = (r.data as NewProjectReport).plan;
  }

  async function undo() {
    const d = result?.data;
    if (!d?.commit || !plan) return;
    if (!undoAsk) {
      undoAsk = d.undo?.kind === "trash" ? `Move ${plan.name} to the Trash? It holds only what speccoder created.` : `Revert ${d.commit} "new project: ${plan.name}"?`;
      return;
    }
    undoAsk = null;
    const r = await app.run(offer({ kind: "undo_new_project", commit: d.commit }, { group: "work", label: "undo", why: "", command: `speccoder new --undo ${shell([plan.target])} --yes` }), undefined, plan.target);
    if (r?.status === "ok") {
      app.newProject = null;
      app.toast({ tone: "ok", text: r.message });
    }
  }

  async function copy(text: string) {
    await navigator.clipboard?.writeText(text).catch(() => {});
    app.toast({ tone: "ok", text: "Copied — run it yourself", detail: text });
  }

  function firstFeature() {
    app.newProject = null;
    void app.refresh().then(() => (app.newFeature = { text: idea, through: "tasks" }));
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (confirming) confirming = false;
      else close();
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void create();
    }
  }
</script>

<div class="d-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={key}>
  <div class="d-sheet wide" role="dialog" aria-modal="true" aria-labelledby="np-title" use:trap={"textarea"}>
    <header class="d-head">
      <span class="d-badge green" aria-hidden="true"><Icon name="plus" size={20} weight={2.4} /></span>
      <div>
        <h2 id="np-title">New project</h2>
        <p>The stack's and the spec format's own tools make it; speccoder sets up its checks and commits once, as you.</p>
      </div>
    </header>

    {#if undoAsk}
      <div class="d-warn attention" role="group" aria-label="Undo">
        <span>{undoAsk}</span>
        <div class="d-row"><button class="btn" onclick={() => (undoAsk = null)}>Cancel</button><button class="btn danger" onclick={undo}>{result?.data?.undo?.kind === "trash" ? "Move to the Trash" : "Revert"}</button></div>
      </div>
    {/if}
    {#if done}
      <div class="done" role="status">
        <p class="big"><span aria-hidden="true">✓</span> {plan?.name} is ready — {done.init?.ran?.length ? done.init.ran.map((r) => `${r.name} ${r.status === "no_tests" ? "no tests yet" : r.passed ? "passes" : "fails"}`).join(" · ") : "no checks yet: add a test runner before the first run"}</p>
        <p class="muted">committed {done.commit}{plan?.identity ? ` as ${plan.identity.name}` : ""} · {done.init?.key?.set ? "model key found" : "no model key: set it before writing the first feature"}</p>
        <div class="d-row">
          {#if done.undo?.available}<button class="btn ghost" onclick={undo}><Icon name="undo" size={12} /> Undo</button>{/if}
          {#if elsewhere}
            <button class="btn" onclick={() => copy(`speccoder ui -C ${plan?.target}`)}>Copy <code>speccoder ui -C …</code></button>
            <button class="btn primary" onclick={() => copy(`speccoder ui -C ${plan?.target}`)}>Open {plan?.name}</button>
          {:else}
            <button class="btn primary" onclick={firstFeature}>Write the first feature</button>
          {/if}
        </div>
      </div>
    {:else}
      <label class="d-field">
        <span class="d-label">What are you building?</span>
        <textarea rows="2" bind:value={idea} placeholder="a CLI that turns Markdown tables into CSV" disabled={running}></textarea>
      </label>
      <dl class="d-grid">
        <dt><label for="np-where">Where</label></dt>
        <dd>
          <input id="np-where" type="text" list="np-folders" bind:value={where} oninput={() => (whereEdited = true)} disabled={running} spellcheck="false" />
          <datalist id="np-folders">{#each folders as f (f)}<option value={f}></option>{/each}</datalist>
        </dd>
        <dt id="np-stack">Stack</dt>
        <dd>
          <div class="d-radios" role="radiogroup" aria-labelledby="np-stack">
            {#each plan?.stacks ?? [] as s (s.id)}
              <label class:off={!!s.disabled} title={s.disabled ?? (s.version ? `${s.label} ${s.version}` : s.label)}>
                <input type="radio" name="np-stack" value={s.id} bind:group={stack} disabled={!!s.disabled || running || plan?.mode === "existing_repository"} /> {s.label}
              </label>
            {/each}
          </div>
          {#if plan}<div class="d-note">{plan.mode === "existing_repository" ? "found" : stack === plan.stack.id && plan.stack.reason !== "chosen" ? "suggested" : "chosen"}: {plan.stack.reason}</div>{/if}
          {#each (plan?.stacks ?? []).filter((s) => s.disabled) as s (s.id)}<div class="d-note">{s.disabled}</div>{/each}
        </dd>
        <dt id="np-format">Specs</dt>
        <dd>
          <div class="d-radios" role="radiogroup" aria-labelledby="np-format">
            {#each plan?.formats ?? [] as f (f.id)}
              <label class:off={!!f.disabled} title={f.disabled ?? ""}>
                <input type="radio" name="np-format" value={f.id} bind:group={format} disabled={!!f.disabled || running} /> {f.label}{f.version ? ` ${f.version}` : ""}
              </label>
            {/each}
          </div>
        </dd>
        {#if format === "speckit"}
          <dt>Spec Kit</dt>
          <dd>
            <div class="d-radios">
              <label><input type="checkbox" bind:checked={preset} disabled={running} /> the speccoder preset (tasks that plan for evidence)</label>
              <label><input type="checkbox" bind:checked={bugExtension} disabled={running} /> bug extension</label>
            </div>
            {#each more as _, i (i)}
              <input type="text" bind:value={more[i]} placeholder="a local path or a URL" aria-label="Extension {i + 1}" />
            {/each}
            <button class="btn small ghost" onclick={() => (more = [...more, ""])} disabled={running}><Icon name="plus" size={10} /> Add a local path or URL</button>
          </dd>
        {/if}
      </dl>

      {#if refusal}
        <div class="d-warn attention" role="alert">
          <strong>{refusal.code}</strong> {refusal.message}
          <span>{refusal.fix.replace(/^→ /, "")}</span>
        </div>
      {:else if plan}
        <div class="d-field">
          <span class="d-label" id="np-plan">This will run, in {where}:</span>
          <ol class="d-steps" aria-labelledby="np-plan">
            {#each rows as r, i (r.step.id)}
              {@const m = stepMark(r.state)}
              <li>
                <span class="n">{i + 1}</span>
                <span class={`st-${r.state}`} aria-hidden={!running && !result}>{running || result ? m.glyph : ""}</span>
                <span>
                  <code>{shell(r.step.argv)}</code>{#if r.step.id === "commit" && plan.identity} <span class="muted">as {plan.identity.name}</span>{/if}
                  {#if running || result}<span class="sr-only">{m.word}</span>{/if}
                  {#if r.step.note}<div class="d-note">{r.step.note}</div>{/if}
                  {#if r.result && r.result.duration_ms}<span class="muted"> · {(r.result.duration_ms / 1000).toFixed(1)}s</span>{/if}
                </span>
                <span class={r.step.access === "network" ? "badge-net" : "badge-local"}>{r.step.access === "network" ? `⇅ network: ${(r.step.hosts ?? []).join(", ")}` : "local"}</span>
                {#if r.state === "failed" && r.result}
                  <details open>
                    <summary>exit {r.result.exit ?? "—"}: the last lines</summary>
                    <pre>{r.result.output_tail.join("\n")}</pre>
                  </details>
                {/if}
              </li>
            {/each}
          </ol>
          <p class="d-note">{plan.sandbox.backend ? `Sandboxed (${plan.sandbox.backend}): writes only in this folder and speccoder's cache.` : "These commands run without an OS sandbox on this system."} Then: write the first feature → review its tests → run.</p>
        </div>
      {:else if loading}
        <p class="d-note" aria-busy="true">Making the plan…</p>
      {/if}

      <div class="sr-only" role="status" aria-live="polite">{rows.filter((r) => r.state === "done").map((r) => `${r.step.id} done`).join(", ")}</div>
      {#if failed}
        <div class="sr-only" role="alert">{failed.step.id} failed</div>
        <div class="d-row">
          <span class="d-hint d-error">{result?.message}</span>
          <button class="btn" onclick={() => copy(shell(failed.step.argv))}>Copy command</button>
          {#if result?.data?.undo?.available}<button class="btn ghost" onclick={undo}>Undo</button>{/if}
          <button class="btn primary" onclick={create}>Retry</button>
        </div>
      {/if}

      {#if confirming}
        <div class="d-warn attention" role="group" aria-label="These reach the network">
          <span>{net.steps.length} command{net.steps.length === 1 ? "" : "s"} reach the network: {net.steps.map((s) => shell(s.argv)).join(" · ")} ({net.hosts.join(", ")})</span>
          <div class="d-row"><button class="btn" onclick={() => (confirming = false)}>Cancel</button><button class="btn primary" onclick={create}>Run</button></div>
        </div>
      {/if}

      {#if !failed}
        <div class="d-row">
          <span class="d-hint">{plan ? `plan ${plan.digest.replace("sha256:", "").slice(0, 12)}` : ""}</span>
          {#if plan}<button class="btn ghost" onclick={() => copy(`speccoder new ${shell([plan!.target])}${idea.trim() ? ` --idea ${shell([idea.trim()])}` : ""} --stack ${stack ?? plan!.stack.id} --format ${format} --yes`)}>Show commands <Icon name="copy" size={11} /></button>{/if}
          <button class="btn" onclick={close} disabled={running}>Cancel</button>
          <button class="btn primary" disabled={!ready} onclick={create}>{running ? "Creating…" : "Create"} <kbd>⌘⏎</kbd></button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .done { display: grid; gap: 0.4rem; }
  .big { margin: 0; font-weight: 650; font-size: 1.02rem; }
  .big span { color: var(--verified); }
  input[type="text"] { width: 100%; box-sizing: border-box; }
</style>
