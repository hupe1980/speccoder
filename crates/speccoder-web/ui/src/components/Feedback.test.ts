// Feedback between projects: what this project asks with each item's
// status, a prepared issue for a project only on GitHub, and what others
// ask of it — taken up, recorded as a bug, or declined with a reason.

import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { button, settle, type, violations } from "../test/dom";

const report = {
  upstreams: [
    { name: "agentplane", path: "/c/agentplane", outbox: "concepts/FEEDBACK_AGENTPLANE.md", exists: true, items: [
      { id: "AP-43", title: "a stop that halts without unwinding", text: "Ask: a halt.", digest: "1", status: { state: "fixed", commit: "3b2c1f0", by_agent: true }, changed: false },
      { id: "AP-44", title: "journal export", text: "As JSON.", digest: "2", status: { state: "not_seen" }, changed: false },
    ] },
    { name: "procage", repo: "you/procage", outbox: ".speccoder/outbox/procage.md", exists: true, items: [
      { id: "FB-1", title: "a proxy for the network", text: "Allow hosts.", digest: "3", status: { state: "not_seen" }, changed: false },
    ] },
  ],
  requests: [
    { source: "web", path: "/c/web", id: "AP-1", title: "retry on 429", text: "Please retry.", digest: "4", status: { state: "not_seen" }, changed: false },
  ],
};
const host = vi.hoisted(() => ({
  app: { job: null, project: { name: "speccoder" }, newFeature: null as unknown, bugSheet: null as unknown, pendingFeedback: null as unknown, toast: vi.fn() },
  get: vi.fn(),
  post: vi.fn(async () => ({ ok: true })),
}));
vi.mock("../lib/app.svelte", () => ({ app: host.app }));
vi.mock("../lib/api", async (actual) => ({ ...(await actual<typeof import("../lib/api")>()), get: host.get, post: host.post }));

import Feedback from "./Feedback.svelte";

let shown: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (shown) unmount(shown);
  shown = null;
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("Feedback", () => {
  it("shows each item's status and offers a prepared issue only for a project on GitHub", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: report });
    shown = mount(Feedback, { target: document.body });
    await settle();
    const text = document.body.textContent ?? "";
    for (const s of ["Asked of agentplane", "fixed in 3b2c1f0 (by an agent)", "not seen yet", "Asked of procage", "Asked of this project"]) expect(text).toContain(s);
    // agentplane is local without a repo: no issue to prepare; its fixed item can be withdrawn.
    expect(document.querySelectorAll("li")[0]!.textContent).not.toContain("Open on GitHub");
    expect(document.querySelectorAll("li")[0]!.textContent).toContain("Adopted — Withdraw");
    expect(button(/Open on GitHub/)).toBeTruthy();
    expect(await violations()).toEqual([]);
  });

  it("files an issue only after showing exactly what is sent", async () => {
    host.get.mockImplementation(async (url: string) =>
      url.startsWith("/api/feedback/prepared") ? { ok: true, prepared: { repo: "you/procage", title: "a proxy for the network", body: "Allow hosts.\n\n---\n_From speccoder_", url: "https://github.com/you/procage/issues/new" } } : { status: "ok", message: "", data: report });
    host.post.mockResolvedValue({ ok: true, number: 57, url: "https://github.com/you/procage/issues/57" } as never);
    shown = mount(Feedback, { target: document.body });
    await settle();
    button(/File Issue/).click();
    await settle();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Allow hosts.");
    expect(host.post).not.toHaveBeenCalled();
    button("File Issue").click();
    await settle();
    expect(host.post).toHaveBeenCalledWith("/api/feedback/issue", { upstream: "procage", id: "FB-1" });
  });

  it("takes a request up as a spec, or declines it with a reason", async () => {
    host.get.mockResolvedValue({ status: "ok", message: "", data: report });
    shown = mount(Feedback, { target: document.body });
    await settle();
    button("Start a Spec").click();
    // The asking project's words, quoted; decided only when the sheet is submitted.
    expect((host.app.newFeature as { text: string }).text).toContain("web asks (AP-1), in its own words:\n\n> Please retry.");
    expect(host.app.pendingFeedback).toEqual({ source: "/c/web", id: "AP-1", decision: "taken_up" });
    expect(host.post).not.toHaveBeenCalled();
    button("Decline…").click();
    flushSync();
    expect(button("Decline").disabled).toBe(true);
    type(document.querySelector<HTMLInputElement>("input[aria-label='Why it is declined']")!, "out of scope");
    button("Decline").click();
    await settle();
    expect(host.post).toHaveBeenLastCalledWith("/api/feedback/decide", { source: "/c/web", id: "AP-1", decision: "declined", reason: "out of scope" });
  });
});
