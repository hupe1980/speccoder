// The model's account: a turn's thinking and words, and the attempt's
// last turn — how it ended, in its words.

import { afterEach, describe, expect, it } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Account from "./Account.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

describe("Account", () => {
  it("ends in how the attempt ended, apart from the words beside its calls", () => {
    shown = mount(Account, {
      target: document.body,
      props: {
        turns: [
          { turn: 1, reasoning: null, said: "Reading the parser first.", answer: null },
          { turn: 4, reasoning: null, said: null, answer: "Added the parser and its tests." },
        ],
      },
    });
    flushSync();
    const [first, last] = [...document.querySelectorAll("li")];
    expect(document.querySelectorAll("li")).toHaveLength(2);
    expect(first!.querySelector(".said")?.textContent).toBe("Reading the parser first.");
    expect(first!.querySelector(".answer")).toBeNull();
    expect(last!.querySelector(".answer .kicker")?.textContent).toBe("How it ended");
    expect(last!.querySelector(".answer blockquote")?.textContent).toBe("Added the parser and its tests.");
  });
});
