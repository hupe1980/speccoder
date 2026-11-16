<script lang="ts">
  // The merge forecast (spec 129): will this branch still work when it meets
  // the others? The last forecast, problems first — each with who, what and
  // how to see it — and the rest in one quiet line. Check Open Branches
  // fetches, pairs this branch with each other open branch and runs the
  // frozen oracles on every merge, in the sandbox.
  import { app } from "../lib/app.svelte";
  import { get, latest, post, q } from "../lib/api";
  import type { ForecastReport, PairForecast, Reply } from "../lib/types";
  import Icon from "./Icon.svelte";
  import OpenIn from "./OpenIn.svelte";

  let report = $state<ForecastReport | null>(null);
  let failed = $state<string | null>(null);
  let now = $state(Date.now() / 1000);
  const fetchLast = latest();
  $effect(() => {
    const feature = app.feature ?? undefined;
    report = null;
    failed = null;
    fetchLast(get<Reply<ForecastReport>>(`/api/forecast${q({ feature })}`), (r) => (report = r.data ?? null), () => (report = null));
  });

  // Which feature the running check is for: an answer for another one
  // (the person switched meanwhile) is not shown here.
  let checkingFor = $state<string | null>(null);
  const checking = $derived(checkingFor !== null && checkingFor === (app.feature ?? ""));
  async function check() {
    const feature = app.feature ?? "";
    checkingFor = feature;
    failed = null;
    try {
      const r = await post<Reply<ForecastReport>>(`/api/forecast${q({ feature: feature || undefined })}`, {});
      if ((app.feature ?? "") !== feature) return;
      if (r.data) report = r.data;
      else failed = r.message;
    } catch (e) {
      if ((app.feature ?? "") === feature) failed = e instanceof Error ? e.message : String(e);
    } finally {
      if (checkingFor === feature) checkingFor = null;
      now = Date.now() / 1000;
    }
  }

  const problems = $derived(report?.pairs.filter((p) => p.verdict === "conflicts" || p.verdict === "breaks") ?? []);
  const rest = $derived(report?.pairs.filter((p) => p.verdict !== "conflicts" && p.verdict !== "breaks") ?? []);
  const clean = $derived(rest.filter((p) => p.verdict === "no_conflict_observed").length);
  const unjudged = $derived(rest.length - clean);
  function ago(t: number) {
    const s = Math.max(0, now - t);
    return s < 60 ? "just now" : s < 3600 ? `${Math.floor(s / 60)} min ago` : s < 86400 ? `${Math.floor(s / 3600)} h ago` : `${Math.floor(s / 86400)} d ago`;
  }
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const fetched = $derived(
    !report ? "" : report.fetch.state === "fetched" ? "" : report.fetch.state === "failed" ? " · not fetched" : report.fetch.state === "skipped" ? " · as last fetched" : "",
  );
  function what(p: PairForecast) {
    const files = p.files ?? [];
    if (p.verdict === "conflicts") return `Conflicts in ${files.length === 1 ? files[0] : plural(files.length, "file", "files")}`;
    const parts = (p.broken ?? []).map((b) => `Breaks ${b.mine ? "" : `${b.unit}'s `}${b.scenario}`);
    if (p.rewritten?.length) parts.push(`Rewrites ${plural(p.rewritten.length, "oracle test", "oracle tests")}`);
    return parts.join(" · ");
  }
  const code = (s: string) => s.split("`");
</script>

<div class="forecast" class:problem={problems.length > 0}>
  <div class="head">
    <Icon name={problems.length ? "exclaim" : report && report.pairs.length ? "check" : "branch"} size={12} weight={2.4} />
    <span class="title">Merge forecast</span>
    {#if report}
      <span class="when">{report.pairs.length ? plural(report.pairs.length, "branch", "branches") : "No other branch"} · {ago(report.at)}{fetched}</span>
    {/if}
    <button class="btn small" disabled={checking} onclick={check} aria-busy={checking}>
      {#if checking}<span class="spin" aria-hidden="true"></span> Checking…{:else}<Icon name="loop" size={11} weight={2.2} /> Check Open Branches{/if}
    </button>
  </div>

  {#if failed}<p class="failed">{failed}</p>{/if}
  {#if !report}
    {#if !failed}
    <p class="muted">Pairs this branch with every other open branch — yours, a teammate's, another agent's — and runs your oracle on each merge before anyone merges.</p>
    {/if}
  {:else}
    {#if problems.length}
      <ul class="problems">
        {#each problems as p (p.branch)}
          <li>
            <p class="who"><code>{p.branch}</code> <span class="by">{p.author ? `${p.author} · ` : ""}{ago(p.updated)}</span></p>
            <p class="what">{what(p)}</p>
            {#if p.verdict === "conflicts"}
              <ul class="files">
                {#each (p.files ?? []).slice(0, 4) as f}<li><code>{f}</code><OpenIn path={f} feature={app.feature} /></li>{/each}
                {#if (p.files ?? []).length > 4}<li class="muted">{(p.files ?? []).length - 4} more</li>{/if}
              </ul>
            {/if}
            {#each p.broken ?? [] as b}
              <div class="broken">
                {#if b.contract}<p class="contract">{b.scenario}: {b.contract}</p>{/if}
                <p class="test"><code>{b.test}</code>{#if b.mine}<OpenIn path={b.test} feature={app.feature} />{/if} fails on the merge, passes on its own branch</p>
                {#if b.output}<details><summary>Output on the merge</summary><pre>{b.output}</pre></details>{/if}
              </div>
            {/each}
            {#each p.rewritten ?? [] as t}<p class="test"><code>{t}</code> is a frozen test the merge changes</p>{/each}
            {#each p.fixes ?? [] as fix}<p class="fix">→ {#each code(fix) as part, i}{#if i % 2}<code>{part}</code>{:else}{part}{/if}{/each}</p>{/each}
          </li>
        {/each}
      </ul>
    {/if}
    {#if rest.length}
      <details class="rest">
        <summary>
          {#if clean}<span class="ok"><Icon name="check" size={10} weight={2.6} /> No conflict observed with {plural(clean, "branch", "branches")}</span>{/if}{#if clean && unjudged}<span class="sep" aria-hidden="true">·</span>{/if}{#if unjudged}<span>{plural(unjudged, "branch", "branches")} not judged</span>{/if}
        </summary>
        <ul>
          {#each rest as p (p.branch)}
            <li><code>{p.branch}</code> {p.verdict === "no_conflict_observed" ? `— ${plural(p.held, "scenario", "scenarios")} held` : `— not judged: ${p.why}`}{#if p.setup}<pre class="why">{p.setup}</pre>{/if}{#if p.flaky?.length} <span class="muted">(flaky, not counted: {p.flaky.join(", ")})</span>{/if}</li>
          {/each}
        </ul>
      </details>
    {/if}
    {#each report.notices ?? [] as n}<p class="muted note">{n}</p>{/each}
    {#if report.unchecked?.length}<p class="muted note">{plural(report.unchecked.length, "older branch", "older branches")} not checked: <code>speccoder forecast {report.unchecked[0]}</code></p>{/if}
  {/if}
</div>

<style>
  .forecast { border-radius: var(--radius); padding: 0.55rem 0.75rem; background: var(--surface-2); display: grid; gap: 0.45rem; }
  .forecast.problem { box-shadow: inset 3px 0 0 var(--danger); }
  .head { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; font-size: 0.85rem; }
  .head :global(svg) { color: var(--verified); }
  .problem .head :global(svg) { color: var(--danger); }
  .title { font-weight: 600; }
  .when { color: var(--muted); font-size: 0.78rem; }
  .head .btn { margin-left: auto; }
  .spin { width: 0.7rem; height: 0.7rem; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; display: inline-block; animation: turn 0.8s linear infinite; vertical-align: -1px; }
  @keyframes turn { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  p { margin: 0; font-size: 0.84rem; }
  .failed { color: var(--danger); }
  ul { margin: 0; padding: 0; list-style: none; display: grid; gap: 0.6rem; }
  .problems > li { display: grid; gap: 0.2rem; }
  .who code { font-weight: 600; overflow-wrap: anywhere; }
  .by { color: var(--muted); font-size: 0.76rem; }
  .what { font-weight: 600; color: var(--danger); }
  .files { gap: 0.1rem; font-size: 0.78rem; }
  .files li, .test { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; font-size: 0.78rem; color: var(--text-2); }
  .broken { display: grid; gap: 0.15rem; }
  .contract { font-size: 0.8rem; color: var(--text-2); }
  .why, details pre { margin: 0.25rem 0 0; padding: 0.4rem 0.55rem; background: var(--surface); border-radius: var(--radius); font-size: 0.74rem; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 12rem; overflow: auto; }
  details summary { cursor: pointer; font-size: 0.78rem; color: var(--text-2); }
  .fix { color: var(--accent-ink); font-size: 0.8rem; overflow-wrap: anywhere; }
  .rest ul { gap: 0.15rem; margin-top: 0.3rem; font-size: 0.78rem; color: var(--text-2); }
  .rest li code, .test code, .files code { font-size: 0.76rem; overflow-wrap: anywhere; }
  .rest summary { display: flex; align-items: center; flex-wrap: wrap; gap: 0 0.4rem; }
  .rest summary::before { content: "▸"; font-size: 0.7rem; }
  .rest[open] summary::before { content: "▾"; }
  .rest summary::-webkit-details-marker { display: none; }
  .rest summary { list-style: none; }
  .sep { color: var(--muted); }
  .ok { color: var(--verified); display: inline-flex; align-items: center; gap: 0.25rem; }
  .note { font-size: 0.78rem; }
</style>
