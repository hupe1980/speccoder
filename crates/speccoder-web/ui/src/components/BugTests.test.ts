// A bug unit's Tests tab: the confirmed reproductions, read only.

import { afterEach, describe, expect, it } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import BugTests from "./BugTests.svelte";
import type { BugTests as Bugs } from "../lib/schema.gen";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
});

const bug: Bugs = {
  task: "T001",
  title: "Saving an empty title crashes",
  confirmation: { digest: "sha256:c0", triad_digest: "sha256:t", by: "Ada", via: "terminal", basis: "shown", at: 1793000000 },
  reproductions: [
    { statement: "E1", file: "tests/repro_e1.rs", test: "e1", red_class: "assertion", content: "#[test] fn e1() {}\n" },
    { statement: "U1", file: "tests/repro_u1.rs", test: "u1", red_class: "passed", content: null },
  ],
};

describe("BugTests", () => {
  it("lists each confirmed reproduction with its confirmation and offers nothing to change", () => {
    shown = mount(BugTests, { target: document.body, props: { bugs: [bug] } });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Confirmed by Ada");
    expect(text).toContain("red: assertion");
    expect(text).toContain("passes (guards what stays)");
    expect(text).toContain("#[test] fn e1() {}");
    expect(text).toContain("The confirmed copy is missing or changed");
    expect(document.querySelectorAll("button")).toHaveLength(0);
  });
});
