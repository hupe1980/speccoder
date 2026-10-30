// The page's state: what the host says, refreshed when a job ends or the
// feature changes, and what the running job is doing, from its events.

import { get, post, q, listen } from "./api";
import { KINDS, kindOf, offer, type Offer } from "./actions";
import { copyBlock, labelOf, render, shown, type Wrong } from "./steps";
import type { Action, AgentCard, LiveAgent, Reply, Inbox, LiveRun, Project, Report, PendingCall, Presentation, Runnable, Document, Remedy, PlanningChainReport, PlanStepResult, Through, WelcomeView, How, Step, View } from "./types";

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
  /** What went wrong, as a block to copy (an error's Copy button). */
  copy?: string;
}

/** The person's own command, or a line for their shell, waiting in its
 *  card: what the host issued, exactly. */
export type PersonStep = { command: string; how: Extract<How, { how: "command" } | { how: "shell" }> };

/** The person's command running from the card: its job, its output, its end. */
export interface FixRun {
  id: number;
  lines: { stream: string; line: string }[];
  result: Reply | null;
}

class AppState {
  project = $state<Project | null>(null);
  feature = $state<string | null>(null);
  report = $state<Reply<Report> | null>(null);
  actions = $state<Action[]>([]);
  approvals = $state<PendingCall[]>([]);
  /** Which document the Spec & Plan view shows (`review`: the review cards). */
  doc = $state("spec");
  /** The section of the shown document, by its outline id. */
  section = $state<string | null>(null);
  /** Every decision waiting for the person, across features. */
  inbox = $state<Reply<Inbox> | null>(null);
  documents = $state<Document[]>([]);
  /** The doctor's warnings and failures: what would stop or spoil a run. */
  preflight = $state<{ check: string; level: string; detail: string; fix: string[]; fix_steps?: Step[]; code?: string | null; remedy?: Remedy; tasks?: string[] }[]>([]);
  /** Every doctor check (the status bar reads the model and the sandbox). */
  doctor = $state<{ check: string; level: string; detail: string; fix?: string[]; fix_steps?: Step[]; code?: string | null }[]>([]);
  /** The person's own command (or shell line) in its card. */
  personStep = $state<PersonStep | null>(null);
  /** That command, running or done. */
  fixRun = $state<FixRun | null>(null);
  /** A sheet of the doctor's checks, or of one code (`explain`). */
  help = $state<{ kind: "doctor" } | { kind: "explain"; code: string } | null>(null);
  /** The quick-change sheet is open. */
  quick = $state(false);
  /** What the quick-change sheet opens with (typed in the palette). */
  quickDraft = $state("");
  /** The Bug sheet is open: a new bug (prefilled from an issue), or the
   *  bug `<unit>/<task>` it shows. */
  bugSheet = $state<{ key?: string; symptom?: string; issue?: number } | null>(null);
  /** The composer is open, with what it opens with. */
  composer = $state<{ text?: string } | null>(null);
  /** The New Feature sheet is open: prefilled text, the source a hand-off
   *  supplied, the *Plan it* choice. */
  newFeature = $state<{ text?: string; source?: { kind: "issue"; number: number; title: string } | null; through?: Through } | null>(null);
  /** The New Project sheet is open (`existing`: the folder is the host's own). */
  newProject = $state<{ idea?: string; path?: string; existing?: boolean } | null>(null);
  /** The Clone sheet is open. */
  clone = $state(false);
  /** The keys (`?`) are shown. */
  keys = $state(false);
  /** The host serves no project yet: the welcome. */
  welcome = $state<WelcomeView | null>(null);
  /** Where the last plan through stopped (the job's result; the stop is
   *  derived from the files again on the next one). */
  chain = $state<PlanningChainReport | null>(null);
  /** A plan through is writing a new unit: follow it when it appears. */
  following = $state(false);
  /** A new project's steps as they run: the finished ones, the running one. */
  steps = $state<PlanStepResult[]>([]);
  stepRunning = $state<string | null>(null);
  /** An open context menu: where, and its items. */
  context = $state<{ x: number; y: number; title: string; items: { label: string; icon?: string; disabled?: string | null; hint?: string; act: () => void }[] } | null>(null);
  /** Whether the sidebar and the status bar are shown (View menu). */
  sidebar = $state(true);
  statusbar = $state(true);
  connected = $state(false);
  loading = $state(true);

  job = $state<{ id: number; command: string; running: boolean; result: Reply | null } | null>(null);
  task = $state<{ id: string; title: string } | null>(null);
  stages = $state<StageView[]>([]);
  calls = $state<string[]>([]);
  /** What the model said last, its first line: a claim. */
  said = $state<string | null>(null);
  /** The external agent making this job's attempt, when one is. */
  agent = $state<LiveAgent | null>(null);
  /** Every agent's card (`doctor --agents`); `probed`: installed ones were
   *  asked for a session. */
  agents = $state<{ cards: AgentCard[]; probed: boolean } | null>(null);
  /** The agent this browser last chose to run with in this project
   *  (`run --agent`); `null`: the configured executor. */
  runWith = $state<string | null>(null);
  /** The Settings section to open next (`executor` from a card's fix). */
  settingsPick = $state<string | null>(null);
  /** The project's current run from any process (a terminal's included). */
  live = $state<LiveRun | null>(null);
  /** Another process runs this project now: it holds the worktree. */
  get elsewhere() {
    return !!(this.live?.running && !this.live.own && this.live.live);
  }
  /** A run whose process is gone before it finished: resumable. */
  get interrupted() {
    return !!(this.live?.interrupted && this.live.live && !this.job?.running);
  }
  /** Where the model's latest reasoning summary landed, its last line: a claim. */
  thinking = $state<string | null>(null);
  usage = $state<{ in: number; out: number; usd: number | null }>({ in: 0, out: 0, usd: null });
  /** Writing a requirement's tests at review: the stage it is in. */
  oracleStage = $state<{ requirement: string; stage: string; n: number; of: number } | null>(null);
  waiting = $state<Waiting | null>(null);
  toasts = $state<Toast[]>([]);
  selected = $state<string | null>(null);
  /** The Code view: which mode, which file is open. */
  codeMode = $state<"changes" | "files">("changes");
  codeFile = $state<string | null>(null);
  codeLine = $state<number | null>(null);
  input = $state<{ action: Action | Offer } | null>(null);
  palette = $state(false);
  view = $state<"inbox" | "overview" | "board" | "code" | "documents" | "report" | "settings" | "setup">("inbox");

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
      if (this.welcome || !this.project) {
        // A host outside a project answers 409 until it is bound.
        try {
          await get<Project>("/api/project");
          this.welcome = null;
        } catch (e) {
          if ((e as { status?: number }).status === 409) {
            this.welcome = await get<WelcomeView>("/api/welcome");
            return;
          }
          throw e;
        }
      }
      const f = this.feature ?? undefined;
      const [project, report, actions, approvals, documents] = await Promise.all([
        get<Project>("/api/project"),
        get<Reply<Report>>(`/api/report${q({ feature: f })}`),
        get<Action[]>(`/api/actions${q({ feature: f, task: this.selected })}`),
        get<Reply<{ pending: PendingCall[] }>>(`/api/approvals${q({ feature: f })}`),
        get<Document[]>(`/api/documents${q({ feature: f })}`),
      ]);
      void get<Reply<Inbox>>("/api/inbox")
        .then((i) => {
          this.inbox = i;
          this.#entered(i.data?.items ?? []);
        })
        .catch(() => {});
      void get<Reply<{ checks: { check: string; level: string; detail: string; fix: string[]; code?: string | null }[] }>>(`/api/doctor${q({ feature: f })}`)
        .then((d) => {
          this.doctor = d.data?.checks ?? [];
          this.preflight = this.doctor.filter((c) => c.level === "warn" || c.level === "fail" || c.check === "lanes") as typeof this.preflight;
        })
        .catch(() => {});
      this.project = project;
      this.#loadRunWith();
      this.report = report;
      this.actions = actions;
      this.approvals = approvals.data?.pending ?? [];
      this.documents = documents;
      if (!this.feature && project.features?.current) this.feature = project.features.current;
      // A plan through made a new unit current: the page follows it.
      if (this.following && project.features?.current && project.features.current !== this.feature) {
        this.feature = project.features.current;
        this.view = "documents";
        this.doc = "spec";
        void this.refresh();
      }
    } catch (e) {
      this.toast({ tone: "error", text: "The host did not answer", detail: String(e) });
    } finally {
      this.loading = false;
    }
  }

  /** The agents' cards: what is known without starting them, then (with
   *  `probe`) after asking each installed one for a session. */
  async loadAgents(probe: boolean) {
    if (this.agents?.probed || (!probe && this.agents)) return this.agents?.cards ?? [];
    const r = await get<Reply<{ agents?: AgentCard[] }>>(`/api/agents${q({ probe: String(probe) })}`);
    this.agents = { cards: r.data?.agents ?? [], probed: probe };
    return this.agents.cards;
  }

  #runWithKey() {
    return `speccoder.runWith.${this.project?.root ?? ""}`;
  }
  #loadRunWith() {
    try {
      this.runWith = localStorage.getItem(this.#runWithKey());
    } catch {
      this.runWith = null;
    }
  }
  /** Remembers the agent to run with here (this browser only). */
  chooseRunWith(id: string | null) {
    this.runWith = id;
    try {
      if (id) localStorage.setItem(this.#runWithKey(), id);
      else localStorage.removeItem(this.#runWithKey());
    } catch {
      /* no storage here: it holds for this page only */
    }
  }
  /** `a` with the agent chosen to run with, when it is a run. */
  withAgent<T extends { run?: Runnable | null; command: string; label: string }>(a: T): T {
    if (!this.runWith || a.run?.kind !== "run" || a.run.agent) return a;
    const name = this.agents?.cards.find((c) => c.id === this.runWith)?.name ?? this.runWith;
    return { ...a, run: { ...a.run, agent: this.runWith }, command: `${a.command} --agent ${this.runWith}`, label: `${a.label} with ${name}` };
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
  confirming = $state<{ action: Action | Offer } | null>(null);
  /** A stop speccoder knows the fix for: offered until done or dismissed,
   *  then what stopped runs again. */
  remedy = $state<{ remedy: Remedy; message: string; code?: string; then: Action | Offer | null } | null>(null);
  #last: Action | Offer | null = null;
  #fixing: Action | Offer | null | undefined = undefined;

  /** Applies the remedy offered; once it holds, runs again what it stopped. */
  async applyRemedy() {
    const r = this.remedy;
    if (!r) return;
    this.remedy = null;
    this.#fixing = r.then;
    try {
      await post("/api/run", { run: r.remedy.run, input: null, feature: this.feature, command: r.remedy.command });
    } catch (e) {
      this.#fixing = undefined;
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
    }
  }

  async perform(a: Action | Offer, input?: string, confirmed = false): Promise<void> {
    if (!a.run) {
      // The host says how: a view, the person's own command, their shell.
      if (a.step) return this.take({ command: a.command, ...a.step } as Step);
      await this.copy(a.command, "Copied", a.command);
      return;
    }
    if (a.blocked) {
      this.toast({ tone: "warn", text: a.blocked });
      return;
    }
    // One run per project: another process holds the worktree and journal.
    if (this.elsewhere && KINDS[kindOf(a.run)].holdsWorktree) {
      const l = this.live!.live!;
      this.toast({ tone: "warn", text: `${l.task} is running in another speccoder (pid ${l.pid})`, detail: "It holds this project; wait for it, or stop it there." });
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
    this.#last = a;
    this.remedy = null;
    try {
      await post("/api/run", { run: a.run as Runnable, input: input ?? null, feature: this.feature, command: a.command });
    } catch (e) {
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
    }
  }

  /** Takes a fix line or a next step the way the host says: performs it,
   *  opens its view, shows the person's own command in its card, or copies
   *  words. A speccoder command is never only copied. */
  async take(step: Step): Promise<void> {
    switch (step.how) {
      case "perform": {
        if (step.feature && step.feature !== this.feature) await this.selectFeature(step.feature);
        const a = offer(step.run, { group: "fix", label: labelOf(step.command), why: "", command: step.command, confirm: step.confirm, input: step.input });
        return this.perform(a, step.text ?? undefined);
      }
      case "view":
        return this.open(step.view, step.subject);
      case "command":
      case "shell":
        this.fixRun = null;
        this.personStep = { command: step.command, how: step };
        return;
      case "words":
        return this.copy(shown(step.command), "Copied", step.why ?? undefined);
    }
  }

  /** Opens what a read-only speccoder command shows. */
  async open(view: View, subject?: string | null): Promise<void> {
    this.palette = false;
    switch (view) {
      case "doctor": this.help = { kind: "doctor" }; return;
      case "bug_report": return this.copyBugReport();
      case "explain": this.help = subject ? { kind: "explain", code: subject } : { kind: "doctor" }; return;
      case "report": this.view = "report"; return;
      case "live": this.view = "board"; return;
      case "approvals": this.view = "inbox"; return;
      case "review": this.doc = "review"; this.view = "documents"; return;
      case "settings": this.view = "settings"; return;
      case "setup": this.view = "setup"; return;
      case "bug": this.bugSheet = {}; return;
      case "quick_change": this.quick = true; return;
      case "new_feature": this.newFeature = {}; return;
      case "new_project": this.newProject = {}; return;
      case "github":
        this.view = "overview";
        return void requestAnimationFrame(() => document.getElementById("github")?.scrollIntoView({ behavior: "smooth", block: "start" }));
      case "task":
        this.view = "board";
        if (subject) void this.selectTask(subject);
        return;
    }
  }

  /** Runs the person's command in the card: exactly the argvs the host
   *  issued (it refuses any other), as them, in the project folder. */
  async runFix() {
    const s = this.personStep;
    if (!s || s.how.how !== "command" || s.how.interactive) return;
    // Its events can arrive before the reply: kept until it says its id.
    this.#early = [];
    try {
      const { id } = await post<{ id: number }>("/api/fix/run", { argvs: s.how.argvs, token: s.how.token ?? "" });
      this.fixRun = { id, lines: [], result: null };
      const early = this.#early.filter((e) => e.id === id);
      this.#early = null;
      for (const e of early) this.#onEvent(e);
    } catch (e) {
      this.#early = null;
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
    }
  }

  /** A command that asks questions as it runs: the desktop window opens a
   *  terminal at the project with the command on the clipboard; a tab
   *  copies it. */
  async openTerminal() {
    const s = this.personStep;
    if (!s || s.how.how !== "command") return;
    const exact = render(s.how.argvs);
    await navigator.clipboard?.writeText(exact).catch(() => {});
    if (!this.project?.window) return this.toast({ tone: "ok", text: "Copied — paste it in a terminal at the project", detail: exact });
    try {
      await post("/api/terminal", { argvs: s.how.argvs, token: s.how.token ?? "" });
      this.toast({ tone: "ok", text: "A terminal is open at the project — paste the command there", detail: exact });
    } catch (e) {
      this.toast({ tone: "error", text: String((e as Error).message ?? e), detail: "The command is on the clipboard." });
    }
  }

  async copy(text: string, said = "Copied", detail?: string) {
    await navigator.clipboard?.writeText(text).catch(() => {});
    this.toast({ tone: "ok", text: said, detail });
  }

  /** What went wrong, as one block (code, message, fixes, docs, version ·
   *  OS · run), on the clipboard. */
  copyError(r: Wrong, kind = "error") {
    return this.copy(this.block(r, kind), "Copied what went wrong", "Paste it into an issue or a chat.");
  }

  block(r: Wrong, kind = "error"): string {
    return copyBlock(r, { version: this.project?.version, os: this.project?.os }, kind);
  }

  /** `speccoder doctor --bug-report`, on the clipboard. */
  async copyBugReport() {
    try {
      const { text } = await get<{ text: string }>("/api/bug-report");
      await this.copy(text, "Copied the bug report", "The setup in Markdown, secrets taken out.");
    } catch (e) {
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
    }
  }

  #waiters = new Map<number, (r: Reply | null) => void>();
  /** A command's events while its start has not answered yet. */
  #early: any[] | null = null;

  /** Takes `a` as a job and waits for its result (a step of a sequence:
   *  the review's approval, then plan through). */
  async run(a: Action | Offer, input?: string, root?: string): Promise<Reply | null> {
    if (!a.run) return null;
    try {
      const { id } = await post<{ id: number }>("/api/run", { run: a.run as Runnable, input: input ?? null, feature: this.feature, command: a.command, root: root ?? null });
      return await new Promise<Reply | null>((resolve) => this.#waiters.set(id, resolve));
    } catch (e) {
      this.toast({ tone: "error", text: String((e as Error).message ?? e) });
      return null;
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
    if (this.#early && (e.type === "output" || e.type === "done")) {
      this.#early.push(e);
      if (e.type === "done") this.job = { id: e.id, command: this.job?.command ?? "", running: false, result: e.result };
      return;
    }
    switch (e.type) {
      case "started":
        this.job = { id: e.id, command: e.command, running: true, result: null };
        this.stages = [];
        this.calls = [];
        this.steps = [];
        this.stepRunning = null;
        this.said = null;
        this.thinking = null;
        this.usage = { in: 0, out: 0, usd: null };
        this.oracleStage = null;
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
      case "bound":
        // Bound to the project a job made, cloned or opened.
        location.reload();
        break;
      case "output":
        if (this.fixRun && this.fixRun.id === e.id) {
          const run: FixRun = this.fixRun;
          this.fixRun = { ...run, lines: [...run.lines.slice(-1999), { stream: String(e.stream), line: String(e.line) }] };
        }
        break;
      case "done": {
        const r = e.result as Reply;
        // The person's command ended: its card shows how, and what it was
        // for is checked again.
        if (this.fixRun && this.fixRun.id === e.id) {
          this.job = { id: e.id, command: this.job?.command ?? "", running: false, result: r };
          this.fixRun = { ...this.fixRun, result: r };
          void this.refresh();
          break;
        }
        this.job = { id: e.id, command: this.job?.command ?? "", running: false, result: r };
        this.waiting = null;
        this.oracleStage = null;
        if (r?.payload === "planning_chain" && r.data) this.chain = r.data as PlanningChainReport;
        const waiter = this.#waiters.get(e.id);
        if (waiter) {
          this.#waiters.delete(e.id);
          waiter(r);
          void this.refresh();
          break;
        }
        if (this.#fixing !== undefined) {
          const then = this.#fixing;
          this.#fixing = undefined;
          this.toast({ tone: r?.status === "ok" ? "ok" : "error", text: r?.message ?? "done" });
          void this.refresh().then(() => {
            if (r?.status === "ok" && then) void this.perform(then);
          });
          break;
        }
        if (r?.remedy) {
          this.remedy = { remedy: r.remedy, message: r.message, code: r.code ?? undefined, then: this.#last };
          void this.refresh();
          break;
        }
        this.toast({
          tone: r?.status === "ok" ? "ok" : r?.status === "needs_human" ? "warn" : "error",
          text: r?.message ?? "done",
          detail: r?.code ? `${r.code}${r.fix?.length ? " · " + r.fix[0] : ""}` : undefined,
          copy: r && r.status !== "ok" ? this.block(r, r.status === "needs_human" ? "stopped" : "error") : undefined,
        });
        void this.refresh();
        break;
      }
    }
  }

  /** How many items block work: the sidebar's badge. */
  get blocked(): number {
    return (this.inbox?.data?.items ?? []).filter((i) => i.group !== "ready_to_review").length;
  }

  #seen: Set<string> | null = null;
  /** An item that has just become blocked on the person calls them (once;
   *  what was there when the page opened does not). */
  #entered(items: { kind: string; group: string; feature: string; task?: string | null; title: string; detail: string }[]) {
    const key = (i: (typeof items)[number]) => `${i.kind}/${i.feature}/${i.task ?? ""}`;
    const blocked = items.filter((i) => i.group !== "ready_to_review");
    if (this.#seen) {
      // This app's own run asks through its sheet and calls already.
      const fresh = blocked.filter((i) => !this.#seen!.has(key(i)) && !(this.waiting && i.task === this.task?.id));
      const first = fresh[0];
      if (first) this.#call(fresh.length === 1 ? first.title : `${fresh.length} things need you`, fresh.length === 1 ? first.detail : fresh.map((i) => i.title).join(" · "));
    }
    this.#seen = new Set(blocked.map(key));
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
        this.agent = null;
        break;
      case "agent_started":
        this.agent = { name: ev.agent, mode: ev.mode, decides_alone: !!ev.decides_alone, known: !!ev.known, unasked: 0, plan: [], asked: 0, allowed: 0, waited: 0, refused: 0 };
        this.calls = [...this.calls.slice(-199), `${ev.agent}: ${ev.mode}`];
        break;
      case "agent_unasked":
        if (this.agent) this.agent = { ...this.agent, decides_alone: true, unasked: ev.calls ?? 0 };
        break;
      case "agent_plan":
        if (this.agent) this.agent = { ...this.agent, plan: ev.entries ?? [] };
        break;
      case "approval_requested":
        if (this.agent) this.agent = { ...this.agent, waited: this.agent.waited + 1 };
        break;
      case "stage": {
        const row = { stage: ev.stage, state: ev.state, detail: ev.detail ?? null, elapsed_ms: ev.elapsed_ms ?? null };
        const i = this.stages.findIndex((s) => s.stage === ev.stage);
        this.stages = i >= 0 ? this.stages.map((s, j) => (j === i ? row : s)) : [...this.stages, row];
        break;
      }
      case "said": {
        // The model's own words: kept apart (a quote mark leads), a claim.
        const first = String(ev.text ?? "").split("\n").find((l: string) => l.trim()) ?? "";
        if (first) {
          this.said = first.trim();
          this.calls = [...this.calls.slice(-199), `“${first.trim()}”`];
        }
        break;
      }
      case "reasoning": {
        // Its reasoning, marked "therefore": the conclusion is shown.
        const last = String(ev.text ?? "").split("\n").filter((l: string) => l.trim()).at(-1) ?? "";
        if (last) {
          this.thinking = last.trim();
          this.calls = [...this.calls.slice(-199), `∴ ${last.trim()}`];
        }
        break;
      }
      case "tool_call":
        if (this.agent && ev.decision !== "reported") {
          const a = this.agent;
          this.agent = { ...a, asked: a.asked + 1, allowed: a.allowed + (ev.decision === "allow" ? 1 : 0), refused: a.refused + (ev.decision === "deny" ? 1 : 0) };
        }
        this.calls = [...this.calls.slice(-199), `turn ${ev.turn}  ${ev.tool} ${ev.target}  ${ev.decision}${ev.ok === false ? "  failed" : ""}`];
        // A skill writing the documents: show them as they are written.
        if (this.following && /write|edit/.test(String(ev.tool))) this.#soon();
        break;
      case "planning_stopped":
        this.following = false;
        void this.refresh();
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
      case "project": {
        // A new project's step: names and states only (never the idea).
        const step = ev.step ?? {};
        if (step.event === "scaffold_started") this.stepRunning = step.step;
        if (step.event === "scaffold_finished") {
          this.stepRunning = null;
          this.steps = [...this.steps.filter((r) => r.id !== step.step), { id: step.step, state: step.state, exit: step.exit ?? null, duration_ms: step.duration_ms ?? 0, output_tail: [] }];
        }
        break;
      }
      case "oracle":
        // Names and counts only: never a test, never held-out material.
        if (ev.step?.kind === "stage") this.oracleStage = { requirement: ev.step.requirement, stage: ev.step.stage, n: ev.step.n, of: ev.step.of };
        break;
    }
  }

  #timer: ReturnType<typeof setTimeout> | null = null;
  /** A refresh within a second, once for a burst of writes. */
  #soon() {
    if (this.#timer) return;
    this.#timer = setTimeout(() => {
      this.#timer = null;
      void this.refresh();
    }, 900);
  }

  /** A feature's title: its spec's own, else its directory name in words. */
  titleOf(name: string): string {
    const f = this.project?.features?.features.find((x) => x.name === name);
    return f?.title ?? name.replace(/^\d+-/, "").replaceAll("-", " ");
  }

  /** Where the page is, in its address: `?feature=…&view=…&task=…`, so a
   *  reload, a bookmark or the back button lands in the same place. */
  get location(): string {
    const s = new URLSearchParams();
    if (this.feature) s.set("feature", this.feature);
    if (this.view !== "inbox") s.set("view", this.view);
    if (this.selected) s.set("task", this.selected);
    if (this.view === "code" && this.codeMode === "files") s.set("mode", "files");
    if (this.view === "code" && this.codeFile) s.set("file", this.codeFile);
    if (this.view === "code" && this.codeFile && this.codeLine) s.set("line", String(this.codeLine));
    if (this.view === "documents" && this.doc !== "spec") s.set("doc", this.doc);
    if (this.view === "documents" && this.section) s.set("section", this.section);
    const t = s.toString();
    return location.pathname + (t ? `?${t}` : "");
  }

  #restore() {
    const s = new URLSearchParams(location.search);
    const view = s.get("view");
    if (view === "inbox" || view === "overview" || view === "board" || view === "code" || view === "documents" || view === "report" || view === "settings" || view === "setup") this.view = view;
    this.feature = s.get("feature") ?? this.feature;
    this.selected = s.get("task");
    this.codeFile = s.get("file");
    const line = Number(s.get("line"));
    this.codeLine = Number.isInteger(line) && line > 0 ? line : null;
    this.codeMode = s.get("mode") === "files" || this.codeFile ? "files" : "changes";
    this.palette = s.has("palette");
    this.doc = s.get("doc") ?? "spec";
    this.section = s.get("section");
  }

  /** Reads the project's live run; every few seconds while the page is seen. */
  async pollLive() {
    try {
      this.live = await get<LiveRun>("/api/live");
    } catch {
      /* the host is away: the connection dot says so */
    }
  }

  /** A project speccoder just created opens its first feature, prefilled
   *  with the idea — once, while it has no unit. */
  async #firstFeature() {
    if (this.welcome || !this.project) return;
    try {
      const f = await post<{ idea: string | null; name: string | null; commit: string | null }>("/api/new/first-feature", {});
      if (!f.idea) return;
      if (f.name) this.toast({ tone: "ok", text: `${f.name} is ready${f.commit ? ` — committed ${f.commit.slice(0, 7)}` : ""}`, detail: "Write the first feature: it is prefilled with your idea" });
      this.newFeature = { text: f.idea, through: "tasks" };
    } catch {
      /* nothing offered */
    }
  }

  start() {
    this.#restore();
    void this.pollLive();
    setInterval(() => {
      if (!document.hidden) void this.pollLive();
    }, 3000);
    addEventListener("popstate", () => {
      this.#restore();
      void this.refresh();
    });
    void this.refresh().then(() => this.#firstFeature());
    addEventListener("speccoder:toast", (e) => this.toast({ tone: "ok", text: String((e as CustomEvent).detail ?? "") }));
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
