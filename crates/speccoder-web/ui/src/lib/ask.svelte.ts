// The Ask panel's state: the conversation, the context chips, the turn
// being answered (its steps and words live, from the event stream) and
// the conversations to go back to. A turn writes nothing to the project;
// a proposal is performed only from its sheet.
import { get, post, q } from "./api";
import { app } from "./app.svelte";
import type { AskReport, AskThread, Conversation, InboxItem, Proposal } from "./schema.gen";
import type { AccountTurn } from "./types";

export type ChipRef = {
  kind: string;
  reference: Record<string, unknown>;
  label?: string;
  text?: string;
  digest?: string;
  from_view?: boolean;
};
export type Step = { tool: string; target: string; refused: boolean };

const read = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const write = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* a private window keeps nothing */
  }
};

/** Marks an element as something Ask can take: ⌘⇧L on it, or words
 *  selected inside it. Kinds the engine resolves (`task`, `feature`,
 *  `file_range`) are read fresh; any other carries `text`. */
export function askable(kind: string, reference: Record<string, unknown>, label: string, text?: string) {
  return { "data-ask": JSON.stringify({ kind, reference, label, text }) };
}

const marked = (el: Element | null | undefined): ChipRef | null => {
  const raw = el?.closest("[data-ask]")?.getAttribute("data-ask");
  try {
    return raw ? (JSON.parse(raw) as ChipRef) : null;
  } catch {
    return null;
  }
};

/** The words selected outside Ask, and where they are from. */
export function selected(): { text: string; from: ChipRef | null; rect: DOMRect | null } | null {
  const sel = typeof window === "undefined" ? null : window.getSelection();
  const text = sel?.toString().trim() ?? "";
  if (!sel || !text || sel.isCollapsed || !sel.rangeCount) return null;
  const node = sel.anchorNode;
  const el = node instanceof Element ? node : node?.parentElement;
  if (!el || el.closest(".ask, textarea, input, [contenteditable]")) return null;
  const range = sel.getRangeAt(0);
  return { text: text.slice(0, 4000), from: marked(el), rect: typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null };
}

/** The chip for what the person points at: selected words, else the
 *  focused item, else nothing. */
export function pointed(): ChipRef | null {
  const s = selected();
  if (s) {
    const excerpt = s.text.length > 36 ? `${s.text.slice(0, 34).trimEnd()}…` : s.text;
    return { kind: "selection", reference: s.from ? { from: s.from.label ?? s.from.kind } : {}, label: `“${excerpt}”`, text: s.from ? `${s.text}\n\n(selected in ${s.from.label ?? s.from.kind})` : s.text };
  }
  return marked(typeof document === "undefined" ? null : document.activeElement);
}

/** What a step reads as: "Read notes.txt 1–3". */
export function stepWords(s: Step): string {
  const verb: Record<string, string> = {
    read_file: "Read",
    list_files: "Listed",
    search_code: "Searched for",
    git_status: "Looked at the working tree",
    git_diff: "Diffed",
    git_log: "History of",
    git_show: "Showed",
    journal_account: "Read the account of",
    task_evidence: "Evidence of",
    report: "Report of",
    inbox: "Looked at Needs You",
    explain: "Looked up",
    propose: "Proposed",
  };
  return `${verb[s.tool] ?? s.tool} ${s.target}`.trim();
}

class AskState {
  open = $state(read("speccoder-ask-open") === "1");
  width = $state(Math.min(640, Math.max(320, Number(read("speccoder-ask-width")) || 400)));
  conversation = $state<string | null>(null);
  turns = $state<AskReport[]>([]);
  /** Chips the person added. */
  chips = $state<ChipRef[]>([]);
  /** The view's own chip was removed for this conversation. */
  viewDropped = $state(false);
  /** The turn being answered. */
  pending = $state<{ question: string; steps: Step[]; words: string; stopping?: boolean } | null>(null);
  /** The next question, asked while a stopped turn's last model call ends:
   * it goes as soon as that call lands. */
  queued = $state<string | null>(null);
  failed = $state<{ question: string; message: string } | null>(null);
  /** The conversations list, when shown. */
  list = $state<Conversation[] | null>(null);
  /** The three most recent conversations, for the empty panel. */
  recent = $state<Conversation[]>([]);
  /** A run's account a citation names, shown in place. */
  peek = $state<{ run: string; turns: AccountTurn[] | null; error?: string } | null>(null);
  /** The proposal sheet. */
  sheet = $state<{ proposal: Proposal; turn: number } | null>(null);
  /** Focus the composer (set by ⌘L, cleared by it). */
  focus = $state(0);

  toggle(open = !this.open) {
    this.open = open;
    write("speccoder-ask-open", open ? "1" : "0");
    if (open) this.focus++;
  }

  resize(w: number) {
    this.width = Math.min(640, Math.max(320, Math.round(w)));
    write("speccoder-ask-width", String(this.width));
  }

  /** The current view's subject: the selected task, else the feature. */
  get viewChip(): ChipRef | null {
    if (this.viewDropped) return null;
    const feature = app.report?.data?.feature ?? null;
    if (app.selected) {
      const t = app.report?.data?.tasks.find((x) => x.id === app.selected);
      return { kind: "task", reference: { task: app.selected, feature }, label: t ? `${t.id} ${t.title}` : app.selected, from_view: true };
    }
    if (feature) return { kind: "feature", reference: { feature }, label: feature, from_view: true };
    return null;
  }

  /** The tasks in context: the view's and the person's chips. */
  get tasks(): { feature: unknown; task: unknown }[] {
    return [this.viewChip, ...this.chips].filter((c): c is ChipRef => c?.kind === "task").map((c) => ({ feature: c.reference.feature, task: c.reference.task }));
  }

  /** A task in context that grew: its promotion offer, as Needs You lists it. */
  get offer(): InboxItem | null {
    const items = app.inbox?.data?.items ?? [];
    return items.find((i) => i.kind === "promote" && this.tasks.some((t) => t.task === i.task && (!t.feature || t.feature === i.feature))) ?? null;
  }

  async loadRecent() {
    try {
      const r = await get<{ data?: { conversations: Conversation[] } }>("/api/ask/conversations");
      this.recent = (r.data?.conversations ?? []).slice(0, 3);
    } catch {
      this.recent = [];
    }
  }

  async peekAccount(run: string) {
    this.peek = { run, turns: null };
    const r = await get<{ turns?: AccountTurn[]; error?: string }>(`/api/account${q({ run })}`).catch((e) => ({ turns: undefined, error: String(e) }));
    if (this.peek?.run === run) this.peek = { run, turns: r.turns ?? [], error: r.error };
  }

  /** Questions that fit the view; computed, never a model call. */
  get suggestions(): string[] {
    const r = app.report?.data;
    const t = app.selected ? r?.tasks.find((x) => x.id === app.selected) : null;
    if (t) {
      return [
        `Why is ${t.id} ${t.state.replace(/_/g, " ")}?`,
        `What would ${t.id} change in the code?`,
        `What does the evidence of ${t.id} show?`,
      ];
    }
    if (app.view === "inbox") return ["What needs me first, and why?", "Which of these is riskiest to accept?"];
    if (r) {
      const open = r.tasks.filter((x) => !["verified", "attested", "skipped"].includes(x.state)).length;
      return [
        open ? `What is left in ${r.feature}?` : `How was ${r.feature} verified?`,
        "What changed most recently, and why?",
        "Where is the main entry point of the code?",
        "Which requirement has the weakest evidence?",
      ];
    }
    return ["What does this project do?", "How is the code organised?"];
  }

  add(chip: ChipRef) {
    const same = (c: ChipRef) => c.kind === chip.kind && JSON.stringify(c.reference) === JSON.stringify(chip.reference);
    if (!this.chips.some(same)) this.chips = [...this.chips, chip].slice(-8);
    this.toggle(true);
  }

  remove(i: number | "view") {
    if (i === "view") this.viewDropped = true;
    else this.chips = this.chips.filter((_, j) => j !== i);
  }

  fresh() {
    this.conversation = null;
    this.turns = [];
    this.failed = null;
    this.list = null;
    this.viewDropped = false;
    this.focus++;
  }

  async send(question: string) {
    const text = question.trim();
    if (!text || this.pending) return;
    const chips = [...(this.viewChip ? [this.viewChip] : []), ...this.chips];
    this.failed = null;
    this.list = null;
    this.pending = { question: text, steps: [], words: "" };
    try {
      const r = await post<{ status: string; message: string; code?: string; data?: AskReport }>("/api/ask", {
        conversation: this.conversation,
        text,
        chips,
        feature: app.report?.data?.feature ?? null,
      });
      if (r.data) {
        this.conversation = r.data.conversation;
        this.turns = [...this.turns, r.data];
        this.chips = [];
      } else {
        this.failed = { question: text, message: r.message };
      }
    } catch (e) {
      this.failed = { question: text, message: String((e as Error).message ?? e) };
    } finally {
      this.pending = null;
      const next = this.queued;
      this.queued = null;
      if (next) void this.send(next);
    }
  }

  /** Stops the turn in flight: its words end where they are now, nothing
   * more runs, and the next question goes on in the same conversation. */
  async stop() {
    const p = this.pending;
    if (!p || p.stopping) return;
    this.pending = { ...p, stopping: true };
    try {
      await post("/api/ask/stop", { conversation: this.conversation });
    } catch {
      /* the turn ends on its own */
    }
  }

  /** An event from the stream, for the turn being answered. */
  event(ev: { event: string; conversation?: string; tool?: string; target?: string; refused?: boolean; delta?: string }) {
    const p = this.pending;
    if (!p) return;
    if (ev.event === "ask_step") this.pending = { ...p, steps: [...p.steps, { tool: ev.tool ?? "", target: ev.target ?? "", refused: !!ev.refused }] };
    if (ev.event === "ask_text" && !p.stopping) this.pending = { ...p, words: p.words + (ev.delta ?? "") };
  }

  async showList() {
    try {
      const r = await get<{ data?: { conversations: Conversation[] } }>("/api/ask/conversations");
      this.list = r.data?.conversations ?? [];
    } catch {
      this.list = [];
    }
  }

  async resume(id: string) {
    try {
      const r = await get<{ data?: AskThread; message: string }>(`/api/ask/conversation${q({ id })}`);
      if (!r.data) return;
      this.conversation = r.data.conversation.id;
      this.turns = r.data.turns;
      this.list = null;
      this.focus++;
    } catch {
      /* the list stays */
    }
  }
}

export const ask = new AskState();
