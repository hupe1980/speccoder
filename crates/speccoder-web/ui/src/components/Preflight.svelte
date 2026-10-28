<script lang="ts">
  // What `doctor` would warn about, as a grouped list in Settings' manner:
  // each check with a coloured symbol badge, what is wrong, its fixes as
  // the host says they are taken, and a copy of what went wrong. Failures
  // first: they stop a run; warnings spoil one.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";
  import FixSteps from "./FixSteps.svelte";

  const items = $derived([...app.preflight].sort((a, b) => (a.level === "fail" ? 0 : 1) - (b.level === "fail" ? 0 : 1)));
  const failing = $derived(app.blockers.length > 0);

  const badge: Record<string, string> = {
    "model key": "key", untracked: "tray", signing: "signature", sandbox: "shield", git: "branch", "spec tool": "terminal",
    config: "doc", tests: "list", plan: "list", feature: "doc", project: "doc", state: "tray", instructions: "doc", lanes: "hand",
  };
  // A plan check reads its code's catalog title; any other check its name.
  const PLAN: Record<string, string> = {
    "PLAN-TESTS-UNCOLLECTED": "The plan puts tests where the runner won't collect them",
    "PLAN-BEHAVIOUR-EARLY": "A story's behaviour is planned before the story",
    "PLAN-TEST-ONLY-TASKS": "Tasks that only write tests",
  };
  const title = (i: { check: string; code?: string | null }) =>
    i.check === "lanes" ? "Who does what" : ((i.code && PLAN[i.code]) ?? i.check.charAt(0).toUpperCase() + i.check.slice(1));
  function openTask(t: string) {
    app.view = "board";
    void app.selectTask(t);
  }

</script>

{#if items.length}
  <section class="pre" aria-label="Before you run">
    <h3 class="section-title">{failing ? "Fix before running" : "Worth a look"}</h3>
    <ul class="grouped">
      {#each items as i (i.check + (i.code ?? "") + (i.tasks ?? []).join(","))}
        <li class={i.level}>
          <span class="badge" aria-hidden="true"><Icon name={badge[i.check] ?? "exclaim"} size={15} weight={2} /></span>
          <span class="what">
            <strong>{title(i)}<span class="sr-only">: {i.level}</span></strong>
            <span class="d"><Code text={i.detail} /></span>
            {#if i.tasks?.length}
              <span class="chips">{#each i.tasks as t (t)}<button class="chip mono" aria-label="Open {t}" onclick={() => openTask(t)}>{t}</button>{/each}</span>
            {/if}
          </span>
          <span class="fixes">
            {#if i.remedy}
              {@const r = i.remedy}
              <!-- What speccoder can fix itself is one click, its command a tooltip away. -->
              <button class="btn small tinted" title="{r.effect} — $ {r.command}" disabled={!!app.job?.running} onclick={() => app.perform(offer(r.run, { group: "work", label: r.label, why: r.effect, command: r.command }))}>
                <Icon name="check" size={11} weight={2.4} /> {r.label}
              </button>
            {:else}
              <FixSteps steps={i.fix_steps ?? []} limit={2} />
            {/if}
            {#if i.level !== "pass"}
              <button class="btn icon small ghost" title="Copy what went wrong" aria-label="Copy what went wrong: {title(i)}" onclick={() => app.copyError({ message: i.detail, code: i.code, fix: i.fix })}><Icon name="copy" size={12} /></button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .pre { display: grid; container-type: inline-size; }
  li { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 0.85rem; align-items: center; padding: 0.7rem 0.9rem; list-style: none; }
  ul { margin: 0; padding: 0; }
  .badge { width: 28px; height: 28px; border-radius: 7px; display: grid; place-items: center; color: #fff; box-shadow: inset 0 -1px 0 rgba(0, 0, 0, 0.12); }
  .fail .badge { background: linear-gradient(180deg, #ff6259, var(--danger-fill)); }
  .warn .badge { background: linear-gradient(180deg, #ffb340, var(--attention-fill)); }
  /* Who does what is information, never a warning. */
  .pass .badge { background: var(--fill-2); color: var(--text-2); box-shadow: none; }
  .what { display: grid; min-width: 0; }
  strong { font-weight: 600; }
  .d { color: var(--muted); font-size: 0.86rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.25rem; }
  .chip { border: 0; border-radius: 6px; padding: 0 0.4rem; background: var(--fill); color: var(--text-2); font-size: 0.74rem; cursor: pointer; }
  .chip:hover { background: var(--fill-2); }
  .fixes { display: flex; flex-wrap: wrap; gap: 0.35rem; justify-content: flex-end; max-width: 46ch; }
  @container (max-width: 720px) {
    li { grid-template-columns: auto minmax(0, 1fr); }
    .fixes { grid-column: 2; justify-content: flex-start; max-width: none; }
  }
</style>
