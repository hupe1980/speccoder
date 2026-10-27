// Clone, as a person meets it: the repositories signed in to, one
// confirmation naming github.com before anything leaves the machine.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    clone: true,
    welcome: { parent_shown: "~/Workspaces" },
    run: vi.fn(async (_a: unknown) => ({ status: "ok", message: "cloned" }) as unknown),
    toast: vi.fn(),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import Clone from "./Clone.svelte";

let c: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  host.app.run.mockClear();
  host.get.mockReset();
});

afterEach(() => {
  if (c) unmount(c);
  c = null;
  document.body.innerHTML = "";
});

describe("Clone", () => {
  it("lists the repositories, and clones only after naming github.com", async () => {
    host.get.mockResolvedValue({ login: "sam", repos: [{ slug: "sam/hello", description: "greetings", private: false, pushed: null }] });
    c = mount(Clone, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("As sam on github.com.");
    button(/sam\/hello/).click();
    flushSync();
    expect(document.querySelector<HTMLInputElement>("#cl-dest")!.value).toBe("~/Workspaces/hello");
    button("Clone").click();
    flushSync();
    expect(host.app.run).not.toHaveBeenCalled();
    expect(document.querySelector('[aria-label="Confirm"]')?.textContent).toContain("Clone sam/hello into ~/Workspaces/hello — this reaches github.com");
    expect(await violations()).toEqual([]);
    button("Clone").click();
    await settle();
    expect((host.app.run.mock.calls[0]![0] as { run: unknown }).run).toEqual({ kind: "clone_repository", repo: "sam/hello", target: "~/Workspaces/hello" });
  });

  it("signed out: says how to sign in, and a public repository can still be named", async () => {
    host.get.mockResolvedValue({ error: "not signed in to github.com", signed_in: false });
    c = mount(Clone, { target: document.body });
    await settle();
    expect(document.body.textContent).toContain("speccoder auth login");
    expect(button("Clone").disabled).toBe(true);
    type(document.querySelector<HTMLInputElement>("#cl-repo")!, "octo/hello");
    expect(button("Clone").disabled).toBe(false);
  });
});
