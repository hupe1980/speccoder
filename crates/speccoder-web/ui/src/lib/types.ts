// The payloads, as the host sends them. Every one with a published schema
// is generated from site/static/schemas/v2 (./schema.gen.ts, drift-tested
// in `just ci`) and re-exported here under the name the page uses. Kept by
// hand, because the host sends them without a schema: Project, Row,
// Presentation, Document, ConfigChange, Pull, GitHubState, AccountTurn,
// LiveRun — and Reply, an envelope whose `data` the caller knows.

import type { Envelope, FeatureEntry, IssueEntry as GitHubIssue } from "./schema.gen";

export type {
  Action,
  AgentAttempt,
  AgentCard,
  AgentTick,
  ApproveSheet,
  BugReport,
  CodeComment,
  CommentAnchor,
  CommentDraft,
  CommentsView,
  OracleTest,
  OracleView,
  RunMode,
  ActionId,
  ActionKind,
  ConfigKey,
  ConfigReport,
  ConfigSection,
  CostReport,
  Drift,
  Envelope,
  FeatureEntry,
  How,
  InboxItem,
  JourneyStep,
  OpenQuestion,
  PendingCall,
  PolicyCoverage,
  Remedy,
  RequirementDrift,
  ReviewChange,
  ReviewItem,
  RevisionView,
  SectionDiff,
  ReviewTotals,
  Runnable,
  SkillRole,
  SpecVersion,
  StatusTotals,
  Step,
  Usage,
  IssueEntry as GitHubIssue,
  InboxReport as Inbox,
  ReviewReport as Review,
  StatusReport as Report,
  TaskStatus as Task,
  ChainStop,
  FormatOption,
  NewChoices,
  NewProjectReport,
  PlanCheck,
  PlanStep,
  PlanStepResult,
  PlanStepState,
  PlanningChainReport,
  ProjectPlan,
  Readiness,
  RungPick,
  RungSuggestion,
  SampleReport,
  SimilarMatch,
  SimilarReport,
  StackOption,
  Through,
  View,
} from "./schema.gen";

/** A route's envelope, its `data` as the route sends it. */
export type Reply<T = unknown> = Omit<Envelope, "data"> & { data?: T | null };

export interface Project {
  name: string;
  root: string;
  format: string;
  unit: string;
  version: string;
  /** `macos aarch64`: where the host runs. */
  os?: string;
  /** The host serves the desktop window (it can open a terminal). */
  window?: boolean;
  /** The person's commands run in an OS sandbox here. */
  sandbox?: boolean;
  features: { current: string | null; reason_text: string; features: FeatureEntry[] } | null;
  /** The format's planning chain, by installed skill name. */
  planning?: { creates: string | null; toward: string[]; checks: string | null; revises: string | null };
  /** The sample, when this host serves its copy. */
  sample?: SampleInfo | null;
}

/** The sample's copy: how it was recorded and whether each run reproduced. */
export interface SampleInfo {
  path: string;
  recorded?: { model: string; recorded_at: string; agentplane: string };
  runs: { id: string; result: string; finding?: string | null }[];
  /** The tool the sample's checks need that is missing here, with its hint. */
  checks_missing?: string | null;
}

/** A recent project, as the welcome shows it. */
export interface RecentProject {
  path: string;
  name: string;
  opened_at: number;
  created: boolean;
  shown: string;
  exists: boolean;
  undo: { available: boolean; kind: string; reason?: string | null } | null;
}

/** The welcome of a host started outside a project. */
export interface WelcomeView {
  bound: string | null;
  start: string;
  start_shown: string;
  parent: string;
  parent_shown: string;
  recents: RecentProject[];
  sample: boolean;
  sample_copy: boolean;
  version: string;
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
    risk: string;
    claim: string | null;
    options: string[];
    sources?: { argument: string; from: string; turn: number }[];
    batched?: boolean;
  };
  title: string;
  subject: string;
  /** The object a decision covers, line by line. */
  object: string[];
  /** Where arguments of the call were copied from. */
  sources: string[];
  /** Said on the prompt that uses the run's asking budget. */
  notice: string | null;
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
    agent?: LiveAgent | null;
  } | null;
  running?: boolean;
  own?: boolean;
  interrupted?: boolean;
  now?: number;
}


/** An external agent's attempt as it goes (live.json's `agent`, and the
 *  app's own job from its events): its mode, plan and what speccoder decided. */
export interface LiveAgent {
  name: string;
  mode: string;
  decides_alone: boolean;
  plan: { content: string; status: string }[];
  asked: number;
  allowed: number;
  waited: number;
  refused: number;
}
