// Your editor, one click away: a click opens the file at its
// line through the host and says where; with no editor yet the sheet asks
// once, saves the choice and the open goes on.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";
import { button, settle, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: { feature: "001-x", toast: vi.fn() },
  get: vi.fn(),
  post: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get, post: host.post }));

import OpenIn from "./OpenIn.svelte";
import ChooseEditor from "./ChooseEditor.svelte";
import { editor } from "../lib/editor.svelte";

const editors = {
  chosen: null,
  detected: "cursor",
  editors: [
    { id: "code", name: "Visual Studio Code", terminal: false, found: "/usr/local/bin/code" },
    { id: "cursor", name: "Cursor", terminal: false, found: "/usr/local/bin/cursor" },
    { id: "zed", name: "Zed", terminal: false },
    { id: "nvim", name: "Neovim", terminal: true, found: "/usr/bin/nvim" },
  ],
};

let shown: ReturnType<typeof mount>[] = [];
afterEach(() => {
  for (const s of shown) unmount(s);
  shown = [];
  document.body.innerHTML = "";
  editor.choosing = null;
  vi.clearAllMocks();
});

describe("OpenIn", () => {
  it("opens the file at its line in the feature's version and says where", async () => {
    host.post.mockResolvedValue({ status: "ok", message: "", data: { editor: "Visual Studio Code", path: "src/lib.rs", line: 42, where: "worktree" } });
    shown.push(mount(OpenIn, { target: document.body, props: { path: "src/lib.rs", line: 42 } }));
    button("Open src/lib.rs:42 in your editor").click();
    await settle();
    expect(host.post).toHaveBeenCalledWith("/api/editor/open", { path: "src/lib.rs", line: 42, column: null, feature: "001-x" });
    expect(host.app.toast).toHaveBeenCalledWith(expect.objectContaining({ tone: "ok", text: "Opened src/lib.rs:42 in Visual Studio Code" }));
    expect(await violations(document.body)).toEqual([]);
  });

  it("asks for an editor when none is chosen, then opens", async () => {
    host.post.mockResolvedValueOnce({ status: "needs_human", code: "EDITOR-NOT-FOUND", message: "no editor is chosen" });
    shown.push(mount(OpenIn, { target: document.body, props: { path: "a.rs", line: 3 } }));
    button("Open a.rs:3 in your editor").click();
    await settle();
    expect(editor.choosing).toEqual({ pending: { path: "a.rs", line: 3, column: null, feature: undefined } });

    host.get.mockResolvedValue({ status: "ok", message: "", data: editors });
    shown.push(mount(ChooseEditor, { target: document.body }));
    await settle();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Choose Your Editor");
    // Found window editors only, the detected one first; no terminal editor.
    const picks = [...document.querySelectorAll(".pick strong")].map((e) => e.textContent);
    expect(picks).toEqual(["Cursor", "Visual Studio Code"]);
    expect(text).not.toContain("Neovim");
    expect(text).toContain("Also known: Zed");
    expect(await violations(document.body)).toEqual([]);

    host.post.mockResolvedValueOnce({ status: "ok", message: "", data: { ...editors, chosen: "cursor" } });
    host.post.mockResolvedValueOnce({ status: "ok", message: "", data: { editor: "Cursor", path: "a.rs", line: 3, where: "checkout" } });
    (document.querySelector(".pick") as HTMLButtonElement).click();
    await settle();
    expect(host.post).toHaveBeenCalledWith("/api/editor", { editor: "cursor" });
    expect(editor.choosing).toBeNull();
    expect(host.post).toHaveBeenLastCalledWith("/api/editor/open", expect.objectContaining({ path: "a.rs", line: 3 }));
  });
});
