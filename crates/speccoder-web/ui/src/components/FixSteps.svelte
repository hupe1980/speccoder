<script lang="ts">
  // A result's fix lines, each the way the host says it is taken: a
  // speccoder command runs or opens its view, the person's own command
  // opens its card (Run after a yes), a shell line or words are copied.
  // Nothing speccoder can do itself is only copied.
  import { app } from "../lib/app.svelte";
  import { prose as words, shown, verb } from "../lib/steps";
  import type { Step } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { steps, limit = 3 }: { steps: Step[]; limit?: number } = $props();
  const icon: Record<Step["how"], string> = { perform: "play", view: "chevron", command: "terminal", shell: "copy", words: "copy" };
  // A `→` line is words for a person: read, not a button.
  const prose = (s: Step) => s.how === "words" && !s.why && words(s.command);
</script>

<span class="steps">
  {#each steps.slice(0, limit) as s, i (i + s.command)}
    {#if prose(s)}
      <span class="hint">{shown(s.command)}</span>
    {:else}
      <button
        class="step {s.how}"
        title="{verb(s)}: {s.command}{s.how === 'words' || s.how === 'shell' ? (s.why ? `\n${s.why}` : '') : ''}"
        disabled={s.how === "perform" && !!app.job?.running}
        onclick={() => app.take(s)}
      >
        <span class="mono">{shown(s.command)}</span>
        <span class="i" aria-hidden="true"><Icon name={icon[s.how]} size={12} /></span>
        <span class="sr-only">{verb(s)}</span>
      </button>
    {/if}
  {/each}
</span>

<style>
  .steps { display: inline-flex; flex-wrap: wrap; gap: 0.35rem; }
  .step {
    display: inline-flex; align-items: center; gap: 0.45rem; max-width: 100%; height: 26px; padding: 0 0.4rem 0 0.65rem;
    border: 0; border-radius: 7px; background: var(--fill); cursor: pointer; font-size: 0.8rem; color: var(--text-2);
    transition: background var(--speed) var(--ease);
  }
  .step:hover { background: var(--fill-2); }
  .step.perform, .step.view { color: var(--accent); }
  .step.words, .step.shell { cursor: copy; }
  .step:disabled { opacity: 0.5; cursor: default; }
  .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .i { display: grid; place-items: center; width: 16px; height: 16px; color: var(--muted); }
  .hint { color: var(--muted); font-size: 0.8rem; }
</style>
