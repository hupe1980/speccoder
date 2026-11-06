<script lang="ts">
  // The sample, replayed: a banner that says what it is — a recorded run,
  // replayed strictly, no model called — and a tour of it: a few stops on
  // the real recorded run, each lighting one part of the screen with what
  // it is and why it matters. A divergence is said, never shown as if it
  // had worked. Nothing here runs anything.
  import { app } from "../lib/app.svelte";
  import { offer } from "../lib/actions";
  import type { SampleInfo } from "../lib/types";
  import TourSpot from "./TourSpot.svelte";

  let { sample }: { sample: SampleInfo } = $props();
  const diverged = $derived(sample.runs.find((r) => r.result !== "reproduced") ?? null);
  const name = $derived(sample.path.split("/").filter(Boolean).at(-1) ?? "the sample");

  type Stop = { view: "board" | "report"; task: string | null; tab?: "tour"; anchor: string; title: string; text: string };
  // The stops, from the sample's own report: what it shows, never more
  // (no verified task, no stop about one).
  const verified = $derived((app.report?.data?.tasks ?? []).find((t) => t.state === "verified")?.id ?? null);
  const stops = $derived<Stop[]>([
    { view: "board", task: null, anchor: '[data-tour="board"]', title: "One feature, every task", text: "The sample's spec became these tasks. A task is ticked only on evidence: ✓ verified means its tests went from red to green and nothing else broke." },
    ...(verified
      ? [
          { view: "board" as const, task: verified, anchor: '[data-tour-row="Acceptance"]', title: "Red first, then green", text: `speccoder wrote ${verified}'s tests from the spec before any code existed. They failed; the model's change made them pass. That is the tick.` },
          { view: "board" as const, task: verified, anchor: '[data-tour-row="Attestation"]', title: "Recorded, and checkable by anyone", text: "Every row comes from the attestation the tick left in git, signed when a key is configured. `speccoder attest verify`, or a pull request's evidence check, re-checks it without trusting the agent." },
          { view: "board" as const, task: verified, tab: "tour" as const, anchor: "ol.tour", title: "One task's whole story", text: "Requirement, frozen tests, red, the change, green, strength: the evidence as six steps, each saying where it comes from." },
        ]
      : []),
    { view: "report", task: null, anchor: '[data-tour="report"]', title: "The report", text: "Every task and the evidence behind its tick — the same as `speccoder report` in a terminal." },
  ]);

  let at = $state(-1);
  let playing = $state(false);
  let hidden = $state(false);
  let ended = $state(false);
  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Long enough to read a caption twice.
  const READ_MS = 7000;

  let timer: ReturnType<typeof setTimeout> | null = null;
  function clear() {
    if (timer) clearTimeout(timer);
    timer = null;
  }
  // Opens what stop `i` is about, the way a person would.
  async function show(i: number) {
    const s = stops[i];
    if (!s) return;
    at = i;
    ended = false;
    app.view = s.view;
    await app.selectTask(s.task);
    // The drawer renders after the task is chosen: its tab, once there.
    for (let i = 0; s.tab && i < 40; i++) {
      const tab = document.querySelector<HTMLButtonElement>('[data-tour="tab-tour"]');
      if (tab && !tab.disabled) {
        tab.click();
        break;
      }
      await new Promise((r) => setTimeout(r, 50));
    }
    // Its part of the screen, once loaded (the evidence is fetched): the
    // reading time starts when there is something to read.
    for (let i = 0; i < 60 && !document.querySelector(s.anchor); i++) {
      await new Promise((r) => setTimeout(r, 50));
    }
  }
  function finish() {
    clear();
    playing = false;
    at = -1;
    ended = true;
    void app.selectTask(null);
    app.view = "board";
  }
  function next() {
    if (at >= stops.length - 1) finish();
    else void show(at + 1);
  }
  function prev() {
    if (at > 0) void show(at - 1);
  }
  function tick() {
    clear();
    if (!playing) return;
    timer = setTimeout(async () => {
      if (at >= stops.length - 1) return finish();
      await show(at + 1);
      tick();
    }, READ_MS);
  }
  async function play() {
    if (diverged || !stops.length) return;
    playing = true;
    if (at < 0) await show(0);
    tick();
  }
  function pause() {
    playing = false;
    clear();
  }
  function end() {
    pause();
    at = -1;
  }
  function skip() {
    pause();
    finish();
  }
  $effect(() => () => clear());

  function key(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    if (typeof el?.closest === "function" && el.closest("input, textarea, select")) return;
    if (hidden) return;
    const touring = at >= 0;
    if (e.key === " " && !el?.closest?.("button")) {
      e.preventDefault();
      if (playing) pause();
      else play();
    } else if (e.key === "ArrowRight" && touring) {
      e.preventDefault();
      pause();
      next();
    } else if (e.key === "ArrowLeft" && touring) {
      e.preventDefault();
      pause();
      prev();
    } else if (e.key === "Escape" && touring) {
      e.preventDefault();
      end();
    } else if (e.key === "End") {
      e.preventDefault();
      skip();
    }
  }
  // A click outside the caption is the person taking over: the tour waits.
  function pointer(e: PointerEvent) {
    const el = e.target as HTMLElement | null;
    if (playing && !el?.closest?.(".caption")) pause();
  }
</script>

<svelte:window onkeydown={key} onpointerdown={pointer} />

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
          <button class="btn small primary" onclick={play} disabled={!stops.length}>{at >= 0 ? "Resume" : "Take the tour"} ▶ <kbd>Space</kbd></button>
        {/if}
        <button class="btn small" onclick={skip} disabled={!stops.length}>Skip to the end <kbd>End</kbd></button>
      </span>
    {/if}
    <button class="btn small ghost" onclick={() => (hidden = true)}>Close the sample</button>
  </div>
{/if}

{#if at >= 0 && !hidden && stops[at]}
  {@const s = stops[at]!}
  <TourSpot anchor={s.anchor} title={s.title} text={s.text} n={at + 1} of={stops.length} {playing} onprev={() => { pause(); prev(); }} onnext={() => { pause(); next(); }} ontoggle={() => (playing ? pause() : play())} onend={end} />
{/if}

{#if ended && !hidden && !diverged}
  <aside class="end glass" aria-label="The tour ended" class:still={reduced}>
    <p><strong>That was speccoder.</strong> Every tick above is a commit with its evidence.</p>
    <div class="row">
      <button class="btn primary" onclick={() => (app.newProject = {})}>Start your own</button>
      <button class="btn" disabled={!!sample.checks_missing} title={sample.checks_missing ?? "the sample's own checks, here, offline"} onclick={() => app.perform(offer({ kind: "sample_checks" }, { group: "work", label: "run the sample's checks", why: "offline, in the sample's copy", command: `speccoder verify -C ${sample.path}` }))}>Run the checks yourself</button>
      {#if sample.checks_missing}<span class="d-note">{sample.checks_missing}</span>{/if}
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
