// The Ask panel's state: the conversation, the context chips, the turn
// being answered (its steps and words live, from the event stream) and
// the conversations to go back to. A turn writes nothing to the project;
// a proposal is performed only from its sheet.
import { get, post, q } from "./api";
import { app } from "./app.svelte";
import type { AskReport, AskThread, Conversation, Proposal } from "./schema.gen";

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
  /** The conversation takes no more turns (ended, idle or full). */
  ended = $state(false);
  /** Chips the person added. */
  chips = $state<ChipRef[]>([]);
  /** The view's own chip was removed for this conversation. */
  viewDropped = $state(false);
  /** The turn being answered. */
  pending = $state<{ question: string; steps: Step[]; words: string } | null>(null);
  failed = $state<{ question: string; message: string } | null>(null);
  /** The conversations list, when shown. */
  list = $state<Conversation[] | null>(null);
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
    this.ended = false;
    this.failed = null;
    this.list = null;
    this.viewDropped = false;
    this.focus++;
  }

  async send(question: string) {
    const text = question.trim();
    if (!text || this.pending) return;
    if (this.ended) this.fresh();
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
        if (r.message.includes("full")) this.ended = true;
      } else {
        if (r.code === "ASK-FULL") this.ended = true;
        this.failed = { question: text, message: r.message };
      }
    } catch (e) {
      this.failed = { question: text, message: String((e as Error).message ?? e) };
    } finally {
      this.pending = null;
    }
  }

  /** An event from the stream, for the turn being answered. */
  event(ev: { event: string; conversation?: string; tool?: string; target?: string; refused?: boolean; delta?: string }) {
    const p = this.pending;
    if (!p) return;
    if (ev.event === "ask_step") this.pending = { ...p, steps: [...p.steps, { tool: ev.tool ?? "", target: ev.target ?? "", refused: !!ev.refused }] };
    if (ev.event === "ask_text") this.pending = { ...p, words: p.words + (ev.delta ?? "") };
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
      this.ended = !r.data.conversation.open;
      this.list = null;
      this.focus++;
    } catch {
      /* the list stays */
    }
  }
}

export const ask = new AskState();
