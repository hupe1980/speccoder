// The payloads, as the host sends them (speccoder's own JSON).

export type Runnable =
  | { kind: "skill"; name: string; arguments: string }
  | { kind: "run"; task: string | null }
  | { kind: "commit"; paths: string[] }
  | { kind: "approve" | "reject" | "accept" | "skip" | "nudge"; task: string }
  | { kind: "feature" };

export interface Action {
  group: "next" | "plan" | "work" | "task" | "feature" | "review" | string;
  label: string;
  why: string;
  command: string;
  run: Runnable | null;
  blocked: string | null;
  input: string | null;
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
  trace: { requirement: string; task: string | null; verdict: string | null; scenario: string | null; test: string | null }[];
  convergence: { round: number; commit: string; tasks: string[] }[];
  totals: Record<string, number>;
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
