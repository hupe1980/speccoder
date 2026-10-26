<script lang="ts">
  // The model's account of an attempt, turn by turn: its reasoning (a
  // summary the provider gave) and its words beside its calls. A claim,
  // shown as one: set apart from the evidence, never as a verdict.
  import type { AccountTurn } from "../lib/types";
  let { turns }: { turns: AccountTurn[] } = $props();
  let open = $state<Record<number, boolean>>({});

  // A summary's last paragraph is where it lands: shown first.
  const paragraphs = (t: string) => t.split(/\n\s*\n|\n/).map((p) => p.trim()).filter(Boolean);
  const conclusion = (t: string) => paragraphs(t).at(-1) ?? t;
  const longer = (t: string) => paragraphs(t).length > 1 || t.length > 280;
</script>

<ol class="timeline">
  {#each turns as t (t.turn)}
    <li>
      <span class="node mono" aria-label="Turn {t.turn}">{t.turn}</span>
      <div class="turn">
        {#if t.reasoning}
          <div class="thought" class:open={open[t.turn]}>
            <span class="kicker"><span class="therefore" aria-hidden="true">∴</span> Thinking</span>
            {#if open[t.turn] || !longer(t.reasoning)}
              {#each paragraphs(t.reasoning) as p}<p>{p}</p>{/each}
            {:else}
              <p>{conclusion(t.reasoning)}</p>
            {/if}
            {#if longer(t.reasoning)}
              <button class="more" onclick={() => (open = { ...open, [t.turn]: !open[t.turn] })} aria-expanded={!!open[t.turn]}>
                {open[t.turn] ? "Show the conclusion" : "Show all of it"}
              </button>
            {/if}
          </div>
        {/if}
        {#if t.said}
          <blockquote class="said">{t.said}</blockquote>
        {/if}
      </div>
    </li>
  {/each}
</ol>

<style>
  .timeline { list-style: none; margin: 0.7rem 0 0; padding: 0; display: grid; gap: 0.85rem; max-height: 46vh; overflow: auto; position: relative; }
  li { display: grid; grid-template-columns: 1.5rem 1fr; gap: 0.6rem; position: relative; }
  /* The rail: turns read as one line of thought. */
  li:not(:last-child)::before {
    content: ""; position: absolute; left: calc(0.75rem - 1px); top: 1.55rem; bottom: -0.85rem; width: 2px;
    background: linear-gradient(var(--fill-2), transparent); border-radius: 2px;
  }
  .node {
    width: 1.5rem; height: 1.5rem; border-radius: 50%; display: grid; place-items: center;
    font-size: 0.68rem; font-weight: 650; color: var(--muted); background: var(--fill); box-shadow: inset 0 0 0 1px var(--border);
  }
  .turn { display: grid; gap: 0.45rem; min-width: 0; }
  .thought {
    position: relative; padding: 0.55rem 0.75rem 0.6rem; border-radius: 12px;
    background: linear-gradient(135deg, color-mix(in srgb, #bf5af2 7%, var(--surface-2)), color-mix(in srgb, #0a84ff 6%, var(--surface-2)));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, #bf5af2 16%, transparent);
  }
  .kicker { display: inline-flex; align-items: center; gap: 0.3rem; font-size: 0.7rem; font-weight: 650; letter-spacing: 0.02em; text-transform: uppercase; color: color-mix(in srgb, #bf5af2 70%, var(--muted)); }
  .therefore { font-size: 0.95rem; line-height: 1; text-transform: none; }
  .thought p { margin: 0.3rem 0 0; font-size: 0.86rem; line-height: 1.45; color: var(--text-2); }
  .more { margin-top: 0.35rem; border: 0; background: none; padding: 0; font: inherit; font-size: 0.76rem; font-weight: 600; color: var(--accent-ink); cursor: pointer; }
  .more:hover { text-decoration: underline; }
  .said {
    margin: 0; padding: 0.5rem 0.75rem; border-radius: 12px 12px 12px 4px; background: var(--surface-2);
    font-size: 0.86rem; font-style: italic; color: var(--text-2); white-space: pre-wrap; quotes: "“" "”";
  }
  .said::before { content: open-quote; }
  .said::after { content: close-quote; }
</style>
