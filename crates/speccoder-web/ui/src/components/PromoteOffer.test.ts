// A change that grew, offered where its task is: what was measured, and
// the two ways on — the same actions Needs You performs.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { violations } from "../test/dom";

const host = vi.hoisted(() => ({ app: { perform: vi.fn(async () => {}) } }));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import PromoteOffer from "./PromoteOffer.svelte";

const start = { label: "Start a spec from T001", command: "speccoder spec promote 002-quick-changes/T001" };
const later = { label: "Not now", command: "speccoder spec promote 002-quick-changes/T001 --decline" };
const item = { kind: "promote", group: "ready_to_review", feature: "002-quick-changes", task: "T001", title: "T001 grew past a quick change", detail: "11 files changed (more than 8)", since: null, actions: [start, later] };

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.perform.mockClear();
});
const button = (t: string) => [...document.querySelectorAll("button")].find((b) => b.textContent === t) as HTMLButtonElement;

describe("PromoteOffer", () => {
  it("names what was measured and performs the inbox's own actions", async () => {
    shown = mount(PromoteOffer, { target: document.body, props: { item: item as never } });
    flushSync();
    expect(document.body.textContent).toContain("T001 grew past a quick change");
    expect(document.body.textContent).toContain("11 files changed (more than 8)");
    expect(await violations()).toEqual([]);
    button("Start a spec").click();
    expect(host.app.perform).toHaveBeenLastCalledWith(start);
    button("Not now").click();
    expect(host.app.perform).toHaveBeenLastCalledWith(later);
  });

  it("lets its host start it its own way", () => {
    const onstart = vi.fn();
    shown = mount(PromoteOffer, { target: document.body, props: { item: item as never, onstart } });
    flushSync();
    button("Start a spec").click();
    expect(onstart).toHaveBeenCalled();
    expect(host.app.perform).not.toHaveBeenCalled();
  });
});
