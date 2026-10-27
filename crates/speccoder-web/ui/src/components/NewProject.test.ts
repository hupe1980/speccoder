// The New Project sheet as a person meets it: the host's plan, again on
// every change; Create disabled on a refusal; one inline confirmation for
// network steps only; Create bound to the digest shown; the checklist's
// states; the hand-off to the first feature.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";

const host = vi.hoisted(() => ({
  app: {
    newProject: {} as Record<string, unknown> | null,
    newFeature: null as unknown,
    welcome: { parent_shown: "~/Workspaces" } as Record<string, unknown> | null,
    project: null as Record<string, unknown> | null,
    job: null as unknown,
    steps: [] as unknown[],
    stepRunning: null as string | null,
    run: vi.fn(async (_a: unknown, _i?: string, _r?: string) => ({ status: "ok", data: {} }) as unknown),
    refresh: vi.fn(async () => {}),
    toast: vi.fn(),
  },
  get: vi.fn(),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get }));

import NewProject from "./NewProject.svelte";

const step = (id: string, argv: string[], access = "local", hosts: string[] = []) => ({ id, argv, cwd: "/w/md2csv", access, hosts, writes: "", limit_s: access === "network" ? 600 : 120 });
function plan(over: Record<string, unknown> = {}) {
  return {
    target: "/w/md2csv",
    name: "md2csv",
    mode: "new_folder",
    stack: { id: "rust", detected: false, reason: '"CLI" and cargo 1.91.0 is installed' },
    stacks: [
      { id: "rust", label: "Rust (cargo)", version: "1.91.0", disabled: null },
      { id: "python", label: "Python (uv)", version: null, disabled: "uv is not installed — install it: https://docs.astral.sh/uv/" },
      { id: "none", label: "I'll add code myself", version: null, disabled: null },
    ],
    format: "speckit",
    formats: [{ id: "speckit", label: "Spec Kit", version: "1.2.3", disabled: null }],
    extensions: [],
    preset: true,
    steps: [step("git_init", ["git", "init"]), step("scaffold.1", ["cargo", "init", "--lib", "--name", "md2csv"]), step("format_init", ["specify", "init", "--here"]), step("speccoder_init", ["speccoder", "init", "--yes"]), step("commit", ["git", "commit", "-m", "new project: md2csv"])],
    sandbox: { backend: "seatbelt" },
    identity: { name: "Sam Doe", email: "sam@example.com" },
    refusal: null,
    observed: { exists: false, entries: [], head: null, porcelain: [], tools: {} },
    digest: "sha256:3f9a1c2e00",
    ...over,
  };
}

let s: ReturnType<typeof mount> | null = null;
let current = plan();
async function wait() {
  await vi.advanceTimersByTimeAsync(350);
  await settle(10);
}

beforeEach(() => {
  vi.useFakeTimers();
  current = plan();
  host.app.run.mockClear();
  host.app.project = null;
  host.app.welcome = { parent_shown: "~/Workspaces" };
  host.app.newFeature = null;
  host.get.mockReset();
  host.get.mockImplementation(async (path: string) => {
    if (path.startsWith("/api/new/plan")) return { plan: current, choices: { idea: "a CLI", stack: null, format: "speckit", extensions: [], no_preset: false } };
    if (path.startsWith("/api/folders")) return { folders: [] };
    throw new Error(path);
  });
  host.app.newProject = {};
  s = mount(NewProject, { target: document.body });
  flushSync();
});

afterEach(() => {
  if (s) unmount(s);
  s = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("NewProject", () => {
  it("lists every command of the host's plan, its badge and the sandbox, after the idea", async () => {
    type(document.querySelector("textarea")!, "a CLI that turns Markdown tables into CSV");
    await wait();
    const asked = host.get.mock.calls.map((c) => String(c[0])).filter((p) => p.startsWith("/api/new/plan"));
    expect(asked.at(-1)).toContain("path=%7E%2FWorkspaces%2Fcli-turns-markdown-tables-csv");
    const rows = [...document.querySelectorAll(".d-steps li")].map((l) => l.querySelector("code")?.textContent);
    expect(rows).toEqual(["git init", "cargo init --lib --name md2csv", "specify init --here", "speccoder init --yes", 'git commit -m "new project: md2csv"']);
    expect(document.body.textContent).toContain("as Sam Doe");
    expect(document.body.textContent).toContain("Sandboxed (seatbelt)");
    expect(document.body.textContent).toContain("uv is not installed — install it");
    expect(document.querySelector<HTMLInputElement>('input[value="python"]')!.disabled).toBe(true);
    expect(await violations()).toEqual([]);
  });

  it("asks the host again on every change", async () => {
    type(document.querySelector("textarea")!, "a CLI");
    await wait();
    const before = host.get.mock.calls.length;
    document.querySelector<HTMLInputElement>('input[value="none"]')!.click();
    flushSync();
    await wait();
    const after = host.get.mock.calls.map((c) => String(c[0])).filter((p) => p.startsWith("/api/new/plan"));
    expect(host.get.mock.calls.length).toBeGreaterThan(before);
    expect(after.at(-1)).toContain("stack=none");
  });

  it("disables Create on a refusal and shows it with its fix", async () => {
    current = plan({ refusal: { code: "NEW-TARGET-NOT-EMPTY", message: "~/w/md2csv is not empty and not a git repository", fix: "→ choose an empty folder" } });
    type(document.querySelector("textarea")!, "a CLI");
    await wait();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain("NEW-TARGET-NOT-EMPTY");
    expect(document.body.textContent).toContain("choose an empty folder");
    expect(button(/^Create/).disabled).toBe(true);
  });

  it("creates the plan on screen, bound to its digest, without a second question when local", async () => {
    type(document.querySelector("textarea")!, "a CLI");
    await wait();
    button(/^Create/).click();
    await settle(10);
    expect(host.app.run).toHaveBeenCalledTimes(1);
    const a = host.app.run.mock.calls[0]![0] as { run: Record<string, unknown> };
    expect(a.run).toMatchObject({ kind: "new_project", target: "/w/md2csv", plan_digest: "sha256:3f9a1c2e00" });
  });

  it("confirms network steps once, naming them and their hosts", async () => {
    current = plan({ steps: [step("scaffold.1", ["uv", "init"]), step("scaffold.2", ["uv", "add", "--dev", "pytest"], "network", ["pypi.org"])] });
    type(document.querySelector("textarea")!, "a python script");
    await wait();
    expect(document.body.textContent).toContain("⇅ network: pypi.org");
    button(/^Create/).click();
    flushSync();
    expect(host.app.run).not.toHaveBeenCalled();
    const ask = document.querySelector('[aria-label="These reach the network"]')!;
    expect(ask.textContent).toContain("uv add --dev pytest");
    expect(ask.textContent).toContain("pypi.org");
    button("Run").click();
    await settle(10);
    expect(host.app.run).toHaveBeenCalledTimes(1);
  });

  it("shows each step's state as it runs, the failed one with its output and the ways out", async () => {
    type(document.querySelector("textarea")!, "a CLI");
    await wait();
    host.app.run.mockResolvedValueOnce({
      status: "error",
      code: "NEW-STEP-FAILED",
      message: "step 3 `specify init --here` exited 1; later steps did not run",
      data: { plan: current, results: [{ id: "git_init", state: "done", exit: 0, duration_ms: 40, output_tail: [] }, { id: "scaffold.1", state: "done", exit: 0, duration_ms: 400, output_tail: [] }, { id: "format_init", state: "failed", exit: 1, duration_ms: 90, output_tail: ["specify: no network"] }, { id: "speccoder_init", state: "waiting", exit: null, duration_ms: 0, output_tail: [] }, { id: "commit", state: "waiting", exit: null, duration_ms: 0, output_tail: [] }], undo: { available: false, kind: "none" } },
    });
    button(/^Create/).click();
    await settle(10);
    const states = [...document.querySelectorAll(".d-steps li .sr-only")].map((x) => x.textContent);
    expect(states).toEqual(["done", "done", "failed", "waiting", "waiting"]);
    expect(document.body.textContent).toContain("specify: no network");
    expect(document.querySelector('[role="alert"]')?.textContent).toContain("format_init failed");
    expect(button("Copy command")).toBeTruthy();
    expect(button("Retry")).toBeTruthy();
  });

  it("hands off to the first feature with the idea when done", async () => {
    type(document.querySelector("textarea")!, "a CLI that turns tables into CSV");
    await wait();
    host.app.run.mockResolvedValueOnce({ status: "ok", message: "md2csv is ready", data: { plan: current, results: [], init: { ran: [{ name: "test", passed: true, status: "passed" }], key: { set: true } }, commit: "1a2b3c4", undo: { available: true, kind: "trash" }, dry_run: false, first_feature: "a CLI that turns tables into CSV" } });
    button(/^Create/).click();
    await settle(10);
    expect(document.body.textContent).toContain("md2csv is ready — test passes");
    expect(document.body.textContent).toContain("committed 1a2b3c4 as Sam Doe");
    button("Write the first feature").click();
    await settle(10);
    expect(host.app.newFeature).toEqual({ text: "a CLI that turns tables into CSV", through: "tasks" });
  });
});
