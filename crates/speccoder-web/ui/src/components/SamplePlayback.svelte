<script lang="ts">
  // The sample, replayed: a banner that says what it is — a recorded run,
  // replayed strictly, no model called — and a playback of its tasks in
  // the order they were judged: each task's evidence opens in turn, the
  // model's words labelled as its account. A divergence is said, never
  // shown as if it had worked. Nothing here runs anything.
  import { app } from "../lib/app.svelte";
  import { pace } from "../lib/start";
  import type { SampleInfo } from "../lib/types";

  let { sample }: { sample: SampleInfo } = $props();
  const diverged = $derived(sample.runs.find((r) => r.result !== "reproduced") ?? null);
  const name = $derived(sample.path.split("/").filter(Boolean).at(-1) ?? "the sample");
  // The tasks the recorded run judged, in the task list's order.
  const order = $derived((app.report?.data?.tasks ?? []).filter((t) => t.commit || t.state !== "ready").map((t) => t.id));
  let at = $state(-1);
  let playing = $state(false);
  let hidden = $state(false);
  const ended = $derived(order.length > 0 && at >= order.length - 1 && !playing);
  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  let timer: ReturnType<typeof setTimeout> | null = null;
  function step() {
    if (at >= order.length - 1) {
      playing = false;
      return;
    }
    at += 1;
    app.view = "board";
    void app.selectTask(order[at] ?? null);
  }
  function tick() {
    if (!playing) return;
    step();
    if (playing && at < order.length - 1) timer = setTimeout(tick, pace(order.length));
    else playing = false;
  }
  function play() {
    if (diverged || !order.length) return;
    if (at >= order.length - 1) at = -1;
    playing = true;
    tick();
  }
  function pause() {
    playing = false;
    if (timer) clearTimeout(timer);
  }
  function skip() {
    pause();
    at = order.length - 1;
    app.view = "board";
    void app.selectTask(order[at] ?? null);
  }
  $effect(() => () => {
    if (timer) clearTimeout(timer);
  });

  function key(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    if (typeof el?.closest === "function" && el.closest("input, textarea, select, button")) return;
    if (hidden) return;
    if (e.key === " ") {
      e.preventDefault();
      if (playing) pause();
      else play();
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      pause();
      step();
    } else if (e.key === "End") {
      e.preventDefault();
      skip();
    }
  }
</script>

<svelte:window onkeydown={key} />

{#if !hidden}
  <div class="banner" class:danger={!!diverged} role="status">
    {#if diverged}
      <span><strong>✗ This build cannot reproduce the recorded run — please report it.</strong> {diverged.finding ?? ""} The board shows the sample's committed state only.</span>
    {:else}
      <span>▶ A recorded run of <strong>{name}</strong>, replayed strictly — no model was called.{#if sample.recorded}{` Recorded ${sample.recorded.recorded_at} with ${sample.recorded.model}.`}{/if}</span>
      <span class="ctl">
        {#if playing}
          <button class="btn small" onclick={pause}>Pause <kbd>Space</kbd></button>
        {:else}
          <button class="btn small primary" onclick={play} disabled={!order.length}>Play ▶ <kbd>Space</kbd></button>
        {/if}
        <button class="btn small" onclick={skip} disabled={!order.length}>Skip to the end <kbd>End</kbd></button>
      </span>
    {/if}
    <button class="btn small ghost" onclick={() => (hidden = true)}>Close the sample</button>
  </div>
  <div class="sr-only" aria-live="polite">{at >= 0 && order[at] ? `${order[at]}: step ${at + 1} of ${order.length}` : ""}</div>
{/if}

{#if ended && !hidden && !diverged}
  <aside class="end glass" aria-label="The sample ended" class:still={reduced}>
    <p><strong>That was speccoder.</strong> Every tick above is a commit with its evidence.</p>
    <div class="row">
      <button class="btn primary" onclick={() => (app.newProject = {})}>Start your own</button>
      <button class="btn" title="speccoder verify, with the sample's checks: needs cargo" onclick={async () => { await navigator.clipboard?.writeText(`speccoder verify -C ${sample.path}`).catch(() => {}); app.toast({ tone: "ok", text: "Copied — the sample's checks need cargo", detail: `speccoder verify -C ${sample.path}` }); }}>Run the checks yourself</button>
      <button class="btn ghost" onclick={() => (hidden = true)}>Close the sample</button>
    </div>
  </aside>
{/if}

<style>
  .banner { display: flex; flex-wrap: wrap; align-items: center; gap: 0.6rem; padding: 0.55rem 0.9rem; margin-bottom: 1rem; border-radius: 10px; background: var(--running-bg); color: var(--text); font-size: 0.88rem; }
  .banner.danger { background: var(--danger-bg); }
  .banner .ctl { display: flex; gap: 0.4rem; margin-left: auto; }
  .end { position: fixed; right: 1.2rem; bottom: 2.4rem; z-index: 15; width: min(360px, calc(100vw - 2rem)); padding: 0.9rem 1rem; border-radius: var(--radius-lg); box-shadow: var(--shadow-lg); display: grid; gap: 0.6rem; animation: rise 300ms var(--spring); }
  .end.still { animation: none; }
  @keyframes rise { from { transform: translateY(10px); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .end { animation: none; } }
  .end p { margin: 0; font-size: 0.9rem; }
  .row { display: flex; flex-wrap: wrap; gap: 0.4rem; }
</style>
