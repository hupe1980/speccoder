// The page's state: what the host says, refreshed when a job ends or the
// feature changes, and what the running job is doing, from its events.

import { get, post, q, listen } from "./api";
import type { Action, Envelope, Project, Report, PendingCall, Presentation, Runnable, Document } from "./types";

export interface StageView {
  stage: string;
  state: string;
  detail: string | null;
  elapsed_ms: number | null;
}

export interface Waiting {
  id: number;
  type: "ask" | "question";
  presentation?: Presentation;
  question?: string;
}

export interface Toast {
  id: number;
  tone: "ok" | "warn" | "error";
  text: string;
  detail?: string;
}

class AppState {
  project = $state<Project | null>(null);
  feature = $state<string | null>(null);
  report = $state<Envelope<Report> | null>(null);
  actions = $state<Action[]>([]);
  approvals = $state<PendingCall[]>([]);
  documents = $state<Document[]>([]);
  /** The doctor's warnings and failures: what would stop or spoil a run. */
  preflight = $state<{ check: string; level: string; detail: string; fix: string[]; code?: string | null }[]>([]);
  connected = $state(false);
  loading = $state(true);

  job = $state<{ id: number; command: string; running: boolean; result: Envelope | null } | null>(null);
  task = $state<{ id: string; title: string } | null>(null);
  stages = $state<StageView[]>([]);
  calls = $state<string[]>([]);
  usage = $state<{ in: number; out: number; usd: number | null }>({ in: 0, out: 0, usd: null });
  waiting = $state<Waiting | null>(null);
  toasts = $state<Toast[]>([]);
  selected = $state<string | null>(null);
  /** The Code view: which mode, which file is open. */
  codeMode = $state<"changes" | "files">("changes");
  codeFile = $state<string | null>(null);
  codeLine = $state<number | null>(null);
  input = $state<{ action: Action } | null>(null);
  palette = $state(false);
  view = $state<"overview" | "board" | "code" | "documents" | "report" | "settings" | "setup">("board");

  #toast = 1;

  get primary(): Action | undefined {
    return this.actions.find((a) => a.group === "next" && a.run && !a.blocked);
  }

  /** Failing doctor checks that would stop a run before it does anything. */
  get blockers() {
    return this.preflight.filter((c) => c.level === "fail");
  }

  toast(t: Omit<Toast, "id">) {
    const id = this.#toast++;
    this.toasts = [...this.toasts, { ...t, id }];
    setTimeout(() => (this.toasts = this.toasts.filter((x) => x.id !== id)), 7000);
  }

  async refresh() {
    try {
      const f = this.feature ?? undefined;
      const [project, report, actions, approvals, documents] = await Promise.all([
        get<Project>("/api/project"),
        get<Envelope<Report>>(`/api/report${q({ feature: f })}`),
        get<Action[]>(`/api/actions${q({ feature: f, task: this.selected })}`),
        get<Envelope<{ pending: PendingCall[] }>>(`/api/approvals${q({ feature: f })}`),
        get<Document[]>(`/api/documents${q({ feature: f })}`),
      ]);
      void get<Envelope<{ checks: { check: string; level: string; detail: string; fix: string[]; code?: string | null }[] }>>(`/api/doctor${q({ feature: f })}`)
        .then((d) => (this.preflight = (d.data?.checks ?? []).filter((c) => c.level === "warn" || c.level === "fail")))
        .catch(() => {});
      this.project = project;
      this.report = report;
      this.actions = actions;
      this.approvals = approvals.data?.pending ?? [];
      this.documents = documents;
      if (!this.feature && project.features?.current) this.feature = project.features.current;
    } catch (e) {
      this.toast({ tone: "error", text: "The host did not answer", detail: String(e) });
    } finally {
      this.loading = false;
    }
  }

  async selectFeature(name: string) {
    this.feature = name;
    this.selected = null;
    await this.refresh();
  }

  async selectTask(id: string | null) {
    this.selected = id;
    const f = this.feature ?? undefined;
    this.actions = await get<Action[]>(`/api/actions${q({ feature: f, task: id })}`);
  }

  /** Runs `a`; asks for its input first when it takes one. */
  /** An action that leaves the machine, waiting for the person's yes. */
  confirming = $state<{ action: Action } | null>(null);

  async perform(a: Action, input?: string, confirmed = false) {
    if (!a.run) {
      await navigator.clipboard?.writeText(a.command).catch(() => {});
      this.toast({ tone: "ok", text: "Copied — run it in your shell", detail: a.command });
      return;
    }
    if (a.blocked) {
      this.toast({ tone: "warn", text: a.blocked });
      return;
    }
    if (a.confirm && !confirmed) {
      this.palette = false;
      this.confirming = { action: a };
      return;
    }
    this.confirming = null;
    if (a.input && input === undefined) {
      this.input = { action: a };
      return;
    }
    this.input = null;
    this.palette = false;
    try {
      await post("/api/run", { run: a.run as Runnable, input: input ?? null, feature: this.feature, command: a.command });
    } catch (e) {
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
    }
  }

  async answer(body: Record<string, unknown>) {
    if (!this.waiting) return;
    const id = this.waiting.id;
    try {
      await post("/api/answer", { id, ...body });
      this.waiting = null;
    } catch (e) {
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
    }
  }

  async stop() {
    await post("/api/stop", {});
  }

  #onEvent(e: any) {
    switch (e.type) {
      case "started":
        this.job = { id: e.id, command: e.command, running: true, result: null };
        this.stages = [];
        this.calls = [];
        this.usage = { in: 0, out: 0, usd: null };
        break;
      case "progress":
        this.#progress(e.event);
        break;
      case "ask":
      case "question":
        this.waiting = e as Waiting;
        this.#call(e.type === "ask" ? `${e.presentation?.request?.task ?? "A task"} needs you` : "A question for you", e.presentation?.title ?? e.question ?? "");
        break;
      case "answered":
        if (this.waiting?.id === e.id) this.waiting = null;
        break;
      case "done": {
        const r = e.result as Envelope;
        this.job = { id: e.id, command: this.job?.command ?? "", running: false, result: r };
        this.waiting = null;
        this.toast({
          tone: r?.status === "ok" ? "ok" : r?.status === "needs_human" ? "warn" : "error",
          text: r?.message ?? "done",
          detail: r?.code ? `${r.code}${r.fix?.length ? " · " + r.fix[0] : ""}` : undefined,
        });
        void this.refresh();
        break;
      }
    }
  }

  /** A run that waits for a person while they look elsewhere calls them:
   *  a desktop notification (when allowed) and a mark in the tab's title. */
  #call(title: string, body: string) {
    if (!document.hidden) return;
    try {
      if ("Notification" in window && Notification.permission === "granted") {
        const n = new Notification(title, { body, tag: "speccoder-ask", icon: "/icon.svg" });
        n.onclick = () => { window.focus(); n.close(); };
      }
    } catch {
      /* no notifications here (an embedded webview) */
    }
  }

  /** Asked once, by a click (browsers allow no other way). */
  async allowNotifications() {
    if ("Notification" in window && Notification.permission === "default") await Notification.requestPermission();
  }

  #progress(ev: any) {
    switch (ev.event) {
      case "task_started":
        this.task = { id: ev.task, title: ev.title };
        this.stages = [];
        break;
      case "stage": {
        const row = { stage: ev.stage, state: ev.state, detail: ev.detail ?? null, elapsed_ms: ev.elapsed_ms ?? null };
        const i = this.stages.findIndex((s) => s.stage === ev.stage);
        this.stages = i >= 0 ? this.stages.map((s, j) => (j === i ? row : s)) : [...this.stages, row];
        break;
      }
      case "tool_call":
        this.calls = [...this.calls.slice(-199), `turn ${ev.turn}  ${ev.tool} ${ev.target}  ${ev.decision}${ev.ok === false ? "  failed" : ""}`];
        break;
      case "usage":
        this.usage = { in: ev.input_tokens, out: ev.output_tokens, usd: ev.usd ?? null };
        break;
      case "task_finished":
        this.calls = [...this.calls, `${ev.task}: ${ev.outcome}`];
        void this.refresh();
        break;
      case "notice":
        this.calls = [...this.calls, ev.text];
        break;
    }
  }

  /** Where the page is, in its address: `?feature=…&view=…&task=…`, so a
   *  reload, a bookmark or the back button lands in the same place. */
  get location(): string {
    const s = new URLSearchParams();
    if (this.feature) s.set("feature", this.feature);
    if (this.view !== "board") s.set("view", this.view);
    if (this.selected) s.set("task", this.selected);
    if (this.view === "code" && this.codeMode === "files") s.set("mode", "files");
    if (this.view === "code" && this.codeFile) s.set("file", this.codeFile);
    if (this.view === "code" && this.codeFile && this.codeLine) s.set("line", String(this.codeLine));
    const t = s.toString();
    return location.pathname + (t ? `?${t}` : "");
  }

  #restore() {
    const s = new URLSearchParams(location.search);
    const view = s.get("view");
    if (view === "overview" || view === "board" || view === "code" || view === "documents" || view === "report" || view === "settings" || view === "setup") this.view = view;
    this.feature = s.get("feature") ?? this.feature;
    this.selected = s.get("task");
    this.codeFile = s.get("file");
    const line = Number(s.get("line"));
    this.codeLine = Number.isInteger(line) && line > 0 ? line : null;
    this.codeMode = s.get("mode") === "files" || this.codeFile ? "files" : "changes";
    this.palette = s.has("palette");
  }

  start() {
    this.#restore();
    addEventListener("popstate", () => {
      this.#restore();
      void this.refresh();
    });
    void this.refresh();
    void get<{ job: any; waiting: Waiting | null }>("/api/job").then((j) => {
      if (j.job) this.job = j.job;
      if (j.waiting) this.waiting = j.waiting;
    });
    listen(
      (e) => this.#onEvent(e),
      (c) => (this.connected = c),
    );
  }
}

export const app = new AppState();
