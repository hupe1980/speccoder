// Finding an action by what it is — its kind, its subject, a skill's role —
// never by its wording, and what the page knows per kind. The one place the
// page writes an action itself (`offer`); everything else is the host's.

import type { Action, ActionKind, Runnable, SkillRole } from "./types";

/** What the page knows about a kind: the verb a button says, its icon, and
 *  whether taking it holds the project's worktree (one run per project).
 *  Exhaustive: a kind the host adds fails the type check until it is here. */
export const KINDS: Record<ActionKind, { verb: string; icon?: string; holdsWorktree: boolean }> = {
  skill: { verb: "Run", icon: "sparkle", holdsWorktree: true },
  run_task: { verb: "Run", icon: "play", holdsWorktree: true },
  run_feature: { verb: "Run", icon: "play", holdsWorktree: true },
  converge: { verb: "Re-check", icon: "loop", holdsWorktree: true },
  commit: { verb: "Commit", icon: "check", holdsWorktree: false },
  approve: { verb: "Approve", icon: "check", holdsWorktree: false },
  reject: { verb: "Reject", icon: "xmark", holdsWorktree: false },
  accept: { verb: "Accept", icon: "check", holdsWorktree: false },
  skip: { verb: "Skip", icon: "skip", holdsWorktree: false },
  nudge: { verb: "Nudge", icon: "sparkle", holdsWorktree: false },
  rule: { verb: "Rule", icon: "shield", holdsWorktree: false },
  review: { verb: "Review", icon: "doc", holdsWorktree: false },
  issue_comment: { verb: "Comment", icon: "branch", holdsWorktree: false },
  quick_change: { verb: "Run", icon: "sparkle", holdsWorktree: true },
  keep_set_aside: { verb: "Keep", icon: "check", holdsWorktree: true },
  drop_set_aside: { verb: "Discard", icon: "xmark", holdsWorktree: false },
  fix_setup: { verb: "Fix", icon: "check", holdsWorktree: true },
  feature: { verb: "Switch", icon: "list", holdsWorktree: false },
  pull_request: { verb: "Open", icon: "branch", holdsWorktree: false },
  allow_always: { verb: "Always Allow", icon: "lock", holdsWorktree: false },
  widen_policy: { verb: "Allow prefix", icon: "lock", holdsWorktree: false },
  decide_batch: { verb: "Decide waiting calls", icon: "hand", holdsWorktree: false },
  bug_fix: { verb: "Record", icon: "exclaim", holdsWorktree: true },
  bug_confirm: { verb: "Confirm", icon: "check", holdsWorktree: true },
  bug_run: { verb: "Reproduce", icon: "play", holdsWorktree: true },
  bug_keep: { verb: "Keep", icon: "check", holdsWorktree: true },
  oracle_tests: { verb: "Write Tests", icon: "shield", holdsWorktree: false },
  approve_and_run: { verb: "Approve and Run", icon: "play", holdsWorktree: true },
  comment: { verb: "Comment", icon: "doc", holdsWorktree: false },
  comments_send: { verb: "Send Comments", icon: "sparkle", holdsWorktree: false },
  revision_undo: { verb: "Undo Revision", icon: "undo", holdsWorktree: false },
  code_nudge: { verb: "Send as a Nudge", icon: "sparkle", holdsWorktree: false },
  acceptance_at: { verb: "Write Acceptance Here", icon: "shield", holdsWorktree: false },
  preset_add: { verb: "Add the Preset", icon: "doc", holdsWorktree: false },
  plan_through: { verb: "Start", icon: "sparkle", holdsWorktree: false },
  new_project: { verb: "Create", icon: "plus", holdsWorktree: false },
  undo_new_project: { verb: "Undo", icon: "undo", holdsWorktree: false },
  clone_repository: { verb: "Clone", icon: "branch", holdsWorktree: false },
  open_sample: { verb: "Try the Sample", icon: "play", holdsWorktree: false },
  command: { verb: "Copy", icon: "terminal", holdsWorktree: false },
};

/** The kind of what runs (a run with a task is `run_task`). */
export function kindOf(run: Runnable): ActionKind {
  if (run.kind === "run") return run.task ? "run_task" : "run_feature";
  return run.kind;
}

/** What a list of actions is keyed by: group and identity, unique in it
 *  (an action the page built has no identity: its group and command). */
export function key(a: Action | Offer): string {
  return a.id ? `${a.group}:${a.id.kind}:${a.id.subject ?? ""}` : `${a.group}:${a.command}`;
}

/** The actions of `kind` (about `subject`, when given). */
export function byKind(actions: Action[], kind: ActionKind, subject?: string | null): Action[] {
  return actions.filter((a) => a.id.kind === kind && (subject === undefined || a.id.subject === subject));
}

/** The planning skills playing `role`. */
export function byRole(actions: Action[], role: SkillRole): Action[] {
  return actions.filter((a) => a.role === role);
}

/** The actions about task `id` that act on it (the `task` and `work` groups). */
export function ofTask(actions: Action[], id: string): Action[] {
  return actions.filter((a) => (a.group === "task" || a.group === "work") && a.id.subject === id);
}

/** An action the page builds itself (a board card's run button, a fix the
 *  code view shows): what runs and how it is shown, without an identity —
 *  the host's actions have those. The job endpoint gets `run` and the input. */
export type Offer = Omit<Action, "id" | "role" | "blocked" | "input" | "confirm"> &
  Partial<Pick<Action, "blocked" | "input" | "confirm">> & { id?: undefined; role?: undefined };

/** The one way the page writes an action (`run` null: one it does not
 *  run, a command to copy or an entry of its own). */
export function offer(run: Runnable | null, shown: { group: string; label: string; why: string; command: string; confirm?: string | null; input?: string | null }): Offer {
  return { ...shown, run, blocked: null, input: shown.input ?? null, confirm: shown.confirm ?? null };
}
