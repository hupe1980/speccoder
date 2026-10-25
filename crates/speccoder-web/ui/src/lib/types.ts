// The payloads, as the host sends them (speccoder's own JSON).

export type Runnable =
  | { kind: "skill"; name: string; arguments: string }
  | { kind: "run"; task: string | null }
  | { kind: "commit"; paths: string[] }
  | { kind: "approve" | "reject" | "accept" | "skip" | "nudge"; task: string }
  | { kind: "feature" }
  | { kind: "converge" }
  | { kind: "rule"; task: string }
  | { kind: "review"; ids: string[]; verdict: "approve" | "request" | "clear" | "answer" | "edit" | "edit-approve" }
  | { kind: "issue_comment"; issue: number; assign: boolean }
  | { kind: "pull_request" };

export interface Action {
  group: "next" | "plan" | "work" | "task" | "feature" | "review" | string;
  label: string;
  why: string;
  command: string;
  run: Runnable | null;
  blocked: string | null;
  input: string | null;
  /** It leaves the machine: say this in a confirmation before running it. */
  confirm?: string | null;
}

export interface JourneyStep {
  name: string;
  state: "done" | "now" | "todo" | "skipped";
}

export interface Task {
  id: string;
  title: string;
  phase: string;
  state: string;
  blocked_by: string[];
  commit: string | null;
  run_id: string | null;
  note: string | null;
  attestation: string | null;
  strength?: string | null;
  last_attempt?: { outcome: string; reason: string; reference: string } | null;
  /** The intent changed since the tick (F-085). */
  drift?: Drift | null;
}

/** A cited requirement as the tick saw it (`then`) and as the spec has it now. */
export interface RequirementDrift {
  id: string;
  then: string | null;
  now: string | null;
}

export interface Drift {
  /** Cited requirements that changed: the evidence is for the earlier wording. */
  requirements: RequirementDrift[];
  /** The intent files that differ; empty when they are not in git. */
  files: string[];
}

export interface Report {
  feature: string;
  branch: string | null;
  source: string;
  selected_by: string;
  artifacts: string[];
  planning?: string[];
  uncommitted?: string[];
  journey: JourneyStep[];
  unmerged_commits: number;
  tasks: Task[];
  trace: { requirement: string; task: string | null; verdict: string | null; scenario: string | null; test: string | null; changed?: boolean }[];
  convergence: { round: number; commit: string; tasks: string[] }[];
  totals: Record<string, number>;
  issues?: string[];
  review?: ReviewTotals | null;
}

export interface Envelope<T = unknown> {
  status: string;
  message: string;
  code?: string;
  fix?: string[];
  next?: { command: string; why: string }[];
  payload?: string;
  data?: T;
}

export interface FeatureEntry {
  number: number;
  name: string;
  dir: string;
  current: boolean;
  done: number;
  tasks: number;
  artifacts: string[];
}

export interface Project {
  name: string;
  root: string;
  format: string;
  unit: string;
  version: string;
  features: { current: string | null; reason_text: string; features: FeatureEntry[] } | null;
}

export interface Row {
  label: string;
  mark: string;
  text: string;
}

export interface Presentation {
  request: {
    task: string;
    task_title: string;
    turn: number;
    tool: string;
    kind: string;
    arguments?: { argv?: unknown } & Record<string, unknown>;
    reason: string;
    covers: string;
    risk: string;
    claim: string | null;
    options: string[];
  };
  title: string;
  subject: string;
  body: string[];
  full: string[];
  sandbox: string;
  options: { choice: string; key: string; number: number; label: string }[];
  scope: string;
}

export interface PendingCall {
  task: string;
  covers: string;
  tool: string;
  reason: string;
  presentation?: { title: string; subject: string; body_lines: string[]; sandbox: string; risk: string; scope: string } | null;
}

export interface Document {
  name: string;
  path: string;
  html: string;
}

export interface ConfigKey {
  key: string;
  kind: "bool" | "integer" | "number" | "string" | "choice" | "duration" | "argv" | "argv_list" | "string_list";
  about: string;
  default: unknown;
  value: unknown;
  choices: string[];
  min: number | null;
  max: number | null;
  required: boolean;
}

export interface ConfigSection {
  name: string;
  title: string;
  about: string;
  named: boolean;
  keys: ConfigKey[];
  tables: { name: string; keys: ConfigKey[] }[];
}

export interface ConfigReport {
  file: string;
  exists: boolean;
  committed: boolean;
  valid: boolean;
  error: string | null;
  sections: ConfigSection[];
  diff: string | null;
}

export interface ConfigChange {
  key: string;
  value?: unknown;
  append?: unknown;
}

export interface Usage {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  usd: number | null;
}

export interface CostReport {
  feature: string;
  tasks: { task: string; attempts: number; usage: Usage }[];
  convergence: Usage;
  total: Usage;
  priced_model: string | null;
}

export interface GitHubIssue {
  number: number;
  title: string;
  url: string;
  labels: string[];
  assignees: string[];
  mine: boolean;
  updated: string | null;
  feature: string | null;
}

export interface Pull {
  number: number;
  title: string;
  url: string;
  state: string;
  draft: boolean;
  review: string | null;
  checks: [number, number, number];
}

export interface GitHubState {
  status: { ready: boolean; host: string | null; login: string | null; source: string | null; repo: string | null; url: string | null; reason: string | null; fix: string | null };
  issues?: { repo: string | null; issues: GitHubIssue[] } | null;
  issues_error?: string | null;
  branch?: string | null;
  pull?: Pull | null;
  pull_error?: string | null;
}

/** One decision waiting for the person (F-086). */
export interface InboxItem {
  kind: "approval" | "ruling" | "gaming" | "stale" | "stuck" | "commit" | "handoff" | string;
  feature: string;
  task: string | null;
  title: string;
  detail: string;
  since: number | null;
  actions: Action[];
}

export interface Inbox {
  items: InboxItem[];
  features: number;
}

/** One requirement and the person's review of its wording (F-087). */
export interface ReviewItem {
  id: string;
  text: string;
  state: "open" | "approved" | "changes_requested" | "lapsed";
  by: string | null;
  at: number | null;
  comment: string | null;
  decided_on: string | null;
}

export interface ReviewTotals {
  approved: number;
  changes_requested: number;
  open: number;
  lapsed: number;
  questions: number;
}

/** A question a document leaves open (F-093). */
export interface OpenQuestion {
  id: string;
  text: string;
  file: string;
  line: number;
  near: string | null;
}

/** One commit of the spec and what it changed. */
export interface SpecVersion {
  commit: string;
  date: string;
  author: string;
  subject: string;
  added: string[];
  reworded: string[];
  removed: string[];
}

export interface Review {
  feature: string;
  file: string;
  requirements: ReviewItem[];
  totals: ReviewTotals;
  revise_with: string | null;
  /** What the revising skill is asked, while changes are requested. */
  revision: string | null;
  questions: OpenQuestion[];
  changes: RequirementDrift[];
  history: SpecVersion[];
}
