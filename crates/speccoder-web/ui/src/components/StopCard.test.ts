// A task that needs the person: the agent's reason as its claim, and
// three ways on in place — I did it, a hint typed right here, skip.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    perform: vi.fn(async (_a: unknown, _input?: string) => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import StopCard from "./StopCard.svelte";
import { stopOffers } from "../lib/actions";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  host.app.perform.mockClear();
});

const button = (text: string) => [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(text)) as HTMLButtonElement;

describe("StopCard", () => {
  it("shows the claim and takes each of the three actions", async () => {
    const actions = stopOffers("T019");
    shown = mount(StopCard, { target: document.body, props: { task: "T019", claim: "needs a GitHub repository", actions } });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("T019 needs you");
    expect(text).toContain("“needs a GitHub repository”");
    expect(text).toContain("a claim, not evidence");

    button("I Did It").click();
    expect(host.app.perform).toHaveBeenLastCalledWith(actions[0]);

    // Hint & retry: the note field inline, then the hint with its words.
    button("Hint & Retry").click();
    flushSync();
    const input = document.querySelector('input[aria-label="Hint for T019"]') as HTMLInputElement;
    expect(input).not.toBeNull();
    input.value = "the repository is octo/md2csv";
    input.dispatchEvent(new Event("input"));
    flushSync();
    (document.querySelector("form") as HTMLFormElement).dispatchEvent(new Event("submit", { cancelable: true }));
    await Promise.resolve();
    expect(host.app.perform).toHaveBeenLastCalledWith(actions[1], "the repository is octo/md2csv");
    expect(actions[1]!.run).toEqual({ kind: "hint", task: "T019" });

    button("Skip").click();
    expect(host.app.perform).toHaveBeenLastCalledWith(actions[2]);
  });

  it("says a [YOU] task is a step for a person", () => {
    shown = mount(StopCard, { target: document.body, props: { task: "T020", claim: null, actions: stopOffers("T020") } });
    flushSync();
    expect(document.body.textContent).toContain("a step only a person can take");
    expect(document.body.textContent).not.toContain("a claim");
  });
});
