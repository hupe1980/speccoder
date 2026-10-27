<script lang="ts">
  // speccoder.toml as System Settings shows preferences: sections on the
  // left, each key as the control its kind calls for, with its default and
  // what it does. Edits gather as a draft; Save writes the file (comments
  // kept, every parser asked first); the change is then offered as a commit,
  // because speccoder reads committed configuration only.
  import { app } from "../lib/app.svelte";
  import { get, post } from "../lib/api";
  import type { ConfigChange, ConfigKey, ConfigReport, ConfigSection, PolicyCoverage, Reply } from "../lib/types";
  import { offer } from "../lib/actions";
  import Icon from "./Icon.svelte";
  import Diff from "./Diff.svelte";

  let report = $state<ConfigReport | null>(null);
  let pick = $state("model");
  let draft = $state<Record<string, ConfigChange>>({});
  let saving = $state(false);
  let error = $state<string | null>(null);
  let newTable = $state("");

  async function load() {
    const r = await get<Reply<ConfigReport>>("/api/config");
    report = r.data ?? null;
  }
  $effect(() => {
    void load();
  });

  const icons: Record<string, [string, string]> = {
    model: ["cpu", "#5e5ce6"], checks: ["check", "#34c759"], budget: ["gauge", "#ff9500"], commands: ["terminal", "#8e8e93"],
    approvals: ["hand", "#ff9f0a"], tests: ["list", "#30b0c7"], spec: ["doc", "#007aff"], context: ["doc", "#64d2ff"],
    acceptance: ["seal", "#34c759"], converge: ["loop", "#5856d6"], monitor: ["shield", "#ff2d55"], git: ["branch", "#ff3b30"],
    format: ["sparkle", "#af52de"], sandbox: ["lock", "#636366"], attest: ["signature", "#bf5af2"], mutation: ["exclaim", "#ff9500"],
    executor: ["play", "#0a84ff"], mcp: ["grid", "#30b0c7"], worktree: ["copy", "#5ac8fa"],
  };
  const section = $derived(report?.sections.find((s) => s.name === pick) ?? null);
  const dirty = $derived(Object.keys(draft).length);
  // The model's keys for its provider: Bedrock's region (required), and
  // none of a compatible server's (its credentials and endpoint are AWS's).
  const providerKey = $derived(report?.sections.find((s) => s.name === "model")?.keys.find((k) => k.key === "provider"));
  const bedrock = $derived(!!providerKey && effective("model.provider", providerKey) === "bedrock");
  const SERVER_KEYS = ["base_url", "api_key_env", "structured"];
  const provider = $derived(providerKey ? String(effective("model.provider", providerKey) ?? "") : "");
  /** A model key the chosen provider does not take. */
  const inapplicable = (path: string, k: ConfigKey) =>
    path === `model.${k.key}` && (bedrock ? SERVER_KEYS.includes(k.key) : k.key === "region");
  // One still in the file stays, with its reset to remove it: Save refuses
  // the file while it is there.
  const shown = (s: ConfigSection) =>
    s.name !== "model" ? s.keys : s.keys.filter((k) => !inapplicable(`model.${k.key}`, k) || current(`model.${k.key}`, k) !== undefined);
  const required = (path: string, k: ConfigKey) => (k.required || (bedrock && path === "model.region")) && !inapplicable(path, k);
  // Where Bedrock's credentials come from: as `doctor` names it when its row
  // is about Bedrock (the saved file's provider), else as the setup preview
  // finds it.
  const keyRow = $derived(app.doctor.find((x) => x.check === "model key"));
  const rowIsBedrock = $derived(!!keyRow && (keyRow.detail.startsWith("bedrock ") || keyRow.detail.startsWith("AWS_BEARER_TOKEN_BEDROCK")));
  let previewSource = $state<string | null | undefined>(undefined);
  $effect(() => {
    if (!bedrock || rowIsBedrock || previewSource !== undefined) return;
    previewSource = null;
    void get<{ preview?: { bedrock?: { source: string | null } } }>("/api/setup")
      .then((r) => (previewSource = r.preview?.bedrock?.source ?? null))
      .catch(() => {});
  });
  const source = $derived.by(() => {
    if (!bedrock) return null;
    if (keyRow && rowIsBedrock) return keyRow.level === "fail" ? keyRow.detail : (keyRow.detail.split(" · ").pop() ?? keyRow.detail);
    return previewSource ?? null;
  });

  const names: Record<string, string> = {
    price_input_per_mtok: "Input price", price_output_per_mtok: "Output price", price_cache_read_per_mtok: "Cache read price",
    price_cache_write_per_mtok: "Cache write price (5 min)", price_cache_write_1h_per_mtok: "Cache write price (1 h)",
    max_output_tokens: "Output tokens per turn", max_run_tokens: "Tokens per attempt", cache_ttl: "Cache lifetime",
    base_url: "Server URL", api_key_env: "API key variable", max_tool_calls: "Tool calls per attempt", max_turns: "Turns per attempt",
    max_attempts: "Attempts per task", best_of: "Samples per task", held_out: "Held-out tests", min_strength: "Minimum strength",
    near_miss: "Near-miss variants", strength_timeout_secs: "Strength time budget (s)", timeout_secs: "Timeout (s)",
    max_rounds: "Rounds", network_allow: "Commands with network", when_exists: "Only when this exists", junit: "JUnit report",
    cwd: "Working directory", argv: "Command", acceptance_argv: "Acceptance command", sign_at: "Sign at", allow: "Allowed",
  };
  const label = (k: string) => {
    const last = k.split(".").pop()!;
    if (names[last]) return names[last];
    const s = last.replaceAll("_", " ");
    return s.charAt(0).toUpperCase() + s.slice(1);
  };
  /** The value the page shows for a key: the draft's, else the file's. */
  function current(path: string, k: ConfigKey): unknown {
    const d = draft[path];
    if (d) return d.value === undefined ? undefined : d.value;
    return k.value ?? undefined;
  }
  function effective(path: string, k: ConfigKey) {
    const v = current(path, k);
    return v === undefined ? k.default : v;
  }
  function set(path: string, k: ConfigKey, value: unknown) {
    const same = JSON.stringify(value) === JSON.stringify(k.value ?? undefined);
    const next = { ...draft };
    if (same) delete next[path];
    else next[path] = { key: path, value };
    draft = next;
  }
  function reset(path: string, k: ConfigKey) {
    const next = { ...draft };
    if (k.value === undefined || k.value === null) delete next[path];
    else next[path] = { key: path };
    draft = next;
  }
  const words = (v: unknown) => (Array.isArray(v) ? v.map((w) => (/\s/.test(String(w)) ? JSON.stringify(w) : String(w))).join(" ") : "");
  // Words as a shell would split them, quotes keeping spaces together.
  const split = (s: string) => [...s.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map((m) => m[1] ?? m[2] ?? m[3] ?? "");

  // A prefix list is never saved from the draft: each entry it gains is
  // shown with every command from the journal it would newly allow, and
  // saved only as the confirmation of that preview (its digest).
  const PREFIX_KEYS = ["commands.allow_prefix", "sandbox.network_allow_prefix"];
  let previews = $state<Record<string, PolicyCoverage | string>>({});
  let adding = $state<Record<string, string>>({});
  async function coverage(path: string, prefix: string[]) {
    const id = `${path}:${JSON.stringify(prefix)}`;
    const r = await get<Reply<PolicyCoverage>>(`/api/policy/coverage?key=${encodeURIComponent(path)}&prefix=${encodeURIComponent(JSON.stringify(prefix))}`);
    previews = { ...previews, [id]: r.data ?? r.message };
  }
  // What the checks allow (configuration, never a grant): shown so a
  // committed allowance is never invisible.
  const fromChecks = $derived.by(() => {
    const checks = report?.sections.find((x) => x.name === "checks");
    return (checks?.tables ?? []).flatMap((t) => {
      const argv = t.keys.find((k) => k.key === "argv")?.value;
      if (!Array.isArray(argv) || !argv.length) return [];
      const a = argv.map(String);
      const head = a.slice(0, a.findIndex((w) => w.startsWith("-")) === -1 ? a.length : a.findIndex((w) => w.startsWith("-")));
      const exact = head.length <= 1;
      return [{ name: t.name, prefix: exact ? a : head, exact }];
    });
  });
  async function widen(path: string, c: PolicyCoverage) {
    const key = path === "commands.allow_prefix" ? "commands_prefix" : "network_prefix";
    await app.perform(
      offer({ kind: "widen_policy", key, prefix: c.entry.prefix, digest: c.digest }, {
        group: "settings",
        label: `allow prefix ${c.entry.prefix.join(" ")}`,
        why: "saved from its coverage preview",
        command: `speccoder config set ${path} '${JSON.stringify(c.entry.prefix)}' --yes`,
      }),
    );
    adding = { ...adding, [path]: "" };
    await load();
  }

  async function save() {
    saving = true;
    error = null;
    try {
      const r = await post<Reply<ConfigReport>>("/api/config", { changes: Object.values(draft) });
      if (r.status !== "ok") error = r.message;
      else {
        report = r.data ?? report;
        draft = {};
        app.toast({ tone: "ok", text: "Saved speccoder.toml", detail: "Commit it to put it in force" });
      }
    } catch (e) {
      error = String((e as Error).message ?? e);
    } finally {
      saving = false;
    }
  }
  async function commit() {
    saving = true;
    try {
      const r = await post<Reply<ConfigReport>>("/api/config/commit", {});
      if (r.status !== "ok") error = r.message;
      else {
        app.toast({ tone: "ok", text: r.message });
        await load();
        void app.refresh();
      }
    } finally {
      saving = false;
    }
  }
  function addTable(s: ConfigSection) {
    const name = newTable.trim().toLowerCase();
    if (!/^[a-z0-9_-]+$/.test(name)) return;
    // An existing name is that table, not a new one to overwrite.
    if (s.tables.some((t) => t.name === name) || Object.keys(draft).some((k) => k.startsWith(`${s.name}.${name}.`))) {
      error = `[${s.name}.${name}] exists already — edit it above`;
      return;
    }
    // Its command starts empty: Save refuses it until you write one (never a
    // placeholder that would pass).
    const req = s.keys.find((k) => k.required);
    if (req) draft = { ...draft, [`${s.name}.${name}.${req.key}`]: { key: `${s.name}.${name}.${req.key}`, value: [] } };
    error = null;
    newTable = "";
  }
  function removeTable(s: ConfigSection, table: string) {
    const t = s.tables.find((x) => x.name === table);
    const next = { ...draft };
    for (const k of t?.keys ?? []) if (k.value !== null && k.value !== undefined) next[`${s.name}.${table}.${k.key}`] = { key: `${s.name}.${table}.${k.key}` };
    draft = next;
  }
  const cli = $derived(
    Object.values(draft)
      .map((c) => (c.value === undefined ? `speccoder config unset ${c.key}` : `speccoder config set ${c.key} '${JSON.stringify(c.value)}'`))
      .join(" && "),
  );
</script>

{#snippet control(path: string, k: ConfigKey)}
  {@const v = effective(path, k)}
  {#if k.kind === "bool"}
    <label class="switch">
      <input type="checkbox" checked={v === true} onchange={(e) => set(path, k, (e.target as HTMLInputElement).checked)} aria-label={label(k.key)} />
      <span class="track"><span class="knob"></span></span>
    </label>
  {:else if k.kind === "choice" && k.choices.length <= 3}
    <div class="seg" role="radiogroup" aria-label={label(k.key)}>
      {#each k.choices as c}
        <button role="radio" aria-checked={v === c} class:on={v === c} onclick={() => set(path, k, c)}>{c}</button>
      {/each}
    </div>
  {:else if k.kind === "choice"}
    <select value={v ?? ""} onchange={(e) => set(path, k, (e.target as HTMLSelectElement).value || undefined)} aria-label={label(k.key)}>
      {#if !k.required && k.default === null}<option value="">—</option>{/if}
      {#each k.choices as c}<option value={c}>{c}</option>{/each}
    </select>
  {:else if k.kind === "integer" || k.kind === "number"}
    <div class="stepper">
      <button aria-label="Less" onclick={() => set(path, k, Math.max(k.min ?? -Infinity, Number(v ?? k.min ?? 0) - (k.kind === "integer" ? 1 : 0.05)))}><Icon name="minus" size={12} weight={2.2} /></button>
      <input type="number" value={v ?? ""} min={k.min} max={k.max} step={k.kind === "integer" ? 1 : "any"} placeholder="—"
        onchange={(e) => { const t = (e.target as HTMLInputElement).value; set(path, k, t === "" ? undefined : Number(t)); }} aria-label={label(k.key)} />
      <button aria-label="More" onclick={() => set(path, k, Math.min(k.max ?? Infinity, Number(v ?? k.min ?? 0) + (k.kind === "integer" ? 1 : 0.05)))}><Icon name="plus" size={12} weight={2.2} /></button>
    </div>
  {:else if k.kind === "argv"}
    <input class="field mono" value={words(v)} placeholder={k.required ? "program and arguments" : "—"} onchange={(e) => { const t = (e.target as HTMLInputElement).value.trim(); set(path, k, t ? split(t) : undefined); }} aria-label={label(k.key)} />
  {:else if PREFIX_KEYS.includes(path)}
    {@const list = (Array.isArray(v) ? v : []) as string[][]}
    <div class="list">
      {#each list as item, i}
        {@const id = `${path}:${JSON.stringify(item)}`}
        <div class="item">
          <span class="field mono ro">{words(item)}</span>
          <button class="btn small ghost" onclick={() => coverage(path, item)}>Coverage</button>
          <button class="x" aria-label="Remove" onclick={() => set(path, k, list.filter((_, j) => j !== i))}><Icon name="minus" size={11} weight={2.4} /></button>
        </div>
        {@render preview(path, previews[id], false)}
      {/each}
      <div class="item">
        <input class="field mono" placeholder="npm run" value={adding[path] ?? ""} oninput={(e) => (adding = { ...adding, [path]: (e.target as HTMLInputElement).value })} aria-label="New prefix" />
        <button class="btn small" disabled={split((adding[path] ?? "").trim()).length < 2} onclick={() => coverage(path, split((adding[path] ?? "").trim()))}>Preview…</button>
      </div>
      {#if (adding[path] ?? "").trim()}
        {@render preview(path, previews[`${path}:${JSON.stringify(split((adding[path] ?? "").trim()))}`], true)}
      {/if}
    </div>
  {:else if k.kind === "argv_list" || k.kind === "string_list"}
    {@const list = (Array.isArray(v) ? v : []) as unknown[]}
    <div class="list">
      {#each list as item, i}
        <div class="item">
          <input class="field mono" value={k.kind === "argv_list" ? words(item) : String(item)}
            onchange={(e) => { const t = (e.target as HTMLInputElement).value.trim(); const copy = [...list]; copy[i] = k.kind === "argv_list" ? split(t) : t; set(path, k, copy.filter((x) => (Array.isArray(x) ? x.length : x))); }} />
          <button class="x" aria-label="Remove" onclick={() => set(path, k, list.filter((_, j) => j !== i))}><Icon name="minus" size={11} weight={2.4} /></button>
        </div>
      {/each}
      <button class="btn small ghost add" onclick={() => set(path, k, [...list, k.kind === "argv_list" ? ["program"] : "value"])}><Icon name="plus" size={11} weight={2.4} /> Add</button>
    </div>
  {:else}
    <input class="field" value={v ?? ""} placeholder={k.kind === "duration" ? "7d" : "—"} onchange={(e) => { const t = (e.target as HTMLInputElement).value.trim(); set(path, k, t || undefined); }} aria-label={label(k.key)} />
  {/if}
{/snippet}

{#snippet preview(path: string, c: PolicyCoverage | string | undefined, saving: boolean)}
  {#if typeof c === "string"}
    <p class="err">{c}</p>
  {:else if c}
    <div class="coverage">
      <p class="muted">{c.rows.filter((r) => !r.still_asks).length} command(s) from the journal would run without asking:</p>
      {#each c.rows as r}
        <div class="mono">{r.argv.join(" ")} <span class="muted">×{r.count} · {r.decided.map((d) => `${d.decision} ×${d.count}`).join(", ")}</span>{#if r.still_asks} <span class="chip">{r.still_asks}</span>{/if}</div>
      {/each}
      {#if !c.complete}<p class="err">The preview is incomplete — {c.unreadable.join("; ")}. Saving waits until the journal can be read.</p>{/if}
      {#if saving}<button class="btn small primary" disabled={!c.complete} onclick={() => widen(path, c)}>Save Prefix</button>{/if}
    </div>
  {/if}
{/snippet}

{#snippet row(path: string, k: ConfigKey)}
  {@const changed = path in draft}
  {@const isSet = current(path, k) !== undefined}
  <div class="row" class:changed>
    <div class="what">
      <span class="name">{label(k.key)}{#if required(path, k)}<span class="req">required</span>{/if}</span>
      <span class="about">{k.about}</span>
      {#if inapplicable(path, k) && isSet}<span class="inapt">does not apply to {provider}: remove it</span>{/if}
      <span class="meta mono">{path}{#if k.default !== null && k.default !== undefined}{" · "}default {JSON.stringify(k.default)}{/if}</span>
    </div>
    <div class="ctl">
      {@render control(path, k)}
      {#if isSet && !required(path, k)}
        {@const remove = inapplicable(path, k)}
        <button class="reset" title={remove ? "Remove it" : "Use the default"} aria-label="{remove ? 'Remove' : 'Reset'} {label(k.key)}" onclick={() => reset(path, k)}><Icon name={remove ? "minus" : "undo"} size={12} weight={2} /></button>
      {/if}
    </div>
  </div>
{/snippet}

<section class="settings">
  <nav aria-label="Sections">
    <h1>Settings</h1>
    <p class="file mono">{report?.file ?? "speccoder.toml"}</p>
    {#each report?.sections ?? [] as s (s.name)}
      {@const [icon, color] = icons[s.name] ?? ["gear", "#8e8e93"]}
      {@const count = Object.keys(draft).filter((p) => p.startsWith(s.name + ".")).length}
      <button class:on={pick === s.name} onclick={() => (pick = s.name)}>
        <span class="badge" style="--c:{color}"><Icon name={icon} size={13} weight={2.2} /></span>
        <span class="t">{s.title}</span>
        {#if count}<span class="dot" title="{count} unsaved">{count}</span>{/if}
      </button>
    {/each}
  </nav>

  <div class="pane">
    {#if report && !report.committed && report.exists && !dirty}
      <div class="commit">
        <div class="ctext">
          <strong>Not in force yet</strong>
          <span>speccoder reads committed configuration only. Commit <code>speccoder.toml</code> — alone, nothing else you have staged.</span>
        </div>
        <button class="btn primary" disabled={saving || !report.valid || !!app.job?.running} onclick={commit}>Commit</button>
      </div>
      {#if report.diff}<Diff patch={report.diff} />{/if}
    {/if}
    {#if report?.error}
      <p class="bad"><Icon name="exclaim" size={14} weight={2} /> {report.error}</p>
    {/if}
    {#if section}
      <header>
        <span class="badge big" style="--c:{icons[section.name]?.[1] ?? '#8e8e93'}"><Icon name={icons[section.name]?.[0] ?? "gear"} size={20} weight={2} /></span>
        <div><h2>{section.title}</h2><p>{section.about}</p></div>
      </header>
      {#if section.named}
        {#each section.tables as t (t.name)}
          <h3 class="section-title">
            <span class="mono">[{section.name}.{t.name}]</span>
            <button class="btn small ghost" onclick={() => removeTable(section, t.name)}>Remove</button>
          </h3>
          <div class="grouped">
            {#each t.keys as k (k.key)}{@render row(`${section.name}.${t.name}.${k.key}`, k)}{/each}
          </div>
        {/each}
        {#each Object.keys(draft).filter((p) => p.startsWith(section.name + ".") && !section.tables.some((t) => p.startsWith(`${section.name}.${t.name}.`))).map((p) => p.split(".")[1]).filter((n, i, a) => a.indexOf(n) === i) as fresh (fresh)}
          <h3 class="section-title"><span class="mono">[{section.name}.{fresh}]</span> <span class="new">new</span></h3>
          <div class="grouped">
            {#each section.keys as k (k.key)}{@render row(`${section.name}.${fresh}.${k.key}`, k)}{/each}
          </div>
        {/each}
        <div class="addtable">
          <input class="field mono" placeholder="name, e.g. lint" bind:value={newTable} onkeydown={(e) => e.key === "Enter" && addTable(section)} />
          <button class="btn small tinted" disabled={!/^[a-z0-9_-]+$/.test(newTable.trim())} onclick={() => addTable(section)}><Icon name="plus" size={11} weight={2.4} /> Add {section.name === "checks" ? "check" : "server"}</button>
        </div>
      {:else}
        {#if section.name === "model" && source}
          <p class="muted small source"><Icon name="key" size={12} /> Credentials: {source}</p>
        {/if}
        <div class="grouped">
          {#each shown(section) as k (k.key)}{@render row(`${section.name}.${k.key}`, k)}{/each}
        </div>
        {#if section.name === "commands" && fromChecks.length}
          <h3 class="sub">From checks</h3>
          <p class="muted small">Each configured check allows its program and subcommands, up to its first option; a check that starts with an option allows exactly itself. Change them in Checks.</p>
          <div class="grouped">
            {#each fromChecks as c (c.name)}
              {@const id = `commands.allow_prefix:${JSON.stringify(c.prefix)}`}
              <div class="row">
                <div class="what"><span class="name">{c.name}</span><span class="meta mono">{words(c.prefix)}{c.exact ? " — exactly this command" : " …"}</span></div>
                <div class="ctl">{#if !c.exact}<button class="btn small ghost" onclick={() => coverage("commands.allow_prefix", c.prefix)}>Coverage</button>{/if}</div>
              </div>
              {#if !c.exact}{@render preview("commands.allow_prefix", previews[id], false)}{/if}
            {/each}
          </div>
        {/if}
      {/if}
    {/if}
  </div>
</section>

{#if dirty}
  <div class="savebar glass" role="region" aria-label="Unsaved changes">
    <span class="n">{dirty} {dirty === 1 ? "change" : "changes"}</span>
    {#if error}<span class="err" title={error}><Icon name="exclaim" size={13} weight={2} /> {error}</span>{:else}<span class="cmd mono" title={cli}>{cli}</span>{/if}
    <button class="btn" onclick={() => { draft = {}; error = null; }}>Revert</button>
    <button class="btn primary" disabled={saving || !!app.job?.running} onclick={save}>Save</button>
  </div>
{/if}

<style>
  .settings { display: grid; grid-template-columns: 15rem minmax(0, 1fr); gap: 2rem; max-width: 1180px; margin: 0 auto; align-items: start; }
  nav { position: sticky; top: 0; display: grid; gap: 1px; }
  h1 { margin: 0; font: 700 2.15rem/1.12 var(--display); letter-spacing: -0.028em; }
  .file { margin: 0.25rem 0 1rem; font-size: 0.78rem; color: var(--muted); }
  nav button { display: flex; align-items: center; gap: 0.6rem; border: 0; background: none; padding: 0.35rem 0.5rem; border-radius: 8px; cursor: pointer; text-align: left; font-weight: 500; }
  nav button:hover { background: var(--fill); }
  nav button.on { background: var(--accent); color: #fff; }
  .t { flex: 1; }
  .dot { min-width: 1.2rem; height: 1.2rem; border-radius: 999px; display: grid; place-items: center; font: 650 0.7rem var(--rounded); background: var(--attention-fill); color: #fff; padding: 0 0.3rem; }
  .badge { width: 24px; height: 24px; border-radius: 6px; display: grid; place-items: center; color: #fff; background: linear-gradient(180deg, color-mix(in srgb, var(--c) 80%, white), var(--c)); box-shadow: inset 0 -1px 0 rgba(0, 0, 0, 0.12); flex: none; }
  .badge.big { width: 44px; height: 44px; border-radius: 11px; }
  .pane { display: grid; gap: 1rem; min-width: 0; }
  header { display: flex; gap: 0.9rem; align-items: center; }
  h2 { margin: 0; font: 650 1.4rem/1.2 var(--display); letter-spacing: -0.018em; }
  header p { margin: 0.15rem 0 0; color: var(--muted); font-size: 0.9rem; }
  .section-title { display: flex; align-items: center; justify-content: space-between; margin: 0.5rem 0.25rem -0.4rem; }
  .new { font-size: 0.72rem; color: var(--accent-ink); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 1.25rem; align-items: center; padding: 0.7rem 0.95rem; }
  .row.changed { background: color-mix(in srgb, var(--attention-fill) 7%, transparent); }
  .what { display: grid; gap: 0.1rem; min-width: 0; }
  .name { font-weight: 550; display: flex; align-items: center; gap: 0.4rem; }
  .req { font-size: 0.68rem; font-weight: 600; color: var(--muted); background: var(--fill); padding: 0 0.4rem; border-radius: 999px; }
  .about { font-size: 0.84rem; color: var(--text-2); }
  .inapt { font-size: 0.8rem; font-weight: 600; color: var(--danger, #ff3b30); }
  .meta { font-size: 0.72rem; color: var(--faint); }
  .ctl { display: flex; align-items: center; gap: 0.4rem; justify-content: flex-end; }
  .reset { width: 24px; height: 24px; border-radius: 50%; border: 0; background: none; color: var(--muted); cursor: pointer; display: grid; place-items: center; }
  .reset:hover { background: var(--fill); color: var(--text); }

  /* Controls, after the system's. */
  .switch { position: relative; display: inline-block; cursor: pointer; }
  .switch input { position: absolute; opacity: 0; inset: 0; margin: 0; cursor: pointer; }
  .track { display: block; width: 42px; height: 25px; border-radius: 999px; background: var(--fill-2); transition: background var(--speed) var(--ease); }
  .knob { position: absolute; top: 2px; left: 2px; width: 21px; height: 21px; border-radius: 50%; background: #fff; box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2), 0 0 0 0.5px rgba(0, 0, 0, 0.04); transition: transform 300ms var(--spring); }
  .switch input:checked + .track { background: var(--verified-fill); }
  .switch input:checked + .track .knob { transform: translateX(17px); }
  .switch input:focus-visible + .track { box-shadow: 0 0 0 3.5px var(--focus); }
  .seg { display: inline-flex; padding: 2px; border-radius: 8px; background: var(--fill); }
  .seg button { height: 24px; padding: 0 0.7rem; border: 0; background: none; border-radius: 6px; cursor: pointer; font-size: 0.82rem; color: var(--text-2); }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.14); }
  select, .field { height: 28px; border: 0; border-radius: 7px; background: var(--fill); padding: 0 0.6rem; outline: none; min-width: 12rem; font-size: 0.86rem; }
  .field.mono { font-size: 0.8rem; min-width: 18rem; }
  select:focus, .field:focus { box-shadow: 0 0 0 3.5px var(--focus); }
  .stepper { display: inline-flex; align-items: center; border-radius: 8px; background: var(--fill); overflow: hidden; }
  .stepper button { width: 26px; height: 28px; border: 0; background: none; cursor: pointer; color: var(--text-2); display: grid; place-items: center; }
  .stepper button:hover { background: var(--fill-2); }
  .stepper input { width: 6rem; height: 28px; border: 0; background: transparent; text-align: center; outline: none; font-variant-numeric: tabular-nums; -moz-appearance: textfield; appearance: textfield; }
  .stepper input::-webkit-inner-spin-button { display: none; }
  .list { display: grid; gap: 0.3rem; justify-items: end; }
  .item { display: flex; gap: 0.3rem; align-items: center; }
  .sub { font-size: 0.86rem; margin: 1rem 0 0.2rem; }
  .small { font-size: 0.8rem; margin: 0 0 0.4rem; }
  .ro { display: inline-block; padding: 0.3rem 0.5rem; }
  .coverage { font-size: 0.82rem; display: grid; gap: 0.2rem; padding: 0.4rem 0; }
  .err { color: var(--danger, #ff3b30); font-size: 0.82rem; margin: 0; }
  .x { width: 20px; height: 20px; border-radius: 50%; border: 0; background: var(--danger-fill); color: #fff; cursor: pointer; display: grid; place-items: center; }

  .commit { display: flex; align-items: center; gap: 1rem; justify-content: space-between; padding: 0.9rem 1.1rem; border-radius: var(--radius); background: var(--attention-bg); }
  .ctext { display: grid; gap: 0.1rem; }
  .ctext span { color: var(--text-2); font-size: 0.88rem; }
  .bad { display: flex; align-items: center; gap: 0.4rem; margin: 0; color: var(--danger); font-weight: 600; }
  .addtable { display: flex; gap: 0.5rem; align-items: center; }
  .savebar {
    position: fixed; left: 50%; bottom: 1.25rem; transform: translateX(-50%); z-index: 16; display: flex; align-items: center; gap: 0.75rem;
    width: min(760px, calc(100vw - 2rem)); padding: 0.55rem 0.6rem 0.55rem 1.1rem; border-radius: 999px; background: var(--glass-strong);
    box-shadow: var(--shadow-lg), var(--glass-edge); animation: rise 380ms var(--spring);
  }
  @keyframes rise { from { transform: translate(-50%, 20px); opacity: 0; } }
  .savebar .n { font-weight: 650; white-space: nowrap; }
  .cmd, .err { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.76rem; color: var(--muted); }
  .err { color: var(--danger); font-weight: 600; display: inline-flex; gap: 0.3rem; align-items: center; }
  @media (max-width: 900px) { .settings { grid-template-columns: 1fr; } nav { position: static; grid-auto-flow: column; overflow-x: auto; } nav h1, .file { display: none; } .row { grid-template-columns: 1fr; } .ctl { justify-content: flex-start; } .field.mono { min-width: 0; width: 100%; } }
</style>
