+++
title = "Tour"
description = "Forty-eight seconds of a recorded run: the tasks, red to green, the attestation, one task's evidence, the report."
weight = 15

[extra]
group = "Start"
+++

<video controls muted playsinline preload="metadata" style="width:100%;border-radius:12px" aria-label="The sample's tour, five stops"><source src="https://hupe1980.github.io/speccoder/tour.mp4" type="video/mp4"></video>

The sample's run, replayed: no model, no key. The same tour plays in the app — `speccoder sample`, then **Take the tour**.

1. **One feature, every task** — a spec became tasks, each ticked only on evidence ([the report](@/docs/evidence.md#the-report)).
2. **Red first, then green** — tests written from the spec failed before the code and pass after it ([what counts as red](@/docs/evidence.md#what-counts-as-red)).
3. **Recorded, and checkable by anyone** — the tick's attestation lives in git ([where the evidence lives](@/docs/evidence.md#where-the-evidence-lives), [the verifier](@/docs/verifier.md)).
4. **One task's whole story** — requirement, frozen tests, red, the change, green, strength ([the evidence tour](@/docs/verifier.md#the-evidence-tour)).
5. **The report** — every task and the evidence behind its tick (`speccoder report`).

A real project, from one line of intent to a pull request with a green evidence check: [speccoder-example-md2csv](https://github.com/hupe1980/speccoder-example-md2csv/pull/1).
