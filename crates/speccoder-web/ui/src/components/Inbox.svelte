<script lang="ts">
  // Needs You: what the work needs from the person, by what it means —
  // Blocked on you (nothing moves until you decide), Ready to review (the
  // riskiest evidence first), Working, Idle. A kind is a row's label; each
  // row decides in place; Open shows it with its evidence.
  import { app } from "../lib/app.svelte";
  import { askable } from "../lib/ask.svelte";
  import { KINDS, key } from "../lib/actions";
  import type { Action, InboxItem } from "../lib/types";
  import Code from "./Code.svelte";
  import Findings from "./Findings.svelte";
  import Icon from "./Icon.svelte";
  import { KINDS as kinds } from "../lib/kinds";
  import ReportTo from "./ReportTo.svelte";
  import StopCard from "./StopCard.svelte";

  // A kind the page does not know is shown under its own name, never dropped.
  const kindOf = (i: InboxItem) => kinds[i.kind] ?? { title: i.kind.replace(/_/g, " "), icon: "info", tone: "needs_approval", why: "" };
  const groupWhy: Record<string, string> = {
    blocked_on_you: "Nothing moves until you decide — the oldest first.",
    ready_to_review: "Done or held — the riskiest evidence first.",
  };
  const items = $derived(app.inbox?.data?.items ?? []);
  const groups = $derived((["blocked_on_you", "ready_to_review"] as const).map((g) => ({
    g,
    title: app.inbox?.data?.groups?.find((x) => x.group === g)?.title ?? (g === "blocked_on_you" ? "Blocked on you" : "Ready to review"),
    items: items.filter((i) => (i.group === "ready_to_review" ? "ready_to_review" : "blocked_on_you") === g),
  })).filter((x) => x.items.length));
  const working = $derived(app.inbox?.data?.working ?? []);
  const idle = $derived(app.inbox?.data?.idle ?? []);
  const blocked = $derived(items.filter((i) => i.group !== "ready_to_review").length);
  const ready = $derived(items.length - blocked);
  const features = $derived(new Set(items.map((i) => i.feature)).size);

  // Several approvals of one tool, decided together — each on its own
  // exact object, as one at a time would.
  const id = (i: InboxItem) => `${i.feature}/${i.task ?? ""}`;
  let picked = $state<string[]>([]);
  let why = $state("");
  let rejecting = $state(false);
  const chosen = $derived(items.filter((i) => i.kind === "approval" && i.tool && picked.includes(id(i))));
  const tools = $derived(new Set(chosen.map((i) => i.tool)));
  const toggle = (i: InboxItem) => (picked = picked.includes(id(i)) ? picked.filter((x) => x !== id(i)) : [...picked, id(i)]);
  // One job at a time (the host runs one): each waits for the one before,
  // and the ones that did not go through stay selected.
  let deciding = $state(false);
  async function decideAll(approve: boolean) {
    deciding = true;
    const failed: string[] = [];
    for (const i of chosen.slice()) {
      const a = i.actions.find((x) => x.id.kind === (approve ? "approve" : "reject"));
      if (!a) continue;
      if (app.feature !== i.feature) await app.selectFeature(i.feature);
      const r = await app.run(a, approve ? undefined : why.trim());
      if (r?.status === "ok") picked = picked.filter((x) => x !== id(i));
      else failed.push(`${i.task}: ${r?.message ?? "not decided"}`);
    }
    deciding = false;
    if (failed.length) app.toast({ tone: "error", text: `${failed.length} not decided`, detail: failed.join("\n") });
    else {
      why = "";
      rejecting = false;
    }
    void app.refresh();
  }

  const ago = (s: number | null) => {
    if (!s) return "";
    const m = Math.round((Date.now() / 1000 - s) / 60);
    return m < 1 ? "just now" : m < 60 ? `${m} min` : m < 1440 ? `${Math.round(m / 60)} h` : `${Math.round(m / 1440)} d`;
  };
  // A task's action says its kind's verb ("Run Again" for a task that
  // did not pass); any other says what it does.
  const verb = (i: InboxItem, a: Action) => {
    // An approval that asks for words answers a question.
    if (a.id.kind === "approve" && a.input) return "Answer";
    if (i.task && a.id.subject === i.task) return KINDS[a.id.kind].verb + (a.id.kind === "run_task" ? " Again" : "");
    return a.label.charAt(0).toUpperCase() + a.label.slice(1);
  };

  async function decide(i: InboxItem, a: Action) {
    if (app.feature !== i.feature) await app.selectFeature(i.feature);
    await app.perform(a);
  }
  async function openTask(i: InboxItem, task: string) {
    if (app.feature !== i.feature) await app.selectFeature(i.feature);
    app.view = "board";
    void app.selectTask(task);
  }
  async function openFeature(feature: string, view: "board" | "overview") {
    if (app.feature !== feature) await app.selectFeature(feature);
    app.view = view;
  }
  async function open(i: InboxItem) {
    // About the project, not a feature: its pane in Settings.
    if (i.kind === "extension_untrusted") {
      app.view = "settings";
      app.settingsSection = "extensions";
      return;
    }
    // The checkout's agent files: what changed, and trust.
    if (i.kind === "checkout_untrusted") {
      app.trustSheet = true;
      return;
    }
    // Between projects: their feedback, on the Projects page.
    if (i.kind === "request" || i.kind === "feedback") {
      app.view = "projects";
      return;
    }
    if (i.kind.startsWith("bug_") && i.task) {
      app.bugSheet = { key: `${i.feature}/${i.task}` };
      return;
    }
    if (app.feature !== i.feature) await app.selectFeature(i.feature);
    if (i.kind === "review" || i.kind === "revise" || i.kind === "question" || i.kind === "ambiguous") {
      app.doc = "review";
      app.view = "documents";
      return;
    }
    app.view = i.kind === "handoff" || i.kind === "stale" || i.kind === "unverified" ? "report" : "board";
    if (i.task) void app.selectTask(i.task);
  }
  // Other projects where something waits (or may): the projects' roll-up.
  $effect(() => {
    if (!app.projects) void app.loadProjects();
  });
  const elsewhere = $derived((app.projects?.projects ?? []).filter((p) => !p.current && (p.state === "blocked_on_you" || p.state === "cant_tell" || (p.waiting ?? 0) > 0)));
</script>

<section class="inbox" aria-labelledby="inbox-title">
  <header>
    <div class="titlebar"><h1 id="inbox-title">Needs You</h1><button class="btn small tinted" onclick={() => (app.quick = true)} title="A small change, judged like any other (q)"><Icon name="sparkle" size={11} weight={2.2} /> Quick Change</button></div>
    <p class="sub">
      {#if items.length}
        {#if blocked}<strong class="lead">{blocked} blocked on you</strong>{:else}Nothing is blocked on you{/if}{#if ready}{" · "}{ready} ready to review{/if}{#if working.length}{" · "}{working.length} working{/if}
      {:else}Every decision the agents wait on lands here.{/if}
    </p>
    {#if elsewhere.length}
      <!-- Other projects with something for you: one quiet line, one click. -->
      <p class="elsewhere">
        <span class="muted">Also waiting:</span>
        {#each elsewhere as p (p.path)}<button class="chip" onclick={() => (app.view = "projects")} title={p.waiting == null ? `${p.name}: ${p.unknown ?? "could not be read"}` : `${p.waiting} waiting in ${p.name}`}>{p.name} <strong>{p.waiting ?? "?"}</strong></button>{/each}
      </p>
    {/if}
  </header>

  {#if working.length}
    <ul class="strip" aria-label="Working">
      {#each working as w (w.feature + w.pid)}
        <li class="run" class:unknown={w.state !== "running"}>
          <span class="pulse" aria-hidden="true"></span>
          <span class="feat">{w.feature.replace(/^\d+-/, "")}</span>
          {#if w.task}<span class="mono task">{w.task}</span>{/if}
          <span class="muted">{w.state === "running" ? `working · ${ago(w.started)}` : "state unknown — it stopped without finishing"}</span>
          <button class="btn small ghost" onclick={() => openFeature(w.feature, "board")}>Watch <Icon name="chevron" size={10} weight={2.6} /></button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if !app.inbox}
    <div class="center"><span class="spinner"></span></div>
  {:else if !items.length}
    <div class="clear">
      <span class="ok" aria-hidden="true"><Icon name="check" size={30} weight={2.4} /></span>
      <strong>Nothing needs you</strong>
      <span class="muted">The agents can work. {#if app.primary}Next: {app.primary.label}.{/if}</span>
      {#if app.primary}<button class="btn primary" disabled={!!app.job?.running} onclick={() => app.primary && app.perform(app.primary)}>{app.primary.label.charAt(0).toUpperCase() + app.primary.label.slice(1)}</button>{/if}
    </div>
  {:else}
    {#each groups as g (g.g)}
      <section class="group {g.g}" aria-label={g.title}>
        <h2><span class="gtitle">{g.title}</span><span class="count">{g.items.length}</span><span class="why">{groupWhy[g.g]}</span></h2>
        <ul class="grouped">
          {#each g.items as i (i.feature + (i.task ?? "") + i.kind + i.title)}
            {@const k = kindOf(i)}
            {#if i.kind === "yours" && i.task}
              <li class="yours">
                <span class="where"><span class="kind s-{k.tone}"><Icon name={k.icon} size={10} weight={2.4} />{k.title}</span>{#if features > 1 && i.feature}<span class="feat">{i.feature.replace(/^\d+-/, "")}</span>{/if}</span>
                <StopCard task={i.task} claim={i.detail || null} actions={i.actions} />
              </li>
            {:else}
            <li class:picked={picked.includes(id(i))} {...askable("inbox_item", { feature: i.feature, task: i.task ?? null, kind: i.kind }, `${k.title}${i.task ? ` ${i.task}` : ""}`, `${k.title}: ${i.title}${i.detail ? `\n${i.detail}` : ""}${i.task ? `\ntask ${i.task}` : ""} (${i.feature})`)}>
              {#if i.kind === "approval" && i.tool}
                <input type="checkbox" class="pick" aria-label="Select {i.task}" checked={picked.includes(id(i))} onchange={() => toggle(i)} />
              {/if}
              <div class="what">
                <span class="where">
                  <span class="kind s-{k.tone}" title={k.why}><Icon name={k.icon} size={10} weight={2.4} />{k.title}</span>
                  {#if features > 1 && i.feature}<span class="feat">{i.feature.replace(/^\d+-/, "")}</span>{/if}
                  {#if i.task}<span class="mono task">{i.task}</span>{/if}
                  {#if i.since}<span class="age">{ago(i.since)}</span>{/if}
                </span>
                <strong>{i.title}</strong>
                {#if i.group === "ready_to_review" && (i.risk?.length || i.kind === "handoff")}
                  <span class="risks">
                    {#each (i.risk ?? []).slice(0, 3) as r (r.reason)}<span class="risk">{r.text}</span>{/each}
                    {#if (i.risk?.length ?? 0) > 3}<span class="risk more">+{(i.risk?.length ?? 0) - 3}</span>{/if}
                    {#if !i.risk?.length}<span class="risk verified"><Icon name="check" size={9} weight={2.8} /> verified</span>{/if}
                  </span>
                {/if}
                {#if i.unverified}
                  <ul class="reasons">
                    {#each i.unverified.counts as c (c.reason)}
                      <li><span>{c.text}</span>{#each c.tasks as t (t)}<button class="chip mono" aria-label="Open {t}" onclick={() => openTask(i, t)}>{t}</button>{/each}</li>
                    {/each}
                  </ul>
                {:else if i.detail}<span class="detail"><Code text={i.detail} /></span>{/if}
                {#if i.findings?.length}<div class="found"><Findings findings={i.findings} /></div>{/if}
              </div>
              <div class="acts">
                {#each i.actions as a, n (key(a))}
                  <button class="btn small" class:primary={n === 0} class:ghost={n > 1} disabled={!!a.blocked || !!app.job?.running} title={`${a.blocked ?? a.why}\n$ ${a.command}`} onclick={() => decide(i, a)}>{verb(i, a)}</button>
                {/each}
                {#if i.kind === "approval" && i.task}<button class="btn small ghost" onclick={() => (app.explain = { task: `${i.feature}/${i.task}`, call: "" })} title="What the call is and can reach, which rule asks — facts, no model"><Icon name="info" size={10} weight={2.4} /> Explain</button>{/if}
                {#if i.kind === "stuck" && i.task && i.feature}<ReportTo feature={i.feature} task={i.task} />{/if}
                <button class="btn small ghost" onclick={() => open(i)} title="Show it with its evidence"><Icon name="chevron" size={10} weight={2.6} /> Open</button>
              </div>
            </li>
            {/if}
          {/each}
        </ul>
        {#if g.g === "blocked_on_you" && chosen.length}
          <div class="batch" role="toolbar" aria-label="Decide the selected approvals">
            {#if tools.size > 1}
              <span class="muted">One kind of call at a time: the selection mixes {[...tools].join(" and ")}.</span>
            {:else if rejecting}
              <input class="reason" placeholder="Why (each model is told)" bind:value={why} onkeydown={(e) => e.key === "Enter" && why.trim() && decideAll(false)} />
              <button class="btn small danger" disabled={!why.trim() || deciding || !!app.job?.running} onclick={() => decideAll(false)}>Reject {chosen.length}</button>
              <button class="btn small ghost" onclick={() => (rejecting = false)}>Back</button>
            {:else}
              <span><strong>{chosen.length}</strong> <span class="mono">{[...tools][0]}</span> {chosen.length === 1 ? "call" : "calls"} — each decided on its own object</span>
              <button class="btn small primary" disabled={deciding || !!app.job?.running} onclick={() => decideAll(true)}>{deciding ? "Deciding…" : `Approve ${chosen.length}`}</button>
              <button class="btn small" onclick={() => (rejecting = true)}>Reject {chosen.length}…</button>
              <button class="btn small ghost" onclick={() => (picked = [])}>Clear</button>
            {/if}
          </div>
        {/if}
      </section>
    {/each}
  {/if}

  {#if idle.length}
    <p class="idle"><span class="muted">Idle:</span>{#each idle as f (f)}<button class="chip" onclick={() => openFeature(f, "overview")}>{f.replace(/^\d+-/, "")}</button>{/each}</p>
  {/if}
</section>

<style>
  .elsewhere { margin: 0.5rem 0 0; display: flex; flex-wrap: wrap; gap: 0.35rem; align-items: center; font-size: 0.8rem; }
  .elsewhere .muted { color: var(--muted); }
  .elsewhere .chip { border: 0; border-radius: 999px; padding: 0.12rem 0.55rem; background: color-mix(in srgb, var(--attention) 13%, transparent); color: var(--text); font: inherit; font-size: 0.78rem; cursor: pointer; }
  .elsewhere .chip strong { color: var(--attention); margin-left: 0.15rem; }
  .elsewhere .chip:hover { background: color-mix(in srgb, var(--attention) 22%, transparent); }
  .elsewhere .chip:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .inbox { max-width: 920px; margin: 0 auto; display: grid; gap: 1.4rem; }
  .titlebar { display: flex; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; }
  h1 { margin: 0; font: 700 1.9rem/1.15 var(--display); letter-spacing: -0.022em; }
  .sub { margin: 0.25rem 0 0; color: var(--text-2); }
  .lead { color: var(--attention); font-weight: 650; }
  .group { display: grid; gap: 0.45rem; }
  .gtitle { font: 650 1.02rem/1.2 var(--display); letter-spacing: -0.01em; }
  .group.blocked_on_you .gtitle { color: var(--attention); }
  .kind { white-space: nowrap; display: inline-flex; align-items: center; gap: 0.25rem; padding: 0.05rem 0.45rem 0.05rem 0.35rem; border-radius: 999px; font-weight: 650; color: var(--fill-c, var(--text-2)); background: color-mix(in srgb, var(--fill-c, var(--muted)) 13%, transparent); }
  .risks { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.15rem; }
  .risk { padding: 0.05rem 0.5rem; border-radius: 999px; font: 600 0.72rem/1.5 var(--rounded); color: var(--attention); background: var(--attention-bg); }
  .risk.verified { color: var(--verified); background: color-mix(in srgb, var(--verified) 12%, transparent); display: inline-flex; align-items: center; gap: 0.2rem; }
  .risk.more { color: var(--muted); background: var(--fill); }
  li:has(.pick) { grid-template-columns: auto minmax(0, 1fr) auto; }
  li.picked { background: color-mix(in srgb, var(--accent) 7%, transparent); }
  .pick { width: 1rem; height: 1rem; accent-color: var(--accent); }
  .batch { position: sticky; bottom: 0.75rem; display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; padding: 0.6rem 0.8rem; border-radius: 14px; background: var(--surface, var(--bg)); box-shadow: 0 8px 28px rgba(0,0,0,0.16), 0 0 0 0.5px var(--separator); font-size: 0.86rem; }
  .batch > span { flex: 1; min-width: 12rem; }
  .reason { flex: 1; min-width: 12rem; padding: 0.35rem 0.6rem; border-radius: 8px; border: 1px solid var(--separator); background: transparent; color: inherit; font: inherit; }
  .strip { display: grid; gap: 0.3rem; }
  .run { display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.9rem; border-radius: 12px; background: var(--fill); font-size: 0.84rem; grid-template-columns: none; }
  .run .muted { flex: 1; }
  .pulse { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); animation: pulse 1.6s ease-in-out infinite; }
  .run.unknown .pulse { background: var(--attention); animation: none; }
  @keyframes pulse { 50% { opacity: 0.35; transform: scale(0.8); } }
  .idle { display: flex; flex-wrap: wrap; align-items: center; gap: 0.35rem; margin: 0; font-size: 0.84rem; }
  h2 { display: flex; align-items: center; gap: 0.45rem; margin: 0 0.3rem; font-size: 0.86rem; font-weight: 650; background: none; flex-wrap: wrap; }
  .gi { width: 22px; height: 22px; border-radius: 7px; display: grid; place-items: center; color: #fff; background: var(--fill-c); }
  .count { min-width: 1.4rem; height: 1.2rem; padding: 0 0.4rem; border-radius: 999px; display: grid; place-items: center; background: var(--fill); color: var(--muted); font: 600 0.72rem/1 var(--rounded); }
  .why { color: var(--muted); font-weight: 500; font-size: 0.8rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 1rem; align-items: center; padding: 0.8rem 1rem; }
  .what { display: grid; gap: 0.18rem; min-width: 0; }
  li.yours { grid-template-columns: 1fr; gap: 0.35rem; }
  li.yours .feat { justify-self: start; font-size: 0.74rem; }
  .where { display: flex; flex-wrap: wrap; gap: 0.45rem; align-items: center; font-size: 0.74rem; color: var(--muted); }
  .feat { padding: 0 0.45rem; border-radius: 999px; background: var(--fill); color: var(--text-2); font-weight: 600; }
  .task { color: var(--text-2); }
  .found { margin-top: 0.45rem; }
  .detail { color: var(--text-2); font-size: 0.86rem; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .reasons { display: grid; gap: 0.25rem; margin: 0.2rem 0 0; padding: 0; }
  .reasons li { display: flex; flex-wrap: wrap; gap: 0.3rem; align-items: center; padding: 0; color: var(--text-2); font-size: 0.86rem; grid-template-columns: none; }
  .chip { border: 0; border-radius: 6px; padding: 0 0.4rem; background: var(--fill); color: var(--text-2); font-size: 0.74rem; cursor: pointer; }
  .chip:hover { background: var(--fill-2); }
  .acts { display: flex; gap: 0.35rem; flex-wrap: wrap; justify-content: flex-end; max-width: 22rem; }
  .clear { display: grid; justify-items: center; gap: 0.4rem; padding: 3.5rem 1rem; text-align: center; }
  .clear strong { font: 650 1.2rem/1.3 var(--display); }
  .ok { width: 60px; height: 60px; border-radius: 50%; display: grid; place-items: center; color: #fff; background: var(--verified-fill); margin-bottom: 0.4rem; }
  .center { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 20px; height: 20px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 640px) { li { grid-template-columns: 1fr; } li:has(.pick) { grid-template-columns: auto minmax(0, 1fr); } li:has(.pick) .acts { grid-column: 2; } .acts { justify-content: flex-start; max-width: none; } }
</style>
