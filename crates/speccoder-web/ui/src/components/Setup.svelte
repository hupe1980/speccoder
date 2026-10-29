<script lang="ts">
  // The setup assistant, after the system's: one step at a time — what was
  // found, the model, the checks, then writing and proving the
  // configuration (its checks run on the untouched code, then it is
  // committed), and the first feature. Every step names its CLI command.
  import { app } from "../lib/app.svelte";
  import { byRole } from "../lib/actions";
  import { get, post } from "../lib/api";
  import type { Reply, Step } from "../lib/types";
  import FixSteps from "./FixSteps.svelte";
  import Agents from "./Agents.svelte";
  import Icon from "./Icon.svelte";

  interface Check { check: string; level: string; detail: string; fix: string[]; fix_steps?: Step[] }
  interface Proposed { name: string; argv: string[]; tier: string; junit: string | null; why: string }
  interface SetupState {
    doctor: Reply<{ checks: Check[] }>;
    preview: { configured: boolean; checks: Proposed[]; allow: string[][]; formatter: string[] | null; keys: [string, boolean][]; bedrock: { source: string | null; region: string | null } };
  }
  interface Ran { name: string; passed: boolean; status: string; duration_ms: number }

  let setup = $state<SetupState | null>(null);
  let step = $state(0);
  let provider = $state<"anthropic" | "openai" | "bedrock">("anthropic");
  let model = $state("");
  let region = $state("");
  let busy = $state(false);
  let result = $state<(Reply<{ ran: Ran[]; wrote: string[]; committed: string | null }> & { to_commit?: string[]; failed?: string[] }) | null>(null);
  let committed = $state<string | null>(null);
  let idea = $state("");
  let error = $state<string | null>(null);
  /** The agents are asked about only once the person opens them. */
  let agentsOpen = $state(false);

  // Once shown, the assistant stays until it is finished or left: committing
  // the configuration must not take it away before the first feature.
  if (app.view !== "setup") app.view = "setup";

  $effect(() => {
    void get<SetupState>("/api/setup").then((s) => {
      setup = s;
      const set = new Map(s.preview.keys);
      if (!set.get("ANTHROPIC_API_KEY") && set.get("OPENAI_API_KEY")) provider = "openai";
      else if (!set.get("ANTHROPIC_API_KEY") && s.preview.bedrock.source) provider = "bedrock";
      region = s.preview.bedrock.region ?? "";
    });
  });

  const checks = $derived(setup?.doctor.data?.checks ?? []);
  const row = (name: string) => checks.find((c) => c.check === name);
  const project = $derived(row("project"));
  const keySet = $derived(new Map(setup?.preview.keys ?? []));
  const keyVar = $derived(provider === "openai" ? "OPENAI_API_KEY" : "ANTHROPIC_API_KEY");
  const providers: [string, string, string, string][] = [["anthropic", "Anthropic", "Claude models", "ANTHROPIC_API_KEY"], ["openai", "OpenAI", "GPT models", "OPENAI_API_KEY"], ["bedrock", "Amazon Bedrock", "Claude and other models through AWS", ""]];
  // Bedrock's credentials come from AWS: a key or a session, named, never shown.
  const awsSource = $derived(setup?.preview.bedrock.source ?? null);
  const providerName = $derived(provider === "openai" ? "OpenAI" : provider === "bedrock" ? "Amazon Bedrock" : "Anthropic");
  const steps = ["Welcome", "Model", "Checks", "Set Up", "First Feature"];
  // The format's skill that creates a unit, by its role: whatever it is named.
  const specify = $derived(byRole(app.actions, "create").find((a) => a.input));
  const words = (a: string[]) => a.map((w) => (/\s/.test(w) ? JSON.stringify(w) : w)).join(" ");
  const cli = $derived(
    `speccoder init --provider ${provider}${provider === "bedrock" && region.trim() ? ` --region ${region.trim()}` : ""}${model.trim() ? ` --model ${model.trim()}` : ""}`,
  );

  async function setUp() {
    busy = true;
    error = null;
    try {
      result = await post("/api/setup", { provider, model: model.trim() || null, region: provider === "bedrock" ? region.trim() || null : null });
      if (result?.status !== "ok") error = result?.message ?? "init refused";
    } catch (e) {
      error = String((e as Error).message ?? e);
    } finally {
      busy = false;
    }
  }
  async function commit() {
    busy = true;
    try {
      const r = await post<{ status: string; message: string }>("/api/setup/commit", { paths: result?.to_commit ?? [] });
      if (r.status === "ok") {
        committed = r.message;
        step = 4;
        void app.refresh();
      } else error = r.message;
    } finally {
      busy = false;
    }
  }
  function copy(text: string) {
    navigator.clipboard?.writeText(text);
    app.toast({ tone: "ok", text: "Copied", detail: text });
  }
  function start() {
    // The New Feature sheet: what will run, how far to plan it, Start.
    if (specify && idea.trim()) app.newFeature = { text: idea.trim(), through: "tasks" };
  }
  const level = (c?: Check) => c?.level ?? "pass";
</script>

<section class="assistant">
  <div class="card glass">
    <ol class="dots" aria-label="Steps">
      {#each steps as s, i}<li class:on={i === step} class:done={i < step} aria-current={i === step ? "step" : undefined}><span class="sr-only">{s}</span></li>{/each}
    </ol>

    {#if !setup}
      <div class="center"><span class="spinner"></span></div>
    {:else if step === 0}
      <div class="hero-icon"><svg viewBox="0 0 64 64" width="72" height="72" aria-hidden="true"><defs><linearGradient id="sg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b9bff" /><stop offset="1" stop-color="#0060df" /></linearGradient></defs><rect width="64" height="64" rx="15" fill="url(#sg)" /><path d="M19 33.5 28.5 43 46 22" fill="none" stroke="#fff" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round" /></svg></div>
      <h1>Welcome to speccoder</h1>
      <p class="lead">Let's set up <strong>{app.project?.name ?? "this project"}</strong>: a model to work with, the checks that decide whether nothing regressed, and your first feature.</p>
      <ul class="grouped found">
        {#each ["git", "repository", "project", "spec tool", "sandbox"] as name}
          {@const c = row(name)}
          {#if c}
            <li class={level(c)}>
              <span class="ic"><Icon name={c.level === "pass" ? "check" : c.level === "fail" ? "xmark" : "exclaim"} size={12} weight={2.6} /></span>
              <span class="w"><strong>{name.charAt(0).toUpperCase() + name.slice(1)}</strong><span class="d">{c.detail}</span></span>
              {#if c.level !== "pass" && c.fix_steps?.length}<FixSteps steps={c.fix_steps} limit={1} />{/if}
            </li>
          {/if}
        {/each}
      </ul>
      {#if level(project) === "fail"}
        <div class="make">
          <button class="btn primary" onclick={() => (app.newProject = { path: app.project?.root ?? "", existing: true })}><Icon name="plus" size={13} /> Make it a project</button>
          <span class="muted small">The spec format's own init, then speccoder's setup and one commit — every command shown first.</span>
        </div>
        <p class="note"><Icon name="info" size={14} /> Or run the command above in your terminal, then come back — this page updates.</p>
      {/if}
    {:else if step === 1}
      <h1>Choose a model</h1>
      <p class="lead">It makes the attempts; speccoder judges them on evidence. The key comes from your environment, never from a file.</p>
      <div class="providers" role="radiogroup" aria-label="Provider">
        {#each providers as [id, name, sub, v] (id)}
          <button role="radio" aria-checked={provider === id} class:on={provider === id} onclick={() => (provider = id as typeof provider)}>
            <span class="pname">{name}</span><span class="psub">{sub}</span>
            {#if id === "bedrock"}
              <span class="key" class:ok={awsSource}>{#if awsSource}<Icon name="check" size={11} weight={2.6} /> {awsSource}{:else}<Icon name="key" size={11} /> AWS credentials not found{/if}</span>
            {:else}
              <span class="key" class:ok={keySet.get(v)}>{#if keySet.get(v)}<Icon name="check" size={11} weight={2.6} /> {v} is set{:else}<Icon name="key" size={11} /> {v} not set{/if}</span>
            {/if}
          </button>
        {/each}
      </div>
      <label class="field-row"><span>Model</span><input class="field" bind:value={model} placeholder="speccoder's default for {providerName}" /></label>
      {#if provider === "bedrock"}
        <label class="field-row"><span>Region</span><input class="field" bind:value={region} placeholder="e.g. eu-central-1 (required)" /></label>
        {#if !awsSource}
          <p class="note"><Icon name="key" size={14} /> Set AWS_BEARER_TOKEN_BEDROCK or run aws login where you start speccoder. You can finish setting up first.</p>
        {/if}
      {:else if !keySet.get(keyVar)}
        <p class="note"><Icon name="key" size={14} /> Set the key where you start speccoder: <button class="fix mono" onclick={() => copy(`export ${keyVar}=…`)}>export {keyVar}=…</button> You can finish setting up first.</p>
      {/if}
      <p class="muted small">Local models (Ollama, vLLM, LM Studio) are set in Settings › Model after setup.</p>
      <details class="agents-step" bind:open={agentsOpen}>
        <summary>Or let an agent you already use make the attempts — Copilot, OpenCode, Claude Code, Codex, Gemini</summary>
        <p class="muted small">speccoder decides each call it asks about and judges every attempt the same way; the model above still writes the acceptance tests. Choose one in Settings › Executor once this project is set up.</p>
        {#if agentsOpen}<Agents compact />{/if}
      </details>
    {:else if step === 2}
      <h1>Checks</h1>
      <p class="lead">They decide whether a change broke anything. speccoder found these for your project and will run them on the untouched code first.</p>
      {#if setup.preview.checks.length}
        <ul class="grouped proposed">
          {#each setup.preview.checks as c (c.name)}
            <li>
              <span class="badge"><Icon name="check" size={13} weight={2.4} /></span>
              <span class="w"><strong>{c.name}</strong> <span class="tier">{c.tier}</span><span class="mono argv">{words(c.argv)}</span><span class="d">{c.why}</span></span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="note"><Icon name="exclaim" size={14} /> No test runner found. Add one in Settings › Checks after setup, or speccoder can only judge by acceptance tests.</p>
      {/if}
      {#if setup.preview.allow.length}<p class="muted small">The model may also run without asking: {setup.preview.allow.map(words).join(" · ")}</p>{/if}
      {#if setup.preview.formatter}<p class="muted small">Formatter after each attempt: <span class="mono">{words(setup.preview.formatter)}</span></p>{/if}
    {:else if step === 3}
      <h1>Set up</h1>
      {#if setup.preview.configured && !result}
        <p class="lead">This project already has a <span class="mono">speccoder.toml</span>. Change it in Settings, or continue.</p>
      {:else if !result}
        <p class="lead">speccoder writes <span class="mono">speccoder.toml</span>, then runs its checks on the untouched code. Nothing is committed until you say so.</p>
        <button class="cmd mono" onclick={() => copy(cli)}><Icon name="terminal" size={12} /> {cli}</button>
      {:else}
        <ul class="grouped ran">
          {#each result.data?.ran ?? [] as r (r.name)}
            <li class={r.passed ? "pass" : "fail"}>
              <span class="ic"><Icon name={r.passed ? "check" : "xmark"} size={12} weight={2.6} /></span>
              <span class="w"><strong>{r.name}</strong><span class="d">{r.passed ? "passes" : "fails"} on the untouched code · {(r.duration_ms / 1000).toFixed(1)}s</span></span>
            </li>
          {/each}
        </ul>
        {#if result.failed?.length}<p class="note bad"><Icon name="exclaim" size={14} /> A check that fails before any change makes every task's baseline red. Fix the code or the check (Settings › Checks), then set up again — or commit and fix it later.</p>{/if}
        {#if committed}<p class="note ok"><Icon name="check" size={14} /> {committed}</p>{/if}
      {/if}
      {#if error}<p class="note bad"><Icon name="exclaim" size={14} /> {error}</p>{/if}
    {:else}
      <h1>Your first feature</h1>
      <p class="lead">Describe what it should do, in your words. Spec Kit's own skill writes the spec; you review it before anything is planned.</p>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea autofocus rows="5" bind:value={idea} placeholder="A function greet(name) that returns a greeting, rejecting empty names…"></textarea>
      {#if !specify}<p class="muted small">The planning skills are not installed here; the board shows how to write the spec.</p>{/if}
    {/if}

    <footer>
      {#if step > 0}<button class="btn" onclick={() => (step -= 1)} disabled={busy}>Back</button>{/if}
      <span class="spacer"></span>
      {#if step === 0}
        <button class="btn primary large" disabled={level(project) === "fail"} onclick={() => (step = setup?.preview.configured ? 3 : 1)}>Continue</button>
      {:else if step === 1 || step === 2}
        <button class="btn primary large" onclick={() => (step += 1)}>Continue</button>
      {:else if step === 3}
        {#if setup?.preview.configured && !result}
          <button class="btn primary large" onclick={() => (step = 4)}>Continue</button>
        {:else if !result || result.status !== "ok"}
          <button class="btn primary large" disabled={busy} onclick={setUp}>{#if busy}<span class="spin"></span> Running the checks…{:else}Set Up{/if}</button>
        {:else if !committed && result.to_commit?.length}
          <button class="btn primary large" disabled={busy} onclick={commit}>Commit {result.to_commit.join(" and ")}</button>
        {:else}
          <button class="btn primary large" onclick={() => (step = 4)}>Continue</button>
        {/if}
      {:else}
        <button class="btn" onclick={() => (app.view = "overview")}>Later</button>
        <button class="btn primary large" disabled={!idea.trim() || !specify} onclick={start}>Write the Spec</button>
      {/if}
    </footer>
  </div>
</section>

<style>
  .agents-step { margin-top: 0.8rem; }
  .agents-step summary { cursor: pointer; font-size: 0.85rem; color: var(--accent); }
  .assistant { display: grid; place-items: start center; padding-top: 3vh; }
  .card { width: min(720px, 100%); border-radius: 26px; background: var(--glass-strong); box-shadow: var(--shadow-lg), var(--glass-edge); padding: 2rem 2.4rem 1.5rem; display: grid; gap: 1rem; animation: in 420ms var(--spring); }
  @keyframes in { from { transform: translateY(10px) scale(0.98); opacity: 0; } }
  .dots { list-style: none; margin: 0 0 0.5rem; padding: 0; display: flex; gap: 6px; justify-content: center; }
  .dots li { width: 7px; height: 7px; border-radius: 50%; background: var(--fill-2); transition: width 300ms var(--spring), background 200ms; }
  .dots li.done { background: var(--accent); opacity: 0.5; }
  .dots li.on { width: 22px; border-radius: 4px; background: var(--accent); }
  .hero-icon { justify-self: center; filter: drop-shadow(0 6px 16px rgba(0, 96, 223, 0.35)); }
  h1 { margin: 0; text-align: center; font: 700 1.9rem/1.15 var(--display); letter-spacing: -0.026em; }
  .lead { margin: 0 auto 0.5rem; max-width: 52ch; text-align: center; color: var(--text-2); font-size: 1rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  .make { display: grid; gap: 0.35rem; justify-items: start; margin: 0.8rem 0 0; }
  .found li, .ran li, .proposed li { display: grid; grid-template-columns: auto 1fr auto; gap: 0.75rem; align-items: center; padding: 0.65rem 0.9rem; }
  .ic { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; color: #fff; background: var(--verified-fill); }
  .warn .ic { background: var(--attention-fill); }
  .fail .ic { background: var(--danger-fill); }
  .w { display: grid; min-width: 0; }
  .d { color: var(--muted); font-size: 0.84rem; overflow-wrap: anywhere; }
  .fix { border: 0; border-radius: 7px; background: var(--fill); padding: 0.25rem 0.55rem; font-size: 0.76rem; cursor: copy; max-width: 22rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { display: flex; align-items: center; gap: 0.45rem; flex-wrap: wrap; margin: 0; padding: 0.7rem 0.9rem; border-radius: var(--radius); background: var(--accent-soft); font-size: 0.88rem; }
  .note.bad { background: var(--danger-bg); color: var(--danger); }
  .note.ok { background: var(--verified-bg); color: var(--verified); }
  .providers { display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.75rem; }
  .providers button { display: grid; gap: 0.2rem; text-align: left; padding: 1rem 1.1rem; border-radius: var(--radius-lg); border: 0; background: var(--surface); box-shadow: var(--shadow); cursor: pointer; transition: box-shadow var(--speed) var(--ease), transform 220ms var(--spring); }
  .providers button:hover { transform: translateY(-1px); }
  .providers button.on { box-shadow: 0 0 0 2.5px var(--accent), var(--shadow-md); }
  .pname { font: 650 1.1rem var(--display); }
  .psub { color: var(--muted); font-size: 0.86rem; }
  .key { margin-top: 0.4rem; justify-self: start; display: inline-flex; align-items: center; gap: 0.3rem; font-size: 0.74rem; font-weight: 600; padding: 0.15rem 0.5rem; border-radius: 999px; background: var(--attention-bg); color: var(--attention); }
  .key.ok { background: var(--verified-bg); color: var(--verified); }
  .field-row { display: grid; grid-template-columns: 6rem 1fr; align-items: center; gap: 0.75rem; }
  .field-row span { color: var(--text-2); font-weight: 550; }
  .field, textarea { border: 0; border-radius: 10px; background: var(--fill); padding: 0.6rem 0.8rem; outline: none; font-size: 0.95rem; width: 100%; }
  .field:focus, textarea:focus { box-shadow: 0 0 0 3.5px var(--focus); }
  textarea { resize: vertical; font-size: 1rem; line-height: 1.5; }
  .badge { width: 28px; height: 28px; border-radius: 7px; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, #4cd964, var(--verified-fill)); }
  .tier { font-size: 0.7rem; font-weight: 600; color: var(--muted); background: var(--fill); padding: 0 0.4rem; border-radius: 999px; margin-left: 0.3rem; }
  .argv { font-size: 0.78rem; color: var(--text-2); }
  .cmd { justify-self: center; display: inline-flex; align-items: center; gap: 0.4rem; border: 0; border-radius: 8px; background: var(--fill); padding: 0.35rem 0.75rem; font-size: 0.8rem; cursor: copy; }
  .small { font-size: 0.82rem; margin: 0; }
  footer { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.75rem; }
  .spacer { flex: 1; }
  .center { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 22px; height: 22px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  .spin { width: 13px; height: 13px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 700ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 640px) { .card { padding: 1.5rem 1.1rem 1rem; } .providers { grid-template-columns: 1fr; } }
</style>
