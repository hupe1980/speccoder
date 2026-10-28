// Fix lines as the page takes them, and what went wrong as one block to
// copy — the twin of the runtime's `copy::block`.

import { afterEach, describe, expect, it, vi } from "vitest";
import { copyBlock, segments, verb } from "./steps";
import { offer } from "./actions";

const api = vi.hoisted(() => ({
  get: vi.fn(async (_p: string): Promise<unknown> => ({ text: "### speccoder doctor\n" })),
  post: vi.fn(async (_p: string, _b: unknown): Promise<unknown> => ({ id: 7 })),
}));
vi.mock("./api", async (actual) => ({ ...(await actual<typeof import("./api")>()), get: api.get, post: api.post }));

import { app } from "./app.svelte";

const clip = { writeText: vi.fn(async (_t: string) => {}) };
Object.defineProperty(navigator, "clipboard", { value: clip, configurable: true });

afterEach(() => {
  vi.clearAllMocks();
  app.help = null;
  app.personStep = null;
  app.fixRun = null;
});

describe("what went wrong, to copy", () => {
  it("names the code, its fixes, the docs and where it happened", () => {
    const block = copyBlock(
      { message: "no API key for anthropic", code: "KEY-MISSING", fix: ["export ANTHROPIC_API_KEY=…", "→ or set it in Settings"] },
      { version: "0.9.0", os: "macos aarch64", run: "r-42" },
    );
    expect(block).toBe(
      "error[KEY-MISSING]: no API key for anthropic\n" +
        "fix: export ANTHROPIC_API_KEY=…\n" +
        "fix: or set it in Settings\n" +
        "docs: https://hupe1980.github.io/speccoder/docs/codes/#key-missing\n" +
        "speccoder 0.9.0 · macos aarch64 · run r-42\n",
    );
    // A run's payload names its run.
    expect(copyBlock({ message: "m", data: { run_id: "r-7" } }, { version: "1" })).toContain("· run r-7");
  });

  it("keeps a masked value masked", () => {
    const block = copyBlock({ message: "the provider said: [redacted:ANTHROPIC_API_KEY] is wrong", code: "PROVIDER-REFUSED" }, { version: "1" });
    expect(block).toContain("[redacted:ANTHROPIC_API_KEY]");
  });
});

describe("a command's output", () => {
  it("makes links and a login's device code clickable", () => {
    expect(segments("Open https://github.com/login/device and enter ABCD-1234 there")).toEqual([
      { text: "Open ", kind: "text" },
      { text: "https://github.com/login/device", kind: "url" },
      { text: " and enter ", kind: "text" },
      { text: "ABCD-1234", kind: "code" },
      { text: " there", kind: "text" },
    ]);
    expect(segments("")).toEqual([{ text: "", kind: "text" }]);
  });
});

describe("taking a step", () => {
  it("opens the doctor for `speccoder doctor`, never copies it", async () => {
    const action = offer(null, { group: "review", label: "check the setup", why: "", command: "speccoder doctor" });
    const shown = { ...action, step: { how: "view" as const, view: "doctor" as const, subject: null } };
    await app.perform(shown);
    expect(app.help).toEqual({ kind: "doctor" });
    expect(clip.writeText).not.toHaveBeenCalled();
  });

  it("performs a speccoder command through the job endpoint", async () => {
    await app.take({ command: "speccoder run --task T004", how: "perform", run: { kind: "run", task: "T004" }, input: null, text: null, confirm: null, feature: null });
    expect(api.post).toHaveBeenCalledWith("/api/run", expect.objectContaining({ run: { kind: "run", task: "T004" }, command: "speccoder run --task T004" }));
    expect(clip.writeText).not.toHaveBeenCalled();
  });

  it("runs the person's own command exactly as the host issued it, after the card", async () => {
    const how = { how: "command" as const, argvs: [["npm", "install"]], interactive: false, token: "f".repeat(64) };
    await app.take({ command: "npm install", ...how });
    expect(app.personStep?.command).toBe("npm install");
    expect(api.post).not.toHaveBeenCalled();
    await app.runFix();
    expect(api.post).toHaveBeenCalledWith("/api/fix/run", { argvs: [["npm", "install"]], token: "f".repeat(64) });
    expect(app.fixRun).toEqual({ id: 7, lines: [], result: null });
    expect(verb(how)).toBe("Run…");
  });

  it("never runs a command that asks questions, nor a shell line", async () => {
    await app.take({ command: "copilot login", how: "command", argvs: [["copilot", "login"]], interactive: true, token: "t" });
    await app.runFix();
    await app.take({ command: "export ANTHROPIC_API_KEY=…", how: "shell", why: "it must be set where speccoder starts", setting: "model" });
    await app.runFix();
    expect(api.post).not.toHaveBeenCalled();
    expect(app.personStep?.how.how).toBe("shell");
  });

  it("copies the bug report from `doctor --bug-report`", async () => {
    await app.take({ command: "speccoder doctor --bug-report", how: "view", view: "bug_report", subject: null });
    expect(api.get).toHaveBeenCalledWith("/api/bug-report");
    expect(clip.writeText).toHaveBeenCalledWith("### speccoder doctor\n");
  });
});
