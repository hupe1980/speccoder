// The payloads, as the host sends them. Every one with a published schema
// is generated from site/static/schemas/v2 (./schema.gen.ts, drift-tested
// in `just ci`) and re-exported here under the name the page uses. Kept by
// hand, because the host sends them without a schema: Project, Row,
// Presentation, Document, ConfigChange, Pull, GitHubState, AccountTurn,
// LiveRun — and Reply, an envelope whose `data` the caller knows.

import type { Envelope, FeatureEntry, IssueEntry as GitHubIssue } from "./schema.gen";

export type {
  Action,
  ActionId,
  ActionKind,
  ConfigKey,
  ConfigReport,
  ConfigSection,
  CostReport,
  Drift,
  Envelope,
  FeatureEntry,
  InboxItem,
  JourneyStep,
  OpenQuestion,
  PendingCall,
  Remedy,
  RequirementDrift,
  ReviewItem,
  ReviewTotals,
  Runnable,
  SkillRole,
  SpecVersion,
  StatusTotals,
  Usage,
  IssueEntry as GitHubIssue,
  InboxReport as Inbox,
  ReviewReport as Review,
  StatusReport as Report,
  TaskStatus as Task,
} from "./schema.gen";

/** A route's envelope, its `data` as the route sends it. */
export type Reply<T = unknown> = Omit<Envelope, "data"> & { data?: T | null };

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

export interface Document {
  name: string;
  path: string;
  html: string;
}

export interface ConfigChange {
  key: string;
  value?: unknown;
  append?: unknown;
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

/** One model turn of an attempt, as its account: reasoning and words (a claim). */
export interface AccountTurn {
  turn: number;
  reasoning: string | null;
  said: string | null;
}

/** The project's current run, from whichever process: `live.json`. */
export interface LiveRun {
  live: {
    pid: number;
    feature: string;
    task: string;
    title: string;
    stage: string | null;
    stage_detail: string | null;
    last_call: string | null;
    said: string | null;
    thinking: string | null;
    input_tokens: number;
    output_tokens: number;
    usd: number | null;
    finished: string | null;
    started_at: number;
    updated_at: number;
  } | null;
  running?: boolean;
  own?: boolean;
  interrupted?: boolean;
  now?: number;
}

