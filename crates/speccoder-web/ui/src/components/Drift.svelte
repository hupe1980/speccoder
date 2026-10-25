<script lang="ts">
  // The spec changed under a tick: which requirement, word by word,
  // what the evidence was for and what the spec asks now — and the step
  // that checks the code against it again.
  import { app } from "../lib/app.svelte";
  import type { Drift } from "../lib/types";
  import { diffWords, wording } from "../lib/words";
  import Icon from "./Icon.svelte";

  let { drift }: { drift: Drift } = $props();
  const recheck = $derived(app.actions.find((a) => a.run?.kind === "converge"));
  const how = (then: string | null, now: string | null) => (!then ? "Added" : !now ? "Removed" : "Reworded");
</script>

{#if drift.requirements.length}
  <section class="drift" aria-label="The spec changed since this tick">
    <header>
      <span class="badge" aria-hidden="true"><Icon name="exclaim" size={15} weight={2.2} /></span>
      <div>
        <strong>The spec changed since this tick</strong>
        <span class="sub">Its evidence is for the earlier wording — it does not prove what the spec asks now.</span>
      </div>
    </header>
    {#each drift.requirements as r (r.id)}
      <div class="req">
        <span class="id mono">{r.id} <span class="how">{how(r.then, r.now)}</span></span>
        <p class="words">
          {#each diffWords(wording(r.then), wording(r.now)) as s, i (i)}<span class={s.kind}>{s.text}</span>{/each}
        </p>
      </div>
    {/each}
    {#if recheck}
      <div class="go">
        <button class="btn small primary" disabled={!!recheck.blocked || !!app.job?.running} title={`${recheck.blocked ?? recheck.why}\n$ ${recheck.command}`} onclick={() => app.perform(recheck)}>
          <Icon name="sparkle" size={11} weight={2.2} /> Re-check Against the Spec
        </button>
        <span class="cli mono">$ {recheck.command}</span>
      </div>
    {/if}
  </section>
{:else}
  <p class="soft"><Icon name="info" size={13} /> The {drift.files.length ? drift.files.map((f) => f.split("/").pop()).join(", ") : "spec, plan or constitution"} changed since this tick — not in what it cites.</p>
{/if}

<style>
  .drift { display: grid; gap: 0.7rem; padding: 0.85rem 0.95rem; border-radius: var(--radius); background: var(--attention-bg); }
  header { display: flex; gap: 0.65rem; align-items: flex-start; }
  .badge { width: 26px; height: 26px; border-radius: 8px; flex: none; display: grid; place-items: center; color: #fff; background: var(--attention-fill); }
  strong { display: block; font-weight: 650; }
  .sub { display: block; font-size: 0.82rem; color: var(--text-2); }
  .req { display: grid; gap: 0.2rem; padding: 0.55rem 0.7rem; border-radius: 9px; background: var(--surface); }
  .id { font-size: 0.76rem; font-weight: 650; }
  .how { font-family: var(--text-font, inherit); font-weight: 500; color: var(--muted); margin-left: 0.3rem; }
  .words { margin: 0; font-size: 0.88rem; line-height: 1.5; }
  .gone { color: var(--danger); text-decoration: line-through; text-decoration-thickness: 1.5px; opacity: 0.8; }
  .new { color: var(--text); background: color-mix(in srgb, var(--verified-fill) 22%, transparent); border-radius: 4px; padding: 0 0.12em; font-weight: 550; }
  .go { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .cli { font-size: 0.74rem; color: var(--muted); }
  .soft { display: flex; align-items: center; gap: 0.35rem; margin: 0 0.25rem; font-size: 0.8rem; color: var(--muted); }
</style>
