// Generated from site/static/schemas/v2 by `SPECCODER_BLESS=1 cargo test -p speccoder ui_types`. Do not edit.

/** What the fix's visible acceptance set came from. */
export interface AcceptanceRef {
  /** The record it reused. */
  pre_frozen: PreFrozenRef;
  /** `bug_confirmation` or `person_test`. */
  source: AcceptanceSource;
}

/** Where a task's visible acceptance set came from. */
export type AcceptanceSource = "authored" | "review" | "bug_confirmation" | "person_test";

/** The acceptance set's strength, as recorded with a verdict. */
export interface AcceptanceStrength {
  /**
   * Visible and held-out together: what the verdict rests on (equal to
   * `visible` without held-out tests). Mutants run with the visible
   * command only, so its mutant counts are a lower bound.
   */
  combined: Strength;
  /** How long the stage took. */
  duration_ms: number;
  /** The gate. */
  gate: Gate;
  /** A held-out set was overlaid. */
  held_out: boolean;
  /** The mutants. */
  mutants?: Mutants | null;
  /** The near misses. */
  near_misses: NearMiss[];
  /** Why not (fully) measured, or what was dropped. */
  notes?: string[];
  /** The permitted alternatives. */
  permitted: Permitted[];
  /** Rulings that applied. */
  rulings?: Ruling[];
  /** The near-miss call's run. */
  run_id?: string | null;
  /** The near-miss call's tokens. */
  usage?: unknown;
  /** The visible set alone. */
  visible: Strength;
}

/** One thing a person can do now. */
export interface Action {
  /** Why it cannot run now; `None`: it can. */
  blocked: string | null;
  /** The equivalent command; `"…"` marks the input. */
  command: string;
  /**
   * It leaves the machine (a push, a pull request) or cannot be undone
   * (a discard): what an interface says in a confirmation before it runs it.
   */
  confirm: string | null;
  /** `next`, `plan`, `work`, `task`, `feature` or `review`. */
  group: string;
  /** What it is, independent of its wording ([`identity`]). */
  id: ActionId;
  /** What to ask for before it runs (`Describe the feature`). */
  input: string | null;
  /** What it does, in a few words. */
  label: string;
  /**
   * A planning skill's part in its format's planning; `None` for
   * everything else.
   */
  role: SkillRole | null;
  /**
   * How an interface runs it; `None`: run the command yourself (it leaves
   * the machine, or it is not something an interface hosts).
   */
  run: Runnable | null;
  /** Why, or what follows (shown beside it). */
  why: string;
}

/**
 * What an action is, independent of its wording: its kind and what it
 * is about (the task, the skill, the issue). An interface finds an action
 * by this, never by its label.
 */
export interface ActionId {
  /** Its kind. */
  kind: ActionKind;
  /**
   * What it is about: a task's id, a skill's name, an issue's number,
   * a review's `verdict` (`verdict:first id`), a command's words.
   */
  subject: string | null;
}

/**
 * What kind of action it is: one per [`Runnable`] kind (a run split by
 * whether it names a task), plus [`ActionKind::Command`] for an action an
 * interface does not run itself. Exhaustive at any one version; a new
 * kind arrives with its `Runnable` variant (the *adding a kind* contract
 * of `speccoder_runtime::actions::perform`).
 */
export type ActionKind = "skill" | "run_task" | "run_feature" | "converge" | "commit" | "approve" | "reject" | "accept" | "skip" | "nudge" | "rule" | "review" | "issue_comment" | "quick_change" | "keep_set_aside" | "drop_set_aside" | "fix_setup" | "feature" | "pull_request" | "bug_fix" | "bug_confirm" | "bug_run" | "bug_keep" | "allow_always" | "widen_policy" | "decide_batch" | "oracle_tests" | "approve_and_run" | "comment" | "comments_send" | "revision_undo" | "code_nudge" | "acceptance_at" | "preset_add" | "plan_through" | "new_project" | "undo_new_project" | "clone_repository" | "open_sample" | "sample_checks" | "command";

/**
 * Result of `speccoder actions`: what a person can do now in the
 * project — the list every interface renders — and where the feature is
 * on its journey.
 */
export interface ActionsReport {
  /** Every action, by group, the most urgent first. */
  actions: Action[];
  /** The feature's journey (spec → plan → tasks → commit → run → review). */
  journey: JourneyStep[];
}

/** Two authors read a scenario differently. */
export interface Ambiguity {
  /** The first author's reading (its tests are the ones kept). */
  a: ReadingText;
  /** The second author's. */
  b: ReadingText;
  /** The person's choice; `None` while it is open. */
  resolution: Resolution | null;
  /** The scenario. */
  scenario: string;
}

/** Where an annotation lands: a file and a 1-based line, at the head. */
export interface Anchor {
  /** 1-based line. */
  line: number;
  /** Repository-relative path. */
  path: string;
}

/**
 * A pending call as a person sees it: what, why, what it can reach, what
 * the model claimed, and what approving it grants.
 */
export interface ApprovalView {
  /** The diff, the file's head, the command or the arguments. */
  body_lines: string[];
  /** What the model said when it asked (a claim). */
  claim: string | null;
  /** `write`, `edit`, `run`, `network` or `mcp`. */
  kind: string;
  /** The object's lines. */
  object: string[];
  /** The choices offered, in order. */
  options: string[];
  /** `low`, `medium` or `high`. */
  risk: string;
  /** What the call can reach. */
  sandbox: string;
  /** What `approve` grants: `for the rest of T004: this file, any content`. */
  scope: string;
  /** Where arguments were copied from. */
  sources: string[];
  /** The path, the whole command, or the MCP tool. */
  subject: string;
  /** `Edit this file?`, `Run this command?`, …. */
  title: string;
}

/** Result of `speccoder report --approvals`, `approve` and `reject`. */
export interface ApprovalsReport {
  /**
   * Runs whose asking budget is used: their waiting calls, decided
   * together (`speccoder approvals batch`).
   */
  batch?: BatchView[];
  /** Decisions recorded for the feature's tasks. */
  decided: DecidedCall[];
  /** Feature directory name. */
  feature: string;
  /** Calls that stopped an open task and await a decision. */
  pending: PendingCall[];
  /**
   * Why the calls waiting in the journal could not be read (another
   * speccoder holds it, …): `pending` may then be incomplete — unknown,
   * never "nothing waiting".
   */
  unreadable: string | null;
}

/**
 * What the approve sheet shows besides the requirements: who can approve
 * and run now, and what autopilot would hold.
 */
export interface ApproveSheet {
  /** Decision lines the approval records. */
  approves: string[];
  /** Why autopilot is disabled (no enforced sandbox); `None`: it can run. */
  autopilot_blocked: string | null;
  /**
   * Why *Approve and Run* is disabled now (each a line naming its
   * cause); empty: it can run.
   */
  blocked: string[];
  /** The grants autopilot may hold, none ticked by default. */
  grants: OfferedGrant[];
  /** The sandbox line (`Seatbelt, network off`). */
  sandbox: string | null;
  /**
   * The approval digest *Approve and Run* would record now (decision
   * lines and frozen manifests): the click sends it back and is refused
   * when it no longer matches. `None`: it cannot be computed here.
   */
  sha256: string | null;
}

/** Result of `speccoder spec archive` (an OpenSpec change). */
export interface ArchiveReport {
  /** Where OpenSpec moved the change. */
  archived_as: string | null;
  /** The change. */
  change: string;
  /** The commit `archive <id>`. */
  commit: string | null;
  /** Who confirmed. */
  confirmed_by: string | null;
  /** What archiving changes (`openspec show <id> --diff`), when shown. */
  diff: string | null;
  /** OpenSpec's own `{severity, code, message, fix}` from its last answer. */
  openspec_status: unknown;
  /**
   * `probe`, `incomplete`, `unverified`, `awaiting_confirmation`,
   * `declined`, `archived` or `failed`.
   */
  stage: string;
  /** OpenSpec's counts of added, modified, removed and renamed requirements. */
  totals: unknown;
  /** Tasks ticked without a speccoder verdict commit. */
  unverified_tasks: string[];
}

/** Presence of a feature's artifacts. */
export interface Artifacts {
  /** `checklists/` */
  checklists: boolean;
  /** `contracts/` */
  contracts: boolean;
  /** `data-model.md` */
  data_model: boolean;
  /** `plan.md` */
  plan: boolean;
  /** `quickstart.md` */
  quickstart: boolean;
  /** `research.md` */
  research: boolean;
  /** `spec.md` */
  spec: boolean;
  /** `tasks.md` */
  tasks: boolean;
}

/** Result of `speccoder attest verify` and `attest sign`. */
export interface AttestReport {
  /** Feature directory name. */
  feature: string;
  /** One per object. */
  objects: ObjectVerification[];
  /** Count per outcome word. */
  totals: Record<string, number>;
  /**
   * Where trust came from: the allowed-signers file, whether GPG's keyring
   * was used, the expected Sigstore identity.
   */
  trust: unknown;
}

/**
 * Result of `speccoder auth`: who speccoder acts as on a GitHub
 * host, and where its token comes from. The token itself is never shown.
 */
export interface AuthReport {
  /** `github.com` or an Enterprise host. */
  host: string;
  /** The account, when signed in. */
  login: string | null;
  /** `GH_TOKEN`, `GITHUB_TOKEN`, `the keychain`, when signed in. */
  source: string | null;
}

/** The author's own words, kept only as a labelled claim. */
export interface AuthorClaim {
  /** Always `author`. */
  by: string;
  /** The words. */
  text: string;
}

/** Who wrote a shown test. */
export type AuthoredBy = "model" | "person";

/** One call an autopilot run refused instead of asking. */
export interface AutopilotDenial {
  /** `write`, `command`, `network`, `mcp` (empty: none resolved). */
  class: string;
  /** What it reached (`src/a.rs`, `cargo add x`). */
  object: string;
  /** `autopilot: not asked — listed for you`. */
  reason: string;
  /** The task. */
  task: string;
  /** The model turn. */
  turn: number;
}

/** How the person decided the reproductions. */
export type Basis = "shown" | "named_test";

/** One class of a batch. */
export interface BatchGroup {
  /** The waiting calls' ids. */
  calls: string[];
  /** `write`, `command`, `network` or `mcp`. */
  class: string;
}

/** A batch decision. */
export type BatchVerdict = "approve" | "reject";

/** A run whose asking budget is used, with its waiting calls by class. */
export interface BatchView {
  /**
   * SHA-256 over the sorted digests of every call listed
   * ([`PendingCall::digest`]): a decision names it and is refused when
   * the list, or the object of a call in it, changed.
   */
  digest: string;
  /** The calls by class, in the order write, command, network, mcp. */
  groups: BatchGroup[];
  /** The run. */
  run: string;
}

/** Result of `speccoder bench import`. */
export interface BenchReport {
  /** Instances turned into cases. */
  imported: string[];
  /** The suite directory written. */
  out: string;
  /** Instances not imported, with why. */
  skipped: string[];
}

/** Result of `speccoder attest verify --branch` / `speccoder attest badge`. */
export interface BranchReport {
  /** Unsigned evidence was accepted (`--require-signed=false`). */
  accepted_unsigned: boolean;
  /** The merge base with the given base, when one was given. */
  base: string | null;
  /** The conclusion. */
  conclusion: Conclusion;
  /** The units checked, in order. */
  features: string[];
  /** The commit checked. */
  head: string;
  /** Which units were covered. */
  mode: Mode;
  /** Predicate fields or revisions seen but unknown here (tolerated). */
  newer_fields: string[];
  /** Rules this verifier version does not evaluate yet. */
  not_evaluated: string[];
  /** Bug homes touched without a verdict commit (informational). */
  open_no_claim: string[];
  /** `regression_only` and `attested` fail too. */
  require_verified: boolean;
  /** Evidenced (not stale) over all ticks; `None` without ticks. */
  share: number | null;
  /** Tasks skipped by a person's decision (informational). */
  skipped: SkippedTask[];
  /** Every tick (and trailer-only task), in document order per unit. */
  ticks: TickReport[];
  /** Counts by outcome. */
  totals: BranchTotals;
  /**
   * Requirement → task → test → verdict, from the signed `trace[]`,
   * with a row (no task) for each requirement no tick cites.
   */
  trace: TraceRow[];
  /**
   * The trust used: the allowed-signers file and the revision it was
   * read at, the Sigstore identity and issuer.
   */
  trust: unknown;
}

/** Counts by outcome, in the verdict bar's order. */
export interface BranchTotals {
  /** Evidenced `attested`. */
  attested: number;
  /** Regressed (reserved). */
  regressed: number;
  /** Evidenced `regression_only`. */
  regression_only: number;
  /** Skipped by a person (not ticks). */
  skipped: number;
  /** Stale. */
  stale: number;
  /** Unverifiable. */
  unverifiable: number;
  /** Evidenced `verified`. */
  verified: number;
}

/** The bug's home, as the format adapter chose it. */
export interface BugHome {
  /** Which structure. */
  kind: HomeKind;
  /** Project-relative directory (or file). */
  location: string;
  /** Why this one, as shown and recorded. */
  reason: string;
}

/** The bug's outcome; distinct from a task's verdict. */
export type BugOutcome = "recorded" | "awaiting_confirmation" | "cannot_reproduce" | "already_satisfied" | "unchanged_not_holding" | "fixed" | "failed";

/**
 * A bug: where it lives, the person's triad, the reproductions and how
 * far it got. The `bug` payload, and what speccoder keeps of it.
 */
export interface BugReport {
  /** What the fix's acceptance set came from. */
  acceptance: AcceptanceRef | null;
  /** Someone else's fix, judged. */
  candidate: Candidate | null;
  /** The code of the last stop or refusal. */
  code: string | null;
  /** The verdict commit. */
  commit: string | null;
  /** The digest waiting for the person's confirmation. */
  confirm_digest: string | null;
  /** The confirmation. */
  confirmation: Confirmation | null;
  /** The engine's hand-off, when it stopped. */
  handoff: Handoff | null;
  /** The held-out variants (never their content or path). */
  held_out: HeldOutVariant[];
  /**
   * Why no held-out variant is sealed (the author's call failed, none
   * was red): the verdict names it, and the confirmation says it first.
   */
  held_out_reason?: string | null;
  /** The sealed set holding them. */
  held_out_sealed: SealedRef | null;
  /** Where the format keeps it. */
  home: BugHome;
  /** The issue it came from. */
  issue: IssueRef | null;
  /** At most this many tries. */
  max_tries: number;
  /** How it stands. */
  outcome: BugOutcome;
  /** The tests selected to stay green. */
  pass_to_pass: PassToPass | null;
  /** The planning commit that recorded it. */
  recorded_commit: string | null;
  /** Rejections of earlier reproductions. */
  rejections: Rejection[];
  /** The reproductions (and *Unchanged* tests). */
  reproductions: Reproduction[];
  /** The requirement (`FR-003`, or `bug:<slug>`). */
  requirement: string;
  /** The revert candidate. */
  revert: RevertResult | null;
  /** The last run. */
  run_id: string | null;
  /** The special-case candidates. */
  special_case: SpecialCase[];
  /** Where it is. */
  stage: BugStage;
  /** The task (`T003`, or `bug:<slug>`). */
  task: string;
  /** A short title. */
  title: string;
  /** The person's words. */
  triad: Triad;
  /** The reproduce tries. */
  tries: ReproduceTry[];
  /** The unit, as the adapter names it (`007-bugfixes`, `bugs/<slug>`). */
  unit: string;
  /** The fix's verdict, once judged. */
  verdict: Verdict | null;
}

/** Where the bug is. */
export type BugStage = "record" | "reproduce" | "confirm" | "unchanged" | "fix" | "ship" | "done";

/** One confirmed reproduction. */
export interface BugTest {
  /**
   * Its content as confirmed; `None` when the kept copy is missing or no
   * longer matches its digest.
   */
  content: string | null;
  /** Project-relative test file. */
  file: string;
  /** Its class on the untouched code (`assertion`). */
  red_class: string;
  /** The statement it reproduces (`E1`) or guards (`U1`). */
  statement: string;
  /** The test's name. */
  test: string;
}

/**
 * A bug's confirmed reproductions as a bug unit's Tests tab shows them,
 * read only: a bug's tests are written and confirmed by its own pipeline.
 */
export interface BugTests {
  /** The person's confirmation of the reproductions. */
  confirmation: Confirmation;
  /** The reproductions, in the order confirmed. */
  reproductions: BugTest[];
  /** The bug's task (`T001`). */
  task: string;
  /** Its title. */
  title: string;
}

/** Someone else's fix, judged as the attempt. */
export interface Candidate {
  /** It was applied as the attempt. */
  applied: boolean;
  /** `sha256:<hex>` of the diff. */
  digest: string;
  /** The pull request's head commit. */
  head_sha?: string | null;
  /** The pull request's number. */
  number?: number | null;
  /** A patch or a pull request. */
  origin: CandidateOrigin;
  /** The patch file, as given. */
  path?: string | null;
  /** Why it was not. */
  refused?: CandidateRefusal | null;
}

/** Where a foreign fix came from. */
export type CandidateOrigin = "patch" | "pr";

/** Why a candidate was refused before it was applied. */
export interface CandidateRefusal {
  /** `BUG-CANDIDATE-JUDGE` or `BUG-CANDIDATE-APPLY`. */
  code: string;
  /** The judge paths it touches, or the hunks that did not apply. */
  paths: string[];
}

/** One skill plan through ran. */
export interface ChainStep {
  /** Its message. */
  message: string;
  /** Its agentplane run, when it ran. */
  run_id: string | null;
  /** The installed skill (`speckit-plan`). */
  skill: string;
  /** Its result's status (`ok`, `error`, …). */
  status: CommandStatus;
}

/**
 * Where plan through stopped: only where a person decides, at a
 * failure, or at the end.
 */
export type ChainStop =
  /** The spec leaves questions open: answer them first. */
  | {
    kind: "questions";
    /** Their ids (`Q1`, …). */
    questions: string[];
  }
  /** Review the spec: planning waits for the requirements' approval. */
  | {
    kind: "review";
    /** The requirements to approve, by id. */
    requirements: string[];
  }
  /** A skill failed or was refused; what ran before it is kept. */
  | {
    /** Its result's code. */
    code: string | null;
    kind: "failed";
    /** Its result's message. */
    message: string;
    /** The skill. */
    skill: string;
  }
  /** Done through the chosen step. */
  | {
    kind: "done";
    /** The plan checks of the task list, when one was written. */
    plan_checks: PlanCheck[];
  };

/** One check run. */
export interface CheckResult {
  /** The command, as run. */
  argv: string[];
  /** Wall-clock duration. */
  duration_ms: number;
  /** Process exit code, if it exited. */
  exit_code: number | null;
  /**
   * It failed once and passed when run again: named, never trusted as
   * task evidence.
   */
  flaky: boolean;
  /** Whether the check declares a JUnit report. */
  junit: boolean;
  /** Name from `speccoder.toml`. */
  name: string;
  /** Why the check was skipped, errored, or has no tests. */
  note: string | null;
  /** What happened. */
  status: CheckStatus;
  /** Last part of stderr (lossy UTF-8). */
  stderr_tail: string;
  /** Last part of stdout (lossy UTF-8). */
  stdout_tail: string;
  /** Tests from the JUnit report, if one was declared and freshly written. */
  tests: TestResult[] | null;
  /** Output exceeded the capture cap. */
  truncated: boolean;
}

/** Outcome of one check. */
export type CheckStatus = "passed" | "failed" | "timed_out" | "skipped" | "error";

/** One Spec Kit checklist (`specs/<feature>/checklists/*.md`). */
export interface ChecklistStatus {
  /** Items checked. */
  done: number;
  /** Path relative to the project root. */
  file: string;
  /** Items not checked. */
  open: number;
  /** Checklist items. */
  total: number;
}

export type Checklists = ChecklistStatus[];

/** One line comment of the Code view. */
export interface CodeComment {
  /** The line (1-based). */
  line: number;
  /** Project-relative path. */
  path: string;
  /** The code on that line. */
  quote: string;
  /** The person's words. */
  text: string;
}

/** One code, as `explain` shows it. */
export interface CodeInfo {
  /** `NEEDS-APPROVAL`. */
  code: string;
  /** Its section on the codes page. */
  docs: string;
  /** Its exit code. */
  exit_code: number;
  /** What it means and why it happens. */
  explain: string;
  /** How to resolve it (templates: `<task>`, `<feature>` are blanks). */
  fix: string[];
  /** Error, stop or warning. */
  kind: Kind;
  /** The status results with this code have. */
  status: CommandStatus;
  /** One line. */
  title: string;
}

/** The attempt's changed test files and whether the run collected them. */
export interface Collection {
  /** Every judged file, by path. */
  files: FileCollection[];
  /** The listings run, in order. */
  listing?: ListingRun[];
}

/**
 * A runner's collection rule: one sentence a person reads, and where it
 * came from.
 */
export interface CollectionRule {
  /** Where it came from (`Cargo.toml (default)`, `pyproject.toml [tool.pytest.ini_options]`). */
  source: string;
  /** The rule (``Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare)``). */
  text: string;
  /** The toolchain. */
  toolchain: Toolchain;
}

/** Command-specific payload of a result. */
export type CommandData = ProjectSnapshot | VerifyReport | WorktreeReport | ProposeReport | ReplayReport | RunReport | EvalReport | FeatureReport | ConvergeReport | StatusReport | ApprovalsReport | ControlReport | CostReport | PrReport | BenchReport | InitReport | SkillsReport | SkillRunReport | ChecklistStatus[] | ArchiveReport | InPlaceReport | AttestReport | ExplainReport | SchemaReport | DoctorReport | FeaturesReport | UiReport | ConfigReport | IssuesReport | InboxReport | ReviewReport | AuthReport | BranchReport | EvidenceTour | ActionsReport | BugReport | PolicyCoverage | CommentsView | PresetReport | NewProjectReport | SampleReport | SimilarReport | RungSuggestion | PlanningChainReport;

/**
 * The envelope as it is serialised — the schema of [`CommandResult`]
 * (whose fields are private and whose serialisation is written by hand).
 */
export interface CommandResult {
  /**
   * The catalog code: present for every non-ok result and for a
   * degraded success.
   */
  code?: string | null;
  /** The command. */
  command: string;
  /** The command's payload. */
  data?: CommandData | null;
  /** The code's section on the codes page. */
  docs?: string | null;
  /** The process exit status (always the status's). */
  exit_code: ExitCode;
  /** How to resolve it: commands, or `→ ` lines for a person. */
  fix?: string[] | null;
  /** One line, for a person. */
  message: string;
  /** The steps that continue. */
  next?: NextStep[] | null;
  /** The name of `data`'s schema (`schemas/v2/<payload>.json`). */
  payload?: string | null;
  /** The fix speccoder can apply itself, on the person's yes. */
  remedy?: Remedy | null;
  /** The envelope's version. */
  schema_version: number;
  /** The outcome class. */
  status: CommandStatus;
}

/** Outcome class of a command. */
export type CommandStatus = "ok" | "usage_error" | "error" | "not_a_project" | "diagnostics_error" | "selection_needed" | "config_error" | "verification_failed" | "git_precondition" | "not_found" | "run_failed" | "replay_diverged" | "task_not_ready" | "needs_human" | "eval_failed" | "not_replayable" | "attestation_failed";

/** What an anchored comment points at. */
export type CommentAnchor =
  /** A passage of a feature document. */
  | {
    /** Last line. */
    end_line: number;
    kind: "passage";
    /** Project-relative path. */
    path: string;
    /** The words selected. */
    quote: string;
    /** `sha256:<hex>` of the quote (filled when the draft is stored). */
    quote_sha256: string;
    /** Its section's heading path (`Requirements › Functional`). */
    section: string;
    /** First line (1-based). */
    start_line: number;
  }
  /** A line of the spec's changes since the last commit. */
  | {
    kind: "diff_line";
    /** The line (1-based) on that side. */
    line: number;
    /** Project-relative path. */
    path: string;
    /** The line's text. */
    quote: string;
    /** `old` or `new`. */
    side: string;
  }
  /** A line of a shown visible test. */
  | {
    kind: "test_line";
    /** The line (1-based). */
    line: number;
    /** The test's path. */
    path: string;
    /** The line's text. */
    quote: string;
    /** The requirement. */
    req: string;
  }
  /** A finding the review shows. */
  | {
    /** Its id. */
    id: string;
    kind: "finding";
    /** `analyze` or `review`. */
    source: string;
    /** Its text. */
    text: string;
  };

/** One comment in the tray. */
export interface CommentDraft {
  /** What it is about. */
  anchor: CommentAnchor;
  /** Unix seconds. */
  at: number;
  /** Who. */
  by: string;
  /** `c1`, `c2`, … */
  id: string;
  /** The person kept it although its quote moved. */
  kept: boolean;
  /** Where it is. */
  state: DraftState;
  /** The person's words. */
  text: string;
}

/** What a comment action does. */
export type CommentOp = "add" | "delete" | "keep";

/** Result of `speccoder review comments`: the tray and the last turn. */
export interface CommentsView {
  /** Every draft not yet sent, in order. */
  drafts: CommentDraft[];
  /** The feature. */
  feature: string;
  /** The skill a send runs (`speckit-clarify`), when the format has one. */
  revise_with: string | null;
  /** The last revision turn. */
  turn: RevisionView | null;
}

/** What changed relative to a baseline (pass-to-pass). */
export interface Comparison {
  /** Tests collected now. */
  count_after: number;
  /** Tests collected in the baseline. */
  count_before: number;
  /** Checks failing now only because of tests that already failed in the baseline. */
  failing_as_before: string[];
  /** Tests not in the baseline. */
  new_tests: string[];
  /** Tests failing in both runs. */
  pre_existing: string[];
  /** Tests that passed in the baseline and now fail or are missing. */
  regressions: Regression[];
}

/** The branch's conclusion. */
export type Conclusion = "success" | "failure" | "neutral";

/** One key: what it is, what it is set to, and what it is otherwise. */
export interface ConfigKey {
  /** What it does. */
  about: string;
  /** For `choice`: the allowed values. */
  choices: string[];
  /**
   * The value speccoder uses when the key is absent; none for keys
   * without a default (unset means off, or the key is required).
   */
  default: unknown;
  /** The key, e.g. `max_turns`; dotted below a sub-table (`sigstore.identity`). */
  key: string;
  /**
   * `bool`, `integer`, `number`, `string`, `choice`, `duration`, `argv`,
   * `argv_list` or `string_list`.
   */
  kind: string;
  /** For numbers: the largest allowed. */
  max: number | null;
  /** For numbers: the smallest allowed. */
  min: number | null;
  /** The section (or table) is refused without it. */
  required: boolean;
  /** The value in the file, when set. */
  value: unknown;
}

/**
 * Result of `speccoder config`: the project's configuration as a catalogue
 * of every key, each with its type, default and value (when set).
 */
export interface ConfigReport {
  /**
   * Whether it is committed as it is on disk (speccoder reads committed
   * configuration only, so an uncommitted change is not yet in force).
   */
  committed: boolean;
  /** The uncommitted change, as a diff, when there is one. */
  diff: string | null;
  /** The first parser's complaint, when not valid. */
  error: string | null;
  /** Whether it exists. */
  exists: boolean;
  /** The file, relative to the project root. */
  file: string;
  /** Sections, in the reference's order. */
  sections: ConfigSection[];
  /** Whether every parser accepts it. */
  valid: boolean;
}

/** One section of `speccoder.toml`. */
export interface ConfigSection {
  /** What the section decides. */
  about: string;
  /** The keys (for a named section: the keys of each table, unset). */
  keys: ConfigKey[];
  /** The table name, e.g. `model`, or `checks` for `[checks.<name>]`. */
  name: string;
  /**
   * Named tables (`[checks.<name>]`, `[mcp.<name>]`): `keys` describe
   * one, `tables` hold the configured ones.
   */
  named: boolean;
  /** The configured named tables. */
  tables: ConfigTable[];
  /** A title for people, e.g. `Model`. */
  title: string;
}

/** One configured named table, e.g. `[checks.test]`. */
export interface ConfigTable {
  /** Its keys, with values. */
  keys: ConfigKey[];
  /** Its name, e.g. `test`. */
  name: string;
}

/** The person's one confirmation, bound to what they saw. */
export interface Confirmation {
  /** Unix seconds. */
  at: number;
  /** Seen or named. */
  basis: Basis;
  /** Who. */
  by: string;
  /**
   * `sha256:<hex>` over the reproductions shown (their files and
   * statement ids).
   */
  digest: string;
  /** The triad's digest when confirmed: a different one lapses this. */
  triad_digest: string;
  /** Through what. */
  via: Via;
}

/** State of the project constitution. */
export type ConstitutionState = "present" | "unfilled" | "absent";

/** One part handed to a call. */
export interface ContextEntry {
  /** Their size. */
  bytes: number;
  /** Cut to a budget. */
  cut: boolean;
  /** What it is. */
  kind: ContextKind;
  /** A path, an id or a part pointer (`/bundle/plan`). */
  reference: string;
  /** Hex SHA-256 of the exact bytes given. */
  sha256: string;
  /**
   * Where it came from: `person`, `worktree`, `speccoder://<producer>/<name>`,
   * `github:<owner>/<repo>#<n>`.
   */
  source: string;
}

/** What a part is. A new kind is a new variant, never a free string. */
export type ContextKind = "constitution" | "plan" | "story" | "task" | "rejection" | "retry_digest" | "issue" | "scenario" | "note" | "comment" | "test_layout";

/** Every part of one call, sorted by kind and reference. */
export interface ContextManifest {
  /** Hex SHA-256 of [`ContextManifest::canonical`]. */
  digest: string;
  /** The parts. */
  entries: ContextEntry[];
}

/** Result of `accept`, `skip` and `nudge`. */
export interface ControlReport {
  /** `accepted`, `skipped`, `unskipped`, `nudged` or `nudges_cleared`. */
  action: string;
  /** Feature directory name. */
  feature: string;
  /** The commit or ref that records it, when it lives in git. */
  record: string | null;
  /** Task ID. */
  task: string;
  /** The person's reason or note. */
  text: string | null;
}

/** Result of `speccoder converge`. */
export interface ConvergeReport {
  /** Tasks speccoder appended, as `T0NN text`. */
  appended: string[];
  /** The commit on `speccoder/<feature>` holding the appended section. */
  commit: string | null;
  /** Feature directory name. */
  feature: string;
  /** Every finding as the model reported it (its claim, not evidence). */
  findings: unknown[];
  /** Extension hooks run, offered or skipped around it. */
  hooks: string[];
  /**
   * `converged`, `tasks_appended`, `needs_review` (only unrequested
   * findings), `no_progress`, `round_limit` or `failed`.
   */
  outcome: string;
  /** Findings that repeat an earlier convergence task: no progress. */
  repeated: unknown[];
  /** This round's number (1 = the first Convergence phase). */
  round: number;
  /** The agentplane run of the assessment, if one ran. */
  run_id: string | null;
  /** Every tool call with the policy's decision. */
  tool_calls: unknown[];
  /** `unrequested` findings: for a person, never turned into tasks. */
  unrequested: unknown[];
  /** Token usage of the assessment. */
  usage: unknown;
}

/** One convergence round, from its commit. */
export interface ConvergenceRound {
  /** The commit that appended it. */
  commit: string;
  /** Round number. */
  round: number;
  /** The appended tasks, as `T0NN text`. */
  tasks: string[];
}

/** Result of `speccoder report --cost`. */
export interface CostReport {
  /** Convergence assessments. */
  convergence: Usage;
  /** Feature directory name. */
  feature: string;
  /** The model the prices apply to, when prices are configured. */
  priced_model: string | null;
  /** Per task, every attempt counted (passed and saved). */
  tasks: TaskCost[];
  /** Everything. */
  total: Usage;
}

/** Rejected of judged, for one source. */
export interface Counts {
  /** Judged (unviable excluded). */
  judged: number;
  /** Rejected (timeouts included). */
  rejected: number;
}

/** How calls of one argv were decided. */
export interface CoverageDecision {
  /** How often. */
  count: number;
  /** `allowed by rule`, `approved by a person`, `waiting`. */
  decision: string;
}

/** A prefix entry. */
export interface CoverageEntry {
  /** `commands.allow_prefix` or `sandbox.network_allow_prefix`. */
  key: string;
  /** The prefix. */
  prefix: string[];
}

/** One argv a prefix would newly allow. */
export interface CoverageRow {
  /** The argv. */
  argv: string[];
  /** How often the journal records it. */
  count: number;
  /** How it was decided then, with how often. */
  decided: CoverageDecision[];
  /** Seconds since the epoch of the latest run that called it. */
  last_at: number | null;
  /**
   * A rule asks for it (or denies it) whatever the list says: the
   * rule's reason. The row stays so.
   */
  still_asks: string | null;
}

/** What decided a file's state. */
export type DecidedBy = "static" | "junit" | "listing";

/** A person's decision. */
export interface DecidedCall {
  /** Who decided, as `name (via)`. */
  by: string | null;
  /** The waiting call it decided, when it was one. */
  call: string | null;
  /** The editor decided in, over the Agent Client Protocol. */
  client?: string | null;
  /** What it covers. */
  covers: string;
  /** `approved` or `rejected`. */
  decision: string;
  /** Nobody decided: the deadline passed and the call was denied. */
  expired: boolean;
  /** An approval's note to the model. */
  note: string | null;
  /** The object it decided on. */
  object: ObjectView | null;
  /** Who decided; `None` for an expiry. */
  operator: OperatorView | null;
  /** The reason, for a rejection. */
  reason: string | null;
  /** The run it belongs to (a grant covers nothing after it). */
  run: string | null;
  /** How far an approval reaches: `once`, `task` or `run`. */
  scope: string | null;
  /** Task ID. */
  task: string;
  /**
   * Through what: an origin of `actions::Via` (`terminal`, `web`,
   * `acp`, …).
   */
  via: string | null;
}

/** One requirement of a change's spec delta. */
export interface DeltaRequirement {
  /** The capability (`openspec/specs/<capability>/spec.md`). */
  capability: string;
  /** Its heading's line. */
  line: number;
  /** The requirement's name (the new name for a rename). */
  name: string;
  /** `added`, `modified`, `removed` or `renamed`. */
  operation: string;
  /** The delta file, relative to the project root. */
  path: string;
  /** Its scenarios' names. */
  scenarios: string[];
}

/** One direct dependency. */
export interface Dependency {
  /** The task depended on. */
  id: string;
  /** Where the edge comes from. */
  kind: DependencyKind;
  /** The ID does not exist in the task list. */
  missing: boolean;
}

/** Origin of a dependency. */
export type DependencyKind = "structural" | "explicit";

/** How dependencies within a phase are determined. */
export type DependencyMode = "structural" | "declared" | "chain";

/** A problem found while discovering or parsing. */
export interface Diagnostic {
  /** Stable code, e.g. `tasks.duplicate-id`. */
  code: string;
  /** What to do about it. */
  hint: string | null;
  /** 1-based line, if known. */
  line: number | null;
  /** What is wrong. */
  message: string;
  /** Workspace-relative file, if the problem is in one. */
  path: string | null;
  /** How serious it is. */
  severity: Severity;
}

/** What the next attempt is told about the previous one. */
export interface Digest {
  /** The attempt this digest describes (1-based). */
  attempt: number;
  /** Checks that did not pass. */
  checks: FailedCheck[];
  /** Paths the previous attempt changed; its changes were discarded. */
  discarded: string[];
  /** The verdict's reasons. */
  reasons: string[];
  /**
   * Test files it wrote that never ran: one line each — the path, the
   * runner's rule and the ways out.
   */
  uncollected?: string[];
}

/** One doctor check. */
export interface DoctorCheck {
  /** `git`, `repository`, `project`, `config`, `key`, `sandbox`, … */
  check: string;
  /** The catalog code of a failure or warning. */
  code: string | null;
  /** What was found. */
  detail: string;
  /** How to resolve it. */
  fix: string[];
  /** Pass, warn or fail. */
  level: DoctorLevel;
  /** The fix speccoder can apply itself, on the person's yes. */
  remedy?: Remedy | null;
  /** The tasks a plan check names. */
  tasks?: string[];
}

/** How a doctor check came out. */
export type DoctorLevel = "pass" | "info" | "warn" | "fail";

/** Result of `speccoder doctor`. */
export interface DoctorReport {
  /** Every check, in order. */
  checks: DoctorCheck[];
  /** `macos aarch64`. */
  os: string;
  /** speccoder's version. */
  speccoder: string;
}

/** Where a draft comment is. */
export type DraftState = "draft" | "outdated" | "sent";

/**
 * What changed since a tick: the tick's commit holds the intent
 * it was judged against; the checkout's `HEAD` holds the intent now.
 */
export interface Drift {
  /**
   * The intent files that differ (project-relative), task lists aside.
   * Empty when the intent is not in git (in place, ignored): it changed,
   * which file is not known.
   */
  files: string[];
  /**
   * The requirements the task cites whose line changed, appeared or went
   * away: the evidence no longer speaks for them. Empty: what changed is
   * elsewhere in the intent.
   */
  requirements: RequirementDrift[];
}

/** A failed attempt that was undone and retried. */
export interface EarlierAttempt {
  /** The attempt's status. */
  attempt_status: string;
  /** What the next attempt was told. */
  digest: Digest;
  /** The failure's fingerprint. */
  fingerprint: string;
  /** The agentplane run of the attempt. */
  run_id: string;
  /** Token usage of the attempt. */
  usage: unknown;
  /** The verifier's verdict. */
  verdict: Verdict | null;
}

/**
 * The envelope as it is serialised — the schema of [`CommandResult`]
 * (whose fields are private and whose serialisation is written by hand).
 */
export interface Envelope {
  /**
   * The catalog code: present for every non-ok result and for a
   * degraded success.
   */
  code?: string | null;
  /** The command. */
  command: string;
  /** The command's payload. */
  data?: CommandData | null;
  /** The code's section on the codes page. */
  docs?: string | null;
  /** The process exit status (always the status's). */
  exit_code: ExitCode;
  /** How to resolve it: commands, or `→ ` lines for a person. */
  fix?: string[] | null;
  /** One line, for a person. */
  message: string;
  /** The steps that continue. */
  next?: NextStep[] | null;
  /** The name of `data`'s schema (`schemas/v2/<payload>.json`). */
  payload?: string | null;
  /** The fix speccoder can apply itself, on the person's yes. */
  remedy?: Remedy | null;
  /** The envelope's version. */
  schema_version: number;
  /** The outcome class. */
  status: CommandStatus;
}

/** One judged case. */
export interface EvalCase {
  /** The attempt status. */
  attempt_status: string | null;
  /** Attempts made at the task, retries included. */
  attempts: number;
  /** `correct`, `false_completion`, `not_completed`, `honest_stop`, `caught`, `cheat`, `invalid_case` or `error`. */
  classification: string;
  /**
   * What the case expects: `complete`, `stop` or `reject`; a bug case also
   * `already_satisfied` or `cannot_reproduce`.
   */
  expect: string;
  /** An attempt passed the visible checks and failed the held-out tests. */
  gap: boolean;
  /** The held-out tests' part (`acceptance.held_out` of the run report). */
  held_out: unknown;
  /** The model's own closing report, or its reason for stopping. */
  model_said: string | null;
  /** Whether the gaming monitor flagged the attempt, when it ran. */
  monitor_flagged: boolean | null;
  /** Directory name. */
  name: string;
  /** Whether the held-out oracle passed (when it ran). */
  oracle_passed: boolean | null;
  /** The run's status word and message. */
  outcome: string;
  /** Stopped because a test rejected a permitted alternative. */
  over_strict: boolean;
  /**
   * Whether the recorded attempt replays strictly from its journal;
   * `None` when there is no journaled run (external executors).
   */
  replays: boolean | null;
  /** The round of a repeated suite run (1-based). */
  round: number;
  /** The attempt's run id, when one ran. */
  run_id: string | null;
  /** The combined acceptance strength, when measured. */
  strength?: number | null;
  /** The attempt's tool calls with the policy's decisions. */
  tool_calls: unknown[];
  /** Token usage of the attempt. */
  usage: unknown;
  /** The verifier's verdict. */
  verdict: Verdict | null;
}

/** Result of `speccoder eval`. */
export interface EvalReport {
  /** One entry per case, in name order. */
  cases: EvalCase[];
  /** The model the suite ran with, `provider/model`. */
  model: string;
  /** Counts per classification. */
  summary: EvalSummary;
}

/** Suite counts and metrics. */
export interface EvalSummary {
  /** Cases run. */
  cases: number;
  /** `expect = "reject"` cases that were not ticked. */
  caught: number;
  /** Impossible cases that finished with green checks. */
  cheats: number;
  /** Completed and right. */
  correct: number;
  /** Completed with a passing verdict but wrong. */
  false_completions: number;
  /**
   * Attempts that passed visibly, and how many of them the held-out tests
   * failed; `None` when no held-out test ran.
   */
  held_out_gap: HeldOutGap | null;
  /** Ticked cases whose held-out tests passed, against the oracle. */
  held_out_oracle: HeldOutOracle | null;
  /** Impossible cases that stopped for a person. */
  honest_stops: number;
  /** Cases that could not be judged. */
  invalid_or_error: number;
  /**
   * The median combined acceptance strength over verified cases that
   * measured it; `None`: not measured.
   */
  median_acceptance_strength: number | null;
  /** How well the gaming monitor did, when it ran on any case. */
  monitor: MonitorScore | null;
  /** Possible cases that stopped or failed verification. */
  not_completed: number;
  /**
   * Cases stopped for a person because a test rejected a permitted
   * alternative.
   */
  over_strict_stops: number;
  /** correct / (correct + false completions), when defined. */
  precision: number | null;
  /** Recorded attempts checked by strict replay, and how many replayed. */
  replay_checked: number;
  /** Of those, the ones that replayed exactly. */
  replayed: number;
  /** Input + output tokens per correct case, when any. */
  tokens_per_correct: number | null;
}

/** One line of the stream. */
export type EventLine = {
  /** 1, 2, 3, … — a gap means a lost line. */
  seq: number;
  /** RFC 3339 UTC, milliseconds. */
  ts: string;
  /** The envelope version: the result envelope's `schema_version`. */
  v: number;
} & (
  /** The first line: who is speaking, and what the stream supports. */
  | {
    /** Feature-detect on these, never on the version. */
    capabilities: string[];
    /** The command. */
    command: string;
    /** speccoder's version. */
    speccoder: string;
    type: "session.started";
  }
  /** A feature run begins, with its plan. */
  | {
    /** Feature directory name. */
    feature: string;
    /** Every task, in document order. */
    tasks: PlannedTask[];
    type: "run.started";
  }
  /** A task's run begins. */
  | {
    /** Feature. */
    feature: string;
    /** Task ID. */
    task: string;
    /** Title. */
    title: string;
    type: "task.started";
  }
  /** A stage begins. */
  | {
    /** `1/2`, a short commit id. */
    detail: string | null;
    /** The stage. */
    stage: Stage;
    /** Task ID. */
    task: string;
    type: "stage.started";
  }
  /** A stage ends (or does not apply). */
  | {
    /** Detail, or why it was skipped. */
    detail: string | null;
    /** How long it took. */
    elapsed_ms: number | null;
    /** The stage. */
    stage: Stage;
    /** `passed`, `failed` or `skipped`. */
    state: StageState;
    /** The last lines of a failed check. */
    tail: string[];
    /** Task ID. */
    task: string;
    type: "stage.finished";
  }
  /** A run picks up an attempt a killed process left behind. */
  | {
    /** The last tool call. */
    last_call: string | null;
    /** Where it was. */
    stage: Stage | null;
    /** Task ID. */
    task: string;
    type: "task.resumed";
  }
  /** A tool call and the policy's decision. */
  | {
    /** `allow`, `deny`, `approval`, `unknown`, `stop`. */
    decision: string;
    /** Whether an allowed call succeeded. */
    ok: boolean | null;
    /** The last lines of a failed command. */
    tail: string[];
    /** A path, a command, a pattern. */
    target: string;
    /** Task ID. */
    task: string;
    /** The tool. */
    tool: string;
    /** Model turn, 1-based. */
    turn: number;
    type: "tool_call";
  }
  /** A call waits for a person. */
  | {
    /** The call id. */
    call: string;
    /** The object's class: `write`, `command`, `network`, `mcp`. */
    class: string;
    /** The object's short form (path, `argv#…`, `tool@…`). */
    object: string;
    /** Task ID. */
    task: string;
    /** The tool. */
    tool: string;
    type: "approval.requested";
    /** What waits. */
    what: string;
  }
  /** A person decided a call. */
  | {
    /** Who. */
    by: string;
    /** The call id. */
    call: string;
    /** The object's class. */
    class: string;
    /** `approve` or `reject`. */
    decision: string;
    /** `asserted`, `connected`, `expired`. */
    how: string;
    /** The object's short form. */
    object: string;
    /** `once`, `task`, `run`. */
    scope: string | null;
    /** Task ID. */
    task: string;
    type: "approval.decided";
  }
  /** Tokens so far in the current attempt (at most once a second). */
  | {
    /** Input tokens read from the cache. */
    cache_read_tokens: number;
    /** Input tokens written to the cache. */
    cache_write_tokens: number;
    /** Input tokens. */
    input_tokens: number;
    /** Output tokens. */
    output_tokens: number;
    /** Task ID. */
    task: string;
    type: "usage";
    /** USD, when priced. */
    usd: number | null;
  }
  /** A task's run ended. */
  | {
    /** Cache reads. */
    cache_read_tokens: number;
    /** Cache writes. */
    cache_write_tokens: number;
    /** Input tokens. */
    input_tokens: number;
    /** The run's message. */
    message: string;
    /** `verified`, `regression_only`, `failed`, `needs_approval`, … */
    outcome: string;
    /** Output tokens. */
    output_tokens: number;
    /** A regression-only tick: the reason with its details and wording. */
    regression?: RegressionOnly | null;
    /** A regression-only tick: the reason's name. */
    regression_reason?: string | null;
    /** The attempt's run id. */
    run: string | null;
    /** Task ID. */
    task: string;
    type: "task.finished";
    /** USD, when priced. */
    usd: number | null;
  }
  /** The attempt's changed test files were checked against what the run collected. */
  | {
    /** Each judged test file. */
    files: PublicCollectionFile[];
    /** The task. */
    task: string;
    type: "collection.checked";
  }
  /** The search for the tick that made already-passing tests pass ended. */
  | {
    /** The implementing task, `before_feature`, or `null`. */
    result: string | null;
    /** Acceptance runs it took. */
    runs: number;
    /** Task ID. */
    task: string;
    type: "regression.bisected";
  }
  /** What the model said in a turn: its account, a claim. */
  | {
    /** Task ID. */
    task: string;
    /** The words. */
    text: string;
    /** Model turn (1-based). */
    turn: number;
    type: "said";
  }
  /** The model's summary of its reasoning in a turn: its account, a claim. */
  | {
    /** Task ID. */
    task: string;
    /** The summary. */
    text: string;
    /** Model turn (1-based). */
    turn: number;
    type: "reasoning";
  }
  /** Anything else worth a line. */
  | {
    /** The line. */
    text: string;
    type: "notice";
  }
  /** A bug's triad was recorded. */
  | {
    /** The home's kind. */
    home: string;
    /** The requirement. */
    requirement: string;
    /** Its task. */
    task: string;
    /** The triad's digest. */
    triad_digest: string;
    type: "bug.recorded";
    /** The unit. */
    unit: string;
    /** Through what. */
    via: string;
  }
  /** One reproduce try ended. */
  | {
    /** The try. */
    n: number;
    /** Of at most. */
    of: number;
    /** Each statement's class. */
    per_statement: StatementClass[];
    /** Its task. */
    task: string;
    type: "bug.reproduce_try";
  }
  /** Every statement is reproduced. */
  | {
    /** The reproductions (never their content). */
    reproductions: ShownReproduction[];
    /** Its task. */
    task: string;
    type: "bug.reproduced";
  }
  /** The reproductions wait for the person. */
  | {
    /** The digest to confirm. */
    digest: string;
    /** The command. */
    next: string;
    /** Its task. */
    task: string;
    type: "bug.confirm_waiting";
  }
  /** The person confirmed them. */
  | {
    /** `shown` or `named_test`. */
    basis: string;
    /** Who. */
    by: string;
    /** The digest confirmed. */
    digest: string;
    /** Its task. */
    task: string;
    type: "bug.confirmed";
    /** Through what. */
    via: string;
  }
  /** The person rejected them. */
  | {
    /** The digest rejected. */
    digest: string;
    /** The person's reason. */
    reason: string;
    /** Its task. */
    task: string;
    type: "bug.confirm_rejected";
  }
  /** The pass-to-pass set was chosen. */
  | {
    /** Members. */
    count: number;
    /** `selected` or `none_selected`. */
    state: string;
    /** Its task. */
    task: string;
    type: "bug.unchanged_selected";
  }
  /** The revert and the special cases were judged. */
  | {
    /** `rejected`, `survived`, `unviable`. */
    revert: string;
    /** The special cases. */
    special_case: ShownSpecialCase[];
    /** Its task. */
    task: string;
    type: "bug.discrimination";
  }
  /** The bug's outcome. */
  | {
    /** The stop's code. */
    code: string | null;
    /** What to do next. */
    next: string[];
    /** `awaiting_confirmation`, `cannot_reproduce`, `fixed`, … */
    outcome: string;
    /** Its task. */
    task: string;
    type: "bug.outcome";
    /** The fix's verdict. */
    verdict: string | null;
  }
  /** Writing tests at review went one stage further. */
  | {
    /** This stage's step. */
    n: number;
    /** Of. */
    of: number;
    /** The requirement. */
    requirement: string;
    /** `writing`, `comparing`, `checking_red`, `held_out`. */
    stage: string;
    type: "oracle.stage";
  }
  /** An approval froze a requirement's tests. */
  | {
    /** The approval's digest. */
    approval_sha256: string;
    /** Who. */
    by: string;
    /** Held-out tests sealed with them. */
    held_out_count: number;
    /** The visible set's manifest digest. */
    manifest_sha256: string;
    /** The requirement. */
    requirement: string;
    type: "oracle.frozen";
    /** The unit. */
    unit: string;
    /** Through what. */
    via: string;
    /** Visible tests frozen. */
    visible_count: number;
  }
  /** A frozen oracle lapsed with its requirement's wording. */
  | {
    /** The wording now. */
    current_wording_sha256: string;
    /** The wording it was frozen on. */
    frozen_wording_sha256: string;
    /** The requirement. */
    requirement: string;
    type: "oracle.lapsed";
    /** The unit. */
    unit: string;
  }
  /** The last line: the command's result, as `--json` prints it. */
  | {
    /** The result envelope. */
    result: CommandResult;
    type: "result";
  }
  /** Plan through stopped where a person decides, at a failure or at the end. */
  | {
    /** `questions`, `review`, `failed`, `done`. */
    kind: string;
    /** The skill a continuation runs next. */
    next_skill: string | null;
    type: "planning.stopped";
    /** The unit, once it exists. */
    unit: string | null;
  }
  /** A step of a new project's plan began. */
  | {
    /** `local` or `network`. */
    access: string;
    /** Its command, exactly. */
    argv: string[];
    /** The step (`git_init`, `scaffold.1`, `format_init`, …). */
    step: string;
    type: "scaffold.started";
  }
  /** A step of a new project's plan ended. */
  | {
    /** How long it took. */
    duration_ms: number;
    /** Its exit status, when it ran. */
    exit: number | null;
    /** `done`, `skipped`, `failed`. */
    state: string;
    /** The step. */
    step: string;
    type: "scaffold.finished";
  }
  /** A new project was created and committed. */
  | {
    /** speccoder's commit. */
    commit: string;
    /** The spec format. */
    format: string;
    /** `new_folder` or `existing_repository`. */
    mode: string;
    /** SHA-256 of the folder's canonical path. */
    path_digest: string;
    /** The stack. */
    stack: string;
    type: "project.created";
    /** Through what. */
    via: string;
  }
  /** A project speccoder created was undone. */
  | {
    /** `trash` or `revert`. */
    kind: string;
    /** SHA-256 of the folder's canonical path. */
    path_digest: string;
    type: "project.undone";
    /** Through what. */
    via: string;
  });

/** Result of `speccoder attest tour` and `/api/tour`: one task's tour. */
export interface EvidenceTour {
  /** The task commit, when one carries the task's evidence. */
  commit: string | null;
  /** The unit. */
  feature: string;
  /** The six steps, in order — only the first for an unverifiable tick. */
  steps: TourStep[];
  /** Task ID. */
  task: string;
  /** The task's title. */
  title: string;
  /** For an unverifiable tick: why the evidence does not count. */
  unverifiable: string | null;
  /** The verdict the evidence claims, when any. */
  verdict: string | null;
}

/**
 * Process exit status of a command.
 *
 * Reserved ranges: `10..=19` for discovery and diagnostic failures, `20..=29`
 * for run outcomes. Values outside the named constants and those ranges are
 * not used.
 */
export type ExitCode = number;

/** Result of `speccoder explain`. */
export interface ExplainReport {
  /** The code asked for, or every code with `--list`. */
  codes: CodeInfo[];
}

/** A Spec Kit extension the person chose. */
export interface ExtensionChoice {
  /** `bundled`, `path` or `url`. */
  kind: ExtensionKind;
  /** A bundled name, a local path or a URL, as given. */
  source: string;
}

/** What an extension source is. */
export type ExtensionKind = "bundled" | "path" | "url";

/** One check that did not pass. */
export interface FailedCheck {
  /** The command, as run. */
  argv: string[];
  /** Output from the first error line on, capped. */
  excerpt: string;
  /** Failing test IDs, when the check reports tests. */
  failing_tests: string[];
  /** Name from `speccoder.toml` (or `acceptance`). */
  name: string;
  /** What happened. */
  status: CheckStatus;
}

/** One feature (or change) of the project, as `speccoder feature` lists it. */
export interface FeatureEntry {
  /** Which planning artifacts exist: `spec`, `plan`, `tasks`. */
  artifacts: string[];
  /** The current one. */
  current: boolean;
  /** Repository-relative directory. */
  dir: string;
  /** Ticked tasks. */
  done: number;
  /** Directory name, e.g. `001-greet`. */
  name: string;
  /** 1-based position, the number `speccoder feature <n>` takes. */
  number: number;
  /** All tasks (0: no task list yet). */
  tasks: number;
  /** The title its spec (or proposal) gives, when it has one. */
  title?: string | null;
}

/** A feature directory under `specs/`. */
export interface FeatureRef {
  /** Workspace-relative directory, e.g. `specs/002-speckit-inspect`. */
  dir: string;
  /** Directory name, e.g. `002-speckit-inspect`. */
  name: string;
  /** Numeric prefix as written, e.g. `002` or `20260319-143022`. */
  prefix: string;
}

/** Result of `speccoder run` without `--task`: a feature worked through. */
export interface FeatureReport {
  /** Every attempt, in order. */
  attempts: RunReport[];
  /**
   * The calls an autopilot run refused instead of asking: listed for
   * the person.
   */
  autopilot_denied?: AutopilotDenial[];
  /** Convergence rounds between task runs (`run --converge`), in order. */
  converge: ConvergeReport[];
  /** Feature directory name. */
  feature: string;
  /** The `full` check tier, run once after the run ticked the last task. */
  full_tier?: FullTier | null;
  /** Extension hooks run, offered or skipped around it. */
  hooks: string[];
  /** How the run asked for a person: `ask` or `autopilot`. */
  mode: RunMode;
  /** What parallel groups did: merged, conflicts, re-verification. */
  parallel: string[];
  /** Why the run stopped. */
  stop: string;
}

/** The selected feature's artifacts and tasks. */
export interface FeatureSnapshot {
  /** Which artifacts exist. */
  artifacts: Artifacts;
  /** The feature directory. */
  feature: FeatureRef;
  /** Dependencies and states of the tasks; present iff `tasks` is. */
  schedule: Schedule | null;
  /** The parsed task list, if `tasks.md` was readable. */
  tasks: ParsedTasks | null;
}

/**
 * Where a new feature's description comes from. Later rungs add theirs
 * (an Explore brief, a promoted quick change) when they are built.
 */
export type FeatureSource =
  /** The person's own words. */
  | {
    kind: "description";
  }
  /** A GitHub issue's description, verbatim. */
  | {
    kind: "issue";
    /** Its number. */
    number: number;
  };

/** Result of `speccoder feature`. */
export interface FeaturesReport {
  /** What this call did: `listed`, `switched` or `cleared`. */
  action: string;
  /** The current one's name. */
  current: string | null;
  /** Every feature, in order. */
  features: FeatureEntry[];
  /**
   * How it is current: `argument`, `environment`, `recorded`,
   * `only_feature` or `none`.
   */
  reason: string;
  /** The same in words (`recorded in .specify/feature.json`). */
  reason_text: string;
  /** `feature` or `change`. */
  unit: string;
}

/** One judged test file. */
export interface FileCollection {
  /** What decided it; `null` only when unknown. */
  decided_by: DecidedBy | null;
  /** The ways out, for this toolchain and file. */
  fixes?: string[];
  /** Workspace-relative path. */
  path: string;
  /** The runner's rule (not collected). */
  rule?: CollectionRule | null;
  /** Collected, not collected, or unknown. */
  state: FileState;
  /** The first 20 of their ids. */
  test_ids: string[];
  /** Tests from it that ran (skipped and ignored ones excluded). */
  tests: number;
  /**
   * Not collected: which part of the rule the file breaks; unknown: why
   * speccoder cannot tell.
   */
  why?: string | null;
}

/** Whether the run collected a test from a file. */
export type FileState = "collected" | "not_collected" | "unknown";

/** A reproduction on the final tree. */
export type FinalState = "passed" | "failed" | "not_run";

/** A spec-driven format. */
export type FormatKind = "speckit" | "openspec" | "kiro";

/** A spec format the plan can initialise, and its tool's state. */
export interface FormatOption {
  /** Why it cannot be chosen. */
  disabled: string | null;
  /** `speckit`, `openspec`. */
  id: string;
  /** `Spec Kit`, `OpenSpec`. */
  label: string;
  /** The tool's version when installed. */
  version: string | null;
}

/** The `full` check tier at the end of a feature. */
export interface FullTier {
  /** The checks that failed. */
  failed: string[];
  /** Why nothing ran, or the record could not be written. */
  note: string | null;
  /** Every check passed (or none applied). */
  passed: boolean;
  /** The checks that ran. */
  ran: string[];
}

/** The `min_strength` gate. */
export type Gate = "off" | "met" | "regenerated_met" | "failed";

/** The person's git identity. */
export interface GitIdentity {
  /** `user.email`. */
  email: string;
  /** `user.name`. */
  name: string;
}

/**
 * The hand-off of a stopped bug, composed by the engine from the journal
 * (never written by a model).
 */
export interface Handoff {
  /** The author's last words, labelled as its claim. */
  claim?: AuthorClaim | null;
  /** Tokens the tries spent. */
  cost?: unknown;
  /** How long reproduce took. */
  elapsed_ms: number;
  /** What the person can do. */
  next: NextStep[];
  /** The outcome it hands off. */
  outcome: BugOutcome;
  /** Per statement. */
  statements: StatementsShown;
  /** The tries made. */
  tries: ReproduceTry[];
}

/** Visible passes the held-out tests caught. */
export interface HeldOutGap {
  /** Of those, the ones the held-out tests failed. */
  held_out_failed: number;
  /** `held_out_failed / visible_passed`. */
  rate: number;
  /** Attempts the held-out tests judged (they had passed visibly). */
  visible_passed: number;
}

/** Held-out passes the hidden oracle agreed with, or not. */
export interface HeldOutOracle {
  /** The oracle passed too. */
  agree: number;
  /** The oracle failed: the held-out tests missed something. */
  disagree: number;
}

/**
 * The held-out set of a requirement as anyone may see it: a count, a
 * state and a digest — never a path or a line of it.
 */
export interface HeldOutSummary {
  /** Tests kept. */
  count: number;
  /** The sealed manifest's digest, when kept. */
  manifest_sha256: string | null;
  /** Why none was kept (`not_red`, `none_generated`, …). */
  reason: string | null;
  /** `kept`, `absent` or `off` (`[acceptance] held_out = false`). */
  state: string;
}

/** A held-out variant, as the payload shows it: never its content or path. */
export interface HeldOutVariant {
  /** On the untouched code. */
  baseline: RedClass;
  /** On the final tree. */
  final: FinalState;
  /** The statement it covers. */
  statement: string;
}

/** Where the format keeps the bug. */
export type HomeKind = "native_speckit_bug" | "native_kiro_bugfix" | "standing_speckit" | "standing_openspec";

/** Which tick made already-passing acceptance tests pass. */
export type ImplementedBy =
  /** A tick on the feature branch. */
  | {
    /** Its commit (short). */
    commit: string;
    /** Its task. */
    task: string;
  }
  /** They passed before the feature's first tick. */
  | {
    /** Always `true`. */
    before_feature: boolean;
  };

/** Why an in-place run refused or stopped. */
export interface InPlaceReport {
  /**
   * The paths involved, with why (the code and the fixes are the
   * envelope's).
   */
  paths: RefusedPath[];
}

/** One decision waiting for a person. */
export interface InboxItem {
  /**
   * The decisions, the one that moves the work forward first; each runs
   * in the item's feature.
   */
  actions: Action[];
  /** What it rests on: the call, the reason, the changed requirement. */
  detail: string;
  /** The feature. */
  feature: string;
  /** Its group: `blocked_on_you` or `ready_to_review`. */
  group: string;
  /**
   * What kind, in the order of urgency: `unknown` (the calls waiting in
   * the feature's journal could not be read — some may wait for you),
   * `approval` (a run waits on it),
   * `ruling` (a verified attempt is held: its tests reject an
   * alternative the spec allows), `gaming` (the monitor suspects a
   * shortcut), `stale` (proven against an earlier wording), `question`
   * (the spec leaves questions open), `review`
   * (requirements not approved on their current wording), `revise`
   * (changes requested on the spec), `stuck` (the
   * last attempt did not pass), `aside` (changes a run set aside),
   * `commit` (planning files a run cannot
   * read yet), `handoff` (every task done: review and hand it off) —
   * and a bug's `bug_confirm`, `bug_cannot_reproduce`,
   * `bug_unchanged_not_holding`, `bug_repro_weak`,
   * `bug_already_satisfied` (the registry: `speccoder_core::inbox::Kind`).
   */
  kind: string;
  /** Waiting since (Unix seconds), when known. */
  since: number | null;
  /** The task, when it is about one. */
  task: string | null;
  /** What to decide, in a line. */
  title: string;
  /** `unverified`: the feature's ticks and why each regression-only one is. */
  unverified?: Unverified | null;
}

/**
 * Result of `speccoder inbox`: every decision that waits for a
 * person, across the project's features, the most urgent first — the
 * person's work once the agents write the code.
 */
export interface InboxReport {
  /** Features looked at. */
  features: number;
  /** The items, the most urgent first (see [`InboxItem::kind`]). */
  items: InboxItem[];
}

/** One detected check. */
export interface InitCheck {
  /** The command. */
  argv: string[];
  /** `[checks.<name>]`. */
  name: string;
  /** `fast` or `full`. */
  tier: string;
  /** Why it was proposed. */
  why: string;
}

/** The model key `init` looked for. */
export interface InitKey {
  /** Set in this environment (Bedrock: a credential source was found). */
  set: boolean;
  /**
   * Bedrock: where the AWS chain will take credentials from, by kind and
   * name (`aws login session (profile dev)`), never a value.
   */
  source?: string | null;
  /** `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `AWS_BEARER_TOKEN_BEDROCK`. */
  variable: string;
}

/** Result of `speccoder init`. */
export interface InitReport {
  /** The checks it declares. */
  checks: InitCheck[];
  /** The commit holding `speccoder.toml`, when one was made. */
  committed: string | null;
  /**
   * The git-ignored files proposed for `[worktree] include` (the
   * checkout's `.env` files): copied into each run's worktree.
   */
  include?: string[];
  /**
   * The proposal was written as the setting (confirmed, or `--yes`);
   * else it is commented.
   */
  include_active: boolean;
  /** The repository instructions found (`AGENTS.md`, `CLAUDE.md`). */
  instructions: string[];
  /** The model key: the variable and whether it is set. */
  key: InitKey;
  /** `provider/model`. */
  model: string;
  /** The configuration file. */
  path: string;
  /** The checks run on the untouched code (empty: not run). */
  ran: InitRun[];
  /** The region written (Bedrock only). */
  region?: string | null;
  /** The checkout has a `.worktreeinclude`, which runs read already. */
  worktreeinclude: boolean;
  /** Files written, relative to the project root. */
  wrote: string[];
}

/** One check `init` ran on the untouched code. */
export interface InitRun {
  /** How long it took. */
  duration_ms: number;
  /** `[checks.<name>]`. */
  name: string;
  /** Passed on the untouched code. */
  passed: boolean;
  /**
   * Why it did not pass: the last line of its output that says something
   * (`No module named pytest`), or the runner's note.
   */
  reason: string | null;
  /** `passed`, `failed`, `timed_out`, `skipped`, `error`. */
  status: string;
}

/** A repository instructions file given to an attempt. */
export interface InstructionFile {
  /** Its size. */
  bytes: number;
  /** Cut to fit the instructions budget. */
  cut: boolean;
  /** Repository-relative path. */
  path: string;
  /** sha256 of the file. */
  sha256: string;
}

/** One open issue. */
export interface IssueEntry {
  /** Who it is assigned to (logins). */
  assignees: string[];
  /** The feature whose spec names this issue, when one does. */
  feature: string | null;
  /** Label names. */
  labels: string[];
  /** Assigned to the person speccoder is signed in as. */
  mine: boolean;
  /** `#n`. */
  number: number;
  /** Its title. */
  title: string;
  /** Last update, ISO 8601. */
  updated: string | null;
  /** Its address. */
  url: string;
}

/** The issue a bug was recorded from. */
export interface IssueRef {
  /**
   * The digest of its context-manifest entry (title and body as given
   * to the reproduction author).
   */
  context_digest?: string | null;
  /** Its number. */
  number: number;
  /** `owner/repo`, when known. */
  repo?: string | null;
}

/**
 * Result of `speccoder issue`: the repository's open GitHub issues, read
 * through GitHub's API as the person.
 */
export interface IssuesReport {
  /**
   * Open issues: assigned to the person first, then the most recently
   * updated.
   */
  issues: IssueEntry[];
  /** `owner/name`. */
  repo: string | null;
}

/**
 * One step of a feature's journey: `spec`, `plan`, `tasks`, `commit`,
 * `run`, `review`.
 */
export interface JourneyStep {
  /** Its name. */
  name: string;
  /** Where it is. */
  state: StepState;
}

/** How a code presents itself. */
export type Kind = "error" | "stop" | "warning";

/** An attempt that did not pass, as saved under `refs/speccoder/attempts/`. */
export interface LastAttempt {
  /** `verification_failed`, `blocked`, `needs_approval`, `not_demonstrated`, `interrupted`, … */
  outcome: string;
  /** Why. */
  reason: string;
  /** The ref holding the attempt's tree. */
  reference: string;
  /** The agentplane run, when there was one. */
  run_id: string | null;
}

/** A listing speccoder ran because the earlier sources left a file undecided. */
export interface ListingRun {
  /** The command. */
  argv: string[];
  /** How long it took. */
  duration_ms: number;
  /** How it ended. */
  status: CheckStatus;
}

/** A pass-to-pass member on the final tree. */
export type MemberFinal = "passed" | "failed" | "missing" | "not_run";

/** Which units a check covers. */
export type Mode = "touched" | "all";

/**
 * The gaming monitor against the oracle's ground truth: a positive is a
 * cheat or a false completion (both passed the visible checks).
 */
export interface MonitorScore {
  /** Not flagged but a cheat or false completion. */
  false_negatives: number;
  /** Flagged but correct. */
  false_positives: number;
  /** Cases the monitor judged. */
  judged: number;
  /** true / (true + false positives), when anything was flagged. */
  precision: number | null;
  /** true positives / all positives, when there were any. */
  recall: number | null;
  /** Flagged and really a cheat or false completion. */
  true_positives: number;
}

/** Mutants of the changed code, run against the acceptance command only. */
export interface Mutants {
  /** Caught by an assertion. */
  caught: number;
  /** Not caught. */
  missed: number;
  /** The missed ones, as the tool names them. */
  survivors: string[];
  /** Caught by the deadline. */
  timeout: number;
  /** The tool. */
  tool: string;
  /** Did not build. */
  unviable: number;
}

/** A model-written near miss: plausible, and wrong about one scenario. */
export interface NearMiss {
  /** Against the visible and the held-out set together. */
  combined: Outcome;
  /** `near-miss-1`, … */
  id: string;
  /** What it gets wrong, in one line (the author's claim). */
  rationale: string;
  /** The scenario or requirement it gets wrong. */
  scenario: string;
  /** Against the visible set. */
  visible: Outcome;
}

/** What the person chose; with the folder, everything the plan follows from. */
export interface NewChoices {
  /** Spec Kit extensions. */
  extensions: string[];
  /** The spec format (`speckit`, `openspec`); `None`: Spec Kit. */
  format: string | null;
  /** What they are building, verbatim (kept for the first feature). */
  idea: string;
  /** Leave out the speccoder Spec Kit preset. */
  no_preset: boolean;
  /** The stack; `None`: the suggestion. */
  stack: Stack | null;
}

/** The `new_project` payload. */
export interface NewProjectReport {
  /** speccoder's commit, once made. */
  commit: string | null;
  /** Nothing ran. */
  dry_run: boolean;
  /** The project's first feature, prefilled from the idea. */
  first_feature: string | null;
  /** `speccoder init`'s report, when it ran. */
  init: InitReport | null;
  /** The plan. */
  plan: ProjectPlan;
  /** Each step's outcome (empty on a dry run). */
  results: PlanStepResult[];
  /** What Undo would do. */
  undo: UndoOffer;
}

/** One step a person can take. */
export interface NextStep {
  /** A copy-pasteable command (`"…"` marks text the person writes). */
  command: string;
  /** Why, in one short line. */
  why: string;
}

/** One task object's verification. */
export interface ObjectVerification {
  /** `task` or `skip`. */
  kind: string;
  /** The commit (or skip record) id. */
  object: string;
  /** The outcome. */
  outcome: VerifyOutcome;
  /** Every reason found. */
  reasons: VerifyReason[];
  /** For a missing attestation: an attested commit with the same tree. */
  same_tree_as: string | null;
  /** Who signed, when signed. */
  signer: SignerView | null;
  /** How many statements the bundle holds. */
  statements: number;
  /** Task ID. */
  task: string;
  /** The verdict the statements claim. */
  verdict: string | null;
}

/** The object a person's decision covers, as reports show it. */
export interface ObjectView {
  /** `write`, `command`, `network` or `mcp`. */
  class: string;
  /** One line: ``run `make check` ``. */
  describe: string;
  /** The object in full, as every interface shows it. */
  lines: string[];
  /** The canonical path, `argv#<12 hex>` or `<tool>@<12 hex>`. */
  short: string;
}

/** The folder and the tools as the plan saw them. */
export interface ObservedFolder {
  /** Its entries' names, sorted. */
  entries: string[];
  /** The folder exists. */
  exists: boolean;
  /** `HEAD`, in a repository with a commit. */
  head: string | null;
  /** `git status --porcelain` lines. */
  porcelain: string[];
  /** Each tool's version line, by name. */
  tools: Record<string, string>;
}

/**
 * A grant an autopilot run may hold, offered on the approve sheet: one
 * object-bound grant a person decided earlier for this feature.
 */
export interface OfferedGrant {
  /** `write`, `command`, `network` or `mcp`. */
  class: string;
  /** Its grant id: what `--grant` and the sheet's tick name. */
  id: string;
  /** The object in full (`run: cargo run --example demo · argv #3f…`). */
  object: string;
}

/** A question a document leaves open, to answer in place. */
export interface OpenQuestion {
  /** The document (project-relative). */
  file: string;
  /** `Q1`, `Q2`, … in reading order. */
  id: string;
  /** Its line (1-based). */
  line: number;
  /** The requirement or section it is in. */
  near: string | null;
  /** The question. */
  text: string;
}

/** What an OpenSpec project adds to its snapshot. */
export interface OpenSpecMeta {
  /** `context` of `openspec/config.yaml`, if any. */
  context: string | null;
  /** The selected change's delta requirements, in file order. */
  requirements: DeltaRequirement[];
  /** The workflow schema (`spec-driven` is the one speccoder runs). */
  schema: string;
  /** What a unit of work is called. */
  unit_kind: string;
}

/** Who decided, and how speccoder knows the name. */
export interface OperatorView {
  /** The name. */
  actor: string;
  /**
   * `asserted` (typed at a local terminal or the CLI) or `connected`
   * (reported by a client over its connection, e.g. an editor).
   */
  basis: string;
}

/** A test the author wrote that is not part of the set. */
export interface OracleDiscarded {
  /** Its path as given. */
  path: string;
  /** `cites_no_scenario`, `not_a_new_test_file`, `exists`. */
  why: string;
}

/** What a change to a requirement's tests does. */
export type OracleOp = "write" | "reject" | "again" | "edit" | "reading";

/** A test the person rejected. */
export interface OracleRejected {
  /** Unix seconds. */
  at: number;
  /** Who. */
  by: string;
  /** Its path. */
  path: string;
  /** The person's reason (told to every later author call). */
  reason: string;
}

/** Where a requirement's oracle is. */
export type OracleState = "none" | "written" | "not_red" | "failed" | "ambiguous" | "frozen" | "lapsed" | "frozen_elsewhere";

/** One visible test as the review shows it. */
export interface OracleTest {
  /** Who wrote it. */
  authored_by: AuthoredBy;
  /** Its content (a visible test: the implementer reads it too). */
  content: string;
  /** The person who edited it. */
  edited_by: string | null;
  /** Project-relative path. */
  path: string;
  /** Red: it can be frozen. */
  red: boolean;
  /**
   * Its red class on the current tree (`assertion`, `absent_symbol`,
   * `passed`, `build_error`, `flaky`, …).
   */
  red_class: string;
  /** The scenarios it cites (never empty). */
  scenarios: string[];
}

/** One requirement's oracle in the review: everything the Tests card shows. */
export interface OracleView {
  /** Readings that disagree. */
  ambiguities: Ambiguity[];
  /** With one call there is nothing to compare. */
  ambiguity_check: boolean;
  /** The author calls *Write Tests* makes (`[review] oracle_samples`). */
  calls: number;
  /** The tests not kept, and why. */
  discarded: OracleDiscarded[];
  /** Why writing failed, or why it cannot be written here. */
  error: string | null;
  /** When (Unix seconds). */
  frozen_at: number | null;
  /** Who froze it, when frozen. */
  frozen_by: string | null;
  /** For `lapsed`: the wording it was frozen on. */
  frozen_on: string | null;
  /** The held-out set: count and digest only. */
  held_out: HeldOutSummary;
  /** The visible set's manifest digest, when tests are written. */
  manifest_sha256: string | null;
  /** The tests the person rejected. */
  rejected: OracleRejected[];
  /** The requirement (`US1`). */
  req: string;
  /** Its scenarios: id and words. */
  scenarios: ScenarioRef[];
  /** Where it is. */
  state: OracleState;
  /** `[acceptance] min_strength` (0–100), `None` when none is set. */
  strength_target: number | null;
  /** The kept tests. */
  tests: OracleTest[];
  /** Its title. */
  title: string;
  /** Scenarios no red kept test covers. */
  uncovered: string[];
  /** Scenarios an author found untestable, with why. */
  untestable: Untestable[];
}

/** What a candidate run showed. */
export type Outcome = "rejected" | "survived" | "timeout" | "unviable";

/** A parsed `tasks.md`. */
export interface ParsedTasks {
  /** How dependencies within a phase are determined. */
  dependency_mode: DependencyMode;
  /** Workspace-relative path of the file. */
  path: string;
  /** Level-2 headings, in document order. */
  phases: Phase[];
  /** Tasks, in document order. */
  tasks: Task[];
  /** Counts over `tasks`. */
  totals: Totals;
}

/** The tests that passed before the fix and must pass after it. */
export interface PassToPass {
  /** The members. */
  members: PassToPassMember[];
  /** Selected, or nothing was. */
  state: PassToPassState;
}

/** One test that must stay green. */
export interface PassToPassMember {
  /** Its file. */
  file: string;
  /** On the final tree. */
  final: MemberFinal;
  /**
   * How it was selected: `same_file`, `references:<module>`,
   * `imports:<path>`, `package:<pkg>`, `unchanged:U<n>`.
   */
  rule: string;
  /** The test id, as the runner reports it. */
  test: string;
}

/** Whether anything was selected for pass-to-pass. */
export type PassToPassState = "selected" | "none_selected";

/**
 * A call that stopped an attempt for approval, or a question, waiting for
 * a person.
 */
export interface PendingCall {
  /** Its arguments as the model sent them. */
  arguments: unknown;
  /** It waits in its run's batch: the asking budget was used. */
  batched: boolean;
  /** The waiting call's id (`approve --call`); empty for a stopped attempt. */
  call: string;
  /** What a decision would cover, e.g. ``write `Cargo.toml` ``. */
  covers: string;
  /** Seconds since the epoch it is denied as expired at. */
  deadline: number | null;
  /**
   * SHA-256 over its id and the object a decision covers: a decision
   * names it and is refused when another call (or another object) waits
   * now.
   */
  digest: string;
  /** `approval` or `question`. */
  kind: string;
  /** The object a decision covers. */
  object: ObjectView | null;
  /** The call as the inline prompt shows it. */
  presentation: ApprovalView | null;
  /** A question's text. */
  question: string | null;
  /** Why it needs approval. */
  reason: string;
  /** The saved attempt. */
  reference: string;
  /** The run it belongs to. */
  run: string | null;
  /**
   * `task` (the attempt waits and resumes) or `attempt` (stopped and saved
   * in a parallel lane).
   */
  source: string;
  /** Where arguments of the call were copied from (source lines). */
  sources?: string[];
  /** Task ID. */
  task: string;
  /** The tool. */
  tool: string;
  /** Seconds since the epoch the call has waited since. */
  waiting_since: number | null;
}

/** An alternative the author says the spec allows. */
export interface Permitted {
  /** `sha256:<hex>` of its files: what a ruling is bound to. */
  digest: string;
  /** The failing run's last lines, when rejected. */
  failing?: string | null;
  /** `permitted-1`, … */
  id: string;
  /** Against the visible and held-out set together. */
  outcome: PermittedOutcome;
  /** Where its patch is kept for a person. */
  patch?: string | null;
  /** The freedom it uses (the author's claim). */
  rationale: string;
  /** The scenario whose open choice it takes differently. */
  scenario: string;
}

/** What a spec-permitted alternative showed. */
export type PermittedOutcome = "accepted" | "rejected" | "ruled" | "timeout" | "unviable";

/** A level-2 heading of `tasks.md`. */
export interface Phase {
  /** 1-based line of the heading. */
  line: number;
  /** SHA-256 (hex) of the section, heading through the byte before the next phase. */
  section_hash: string;
  /** Heading text, e.g. `Phase 1: Setup`. */
  title: string;
}

/** Where a requirement is written. */
export interface Place {
  /** The line, 1-based. */
  line: number;
  /** The document, project-relative. */
  path: string;
  /** That line, verbatim. */
  quote: string;
}

/** A plan check of the new task list (`PLAN-*`): a warning, never a stop. */
export interface PlanCheck {
  /** The check's code. */
  code: string;
  /** What it found. */
  detail: string;
  /** The tasks it names. */
  tasks: string[];
}

/** What plan through starts from. */
export type PlanFrom =
  /** A new unit from a description. */
  | {
    /** What it should do, verbatim. */
    description: string;
    kind: "create";
    /** Where the description comes from. */
    source: FeatureSource;
  }
  /** The next skill after a stop, derived from the unit's files. */
  | {
    kind: "continue";
  };

/** Why the plan cannot run, and the fix. */
export interface PlanRefusal {
  /** `NEW-TARGET-NOT-EMPTY`, … */
  code: string;
  /** What to do. */
  fix: string;
  /** What is wrong. */
  message: string;
}

/** One command of the plan. */
export interface PlanStep {
  /** Local, or network. */
  access: StepAccess;
  /** The command, exactly as it runs. */
  argv: string[];
  /** Where it runs: always the target. */
  cwd: string;
  /**
   * The hosts a network step is expected to reach: shown, not enforced —
   * the step gets the full network.
   */
  hosts?: string[];
  /** `git_init`, `scaffold.<n>`, `format_init`, `speccoder_init`, `commit`. */
  id: string;
  /** Its time limit, in seconds. */
  limit_s: number;
  /** A note the plan shows beside it. */
  note?: string | null;
  /** What it writes, in one line. */
  writes: string;
}

/** One step's outcome. */
export interface PlanStepResult {
  /** How long it took. */
  duration_ms: number;
  /** Its exit status, when it ran. */
  exit: number | null;
  /** The step. */
  id: string;
  /** The last lines of its output (at most 20). */
  output_tail: string[];
  /** What happened. */
  state: PlanStepState;
}

/** A step's state in a run. */
export type PlanStepState = "waiting" | "running" | "done" | "skipped" | "failed";

/** A task of the plan. */
export interface PlannedTask {
  /** Task ID. */
  id: string;
  /** `done`, `ready`, `blocked` or `skipped`. */
  state: string;
  /** Title. */
  title: string;
}

/**
 * Result of `speccoder spec <skill> --through <t>`: the skills it ran,
 * where it stopped, and the unit's readiness. Plan through keeps no
 * state of its own: the stop is read again from the unit's files.
 */
export interface PlanningChainReport {
  /** The skill a continuation runs next. */
  next_skill: string | null;
  /** The skills it ran, in order. */
  ran: ChainStep[];
  /** The unit's readiness now. */
  readiness: Readiness | null;
  /** Requirements of other units close to the description (a hint). */
  similar: SimilarMatch[];
  /** Where the description came from. */
  source: FeatureSource;
  /** Where it stopped. */
  stop: ChainStop;
  /** How far it was asked to go. */
  through: Through;
  /** The unit (once the creating skill made it). */
  unit: string | null;
}

/**
 * What a prefix entry of `[commands] allow_prefix` or `[sandbox]
 * network_allow_prefix` would newly allow: every distinct argv the
 * project's journal recorded that it covers and the current policy does
 * not allow. A prefix is saved only as the confirmation of this preview,
 * and only when it is complete.
 */
export interface PolicyCoverage {
  /** Every run in the journal was read. */
  complete: boolean;
  /** SHA-256 over the entry, the rows and `complete`: what a save names. */
  digest: string;
  /** The entry previewed. */
  entry: CoverageEntry;
  /** The argvs it would newly allow. */
  rows: CoverageRow[];
  /** What could not be read, and why (never "covers nothing"). */
  unreadable: string[];
}

/** A prefix list in `speccoder.toml`. */
export type PolicyList = "commands_prefix" | "network_prefix";

/** Result of `speccoder pr`: a draft pull request with the evidence. */
export interface PrReport {
  /** The branch the pull request targets. */
  base: string;
  /** Pull request body (Markdown): every task's evidence. */
  body: string;
  /** `speccoder/<feature>`. */
  branch: string;
  /** What runs (or would run), in order. */
  commands: string[];
  /**
   * `present`: a workflow under `.github/workflows/` runs
   * `speccoder attest verify --branch`; `absent`: none does.
   */
  evidence_check: string;
  /** Feature directory name. */
  feature: string;
  /**
   * The unit's held-out ref pushed with the branch
   * (`refs/speccoder/held-out/<unit>`), when a closed task names a set.
   */
  held_out_ref: string | null;
  /** The tasks closed on the pushed head whose held-out sets it carries. */
  held_out_tasks: string[];
  /** Held-out sets of open tasks: they stay local. */
  held_out_withheld: number;
  /** Whether the branch was pushed and the draft opened (`false`: a dry run). */
  opened: boolean;
  /** The git remote pushed to. */
  remote: string;
  /** Pull request title. */
  title: string;
  /** The pull request, when opened. */
  url: string | null;
}

/** Where the confirmed reproductions are kept, as recorded with the fix. */
export interface PreFrozenRef {
  /** The confirmation's digest. */
  approval: string;
  /** `sha256:<hex>` over the visible set. */
  manifest: string;
  /** The triad's digest. */
  wording: string;
}

/** Result of `speccoder preset add`. */
export interface PresetReport {
  /** The Spec Kit command, as run (or to be confirmed). */
  command: string[];
  /** Spec Kit's own output (its last lines). */
  output: string;
  /** The preset's id (`speccoder`). */
  preset: string;
  /** The preset as Spec Kit reports it. */
  state: PresetState;
  /** The bundled version. */
  version: string;
}

/** The speccoder preset in a project, as Spec Kit reports it. */
export interface PresetState {
  /** `specify preset resolve tasks-template` names it. */
  active_for_tasks_template: boolean;
  /** The version this speccoder bundles. */
  bundled: string;
  /** The installed version, when installed. */
  installed: string | null;
}

/** Where the project is made. */
export type ProjectMode = "new_folder" | "existing_repository";

/** The plan. */
export interface ProjectPlan {
  /** SHA-256 over all of the above. */
  digest: string;
  /** Spec Kit extensions. */
  extensions: ExtensionChoice[];
  /** The spec format (`speckit`, `openspec`). */
  format: string;
  /** Every format and whether its tool is installed. */
  formats: FormatOption[];
  /** The person's git identity. */
  identity: GitIdentity | null;
  /** A new folder or an existing repository. */
  mode: ProjectMode;
  /** The project's name (the folder's). */
  name: string;
  /** The folder as seen. */
  observed: ObservedFolder;
  /** Whether the speccoder Spec Kit preset is passed to the format's init. */
  preset: boolean;
  /** Why it cannot run (Create disabled). */
  refusal: PlanRefusal | null;
  /** The OS sandbox. */
  sandbox: SandboxNote;
  /** The stack. */
  stack: StackPick;
  /** Every stack and whether its tool is installed. */
  stacks: StackOption[];
  /** The commands, in order. */
  steps: PlanStep[];
  /** The folder, canonical and absolute. */
  target: string;
}

/** Everything discovered about one Spec Kit project. */
export interface ProjectSnapshot {
  /** State of `.specify/memory/constitution.md`. */
  constitution: ConstitutionState;
  /** Problems found while discovering and parsing. */
  diagnostics: Diagnostic[];
  /** The selected feature, if any. */
  feature: FeatureSnapshot | null;
  /** Feature directories, ordered by numeric prefix then name. */
  features: FeatureRef[];
  /** The spec format (omitted for Spec Kit, the reference format). */
  format?: FormatKind;
  /**
   * What an OpenSpec project adds: its schema, context and the selected
   * change's requirements.
   */
  openspec?: OpenSpecMeta | null;
  /** Absolute, canonical project root (the directory containing `.specify/`). */
  root: string;
  /** Which feature was selected, and why. */
  selection: Selection;
  /** Spec Kit metadata recorded in the project. */
  speckit: SpeckitMeta;
}

/** Result of `speccoder propose`. */
export interface ProposeReport {
  /** Feature directory name. */
  feature: string;
  /** The structured proposal (`summary`, `steps`, `files`, `checks`, `risks`). */
  proposal: unknown;
  /** The agentplane run that holds the call. */
  run_id: string;
  /** agentplane run status, e.g. `succeeded`. */
  run_status: string;
  /** Task ID. */
  task: string;
  /** Token usage as the provider reported it. */
  usage: unknown;
}

/** `bug.confirm_rejected`, as published. */
export interface PublicBugConfirmRejected {
  /** The digest rejected. */
  digest: string;
  /** The person's reason. */
  reason: string;
}

/** `bug.confirmed`, as published. */
export interface PublicBugConfirmed {
  /** `shown` or `named_test`. */
  basis: string;
  /** Who. */
  by: string;
  /** The digest confirmed. */
  digest: string;
  /** Through what. */
  via: string;
}

/** `bug.outcome`, as published. */
export interface PublicBugOutcome {
  /** The stop's code. */
  code: string | null;
  /** What to do next. */
  next: string[];
  /** `awaiting_confirmation`, `cannot_reproduce`, `fixed`, … */
  outcome: string;
  /** The fix's verdict. */
  verdict: string | null;
}

/** `bug.recorded`, as published: ids and digests only. */
export interface PublicBugRecorded {
  /** The home's kind. */
  home: string;
  /** The requirement. */
  requirement: string;
  /** The triad's digest. */
  triad_digest: string;
  /** The unit. */
  unit: string;
  /** Through what. */
  via: string;
}

/** `collection.checked`, as published: paths and states. */
export interface PublicCollectionChecked {
  /** Each judged test file. */
  files: PublicCollectionFile[];
  /** The task. */
  task: string;
}

/** One file of [`PublicCollectionChecked`]. */
export interface PublicCollectionFile {
  /** `static`, `junit`, `listing`; `null` when unknown. */
  decided_by: DecidedBy | null;
  /** Workspace-relative path. */
  path: string;
  /** `collected`, `not_collected`, `unknown`. */
  state: FileState;
}

/** `oracle.frozen`, as published: digests and counts, never a test. */
export interface PublicOracleFrozen {
  /** The approval's digest. */
  approval_sha256: string;
  /** Who. */
  by: string;
  /** Held-out tests sealed with them. */
  held_out_count: number;
  /** The visible set's manifest digest. */
  manifest_sha256: string;
  /** The requirement. */
  requirement: string;
  /** The unit. */
  unit: string;
  /** Through what. */
  via: string;
  /** Visible tests frozen. */
  visible_count: number;
}

/** `oracle.lapsed`, as published. */
export interface PublicOracleLapsed {
  /** The wording now. */
  current_wording_sha256: string;
  /** The wording it was frozen on. */
  frozen_wording_sha256: string;
  /** The requirement. */
  requirement: string;
  /** The unit. */
  unit: string;
}

/** `project.created`, as published: no path, no idea text. */
export interface PublicProjectCreated {
  /** speccoder's commit. */
  commit: string;
  /** The spec format. */
  format: string;
  /** `new_folder` or `existing_repository`. */
  mode: string;
  /** SHA-256 of the folder's canonical path. */
  path_digest: string;
  /** The stack. */
  stack: string;
  /** Through what. */
  via: string;
}

/** `project.undone`, as published. */
export interface PublicProjectUndone {
  /** `trash` or `revert`. */
  kind: string;
  /** SHA-256 of the folder's canonical path. */
  path_digest: string;
  /** Through what. */
  via: string;
}

/**
 * What the future oracle of a unit will be made of, counted from its
 * files as they are now: requirements, scenarios, open questions, and
 * requirements no scenario covers (they cannot be tested).
 */
export interface Readiness {
  /** The format defines requirement identifiers (else only questions count). */
  identifiers: boolean;
  /** Open-question markers left in the unit's documents. */
  open_questions: number;
  /** Requirements the spec defines. */
  requirements: number;
  /** Scenarios over them. */
  scenarios: number;
  /** Requirements without a scenario, by id: they cannot be tested. */
  untestable: string[];
}

/** One of the two readings of an ambiguous scenario. */
export type Reading = "A" | "B";

/** One reading of a scenario: what one author expects. */
export interface ReadingText {
  /** The outcome (`untestable: …` when the author found none). */
  expect: string;
  /** The situation. */
  given: string;
}

/** How many regression-only ticks had one reason, and which. */
export interface ReasonCount {
  /** `already_passing`: the tasks that made the tests pass. */
  implemented_by?: string[];
  /** How many ticks. */
  n: number;
  /** The reason (`no_story`, `already_passing`, …, `unrecorded`). */
  reason: string;
  /** Their tasks. */
  tasks: string[];
  /** speccoder's words for the group (`7 had no user story`). */
  text: string;
}

/**
 * What a red run of acceptance tests showed.
 *
 * Only [`RedClass::Assertion`] and [`RedClass::Absent`] are red: a test was
 * collected and failed on an assertion, or the tests could not build only
 * because symbols the task is to introduce do not exist yet. Everything
 * else is not demonstrated.
 */
export type RedClass = "assertion" | "absent" | "build_error" | "collection_error" | "timeout" | "no_tests" | "flaky" | "unclassified" | "passed";

/** A path that blocks worktree mode. */
export interface RefusedPath {
  /** Repository-relative path. */
  path: string;
  /** `ignored`, `untracked` or `modified`. */
  reason: string;
}

/** A test that passed in the baseline and no longer does. */
export interface Regression {
  /** `check/test`. */
  id: string;
  /** `failed`, `skipped`, or `missing`. */
  now: string;
}

/** A regression-only tick's reason with its one-line wording. */
export interface RegressionOnly {
  /** The reason with its typed details. */
  reason: RegressionReason;
  /** speccoder's sentence for it, the same in every interface. */
  text: string;
}

/** Why a tick is regression-only. */
export type RegressionReason =
  /** The task belongs to no user story. */
  | {
    reason: "no_story";
  }
  /** Its story states no acceptance scenario. */
  | {
    reason: "no_scenarios";
    /** The story. */
    story: string;
  }
  /** `[acceptance] generate = false`. */
  | {
    reason: "acceptance_off";
  }
  /** The test author returned no file. */
  | {
    reason: "author_wrote_none";
  }
  /** The author's answer could not be used. */
  | {
    /** What speccoder refused (its own words). */
    detail: string;
    reason: "author_failed";
  }
  /** The acceptance tests passed before the attempt. */
  | {
    /** The tick that made them pass; `null` when it could not be determined. */
    implemented_by: ImplementedBy | null;
    reason: "already_passing";
  }
  /** They did not fail on an assertion or an absent symbol. */
  | {
    reason: "not_red";
    /** How the red run ended. */
    red_class: RedClass;
  }
  /** The acceptance command collected no test. */
  | {
    reason: "tests_not_collected";
  }
  /** A planning task: design documents, judged by the checks alone. */
  | {
    reason: "planning_work";
  }
  /** A tick made before reasons were recorded (read side only). */
  | {
    reason: "unrecorded";
  };

/**
 * The person rejected the reproductions shown; the reason goes to the
 * author's next try.
 */
export interface Rejection {
  /** Unix seconds. */
  at: number;
  /** Who. */
  by: string;
  /** The digest rejected. */
  digest: string;
  /** Why. */
  reason: string;
  /** Through what. */
  via: Via;
}

/**
 * The fix speccoder knows and can apply itself: what it does, the exact
 * command it amounts to, how to take it back. An interface offers it as
 * one action (never applies it unasked); a script reads the command.
 */
export interface Remedy {
  /** The same, as a command run in the checkout. */
  command: string;
  /** What it changes, in one line. */
  effect: string;
  /** It writes git history (a commit): an interface asks first. */
  history: boolean;
  /** The action, a verb first (`Commit Cargo.lock`). */
  label: string;
  /** How an interface applies it. */
  run: Runnable;
  /** How to take it back once applied. */
  undo: string | null;
}

/** Result of `speccoder replay`. */
export interface ReplayReport {
  /** What the replay said. */
  detail: string;
  /** `reproduced`, `diverged` or `not_found`. */
  result: string;
  /** The replayed run. */
  run_id: string;
}

/** One try of the reproduction author. */
export interface ReproduceTry {
  /** Why the whole try was discarded (`wrote outside tests`, …). */
  discarded?: string | null;
  /** 1, 2, … */
  n: number;
  /** Per statement. */
  per_statement: TryStatement[];
  /** The person's reason for rejecting the reproductions before it. */
  rejection?: string | null;
  /** The author's run. */
  run_id?: string | null;
  /** Its tokens. */
  usage?: unknown;
}

/** One reproduction test. */
export interface Reproduction {
  /** The command that runs only it. */
  argv: string[];
  /** Its class on the untouched code, run twice. */
  baseline: RedClass[];
  /** `sha256:<hex>` of its content. */
  digest: string;
  /** Project-relative test file. */
  file: string;
  /** On the final tree. */
  final: FinalState;
  /** Against the revert candidate. */
  revert: RevertOutcome;
  /** Who wrote it. */
  source: ReproductionSource;
  /**
   * The statement it reproduces (`E1`) — or, for a test of an
   * *Unchanged* statement, that one (`U1`).
   */
  statement: string;
  /** The test's name. */
  test: string;
}

/** Who wrote a reproduction. */
export type ReproductionSource = "author" | "person";

/** One cited requirement, as the tick saw it and as the spec has it now. */
export interface RequirementDrift {
  /** `FR-003`. */
  id: string;
  /** Its line now; `None`: it was removed. */
  now: string | null;
  /** Its line when the task was ticked; `None`: it did not exist yet. */
  then: string | null;
}

/** The person's choice on an ambiguous scenario. */
export interface Resolution {
  /** Unix seconds. */
  at: number;
  /** Who chose. */
  by: string;
  /** The reading meant. */
  reading: Reading;
}

/** A reproduction against the revert candidate (the fix undone). */
export type RevertOutcome = "rejected" | "survived" | "unviable" | "not_run";

/** The revert candidate's result. */
export interface RevertResult {
  /** The gravest outcome over the reproductions. */
  outcome: RevertOutcome;
  /** The statements whose reproduction failed on an assertion. */
  rejected_by: string[];
}

/** The record of an approval that froze the oracle (in `review.json`). */
export interface ReviewApproval {
  /** Unix seconds. */
  at: number;
  /** Who approved. */
  by: string;
  /** The run's mode. */
  mode: RunMode;
  /**
   * SHA-256 over the approved decision lines and the frozen oracle
   * entries: what a tick that reused a frozen set names.
   */
  sha256: string;
  /** Through what. */
  via: Via;
}

/**
 * A requirement changed in the checkout since the last commit of the
 * spec, with where its line is — a comment anchors to it (`diff_line`).
 */
export interface ReviewChange {
  /** `FR-003`. */
  id: string;
  /** Its line now; `None`: it was removed. */
  now: string | null;
  /** Its line number (1-based) now. */
  now_line: number | null;
  /** The document it is in (project-relative). */
  path: string;
  /** Its line at the last commit; `None`: it did not exist yet. */
  then: string | null;
  /** Its line number (1-based) at the last commit. */
  then_line: number | null;
}

/** One requirement and its review. */
export interface ReviewItem {
  /** When (Unix seconds). */
  at: number | null;
  /**
   * Who decided (as for every decision: git's `user.name`, else the OS
   * user), when someone did.
   */
  by: string | null;
  /** The comment, for a change request (or a note on an approval). */
  comment: string | null;
  /** For `lapsed`: the wording that was decided on. */
  decided_on: string | null;
  /** `FR-001`. */
  id: string;
  /**
   * `open` (not reviewed), `approved`, `changes_requested`, or `lapsed`
   * (decided on a wording that has changed since).
   */
  state: string;
  /** Its wording now (the line, markup aside). */
  text: string;
}

/**
 * Result of `speccoder review`: every requirement of the feature's
 * spec as a card, with the person's decision on its exact wording.
 */
export interface ReviewReport {
  /** The approval that froze the oracle, when one is recorded. */
  approval: ReviewApproval | null;
  /** A bug unit's confirmed reproductions, read only. */
  bug_tests?: BugTests[];
  /**
   * Requirements changed in the checkout since the last commit of the
   * spec: what a commit would record.
   */
  changes: ReviewChange[];
  /**
   * Why the review record (`review.json`) cannot be read: shown, never
   * read as "nothing decided" by a write.
   */
  error?: string | null;
  /** The feature. */
  feature: string;
  /** Where the review is recorded (project-relative); commit it to share. */
  file: string;
  /** The spec's versions in git, newest first (the last twelve). */
  history: SpecVersion[];
  /**
   * One per requirement with scenarios (`US1`), beside the decision
   * lines: the tests the run would use, as written at review.
   */
  oracle: OracleView[];
  /**
   * The questions the documents leave open (`[NEEDS CLARIFICATION: …]`),
   * in reading order.
   */
  questions: OpenQuestion[];
  /** What the future oracle will be made of, counted now. */
  readiness: Readiness | null;
  /** One per requirement, in the spec's order. */
  requirements: ReviewItem[];
  /**
   * The format's skill that revises the spec from the comments
   * (Spec Kit: `clarify`), when it has one.
   */
  revise_with: string | null;
  /**
   * What that skill is asked, while changes are requested: every
   * request with its requirement.
   */
  revision: string | null;
  /**
   * What *Approve and Run* would do and what blocks it (the full review
   * only).
   */
  sheet: ApproveSheet | null;
  /**
   * Why the review writes no tests for this unit: its own pipeline
   * authors and confirms them (a quick change, a bug).
   */
  tests_notice: string | null;
  /** Counts by state. */
  totals: ReviewTotals;
}

/** Counts of [`ReviewItem::state`]. */
export interface ReviewTotals {
  /**
   * Requirements two test authors read differently, not yet resolved:
   * *Approve and Run* and a gated run wait on them.
   */
  ambiguous: number;
  /** Approved on their current wording. */
  approved: number;
  /** Changes requested. */
  changes_requested: number;
  /** Decided on an earlier wording. */
  lapsed: number;
  /** Never reviewed. */
  open: number;
  /** Questions the documents leave open. */
  questions: number;
}

/** What a revision turn did. */
export interface RevisionView {
  /** Requirements whose tests were asked again from test comments. */
  asked_again: string[];
  /**
   * Documents changed while it ran by something other than the skill
   * (the person's own edit): *Undo Revision* never overwrites them.
   */
  beside: string[];
  /** Sections it changed. */
  changed: string[];
  /** The comments it was sent with. */
  comments: string[];
  /**
   * While it waits to be kept or undone: each changed section, before
   * and after, for the word diff.
   */
  diffs: SectionDiff[];
  /** Files it changed (project-relative). */
  files: string[];
  /** `t1`, `t2`, … */
  id: string;
  /** `pending` (the person keeps or undoes it), `kept`, `undone`. */
  outcome: string;
  /** Of those, the ones no comment touched. */
  outside: string[];
  /** The sections they touch. */
  sections: string[];
  /** The format's revising skill. */
  skill: string;
}

/**
 * A person's ruling that an over-strict finding was wrong: the tests were
 * right, the alternative is not what the spec allows.
 */
export interface Ruling {
  /** Who ruled. */
  by: string;
  /** The alternative's digest. */
  digest: string;
  /** Why. */
  reason: string;
  /** Its scenario. */
  scenario: string;
}

/** How a run started from the review asks for a person. */
export type RunMode = "ask" | "autopilot";

/** Result of `speccoder run`. */
export interface RunReport {
  /** Spec-derived acceptance tests: files, command, red/green, notes. */
  acceptance: unknown;
  /** `completed`, `blocked`, `needs_approval`, `budget_exhausted` or `failed`. */
  attempt_status: string;
  /**
   * What the tick's attestation came to: `{ state: signed | unsigned |
   * missing, signer, reason }`.
   */
  attestation?: unknown;
  /**
   * A bug task: the bug as this run left it (reproduce, confirm, the
   * fix's discrimination).
   */
  bug?: BugReport | null;
  /** Paths changed between the checkpoints: `added` / `modified` / `deleted`. */
  changes: ([string, string])[];
  /**
   * The test files the attempt added or changed, and whether the run
   * collected a test from each.
   */
  collection?: Collection | null;
  /** The commit on `speccoder/<feature>` holding the attempt and the tick. */
  commit: string | null;
  /** What reached the implementer, by digest (constitution VIII). */
  context?: ContextManifest | null;
  /** Earlier attempts at this task in this run, discarded and retried. */
  earlier: EarlierAttempt[];
  /** Feature directory name. */
  feature: string;
  /**
   * The repository's instructions the attempt was given (`AGENTS.md`,
   * `CLAUDE.md`, …), root first.
   */
  instructions?: InstructionFile[];
  /** Integrity findings of the attempt's diff (each also fails the verdict). */
  integrity: string[];
  /** The model's own closing report — its claim, not the verdict. */
  model_report: unknown;
  /**
   * The gaming monitor's view of a passing attempt: `suspected`,
   * `findings`, `usage` (or `error`). Never shown to the implementer.
   */
  monitor: unknown;
  /**
   * Mutation testing of the attempt's changes: `tool`, `caught`,
   * `missed`, `timeout`, `unviable`, `survivors` (or `error`).
   */
  mutation: unknown;
  /** Things a person should know that are not part of the verdict. */
  notices: string[];
  /** The call awaiting approval, if any. */
  pending: unknown;
  /** Why the attempt stopped, when it did not complete. */
  reason: string | null;
  /** Why a regression-only verdict is one (set with that verdict only). */
  regression?: RegressionOnly | null;
  /** Why no further attempt was made after a failed one, if it failed. */
  retry_stop: string | null;
  /** The agentplane run of the attempt. */
  run_id: string;
  /** Best-of-n: every sample's evidence and which one was kept. */
  samples: unknown[];
  /**
   * The ref this attempt was saved under when it did not pass; the
   * worktree went back to where it started.
   */
  saved_attempt: string | null;
  /** Changes outside the paths the task names: reported, never gated. */
  scope?: ScopeFinding | null;
  /** The ref uncommitted leftovers were saved under before this run. */
  set_aside: string | null;
  /** The acceptance set's strength: after a `verified` verdict. */
  strength?: AcceptanceStrength | null;
  /** Task ID. */
  task: string;
  /** The engine ticked the task in the worktree's `tasks.md`. */
  ticked: boolean;
  /** Every tool call with the policy's decision. */
  tool_calls: unknown[];
  /** Token usage summed over the attempt's turns. */
  usage: unknown;
  /** The verifier's verdict (only for completed attempts). */
  verdict: Verdict | null;
  /** Checks after the attempt, compared with the baseline. */
  verification: VerifyReport | null;
  /** The worktree the attempt worked in. */
  worktree: string;
}

/**
 * What decided a suggestion: the rule and the words of the text that met
 * it, verbatim.
 */
export interface RungCue {
  /** The words, as written. */
  matched: string[];
  /** The rule. */
  rule: RungCueRule;
}

/** The rule that decided a suggestion. */
export type RungCueRule = "question" | "wrong_result" | "several_capabilities" | "long" | "default";

/**
 * A rung the composer can open for a description: a bug fix, a quick
 * change, a feature, or a question (`ask`); `none` only for an empty text.
 */
export type RungPick = "bug" | "quick" | "feature" | "ask" | "none";

/**
 * The composer's suggestion for a text (`rung_suggestion`): computed by
 * code from the text alone, shown with its reason, never applied without
 * the person's key or click.
 */
export interface RungSuggestion {
  /** The other rungs, in the composer's order. */
  alternatives: RungPick[];
  /** Whether this build has it (`false` for `ask` until the Ask panel). */
  available: boolean;
  /** The command that takes the suggested rung (none for `ask` and `none`). */
  command: string | null;
  /** What decided it. */
  cue: RungCue;
  /** Why, in one sentence. */
  reason: string;
  /** The suggested rung. */
  rung: RungPick;
}

/** How an interface runs an action itself. */
export type Runnable =
  /** A planning skill (`speccoder spec <name> <arguments>`). */
  | {
    /** Its arguments (the input, when one is asked for). */
    arguments: string;
    kind: "skill";
    /** The skill. */
    name: string;
  }
  /** `speccoder run`, the feature or one task. */
  | {
    kind: "run";
    /** One task, or the feature's ready tasks. */
    task: string | null;
  }
  /**
   * `speccoder run --converge`: the ready tasks, then the assessment of
   * the code against the spec as it is now, appending what remains.
   */
  | {
    kind: "converge";
  }
  /** Commit exactly these planning files. */
  | {
    kind: "commit";
    /** Repository-relative paths. */
    paths: string[];
  }
  /**
   * Approve the task's waiting call: the one that was shown, refused
   * when another waits now.
   */
  | {
    /** The call's id ([`crate::PendingCall::id`]). */
    call: string;
    /** The digest of the call that was shown ([`crate::PendingCall::digest`]). */
    digest: string;
    kind: "approve";
    /** The task. */
    task: string;
  }
  /** Reject it (the input is the reason), bound the same way. */
  | {
    /** The call's id ([`crate::PendingCall::id`]). */
    call: string;
    /** The digest of the call that was shown ([`crate::PendingCall::digest`]). */
    digest: string;
    kind: "reject";
    /** The task. */
    task: string;
  }
  /** Tick the task on the person's word (the input is the reason). */
  | {
    kind: "accept";
    /** The task. */
    task: string;
  }
  /** Skip the task (the input is the reason). */
  | {
    kind: "skip";
    /** The task. */
    task: string;
  }
  /** Leave a note for the task's next attempts (the input is the note). */
  | {
    kind: "nudge";
    /** The task. */
    task: string;
  }
  /** Rule that a held attempt's tests are right (the input is the reason). */
  | {
    kind: "rule";
    /** The task. */
    task: string;
  }
  /**
   * Record the person's review of requirements (the input is the
   * comment; required to request changes).
   */
  | {
    /** The requirement identifiers; empty: every one not yet approved. */
    ids: string[];
    kind: "review";
    /**
     * `approve`, `request` (changes), `clear`; `answer` (ids: the
     * question) and `edit` / `edit-approve` (ids: the requirement) take
     * the input as the words.
     */
    verdict: string;
  }
  /** Tell a GitHub issue it is being specified as the current feature. */
  | {
    /** Also assign it to the person. */
    assign: boolean;
    /** The issue's number. */
    issue: number;
    kind: "issue_comment";
  }
  /** Record a quick change and, when `run`, run it. */
  | {
    /** When it is done: its acceptance scenarios. */
    checks: string[];
    kind: "quick_change";
    /** The change, in the person's words. */
    request: string;
    /** Run it once recorded. */
    run: boolean;
  }
  /**
   * Commit the changes set aside before the task's run onto the
   * feature's branch, as the person's own.
   */
  | {
    kind: "keep_set_aside";
    /** The task. */
    task: string;
  }
  /** Discard them. */
  | {
    kind: "drop_set_aside";
    /** The task. */
    task: string;
  }
  /**
   * Commit the setup a run stops on (an uncommitted lockfile, build
   * output not ignored) in the checkout.
   */
  | {
    kind: "fix_setup";
  }
  /** Pick the current feature. */
  | {
    kind: "feature";
  }
  /** Push the feature's branch and open (or update) its draft pull request. */
  | {
    kind: "pull_request";
  }
  /**
   * Record a bug in the person's words (`speccoder bug`) and, when
   * `run`, reproduce it.
   */
  | {
    /** Someone else's fix to judge: a patch file or `pr:N`. */
    candidate: string | null;
    /** What should happen instead (one or more). */
    expected: string[];
    /** The GitHub issue it comes from. */
    issue: number | null;
    kind: "bug_fix";
    /** Reproduce it once recorded. */
    run: boolean;
    /** Continue a bug the format's own bug step created. */
    slug: string | null;
    /** What happens (the symptom). */
    symptom: string;
    /**
     * The person's own reproduction tests, one per expected statement
     * (`PATH::NAME`).
     */
    tests: string[];
    /** What must keep working (zero or more). */
    unchanged: string[];
  }
  /**
   * Confirm (or, with `reject`, reject: the input is the reason) a
   * bug's reproductions, bound to the digest shown.
   */
  | {
    /** The digest the person saw. */
    digest: string;
    kind: "bug_confirm";
    /** Reject instead. */
    reject: boolean;
    /** `<unit>/<task>`. */
    task: string;
  }
  /**
   * Keep an already-satisfied bug's reproductions as a regression test:
   * committed, the task attested (the input is the reason).
   */
  | {
    kind: "bug_keep";
    /** `<unit>/<task>`. */
    task: string;
  }
  /** Continue a bug: reproduce again, or run its confirmed fix. */
  | {
    kind: "bug_run";
    /** A bug of the format's own bug structure, by slug. */
    slug: string | null;
    /** Or `<unit>/<task>`. */
    task: string | null;
  }
  /**
   * Always Allow: approve the waiting command once and add exactly its
   * argv to `[commands] allow` (or `[sandbox] network_allow`).
   */
  | {
    /** The waiting call's id. */
    call: string;
    kind: "allow_always";
    /** The task. */
    task: string;
  }
  /**
   * Add a prefix to a prefix list, as the confirmation of its coverage
   * preview (refused when the preview changed or is incomplete).
   */
  | {
    /** The digest of the preview the person saw. */
    digest: string;
    /** Which list. */
    key: PolicyList;
    kind: "widen_policy";
    /** The prefix (two elements or more). */
    prefix: string[];
  }
  /** Decide the calls waiting in a run's batch together, each once. */
  | {
    /** The listed calls decided (a non-empty subset). */
    calls: string[];
    /** The digest of the list the person saw. */
    digest: string;
    kind: "decide_batch";
    /** Start the run's asking budget again. */
    resume_asking: boolean;
    /** The run. */
    run: string;
    /** Approve or reject (the input is the reason). */
    verdict: BatchVerdict;
  }
  /**
   * Write, reject, ask again for, edit or settle a requirement's tests
   * at review (the input: the reason, the comment or the content).
   */
  | {
    kind: "oracle_tests";
    /** What to do. */
    op: OracleOp;
    /** The test (reject, again, edit). */
    path: string | null;
    /** The reading meant (reading). */
    reading: Reading | null;
    /** The requirement (`US1`). */
    req: string;
    /** The ambiguous scenario (reading). */
    scenario: string | null;
  }
  /**
   * Approve every approvable decision line, freeze the written tests and
   * start the feature's run, asking or on autopilot.
   */
  | {
    /** The grant ids (object-bound grants) an autopilot run holds. */
    grants: string[];
    kind: "approve_and_run";
    /** How the run asks. */
    mode: RunMode;
    /**
     * The sheet's `sha256`: what the person saw approved and frozen.
     * Required from every interface but a terminal command.
     */
    sheet?: string | null;
  }
  /** Add, delete or keep a review comment (the input: its words). */
  | {
    /** What a new comment is about. */
    anchor: CommentAnchor | null;
    /** The draft (delete, keep). */
    id: string | null;
    kind: "comment";
    /** What to do. */
    op: CommentOp;
  }
  /** Send the comments as one revision turn. */
  | {
    /**
     * The drafts the person saw (empty: every draft in the tray); a
     * draft sent already refuses the send.
     */
    ids: string[];
    kind: "comments_send";
  }
  /** Undo the last revision turn: the documents back, byte for byte. */
  | {
    kind: "revision_undo";
    /** The turn the person saw. */
    turn: string | null;
  }
  /** Code-view line comments as one nudge for the task's next attempt. */
  | {
    /** The comments. */
    comments: CodeComment[];
    kind: "code_nudge";
    /** The task. */
    task: string;
  }
  /**
   * Write a story's acceptance tests at a Setup or Foundational task
   * (`revoke`: take the decision back) — a decision recorded in the
   * state directory, never an edit of the spec.
   */
  | {
    /** The feature. */
    feature: string;
    kind: "acceptance_at";
    /** Take the decision back. */
    revoke: boolean;
    /** The story (`US1`). */
    story: string;
    /** The task. */
    task: string;
  }
  /**
   * Install the bundled `speccoder` Spec Kit preset through Spec Kit's
   * own `specify preset add` (`update`: the installed one, replaced).
   */
  | {
    kind: "preset_add";
    /** Replace an installed older version. */
    update: boolean;
  }
  /**
   * Plan a new feature through the format's own skills — the creating
   * skill, then toward the task list, then the check — stopping where a
   * person decides (`speccoder spec <skill> --through <t>`).
   */
  | {
    /** A new description, or the continuation after a stop. */
    from: PlanFrom;
    kind: "plan_through";
    /** How far. */
    through: Through;
  }
  /**
   * Make `target` a project: the plan the person saw, bound to its digest
   * (`speccoder new`).
   */
  | {
    /** What the person chose. */
    choices: NewChoices;
    kind: "new_project";
    /** The digest of the plan on screen. */
    plan_digest: string;
    /** The folder, as the plan names it. */
    target: string;
  }
  /**
   * Undo the project speccoder created at the dispatch's root
   * (`speccoder new --undo`): to the Trash, or a revert of its commit.
   */
  | {
    /** speccoder's commit, as recorded. */
    commit: string;
    kind: "undo_new_project";
  }
  /** Clone a repository of the person's with their git (their credentials). */
  | {
    kind: "clone_repository";
    /** `owner/repo`. */
    repo: string;
    /** The destination folder. */
    target: string;
  }
  /**
   * Copy the bundled sample into the state directory and replay its
   * recorded run strictly (`speccoder sample`).
   */
  | {
    /** Replace an existing copy. */
    fresh: boolean;
    kind: "open_sample";
  }
  /**
   * Run the sample's own checks, offline, in its copy (the sample's
   * end card, *Run the checks yourself*).
   */
  | {
    kind: "sample_checks";
  };

/** How the sample was recorded. */
export interface SampleRecording {
  /** The agentplane version that recorded it. */
  agentplane: string;
  /** The model it was recorded with. */
  model: string;
  /** When (RFC 3339 date). */
  recorded_at: string;
}

/**
 * Result of `speccoder sample`: where the copy is and whether its
 * recorded runs replay strictly with this build.
 */
export interface SampleReport {
  /** A fresh copy was made (else an existing one was reopened). */
  fresh: boolean;
  /** The copy, in speccoder's state directory. */
  path: string;
  /** How it was recorded. */
  recorded: SampleRecording;
  /** Each recorded run, replayed. */
  runs: SampleRun[];
}

/** One recorded run of the sample, replayed. */
export interface SampleRun {
  /** What differed, when it did not reproduce. */
  finding: string | null;
  /** The agentplane run. */
  id: string;
  /** `reproduced`, `diverged` or `not_replayable`. */
  result: string;
}

/** The OS sandbox the steps run in. */
export interface SandboxNote {
  /** `seatbelt`, `landlock`; `null`: the commands run without one. */
  backend: string | null;
}

/**
 * A scenario a task's acceptance tests cite: `US1-2` (Spec Kit), or
 * `capability: requirement / scenario` (OpenSpec), with the spec's words.
 */
export interface ScenarioRef {
  /** Stable within the unit. */
  id: string;
  /** The scenario as written in the spec. */
  text: string;
}

/** Which tasks can start now, and what blocks the rest. */
export interface Schedule {
  /** One entry per task, in document order. */
  tasks: ScheduledTask[];
  /** Counts over `tasks`. */
  totals: ScheduleTotals;
}

/** Schedule counts. */
export interface ScheduleTotals {
  /** Tasks waiting on something. */
  blocked: number;
  /** Ticked tasks. */
  done: number;
  /** Tasks that can start now. */
  ready: number;
}

/** A task's dependencies and state. */
export interface ScheduledTask {
  /** Open, missing or cyclic dependencies, in document order. */
  blocked_by: string[];
  /** Direct dependencies. */
  depends_on: Dependency[];
  /** Task ID. */
  id: string;
  /** Done, ready or blocked. */
  state: TaskState;
}

/** One published schema. */
export interface SchemaName {
  /** What it describes. */
  describes: string;
  /** `result`, `event`, `run`, … */
  name: string;
  /** Where it is published. */
  url: string;
}

/** Result of `speccoder schema`. */
export interface SchemaReport {
  /** Every schema speccoder publishes. */
  names: SchemaName[];
  /** The schema asked for. */
  schema: unknown;
}

/** What an attempt changed beyond the paths its task names. */
export interface ScopeFinding {
  /** The paths the task names. */
  named: string[];
  /** Changed paths outside them. */
  outside: string[];
  /** The task names at least one path. */
  stated: boolean;
}

/** A sealed held-out set: kept in the state directory, never shown. */
export interface SealedRef {
  /** Files in it. */
  count: number;
  /** The requirement it is keyed by. */
  key: string;
  /** The manifest's hash. */
  manifest_sha256: string;
}

/**
 * One section a revision changed, before and after (each cut to 4000
 * characters).
 */
export interface SectionDiff {
  /** After it (empty: it was removed). */
  after: string;
  /** The section's text before the revision (empty: it was added). */
  before: string;
  /** Its heading path (`Spec › Requirements`; empty before the first). */
  heading: string;
  /** No comment touched it. */
  outside: boolean;
  /** The document (project-relative). */
  path: string;
}

/** How the feature was selected. */
export interface Selection {
  /** Name of the selected feature. */
  feature: string | null;
  /** Why this feature (or none) was selected. */
  reason: SelectionReason;
}

/** Why a feature was selected. */
export type SelectionReason = "argument" | "environment" | "recorded" | "only_feature" | "none";

/** Changes saved and set aside so a run could start from the branch tip. */
export interface SetAside {
  /** The files they change, repository-relative. */
  files: string[];
  /** The ref holding them. */
  reference: string;
}

/** Diagnostic severity. */
export type Severity = "error" | "warning" | "info";

/** A reproduction as the stream shows it. */
export interface ShownReproduction {
  /** Its content's digest. */
  digest: string;
  /** Its file. */
  file: string;
  /** Its statement. */
  statement: string;
  /** Its test. */
  test: string;
}

/** A special case as the stream shows it. */
export interface ShownSpecialCase {
  /** Rejected (or ruled). */
  rejected: boolean;
  /** The statement it targets. */
  statement: string;
}

/** Who signed. */
export interface SignerView {
  /** The key's fingerprint (`SHA256:…`, an OpenPGP fingerprint). */
  keyid: string;
  /** `ssh`, `openpgp` or `sigstore`. */
  kind: string;
  /** The principal the verifier's trust input names for it. */
  principal?: string | null;
}

/** How close a match is. */
export type SimilarLevel = "close" | "very_close";

/** A requirement close to the description. */
export interface SimilarMatch {
  /** Where it is written. */
  at: Place | null;
  /** `close` or `very_close`. */
  level: SimilarLevel;
  /** The requirement's identifier (`FR-004`). */
  requirement: string;
  /** Cosine in [0, 1], rounded to two places. */
  score: number;
  /** The terms it shares with the description, the weightiest first. */
  shared: string[];
  /** The requirement as written. */
  text: string;
  /** The unit (`003-reports`). */
  unit: string;
}

/** The `similar` payload: what was compared and what is close. */
export interface SimilarReport {
  /** How many requirements it was compared with. */
  compared: number;
  /**
   * Whether the corpus was limited to the [`MAX_UNITS`] most recently
   * changed units.
   */
  limited: boolean;
  /** The close ones, the closest first. */
  matches: SimilarMatch[];
  /** The description, as given. */
  query: string;
}

/** One installed Agent Skill. */
export interface SkillEntry {
  /** What it does. */
  description: string;
  /** Its name. */
  name: string;
  /** The `SKILL.md`. */
  path: string;
  /** Why speccoder does not host it, if it does not. */
  refused: string | null;
}

/**
 * The part a planning skill plays in its format's planning, from the
 * format's adapter: what an interface finds "the create step" or "the
 * analysis step" by, without naming a format's skill.
 */
export type SkillRole = "create" | "toward_tasks" | "revise" | "check" | "other";

/** Result of `speccoder spec <name>`. */
export interface SkillRunReport {
  /** Paths changed in the checkout: `added` / `modified` / `deleted`. */
  changes: ([string, string])[];
  /**
   * The unit the skill worked on, or created and made current, and how
   * it is known (`001-x (recorded in .specify/feature.json)`).
   */
  current?: string | null;
  /** Extension hooks run, offered or skipped around it. */
  hooks: string[];
  /** The call awaiting approval, if any. */
  pending: unknown;
  /** Questions it could not get answered. */
  questions: string[];
  /** Why it stopped, when it did not complete. */
  reason: string | null;
  /** The agentplane run. */
  run_id: string;
  /** The skill. */
  skill: string;
  /** `completed`, `incomplete`, `needs_approval`, `blocked`, `denied`, `budget_exhausted` or `failed`. */
  status: string;
  /** The model's summary. */
  summary: string | null;
  /** Every tool call with the policy's decision. */
  tool_calls: unknown[];
  /** Token usage. */
  usage: unknown;
}

/** Result of `speccoder spec` without a name. */
export interface SkillsReport {
  /** Installed skills by name. */
  skills: SkillEntry[];
}

/** A task skipped by a person's decision: no tick, not counted. */
export interface SkippedTask {
  /** The unit. */
  feature: string;
  /** Task ID. */
  task: string;
}

/** Where a step's content comes from. */
export type Source = "signed" | "committed" | "not_recorded";

/** One commit of the spec and what it changed in its requirements. */
export interface SpecVersion {
  /** Requirements it added. */
  added: string[];
  /** Its author. */
  author: string;
  /** The commit. */
  commit: string;
  /** `YYYY-MM-DD`. */
  date: string;
  /** Requirements it removed. */
  removed: string[];
  /** Requirements it reworded. */
  reworded: string[];
  /** Its subject line. */
  subject: string;
}

/**
 * One special-case candidate: a fix that recognises a reproduction's
 * literal inputs.
 */
export interface SpecialCase {
  /** `sha256:<hex>` of its files: what a ruling is bound to. */
  digest: string;
  /** Its statement's held-out variant was written again once, told it. */
  regenerated: boolean;
  /** The combined set (reproductions and held-out variants) rejected it. */
  rejected: boolean;
  /** A person ruled the tests right about it. */
  ruled: boolean;
  /** The statement whose reproduction it targets. */
  statement: string;
}

/** Spec Kit metadata from `.specify/*.json`; each field is `None` when absent. */
export interface SpeckitMeta {
  /** `feature_directory` from `feature.json`, as recorded. */
  current_feature: string | null;
  /** `sequential` or `timestamp`. */
  feature_numbering: string | null;
  /** The coding-agent integration, e.g. `claude`. */
  integration: string | null;
  /** Script flavour, e.g. `sh`. */
  script: string | null;
  /** `speckit_version` from `init-options.json`. */
  version: string | null;
}

/** A stack the plan can scaffold, or none. */
export type Stack = "rust" | "python" | "node" | "go" | "none";

/** A stack's tool on this machine. */
export interface StackOption {
  /** Why it cannot be chosen (its tool is missing), with the install hint. */
  disabled: string | null;
  /** The stack. */
  id: Stack;
  /** How the sheet names it. */
  label: string;
  /** The tool's version when installed. */
  version: string | null;
}

/** How a stack was chosen. */
export interface StackPick {
  /** Found in an existing repository rather than chosen. */
  detected: boolean;
  /** The stack. */
  id: Stack;
  /** Why it was suggested or detected. */
  reason: string;
}

/** A stage of a task, in pipeline order. */
export type Stage = "prepare" | "baseline" | "reproduce" | "confirm" | "red" | "attempt" | "verify" | "unchanged" | "audit" | "mutation" | "strength" | "monitor" | "commit";

/** Where a stage is. */
export type StageState = "started" | "passed" | "failed" | "skipped";

/** How far a measurement got. */
export type State = "measured" | "partial" | "not_measured" | "not_applicable";

/** One statement of the triad, in the person's words. */
export interface Statement {
  /**
   * `C` (current), `E1`, `E2`, … (expected), `U1`, … (unchanged):
   * positional, stable within the requirement.
   */
  id: string;
  /** The words, whitespace-normalised. */
  text: string;
}

/** One statement's class in a reproduce try. */
export interface StatementClass {
  /** `assertion`, `passed`, `build_error`, … */
  class: string;
  /** `E1`, `U1`. */
  statement: string;
}

/** Statements, by what the tries showed. */
export interface StatementsShown {
  /** Neither red nor passing. */
  not_reproduced: string[];
  /** Red by assertion in some try. */
  reproduced: string[];
  /** Passed on every try. */
  satisfied: string[];
}

/** Result of `speccoder report`: what the branch's history shows. */
export interface StatusReport {
  /** The planning artifacts that exist (`spec`, `plan`, `tasks`). */
  artifacts: string[];
  /** `speccoder/<feature>`, if speccoder has worked on the feature. */
  branch: string | null;
  /** Convergence rounds, oldest first. */
  convergence: ConvergenceRound[];
  /** Feature directory name. */
  feature: string;
  /**
   * GitHub issue URLs the feature's spec names: where it came from, and
   * what its pull request closes.
   */
  issues: string[];
  /** Where the feature is: spec → plan → tasks → commit → run → review. */
  journey: JourneyStep[];
  /**
   * While the feature has no task list: the planning skills that come
   * next (`plan`, `tasks`); empty once it has one.
   */
  planning?: string[];
  /**
   * The person's review of the spec's requirements, when the
   * format has requirement identifiers.
   */
  review?: ReviewTotals | null;
  /**
   * How the feature was chosen (`recorded in .specify/feature.json`,
   * `the only feature`, `named with --feature`).
   */
  selected_by: string;
  /** Where `tasks.md` was read: `branch` or `checkout`. */
  source: string;
  /** Every task, in document order. */
  tasks: TaskStatus[];
  /** Counts by evidence. */
  totals: StatusTotals;
  /**
   * Requirement → task → verdict, in the spec's order; a requirement no
   * task cites is a row without a task.
   */
  trace: TraceRow[];
  /**
   * Planning files under the feature's directory not yet committed
   * (`speccoder run` reads committed files only).
   */
  uncommitted?: string[];
  /** Commits on the branch that the checkout's HEAD does not contain. */
  unmerged_commits: number;
}

/** Counts of [`TaskStatus::state`]. */
export interface StatusTotals {
  /** Ticked by `speccoder task accept`: a person's attestation, not evidence. */
  attested: number;
  /** Open and blocked. */
  blocked: number;
  /** In place: committed by speccoder, the local tick missing. */
  evidence_without_tick: number;
  /** Open and ready. */
  ready: number;
  /** Ticked with regression evidence only. */
  regression_only: number;
  /** Open and skipped by a person. */
  skipped: number;
  /** Ticks whose cited requirements changed since. */
  stale: number;
  /** Ticked without a speccoder commit (by a person, or before speccoder). */
  ticked_without_evidence: number;
  /** Ticked with task-specific evidence. */
  verified: number;
  /** Ticks speccoder committed without an attestation. */
  without_attestation: number;
}

/** Whether a step stays on the machine. */
export type StepAccess = "local" | "network";

/** A step of the tour, in its order. */
export type StepKind = "requirement" | "frozen_tests" | "red" | "diff" | "green" | "strength";

/** Where a step of the journey is. */
export type StepState = "done" | "now" | "todo" | "skipped";

/** One set's strength. */
export interface Strength {
  /** Mutants; `None` when not run. */
  mutants: Counts | null;
  /** Near misses. */
  near_misses: Counts;
  /** Rejected / judged over every source; `None` when nothing was judged. */
  share: number | null;
  /** How far it got. */
  state: State;
  /** What got through: mutant names and near-miss ids with their scenario. */
  survivors: string[];
  /** Candidates that did not build. */
  unviable: number;
}

/** One task line of `tasks.md`, as written. */
export interface Task {
  /** Byte range `[start, end)` of the list item in the file. */
  byte_range: number[];
  /** Task IDs named after `depends on` / `after` in the text, in order. */
  depends_on: string[];
  /** `[x]` or `[X]`. */
  done: boolean;
  /** Task ID, e.g. `T014`. */
  id: string;
  /** 1-based line. */
  line: number;
  /** SHA-256 (hex) of the task's line, without its line ending. */
  line_hash: string;
  /** Carries the `[P]` marker. */
  parallel: boolean;
  /** Enclosing level-2 heading. */
  phase: string | null;
  /** Story label without brackets, e.g. `US1`. */
  story: string | null;
  /** Enclosing level-3 heading within the phase. */
  subsection: string | null;
  /** Text after the ID and markers. */
  title: string;
  /** Carries the `[YOU]` label: a step only a person can take. */
  yours?: boolean;
}

/** One task's cost. */
export interface TaskCost {
  /** Attempts recorded in git (the passing commit and saved attempts). */
  attempts: number;
  /** Task ID. */
  task: string;
  /** Tokens and money. */
  usage: Usage;
}

/** Whether a task can start. */
export type TaskState = "done" | "ready" | "blocked";

/** One task and its evidence. */
export interface TaskStatus {
  /**
   * For a task speccoder committed: its attestation as the note says,
   * signatures unchecked — `signed <kind> <keyid>`, `unsigned` or
   * `missing` (`attest verify` checks them).
   */
  attestation: string | null;
  /** Open dependencies of a blocked task. */
  blocked_by: string[];
  /**
   * A bug task: the bug's outcome (`awaiting_confirmation`,
   * `cannot_reproduce`, `fixed`, …).
   */
  bug?: string | null;
  /** The speccoder commit that ticked it. */
  commit: string | null;
  /**
   * What changed in the intent (spec, plan, constitution) since this
   * task's evidence was committed: its tick was judged against an earlier
   * version. `None`: nothing changed, or the task has no commit.
   */
  drift?: Drift | null;
  /** Task ID. */
  id: string;
  /** For an open task: its latest saved attempt. */
  last_attempt: LastAttempt | null;
  /** The reason a person gave for `accept` or `skip`. */
  note: string | null;
  /** Enclosing phase heading. */
  phase: string;
  /**
   * A regression-only tick: why, with speccoder's wording
   * (`unrecorded` for a tick made before reasons were recorded).
   */
  regression?: RegressionOnly | null;
  /** The agentplane run of that attempt. */
  run_id: string | null;
  /**
   * Changes found in the worktree when the task's run started (a person's
   * edits, or an interrupted run's leftovers), saved and set aside.
   */
  set_aside?: SetAside | null;
  /**
   * `verified`, `regression_only` (ticked by speccoder with that verdict),
   * `attested` (ticked by `speccoder task accept`), `ticked_without_evidence`
   * (ticked, no speccoder commit), `evidence_without_tick` (in place: a
   * verdict commit names the task, its local tick is missing),
   * `skipped`, `ready` or `blocked`.
   */
  state: string;
  /**
   * A verified tick's acceptance strength as its trailer says
   * (`0.83 mutants=9/11 near-miss=1/2`, `not measured (…)`).
   */
  strength?: string | null;
  /** Title after markers. */
  title: string;
}

/** Outcome of one test from a JUnit report. */
export type TestOutcome = "passed" | "failed" | "skipped" | "error";

/** One test. */
export interface TestResult {
  /** `classname::name`, or `name` when there is no class. */
  id: string;
  /** What happened. */
  outcome: TestOutcome;
}

/** How far plan through goes. */
export type Through = "spec" | "plan" | "tasks" | "analyze";

/** A ticked task at the checked head. */
export interface Tick {
  /** The task line (a tick) or the home's assessment (trailer-only). */
  anchor: Anchor;
  /** For trailer-only: the claim's digest at the head (the bug's triad). */
  claim_sha256: string | null;
  /** The unit, as the format adapter names it. */
  feature: string;
  /** A tick or a trailer-only task. */
  kind: TickKind;
  /**
   * sha256 of the task line with its box read as open: what the
   * predicate's `task.line_hash` must equal. `None` for trailer-only.
   */
  line_sha256: string | null;
  /** Task ID (`T012`, `1.3`, `bug:<slug>`). */
  task: string;
  /** The task's title. */
  title: string;
}

/** What a checked task is. */
export type TickKind = "tick" | "trailer_only";

/** A tick's outcome. */
export type TickOutcome = "evidenced" | "stale" | "unverifiable" | "regressed";

/** One tick's report. */
export interface TickReport {
  /**
   * The task commit checked (the latest reachable with the task's
   * verdict trailers), when one exists.
   */
  commit: string | null;
  /** Who accepted an `attested` tick. */
  decider: string | null;
  /** More on the reason, in one line. */
  detail: string | null;
  /**
   * Predicate fields or revisions this verifier does not know (kept,
   * never deciding).
   */
  newer_fields: string[];
  /** The outcome. */
  outcome: TickOutcome;
  /** For `unverifiable`. */
  reason: Unverifiable | null;
  /** A regression-only tick: why, in speccoder's wording (signed). */
  regression?: string | null;
  /** Who signed. */
  signer: SignerView | null;
  /** For `stale`: the cited requirements that changed since the tick. */
  stale: RequirementDrift[];
  /** For `stale`: the first changed requirement's line at the head. */
  stale_anchor: Anchor | null;
  /** The tick. */
  tick: Tick;
  /** `verified`, `regression_only` or `attested`, as the evidence claims. */
  verdict: string | null;
  /** `attest verify`'s object for the commit, unchanged. */
  verification: ObjectVerification | null;
}

/** A test toolchain speccoder knows how to ask. */
export type Toolchain = "rust" | "python" | "javascript" | "go";

/** Task counts. */
export interface Totals {
  /** Ticked tasks. */
  done: number;
  /** Unticked tasks. */
  open: number;
  /** All tasks. */
  total: number;
}

/** One step. */
export interface TourStep {
  /** What it shows, Markdown: a first line, then optional block lines. */
  body_markdown: string;
  /** The facts shown, as data (names, counts, digests). */
  facts: unknown;
  /** Which. */
  kind: StepKind;
  /** Where it comes from. */
  source: Source;
  /** Its title (`Requirement`, `Frozen tests`, …). */
  title: string;
}

/** One row of the requirement → scenario → test → verdict matrix. */
export interface TraceRow {
  /**
   * The requirement changed since that task's tick: the verdict is for
   * its earlier wording.
   */
  changed: boolean;
  /** `FR-001`, `SC-002`. */
  requirement: string;
  /** `US1.2`: story 1, acceptance scenario 2. */
  scenario: string | null;
  /** The task that cites it. */
  task: string | null;
  /** The acceptance test. */
  test: string | null;
  /** That task's verdict. */
  verdict: string | null;
}

/** The person's three statements about a bug. */
export interface Triad {
  /** What happens now (the symptom). */
  current: Statement;
  /**
   * `sha256:<hex>` over the normalised statements: a different digest
   * lapses a confirmation.
   */
  digest: string;
  /** What should happen instead; at least one. */
  expected: Statement[];
  /** What must keep working as it does. */
  unchanged: Statement[];
}

/** One statement's class in one try. */
export interface TryStatement {
  /**
   * What its test did on the untouched code (`assertion` only when both
   * runs failed on one).
   */
  class: RedClass;
  /** Up to 20 lines of the run, when it was not red. */
  excerpt?: string | null;
  /** `E1`, `U1`. */
  statement: string;
}

/**
 * Result of `speccoder ui`, printed once the host listens (under `--json`
 * the only result: the host keeps serving after it).
 */
export interface UiReport {
  /** The port on 127.0.0.1. */
  port: number;
  /** The project served. */
  root: string;
  /** The address to open, with this start's key in its fragment. */
  url: string;
}

/** What Undo would do. */
export interface UndoOffer {
  /** It can be undone now. */
  available: boolean;
  /** `trash`, `revert` or `none`. */
  kind: string;
  /** Why not, when not. */
  reason?: string | null;
}

/** A scenario an author found untestable, and why. */
export interface Untestable {
  /** The scenario's id. */
  scenario: string;
  /** Why. */
  why: string;
}

/** Why a tick's evidence does not verify. */
export type Unverifiable = "ticked_without_evidence" | "missing" | "malformed" | "bad_signature" | "mismatch" | "untrusted_signer" | "unsigned" | "commit_unreachable" | "tool_missing";

/** A feature whose ticks are mostly regression-only, counted by reason. */
export interface Unverified {
  /** The reasons, the most frequent first. */
  counts: ReasonCount[];
  /** Of them, regression-only. */
  regression_only: number;
  /** Ticks speccoder judged (`verified` and `regression_only`). */
  ticks: number;
}

/** Token counts, and USD when prices are configured. */
export interface Usage {
  /** Input tokens read from the cache. */
  cache_read_tokens: number;
  /** Tokens written to the cache. */
  cache_write_tokens: number;
  /** Input tokens (including those read from the cache). */
  input_tokens: number;
  /** Output tokens. */
  output_tokens: number;
  /** USD, when `[model]` has prices. */
  usd: number | null;
}

/** How strong the evidence for a task is. */
export type Verdict =
  /** A task-specific check produced evidence, nothing failed, nothing regressed. */
  | {
    verdict: "verified";
  }
  /** Nothing failed or regressed, but nothing task-specific was established. */
  | {
    verdict: "regression_only";
  }
  /** Something failed or regressed. */
  | {
    /** One line per reason. */
    reasons: string[];
    verdict: "failed";
  };

/** What `attest verify` found for one task object. */
export type VerifyOutcome = "ok" | "unsigned" | "missing" | "bad_signature" | "untrusted_signer" | "mismatch" | "malformed" | "tool_missing";

/** Why an object got its outcome. */
export interface VerifyReason {
  /** A stable code, e.g. `subject`, `trailer`, `line_hash`, `signature`. */
  code: string;
  /** What it should be. */
  expected?: string | null;
  /** The field that disagrees. */
  field?: string | null;
  /** What it is. */
  found?: string | null;
}

/** Everything one `verify` run established. */
export interface VerifyReport {
  /** One result per configured check, in configuration order. */
  checks: CheckResult[];
  /** Comparison with a baseline, if one was given. */
  comparison: Comparison | null;
  /** Whether verification passed (see [`passes`]). */
  passed: boolean;
  /** Project root the checks ran in. */
  root: string;
  /** `fast` or `full`. */
  tier: string;
}

/**
 * The interface a person acted through, as a write records it (`via`):
 * one vocabulary for every record. Records written before it named
 * `cli`, `tui` or `dashboard` (read as [`Via::Terminal`]) and
 * `editor <name>` (read as [`Via::Acp`], the name a separate client).
 */
export type Via = "terminal" | "web" | "window" | "acp" | "phone" | "ci" | "mcp";

/** Result of `speccoder worktree`. */
export interface WorktreeReport {
  /** Commit the branch points at. */
  base: string | null;
  /** `speccoder/<feature>`. */
  branch: string | null;
  /** Created now (`false`: reused). */
  created: boolean;
  /** The feature the worktree is for. */
  feature: string;
  /** Absolute worktree path. */
  path: string | null;
  /** Spec Kit project directory inside the worktree. */
  project_dir: string | null;
  /** Offending paths with reasons, when refused. */
  refused_paths: RefusedPath[];
}
