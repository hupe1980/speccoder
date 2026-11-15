<script lang="ts">
  // Every project speccoder was used in, and what waits across them: one
  // list merged from each project's Needs You, then the projects by where
  // they stand. Nothing is decided here — each item opens its project in
  // its own host (its own key) at the item. A count that could not be
  // read is shown as unknown, never 0.
  import { app } from "../lib/app.svelte";
  import { post } from "../lib/api";
  import type { ItemAcross, ProjectState, ProjectSummary } from "../lib/types";
  import Icon from "./Icon.svelte";
  import { kindTitle } from "../lib/kinds";
  import Feedback from "./Feedback.svelte";

  // Read on arrival, after every job here, and every 30 s while shown.
  $effect(() => {
    void app.job?.running;
    void app.loadProjects();
    const t = setInterval(() => void app.loadProjects(), 30_000);
    return () => clearInterval(t);
  });
  const r = $derived(app.projects);
  const blocked = $derived((r?.items ?? []).filter((i) => i.group === "blocked_on_you"));
  const review = $derived((r?.items ?? []).filter((i) => i.group !== "blocked_on_you"));
  const STATES: { s: ProjectState; title: string }[] = [
    { s: "blocked_on_you", title: "Blocked on You" },
    { s: "working", title: "Working" },
    { s: "cant_tell", title: "Can't Tell — May Need You" },
    { s: "ready_to_review", title: "Ready to Review" },
    { s: "idle", title: "Idle" },
    { s: "missing", title: "Gone" },
  ];
  const ago = (s: number | null | undefined) => {
    if (!s) return "";
    const m = Math.round((Date.now() / 1000 - s) / 60);
    return m < 1 ? "just now" : m < 60 ? `${m} min` : m < 1440 ? `${Math.round(m / 60)} h` : `${Math.round(m / 1440)} d`;
  };
  const kind = kindTitle;
  const spend = (p: ProjectSummary) => {
    const u = p.spend;
    if (!u) return "";
    if (u.usd != null) return `$${u.usd.toFixed(2)}`;
    const t = u.input_tokens + u.output_tokens;
    return t ? `${t >= 1000 ? `${(t / 1000).toFixed(1)}k` : t} tokens` : "";
  };
  const here = $derived(r?.projects.find((p) => p.current) ?? null);

  function decide(i: ItemAcross) {
    if (here && i.path === here.path) {
      void app.selectFeature(i.feature).then(() => {
        app.view = "inbox";
        if (i.task) void app.selectTask(i.task);
      });
      return;
    }
    void app.openProject(i.path, i.project, i.task ? `inbox:${i.feature}/${i.task}` : `inbox:${i.feature}`);
  }
  async function forget(p: ProjectSummary) {
    await post("/api/recent/forget", { path: p.path });
    await app.loadProjects();
  }
</script>

<section class="projects" aria-labelledby="p-title">
  <header>
    <h1 id="p-title">Projects</h1>
    {#if r}<p class="sub">{r.projects.length} {r.projects.length === 1 ? "project" : "projects"} · {blocked.length} waiting for you</p>{/if}
  </header>
  {#if !r}
    <div class="center"><span class="spinner"></span></div>
  {:else if !r.projects.length}
    <div class="empty">
      <Icon name="tray" size={28} />
      <p>No projects yet. Open one with <strong>Open Project…</strong> (⌘O in the app), or run <code>speccoder ui</code> in its folder.</p>
    </div>
  {:else}
    <div class="cols">
      <section aria-labelledby="w-title">
        <h2 id="w-title" class="section-title">Needs You Across Projects <span class="count">{blocked.length + review.length}</span></h2>
        {#if !blocked.length && !review.length}
          <p class="quiet">{r.projects.some((p) => p.state === "cant_tell") ? "Nothing waits in any project that could be read." : "Nothing waits in any project."}</p>
        {/if}
        <ul class="items">
          {#each [...blocked, ...review] as i (i.path + i.feature + (i.task ?? "") + i.kind + i.title)}
            {@const mine = here?.path === i.path}
            <li class:blocked={i.group === "blocked_on_you"}>
              <div class="what">
                <span class="where"><span class="proj">{i.project}</span><span class="mono">{i.feature}</span>{#if i.task}<span class="mono">{i.task}</span>{/if}<span class="kind">{kind(i.kind)}</span>{#if i.since}<span class="age">{ago(i.since)}</span>{/if}</span>
                <strong>{i.title}</strong>
              </div>
              <button class="btn small" class:primary={i.group === "blocked_on_you"} onclick={() => decide(i)} title={mine ? "In this project" : `Opens ${i.project} in its own ${app.project?.window ? "window" : "tab"}, at this item`}>
                {mine ? "Decide Here" : `Decide in ${i.project}`} <Icon name="chevron" size={10} weight={2.6} />
              </button>
            </li>
          {/each}
        </ul>
      </section>
      <section aria-labelledby="l-title">
        <h2 id="l-title" class="section-title">All Projects</h2>
        {#each STATES as g (g.s)}
          {@const list = r.projects.filter((p) => p.state === g.s)}
          {#if list.length}
            <h3 class="group s-{g.s}">{g.title} <span class="count">{list.length}</span></h3>
            <ul class="rows">
              {#each list as p (p.path)}
                <li class:gone={p.state === "missing"} class:current={p.current}>
                  <div class="what">
                    <span class="name">{p.name}{#if p.current}<span class="here">this project</span>{/if}</span>
                    <span class="path mono" title={p.path}>{p.shown}</span>
                    <span class="facts">
                      {#if p.state === "missing"}{p.unknown}{:else}
                        {#if p.feature}<span class="mono">{p.feature}</span>{/if}
                        {#if p.live}<span class="live"><span class="pulse" aria-hidden="true"></span>{p.live.task}{p.live.stage ? ` · ${p.live.stage}` : ""}</span>{/if}
                        {#if p.waiting != null}{#if p.waiting}<span class="wait">{p.waiting} waiting</span>{/if}{:else}<span class="wait" title={p.unknown ?? ""}>? waiting — {p.unknown}</span>{/if}
                        {#if spend(p)}<span>{spend(p)}</span>{/if}
                        {#if p.elsewhere}<span class="quiet">open in process {p.elsewhere.pid}</span>{/if}
                      {/if}
                    </span>
                  </div>
                  {#if p.state === "missing"}
                    <button class="btn small ghost" onclick={() => forget(p)}>Forget</button>
                  {:else if !p.current}
                    <button class="btn small" onclick={() => app.openProject(p.path, p.name)} title="In its own {app.project?.window ? 'window' : 'tab'}, with its own key">Open</button>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        {/each}
      </section>
    </div>
  {/if}
  {#if app.project}<Feedback />{/if}
</section>

<style>
  .projects { display: grid; gap: 1.2rem; max-width: 1180px; }
  header h1 { margin: 0; font: 700 2rem/1.15 var(--display); letter-spacing: -0.02em; }
  .sub { margin: 0.25rem 0 0; color: var(--text-2); }
  .cols { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr); gap: 1.4rem; align-items: start; }
  @media (max-width: 980px) { .cols { grid-template-columns: minmax(0, 1fr); } }
  .section-title { display: flex; gap: 0.4rem; align-items: baseline; margin: 0 0 0.5rem; font: 650 1rem/1.2 var(--display); }
  .count { color: var(--faint); font-weight: 500; font-variant-numeric: tabular-nums; }
  ul { list-style: none; margin: 0; padding: 0; border-radius: var(--radius-lg); background: var(--surface); box-shadow: var(--shadow); overflow: hidden; }
  li { display: flex; gap: 0.8rem; align-items: center; justify-content: space-between; padding: 0.7rem 0.95rem; min-width: 0; }
  li + li { border-top: 0.5px solid var(--separator); }
  .what { display: grid; gap: 0.15rem; min-width: 0; }
  .where, .facts { display: flex; flex-wrap: wrap; gap: 0.45rem; align-items: center; font-size: 0.76rem; color: var(--muted); }
  .proj { font-weight: 650; color: var(--text); }
  .kind { border-radius: 999px; padding: 0 0.45rem; background: var(--fill); color: var(--text-2); }
  .blocked .kind { background: color-mix(in srgb, var(--attention) 15%, transparent); color: var(--attention); }
  strong { font-size: 0.9rem; font-weight: 600; overflow-wrap: anywhere; }
  .btn { flex: none; display: inline-flex; gap: 0.3rem; align-items: center; }
  .group { margin: 0.9rem 0 0.4rem; font-size: 0.78rem; font-weight: 650; color: var(--text-2); display: flex; gap: 0.35rem; }
  .group:first-of-type { margin-top: 0; }
  .s-blocked_on_you { color: var(--attention); }
  .s-working { color: var(--running); }
  .s-cant_tell { color: var(--attention); }
  .name { font-weight: 650; display: flex; gap: 0.45rem; align-items: baseline; }
  .here { font-size: 0.72rem; font-weight: 500; color: var(--accent-ink); }
  .path { font-size: 0.74rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wait { color: var(--attention); font-weight: 600; }
  .live { color: var(--running); display: inline-flex; gap: 0.3rem; align-items: center; }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: var(--running-fill); }
  .gone { opacity: 0.6; }
  .quiet { color: var(--muted); font-size: 0.86rem; }
  .empty { display: grid; justify-items: center; gap: 0.6rem; padding: 3rem 1rem; color: var(--text-2); text-align: center; }
  .center { display: grid; place-items: center; padding: 3rem; }
  .spinner { width: 22px; height: 22px; border-radius: 50%; border: 2.5px solid var(--fill-2); border-top-color: var(--muted); animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 600px) { li { flex-wrap: wrap; } }
</style>
