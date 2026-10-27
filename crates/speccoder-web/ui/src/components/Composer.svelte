<script lang="ts">
  // The ladder's front door: one field, the rungs beside it, one of them
  // suggested with its reason — computed by the host from the text alone,
  // never applied without the person's key or click. Taking a rung opens
  // that rung's own sheet with the text; the composer performs nothing.
  import { app } from "../lib/app.svelte";
  import { get, q } from "../lib/api";
  import { RUNGS, available, composerCommand, debounce, move, rungForKey } from "../lib/start";
  import { trap } from "../lib/sheet";
  import type { Reply, RungPick, RungSuggestion } from "../lib/types";
  import Icon from "./Icon.svelte";

  let text = $state(app.composer?.text ?? "");
  let suggestion = $state<RungSuggestion | null>(null);
  let selected = $state<RungPick | null>(null);
  let touched = $state(false);
  let invalid = $state(false);

  const create = $derived(app.project?.planning?.creates ?? null);
  const unit = $derived(app.project?.features?.current ?? app.feature ?? null);

  const ask = debounce(async (t: string) => {
    if (!t.trim()) {
      suggestion = null;
      if (!touched) selected = null;
      return;
    }
    try {
      const r = await get<Reply<RungSuggestion>>(`/api/suggest${q({ text: t })}`);
      suggestion = r.data ?? null;
      // The suggestion moves the selection until the person moves it.
      if (!touched && suggestion) selected = suggestion.available ? suggestion.rung : null;
    } catch {
      suggestion = null;
    }
  }, 300);
  $effect(() => {
    void ask(text);
    return () => ask.cancel();
  });

  function close() {
    app.composer = null;
  }

  function take(rung: RungPick | null) {
    const t = text.trim();
    if (!t) {
      invalid = true;
      return;
    }
    if (!rung || !available(rung)) return;
    app.composer = null;
    if (rung === "bug") app.bugSheet = { symptom: t };
    else if (rung === "quick") {
      app.quickDraft = t;
      app.quick = true;
    } else if (rung === "feature") app.newFeature = { text: t };
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      return close();
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      touched = true;
      selected = move(selected, e.key === "ArrowDown" ? 1 : -1);
      return;
    }
    const k = rungForKey(e);
    if (!k) return;
    e.preventDefault();
    take(k === "selected" ? selected : k);
  }

  const reason = $derived(
    invalid && !text.trim() ? "Describe the change first" : suggestion?.reason ?? "",
  );
  const footer = $derived(composerCommand(selected, text, create));
  async function copy() {
    if (!footer) return;
    await navigator.clipboard?.writeText(footer).catch(() => {});
    app.toast({ tone: "ok", text: "Copied", detail: footer });
  }
</script>

<div class="d-scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="d-sheet" role="dialog" aria-modal="true" aria-labelledby="cmp-title" use:trap={"textarea"}>
    <h2 id="cmp-title" class="sr-only">What do you want to change?</h2>
    <label class="d-field">
      <span class="d-label" aria-hidden="true">What do you want to change?</span>
      <textarea
        rows="2"
        bind:value={text}
        oninput={() => (invalid = false)}
        onkeydown={key}
        placeholder="Describe a change, a bug, a feature, or a question"
        aria-invalid={invalid}
        aria-describedby="cmp-reason"
        aria-label="Describe a change, a bug, a feature, or a question"
      ></textarea>
    </label>
    <div class="rungs" role="radiogroup" aria-label="Rung">
      {#each RUNGS as r (r.rung)}
        {@const on = selected === r.rung}
        {@const off = !available(r.rung)}
        <button
          type="button"
          role="radio"
          aria-checked={on}
          aria-disabled={off}
          class:on
          class:off
          tabindex={on || (!selected && r.rung === "bug") ? 0 : -1}
          title={off ? "Ask arrives with the Ask panel" : `${r.label} (${r.keys})`}
          onclick={() => {
            if (off) return;
            touched = true;
            selected = r.rung;
            take(r.rung);
          }}
          onkeydown={key}
        >
          <span class="dot" aria-hidden="true">{on ? "●" : "○"}</span>
          <span class="label">{r.label}</span>
          <span class="ceremony">{off ? "arrives with the Ask panel" : r.ceremony}</span>
          <kbd>{suggestion?.rung === r.rung && suggestion.available ? `⏎ ${r.keys}` : r.keys}</kbd>
          {#if off}<span class="sr-only">(disabled)</span>{/if}
        </button>
      {/each}
    </div>
    <p id="cmp-reason" class="reason" class:invalid={invalid && !text.trim()} aria-live="polite">{reason}</p>
    <div class="foot">
      <span class="where muted">{unit ? `In ${unit}` : ""}</span>
      {#if footer}
        <code class="cli">{footer}</code>
        <button class="btn icon ghost" aria-label="Copy the command" title="Copy the command" onclick={copy}><Icon name="copy" size={14} /></button>
      {/if}
    </div>
  </div>
</div>

<style>
  .rungs { display: grid; gap: 2px; }
  .rungs button {
    display: grid; grid-template-columns: 1.2rem 7.5rem 1fr auto; gap: 0.6rem; align-items: baseline; text-align: left;
    padding: 0.5rem 0.6rem; border: 0; border-radius: 9px; background: none; color: var(--text); cursor: pointer; font: inherit;
  }
  .rungs button:hover:not(.off) { background: var(--fill); }
  .rungs button.on { background: var(--accent-soft); }
  .rungs button.off { opacity: 0.5; cursor: default; }
  .rungs button:focus-visible { outline: none; box-shadow: 0 0 0 3px var(--focus); }
  .dot { color: var(--accent); }
  .label { font-weight: 600; }
  .ceremony { color: var(--text-2); font-size: 0.86rem; }
  .reason { margin: 0; min-height: 1.3em; font-size: 0.86rem; color: var(--text-2); }
  .reason.invalid { color: var(--danger); }
  .foot { display: flex; align-items: center; gap: 0.4rem; font-size: 0.8rem; }
  .where { margin-right: auto; }
  .cli { font: 0.78rem var(--mono); color: var(--muted); max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  @media (max-width: 640px) { .rungs button { grid-template-columns: 1.2rem 1fr auto; } .ceremony { grid-column: 2 / -1; } }
</style>
