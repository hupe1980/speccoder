<script lang="ts">
  // What `doctor` would warn about, as a grouped list in Settings' manner:
  // each check with a coloured symbol badge, what is wrong, and its fix as a
  // command to copy. Failures first: they stop a run; warnings spoil one.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import Icon from "./Icon.svelte";
  import Code from "./Code.svelte";

  const items = $derived([...app.preflight].sort((a, b) => (a.level === "fail" ? 0 : 1) - (b.level === "fail" ? 0 : 1)));
  const failing = $derived(app.blockers.length > 0);
  let copied = $state<string | null>(null);

  const badge: Record<string, string> = {
    "model key": "key", untracked: "tray", signing: "signature", sandbox: "shield", git: "branch", "spec tool": "terminal",
    config: "doc", tests: "list", plan: "list", feature: "doc", project: "doc", state: "tray", instructions: "doc",
  };
  // A fix line is a command, sometimes followed by "(why)" or prose.
  const command = (f: string) => f.replace(/^→\s*/, "").replace(/\s+\(.*\)$/, "");
  const runnable = (f: string) => /^[a-z][\w.-]*( |$)/.test(command(f)) && !/\b(set|add|configure)\b.*\bto\b/i.test(command(f));

  function copy(cmd: string) {
    navigator.clipboard?.writeText(cmd);
    copied = cmd;
    setTimeout(() => (copied === cmd ? (copied = null) : null), 1600);
  }
</script>

{#if items.length}
  <section class="pre" aria-label="Before you run">
    <h3 class="section-title">{failing ? "Fix before running" : "Worth a look"}</h3>
    <ul class="grouped">
      {#each items as i (i.check)}
        <li class={i.level}>
          <span class="badge" aria-hidden="true"><Icon name={badge[i.check] ?? "exclaim"} size={15} weight={2} /></span>
          <span class="what">
            <strong>{i.check.charAt(0).toUpperCase() + i.check.slice(1)}<span class="sr-only">: {i.level}</span></strong>
            <span class="d"><Code text={i.detail} /></span>
          </span>
          <span class="fixes">
            {#if i.remedy}
              {@const r = i.remedy}
              <!-- What speccoder can fix itself is one click, its command a tooltip away. -->
              <button class="btn small tinted" title="{r.effect} — $ {r.command}" disabled={!!app.job?.running} onclick={() => app.perform(offer(r.run, { group: "work", label: r.label, why: r.effect, command: r.command }))}>
                <Icon name="check" size={11} weight={2.4} /> {r.label}
              </button>
            {:else}
            {#each i.fix.slice(0, 2) as f}
              {#if runnable(f)}
                <button class="fix" title="Copy: {command(f)}" onclick={() => copy(command(f))}>
                  <span class="mono">{command(f)}</span>
                  <span class="copy">{#if copied === command(f)}<Icon name="check" size={13} />{:else}<Icon name="copy" size={13} />{/if}</span>
                </button>
              {:else}
                <span class="hint">{f.replace(/^→\s*/, "")}</span>
              {/if}
            {/each}
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
  .what { display: grid; min-width: 0; }
  strong { font-weight: 600; }
  .d { color: var(--muted); font-size: 0.86rem; }
  .fixes { display: flex; flex-wrap: wrap; gap: 0.35rem; justify-content: flex-end; max-width: 46ch; }
  .fix {
    display: inline-flex; align-items: center; gap: 0.45rem; max-width: 100%; height: 26px; padding: 0 0.35rem 0 0.65rem;
    border: 0; border-radius: 7px; background: var(--fill); cursor: copy; font-size: 0.8rem; color: var(--text-2);
    transition: background var(--speed) var(--ease);
  }
  .fix:hover { background: var(--fill-2); }
  .fix .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .copy { display: grid; place-items: center; width: 18px; height: 18px; color: var(--muted); }
  .hint { color: var(--muted); font-size: 0.8rem; }
  @container (max-width: 720px) {
    li { grid-template-columns: auto minmax(0, 1fr); }
    .fixes { grid-column: 2; justify-content: flex-start; max-width: none; }
  }
</style>
