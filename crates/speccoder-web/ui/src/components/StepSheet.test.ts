// The person's own step: the exact command, the folder, that it runs in
// the sandbox (or what it changes outside); Run streams its output with Cancel. A command
// that asks questions goes to a terminal; `export` stays in the shell.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

const host = vi.hoisted(() => ({
  app: {
    job: null as { running: boolean } | null,
    view: "board",
    project: { root: "/work/app", window: false, sandbox: true } as { root: string; window: boolean; sandbox: boolean },
    personStep: null as unknown,
    fixRun: null as unknown,
    runFix: vi.fn(async () => {}),
    openTerminal: vi.fn(async () => {}),
    stop: vi.fn(async () => {}),
    copy: vi.fn(async (_t: string) => {}),
    copyError: vi.fn(async (_r: unknown) => {}),
  },
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));

import StepSheet from "./StepSheet.svelte";
import FixSteps from "./FixSteps.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  vi.clearAllMocks();
  host.app.fixRun = null;
  host.app.project.window = false;
});
const button = (text: string) => [...document.querySelectorAll("button")].find((b) => b.textContent?.trim().includes(text));

describe("the person's step", () => {
  it("shows the exact command and where it runs, and runs it on Run", () => {
    host.app.personStep = { command: "npm install   # fill the cache", how: { how: "command", argvs: [["npm", "install"]], interactive: false, token: "t" } };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    const text = document.body.textContent ?? "";
    expect(text).toContain("Run this command?");
    expect(text).toContain("npm install");
    // What runs, never the line it was printed in.
    expect(text).not.toContain("fill the cache");
    expect(text).toContain("/work/app");
    expect(text).toContain("in the sandbox, with network");
    // One Enter never starts it: the focus is on Copy.
    expect((document.activeElement as HTMLElement | null)?.textContent?.trim()).toBe("Copy");
    button("Run")!.click();
    expect(host.app.runFix).toHaveBeenCalled();
  });

  it("says when it runs unsandboxed, and what it changes", () => {
    host.app.personStep = { command: "git config --global gpg.format ssh", how: { how: "command", argvs: [["git", "config", "--global", "gpg.format", "ssh"]], interactive: false, outside: "changes your global git config", token: "t" } };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    expect(document.body.textContent).toContain("It runs unsandboxed: it changes your global git config.");
    unmount(shown);
    host.app.project.sandbox = false;
    host.app.personStep = { command: "npm install", how: { how: "command", argvs: [["npm", "install"]], interactive: false, token: "t" } };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    expect(document.body.textContent).toContain("It runs unsandboxed: no OS sandbox here.");
    host.app.project.sandbox = true;
  });

  it("streams the output with Cancel, its links and codes clickable", () => {
    host.app.personStep = { command: "gh auth refresh", how: { how: "command", argvs: [["gh", "auth", "refresh"]], interactive: false, token: "t" } };
    host.app.fixRun = { id: 3, lines: [{ stream: "stdout", line: "Open https://github.com/login/device and enter ABCD-1234" }], result: null };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    expect(document.querySelector("a")?.getAttribute("href")).toBe("https://github.com/login/device");
    button("ABCD-1234")!.click();
    expect(host.app.copy).toHaveBeenCalledWith("ABCD-1234", "Copied the code", "ABCD-1234");
    expect(button("Run")).toBeUndefined();
    button("Cancel")!.click();
    expect(host.app.stop).toHaveBeenCalled();
  });

  it("offers a copy of what went wrong once it failed", () => {
    host.app.personStep = { command: "uv sync", how: { how: "command", argvs: [["uv", "sync"]], interactive: false, token: "t" } };
    host.app.fixRun = { id: 3, lines: [], result: { status: "error", message: "`uv sync` exited with 2", code: "FIX-FAILED" } };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    expect(document.body.textContent).toContain("It did not succeed");
    button("Copy What Went Wrong")!.click();
    expect(host.app.copyError).toHaveBeenCalled();
    expect(button("Run Again")).toBeDefined();
  });

  it("sends a sign-in to a terminal: the window opens one, a tab copies", () => {
    host.app.personStep = { command: "copilot login", how: { how: "command", argvs: [["copilot", "login"]], interactive: true, token: "t" } };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    expect(document.body.textContent).toContain("Run it in a terminal");
    expect(button("Open Terminal Here")).toBeUndefined();
    expect(button("Run")).toBeUndefined();
    unmount(shown);
    host.app.project.window = true;
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    button("Open Terminal Here")!.click();
    expect(host.app.openTerminal).toHaveBeenCalled();
  });

  it("says why `export` cannot run here and links the setting", () => {
    host.app.personStep = { command: "export ANTHROPIC_API_KEY=…", how: { how: "shell", why: "`ANTHROPIC_API_KEY` must be set where speccoder starts", setting: "model" } };
    shown = mount(StepSheet, { target: document.body });
    flushSync();
    expect(document.body.textContent).toContain("This one is for your shell");
    expect(document.body.textContent).toContain("must be set where speccoder starts");
    expect(button("Run")).toBeUndefined();
    button("Settings › Model")!.click();
    expect(host.app.view).toBe("settings");
  });
});

describe("fix lines", () => {
  it("are buttons that take each step; a person's words are text", () => {
    const take = vi.fn();
    (host.app as unknown as { take: typeof take }).take = take;
    const steps = [
      { command: "speccoder doctor", how: "view" as const, view: "doctor" as const, subject: null },
      { command: "→ re-run with -v", how: "words" as const, why: null },
    ];
    shown = mount(FixSteps, { target: document.body, props: { steps } });
    flushSync();
    button("speccoder doctor")!.click();
    expect(take).toHaveBeenCalledWith(steps[0]);
    expect(document.body.textContent).toContain("re-run with -v");
    expect(document.querySelectorAll("button").length).toBe(1);
  });
});
