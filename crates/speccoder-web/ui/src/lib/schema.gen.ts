// Generated from site/static/schemas/v2 by `SPECCODER_BLESS=1 cargo test -p speccoder ui_types`. Do not edit.

/** What the fix's visible acceptance set came from. */
export interface AcceptanceRef {
  /** `bug_confirmation` or `person_test`. */
  source: AcceptanceSource;
  /** The record it reused. */
  pre_frozen: PreFrozenRef;
}

/** Where a task's visible acceptance set came from. */
export type AcceptanceSource = "authored" | "review" | "bug_confirmation" | "person_test";

/** The acceptance set's strength, as recorded with a verdict. */
export interface AcceptanceStrength {
  /** The visible set alone. */
  visible: Strength;
  /**
   * Visible and held-out together: what the verdict rests on (equal to
   * `visible` without held-out tests). Mutants run with the visible
   * command only, so its mutant counts are a lower bound.
   */
  combined: Strength;
  /** A held-out set was overlaid. */
  held_out: boolean;
  /** The mutants. */
  mutants?: Mutants | null;
  /** The near misses. */
  near_misses: NearMiss[];
  /** The permitted alternatives. */
  permitted: Permitted[];
  /** Rulings that applied. */
  rulings?: Ruling[];
  /** The gate. */
  gate: Gate;
  /** Why not (fully) measured, or what was dropped. */
  notes?: string[];
  /** The near-miss call's run. */
  run_id?: string | null;
  /** The near-miss call's tokens. */
  usage?: unknown;
  /** How long the stage took. */
  duration_ms: number;
}

/** One thing a person can do now. */
export interface Action {
  /** `next`, `plan`, `work`, `task`, `feature` or `review`. */
  group: string;
  /** What it does, in a few words. */
  label: string;
  /** Why, or what follows (shown beside it). */
  why: string;
  /** The equivalent command; `"…"` marks the input. */
  command: string;
  /** How an interface runs it; `None`: see `step`. */
  run: Runnable | null;
  /**
   * How an interface takes it when it does not run it (`run` is `None`):
   * a view it opens, the person's own command, a line for their shell.
   */
  step: How | null;
  /** Why it cannot run now; `None`: it can. */
  blocked: string | null;
  /** What to ask for before it runs (`Describe the feature`). */
  input: string | null;
  /**
   * It leaves the machine (a push, a pull request) or cannot be undone
   * (a discard): what an interface says in a confirmation before it runs it.
   */
  confirm: string | null;
  /** What it is, independent of its wording ([`identity`]). */
  id: ActionId;
  /**
   * A planning skill's part in its format's planning; `None` for
   * everything else.
   */
  role: SkillRole | null;
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
export type ActionKind = "skill" | "run_task" | "run_feature" | "converge" | "commit" | "approve" | "reject" | "accept" | "skip" | "nudge" | "done" | "hint" | "rule" | "review" | "issue_comment" | "quick_change" | "keep_set_aside" | "drop_set_aside" | "fix_setup" | "feature" | "pull_request" | "bug_fix" | "bug_confirm" | "bug_run" | "bug_keep" | "allow_always" | "widen_policy" | "decide_batch" | "oracle_tests" | "approve_and_run" | "comment" | "comments_send" | "revision_undo" | "code_nudge" | "acceptance_at" | "preset_add" | "plan_through" | "new_project" | "undo_new_project" | "clone_repository" | "open_sample" | "sample_checks" | "command";

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

/** An external agent's attempt, as the report keeps it. */
export interface AgentAttempt {
  /** Its name, as it gave it. */
  agent: string;
  /** The preset (`copilot`, …) or `acp` for an `argv` of the person's. */
  preset: string;
  /** Its permission mode, in a sentence. */
  mode: string | null;
  /**
   * It decided some calls without asking — its mode says so, or it was
   * seen changing something unasked: those never reached speccoder.
   */
  decides_alone: boolean;
  /** A recording of its version says whether it asks. */
  known: boolean;
  /**
   * Calls it reported changing something (an edit, a command) that
   * never reached speccoder as a request.
   */
  unasked: number;
  /** What speccoder decided of what it asked. */
  tally: PermissionTally;
  /** Its plan as it last sent it (a claim). */
  plan: AgentPlanStep[];
}

/**
 * One agent that can make attempts, and how ready it is: installed,
 * signed in, asking before it acts — each with its one action.
 */
export interface AgentCard {
  /** `agent = "<id>"` in `[executor]`. */
  id: string;
  /** Its name. */
  name: string;
  /** It is the configured `[executor]` agent. */
  chosen: boolean;
  /** Its command can start. */
  installed: AgentTick;
  /** It answers a session without asking for a sign-in. */
  signed_in: AgentTick;
  /** speccoder can put it into a mode that asks before it acts. */
  asks_first: AgentTick;
}

/** One entry of an external agent's plan. */
export interface AgentPlanEntry {
  /** Its words. */
  content: string;
  /** `pending`, `in_progress` or `completed`. */
  status: string;
}

/** One step of an external agent's plan. */
export interface AgentPlanStep {
  /** Its words. */
  content: string;
  /** `pending`, `in_progress` or `completed`. */
  status: string;
}

/** One tick of an agent card. */
export interface AgentTick {
  /** Yes, no or not known. */
  state: TickState;
  /** What was found. */
  detail: string;
  /** Its action, as lines. */
  fix: string[];
  /** The same lines, with how an interface takes each. */
  fix_steps: Step[];
}

/** Two authors read a scenario differently. */
export interface Ambiguity {
  /** The scenario. */
  scenario: string;
  /** The first author's reading (its tests are the ones kept). */
  a: ReadingText;
  /** The second author's. */
  b: ReadingText;
  /** The person's choice; `None` while it is open. */
  resolution: Resolution | null;
}

/** Where an annotation lands: a file and a 1-based line, at the head. */
export interface Anchor {
  /** Repository-relative path. */
  path: string;
  /** 1-based line. */
  line: number;
}

/**
 * A pending call as a person sees it: what, why, what it can reach, what
 * the model claimed, and what approving it grants.
 */
export interface ApprovalView {
  /** `write`, `edit`, `run`, `network` or `mcp`. */
  kind: string;
  /** `Edit this file?`, `Run this command?`, …. */
  title: string;
  /** The path, the whole command, or the MCP tool. */
  subject: string;
  /** The diff, the file's head, the command or the arguments. */
  body_lines: string[];
  /** What the call can reach. */
  sandbox: string;
  /** What the model said when it asked (a claim). */
  claim: string | null;
  /** `low`, `medium` or `high`. */
  risk: string;
  /** What `approve` grants: `for the rest of T004: this file, any content`. */
  scope: string;
  /** The object's lines. */
  object: string[];
  /** Where arguments were copied from. */
  sources: string[];
  /** The choices offered, in order. */
  options: string[];
}

/** Result of `speccoder report --approvals`, `approve` and `reject`. */
export interface ApprovalsReport {
  /** Feature directory name. */
  feature: string;
  /** Calls that stopped an open task and await a decision. */
  pending: PendingCall[];
  /** Decisions recorded for the feature's tasks. */
  decided: DecidedCall[];
  /**
   * Why the calls waiting in the journal could not be read (another
   * speccoder holds it, …): `pending` may then be incomplete — unknown,
   * never "nothing waiting".
   */
  unreadable: string | null;
  /**
   * Runs whose asking budget is used: their waiting calls, decided
   * together (`speccoder approvals batch`).
   */
  batch?: BatchView[];
}

/**
 * What the approve sheet shows besides the requirements: who can approve
 * and run now, and what autopilot would hold.
 */
export interface ApproveSheet {
  /**
   * The approval digest *Approve and Run* would record now (decision
   * lines and frozen manifests): the click sends it back and is refused
   * when it no longer matches. `None`: it cannot be computed here.
   */
  sha256: string | null;
  /** Decision lines the approval records. */
  approves: string[];
  /**
   * Why *Approve and Run* is disabled now (each a line naming its
   * cause); empty: it can run.
   */
  blocked: string[];
  /** Why autopilot is disabled (no enforced sandbox); `None`: it can run. */
  autopilot_blocked: string | null;
  /** The sandbox line (`Seatbelt, network off`). */
  sandbox: string | null;
  /** The grants autopilot may hold, none ticked by default. */
  grants: OfferedGrant[];
}

/** Result of `speccoder spec archive` (an OpenSpec change). */
export interface ArchiveReport {
  /** The change. */
  change: string;
  /**
   * `probe`, `incomplete`, `unverified`, `awaiting_confirmation`,
   * `declined`, `archived` or `failed`.
   */
  stage: string;
  /** OpenSpec's own `{severity, code, message, fix}` from its last answer. */
  openspec_status: unknown;
  /** Tasks ticked without a speccoder verdict commit. */
  unverified_tasks: string[];
  /** What archiving changes (`openspec show <id> --diff`), when shown. */
  diff: string | null;
  /** Where OpenSpec moved the change. */
  archived_as: string | null;
  /** OpenSpec's counts of added, modified, removed and renamed requirements. */
  totals: unknown;
  /** The commit `archive <id>`. */
  commit: string | null;
  /** Who confirmed. */
  confirmed_by: string | null;
}

/** Presence of a feature's artifacts. */
export interface Artifacts {
  /** `spec.md` */
  spec: boolean;
  /** `plan.md` */
  plan: boolean;
  /** `tasks.md` */
  tasks: boolean;
  /** `research.md` */
  research: boolean;
  /** `data-model.md` */
  data_model: boolean;
  /** `contracts/` */
  contracts: boolean;
  /** `quickstart.md` */
  quickstart: boolean;
  /** `checklists/` */
  checklists: boolean;
}

/** What the assistant says it introduced. */
export interface Assumption {
  /** The proposal's words that carry it (`up to 10 000 rows`). */
  words: string;
  /** What the person wrote that it reads into, when anything (`fast`). */
  from: string;
  /** The question to ask instead of assuming it. */
  question: string;
}

/** Result of `speccoder attest verify` and `attest sign`. */
export interface AttestReport {
  /** Feature directory name. */
  feature: string;
  /**
   * Where trust came from: the allowed-signers file, whether GPG's keyring
   * was used, the expected Sigstore identity.
   */
  trust: unknown;
  /** One per object. */
  objects: ObjectVerification[];
  /** Count per outcome word. */
  totals: Record<string, number>;
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
  /** The words. */
  text: string;
  /** Always `author`. */
  by: string;
}

/** Who wrote a shown test. */
export type AuthoredBy = "model" | "person";

/** One call an autopilot run refused instead of asking. */
export interface AutopilotDenial {
  /** The task. */
  task: string;
  /** The model turn. */
  turn: number;
  /** `write`, `command`, `network`, `mcp` (empty: none resolved). */
  class: string;
  /** What it reached (`src/a.rs`, `cargo add x`). */
  object: string;
  /** `autopilot: not asked — listed for you`. */
  reason: string;
}

/** How the person decided the reproductions. */
export type Basis = "shown" | "named_test";

/** One class of a batch. */
export interface BatchGroup {
  /** `write`, `command`, `network` or `mcp`. */
  class: string;
  /** The waiting calls' ids. */
  calls: string[];
}

/** A batch decision. */
export type BatchVerdict = "approve" | "reject";

/** A run whose asking budget is used, with its waiting calls by class. */
export interface BatchView {
  /** The run. */
  run: string;
  /**
   * SHA-256 over the sorted digests of every call listed
   * ([`PendingCall::digest`]): a decision names it and is refused when
   * the list, or the object of a call in it, changed.
   */
  digest: string;
  /** The calls by class, in the order write, command, network, mcp. */
  groups: BatchGroup[];
}

/** Result of `speccoder bench import`. */
export interface BenchReport {
  /** The suite directory written. */
  out: string;
  /** Instances turned into cases. */
  imported: string[];
  /** Instances not imported, with why. */
  skipped: string[];
}

/** Result of `speccoder attest verify --branch` / `speccoder attest badge`. */
export interface BranchReport {
  /** The commit checked. */
  head: string;
  /** The merge base with the given base, when one was given. */
  base: string | null;
  /** Which units were covered. */
  mode: Mode;
  /** The units checked, in order. */
  features: string[];
  /** Every tick (and trailer-only task), in document order per unit. */
  ticks: TickReport[];
  /** Tasks skipped by a person's decision (informational). */
  skipped: SkippedTask[];
  /** Counts by outcome. */
  totals: BranchTotals;
  /** The conclusion. */
  conclusion: Conclusion;
  /**
   * The trust used: the allowed-signers file and the revision it was
   * read at, the Sigstore identity and issuer.
   */
  trust: unknown;
  /** Unsigned evidence was accepted (`--require-signed=false`). */
  accepted_unsigned: boolean;
  /** `regression_only` and `attested` fail too. */
  require_verified: boolean;
  /**
   * Requirement → task → test → verdict, from the signed `trace[]`,
   * with a row (no task) for each requirement no tick cites.
   */
  trace: TraceRow[];
  /** Evidenced (not stale) over all ticks; `None` without ticks. */
  share: number | null;
  /** Bug homes touched without a verdict commit (informational). */
  open_no_claim: string[];
  /** Rules this verifier version does not evaluate yet. */
  not_evaluated: string[];
  /** Predicate fields or revisions seen but unknown here (tolerated). */
  newer_fields: string[];
}

/** Counts by outcome, in the verdict bar's order. */
export interface BranchTotals {
  /** Evidenced `verified`. */
  verified: number;
  /** Evidenced `regression_only`. */
  regression_only: number;
  /** Evidenced `attested`. */
  attested: number;
  /** Stale. */
  stale: number;
  /** Unverifiable. */
  unverifiable: number;
  /** Regressed (reserved). */
  regressed: number;
  /** Skipped by a person (not ticks). */
  skipped: number;
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
  /** Where the format keeps it. */
  home: BugHome;
  /** The unit, as the adapter names it (`007-bugfixes`, `bugs/<slug>`). */
  unit: string;
  /** The requirement (`FR-003`, or `bug:<slug>`). */
  requirement: string;
  /** The task (`T003`, or `bug:<slug>`). */
  task: string;
  /** A short title. */
  title: string;
  /** The person's words. */
  triad: Triad;
  /** Where it is. */
  stage: BugStage;
  /** How it stands. */
  outcome: BugOutcome;
  /** The fix's verdict, once judged. */
  verdict: Verdict | null;
  /** The code of the last stop or refusal. */
  code: string | null;
  /** The reproductions (and *Unchanged* tests). */
  reproductions: Reproduction[];
  /** The held-out variants (never their content or path). */
  held_out: HeldOutVariant[];
  /** The sealed set holding them. */
  held_out_sealed: SealedRef | null;
  /**
   * Why no held-out variant is sealed (the author's call failed, none
   * was red): the verdict names it, and the confirmation says it first.
   */
  held_out_reason?: string | null;
  /** The digest waiting for the person's confirmation. */
  confirm_digest: string | null;
  /** The confirmation. */
  confirmation: Confirmation | null;
  /** Rejections of earlier reproductions. */
  rejections: Rejection[];
  /** What the fix's acceptance set came from. */
  acceptance: AcceptanceRef | null;
  /** The tests selected to stay green. */
  pass_to_pass: PassToPass | null;
  /** The revert candidate. */
  revert: RevertResult | null;
  /** The special-case candidates. */
  special_case: SpecialCase[];
  /** The reproduce tries. */
  tries: ReproduceTry[];
  /** At most this many tries. */
  max_tries: number;
  /** The engine's hand-off, when it stopped. */
  handoff: Handoff | null;
  /** The issue it came from. */
  issue: IssueRef | null;
  /** Someone else's fix, judged. */
  candidate: Candidate | null;
  /** The planning commit that recorded it. */
  recorded_commit: string | null;
  /** The verdict commit. */
  commit: string | null;
  /** The last run. */
  run_id: string | null;
}

/** Where the bug is. */
export type BugStage = "record" | "reproduce" | "confirm" | "unchanged" | "fix" | "ship" | "done";

/** One confirmed reproduction. */
export interface BugTest {
  /** The statement it reproduces (`E1`) or guards (`U1`). */
  statement: string;
  /** Project-relative test file. */
  file: string;
  /** The test's name. */
  test: string;
  /** Its class on the untouched code (`assertion`). */
  red_class: string;
  /**
   * Its content as confirmed; `None` when the kept copy is missing or no
   * longer matches its digest.
   */
  content: string | null;
}

/**
 * A bug's confirmed reproductions as a bug unit's Tests tab shows them,
 * read only: a bug's tests are written and confirmed by its own pipeline.
 */
export interface BugTests {
  /** The bug's task (`T001`). */
  task: string;
  /** Its title. */
  title: string;
  /** The person's confirmation of the reproductions. */
  confirmation: Confirmation;
  /** The reproductions, in the order confirmed. */
  reproductions: BugTest[];
}

/** Someone else's fix, judged as the attempt. */
export interface Candidate {
  /** A patch or a pull request. */
  origin: CandidateOrigin;
  /** The patch file, as given. */
  path?: string | null;
  /** The pull request's number. */
  number?: number | null;
  /** `sha256:<hex>` of the diff. */
  digest: string;
  /** The pull request's head commit. */
  head_sha?: string | null;
  /** It was applied as the attempt. */
  applied: boolean;
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
  /** The installed skill (`speckit-plan`). */
  skill: string;
  /** Its result's status (`ok`, `error`, …). */
  status: CommandStatus;
  /** Its message. */
  message: string;
  /** Its agentplane run, when it ran. */
  run_id: string | null;
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
    kind: "failed";
    /** The skill. */
    skill: string;
    /** Its result's message. */
    message: string;
    /** Its result's code. */
    code: string | null;
  }
  /** Done through the chosen step. */
  | {
    kind: "done";
    /** The plan checks of the task list, when one was written. */
    plan_checks: PlanCheck[];
  };

/** One check run. */
export interface CheckResult {
  /** Name from `speccoder.toml`. */
  name: string;
  /** The command, as run. */
  argv: string[];
  /** What happened. */
  status: CheckStatus;
  /** Process exit code, if it exited. */
  exit_code: number | null;
  /** Wall-clock duration. */
  duration_ms: number;
  /** Whether the check declares a JUnit report. */
  junit: boolean;
  /** Tests from the JUnit report, if one was declared and freshly written. */
  tests: TestResult[] | null;
  /** Last part of stdout (lossy UTF-8). */
  stdout_tail: string;
  /** Last part of stderr (lossy UTF-8). */
  stderr_tail: string;
  /** Output exceeded the capture cap. */
  truncated: boolean;
  /** Why the check was skipped, errored, or has no tests. */
  note: string | null;
  /**
   * It failed once and passed when run again: named, never trusted as
   * task evidence.
   */
  flaky: boolean;
}

/** Outcome of one check. */
export type CheckStatus = "passed" | "failed" | "timed_out" | "skipped" | "error";

/** One Spec Kit checklist (`specs/<feature>/checklists/*.md`). */
export interface ChecklistStatus {
  /** Path relative to the project root. */
  file: string;
  /** Checklist items. */
  total: number;
  /** Items checked. */
  done: number;
  /** Items not checked. */
  open: number;
}

export type Checklists = ChecklistStatus[];

/** One line comment of the Code view. */
export interface CodeComment {
  /** Project-relative path. */
  path: string;
  /** The line (1-based). */
  line: number;
  /** The code on that line. */
  quote: string;
  /** The person's words. */
  text: string;
}

/** One code, as `explain` shows it. */
export interface CodeInfo {
  /** `NEEDS-APPROVAL`. */
  code: string;
  /** Error, stop or warning. */
  kind: Kind;
  /** The status results with this code have. */
  status: CommandStatus;
  /** Its exit code. */
  exit_code: number;
  /** One line. */
  title: string;
  /** What it means and why it happens. */
  explain: string;
  /** How to resolve it (templates: `<task>`, `<feature>` are blanks). */
  fix: string[];
  /** Its section on the codes page. */
  docs: string;
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
  /** The toolchain. */
  toolchain: Toolchain;
  /** The rule (``Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare)``). */
  text: string;
  /** Where it came from (`Cargo.toml (default)`, `pyproject.toml [tool.pytest.ini_options]`). */
  source: string;
}

/** Command-specific payload of a result. */
export type CommandData = ProjectSnapshot | VerifyReport | WorktreeReport | ProposeReport | ReplayReport | RunReport | EvalReport | FeatureReport | ConvergeReport | StatusReport | ApprovalsReport | ControlReport | CostReport | PrReport | BenchReport | InitReport | SkillsReport | SkillRunReport | ChecklistStatus[] | ArchiveReport | InPlaceReport | AttestReport | ExplainReport | SchemaReport | DoctorReport | FeaturesReport | UiReport | ConfigReport | IssuesReport | InboxReport | ShapeReport | ReviewReport | AuthReport | BranchReport | EvidenceTour | ActionsReport | BugReport | PolicyCoverage | CommentsView | PresetReport | NewProjectReport | SampleReport | SimilarReport | RungSuggestion | PlanningChainReport;

/**
 * The envelope as it is serialised — the schema of [`CommandResult`]
 * (whose fields are private and whose serialisation is written by hand).
 */
export interface CommandResult {
  /** The envelope's version. */
  schema_version: number;
  /** The command. */
  command: string;
  /** The outcome class. */
  status: CommandStatus;
  /** The process exit status (always the status's). */
  exit_code: ExitCode;
  /** One line, for a person. */
  message: string;
  /**
   * The catalog code: present for every non-ok result and for a
   * degraded success.
   */
  code?: string | null;
  /** How to resolve it: commands, or `→ ` lines for a person. */
  fix?: string[] | null;
  /** The same lines, with how an interface takes each. */
  fix_steps?: Step[] | null;
  /** The code's section on the codes page. */
  docs?: string | null;
  /** The fix speccoder can apply itself, on the person's yes. */
  remedy?: Remedy | null;
  /** The steps that continue. */
  next?: NextStep[] | null;
  /** The name of `data`'s schema (`schemas/v2/<payload>.json`). */
  payload?: string | null;
  /** The command's payload. */
  data?: CommandData | null;
}

/** Outcome class of a command. */
export type CommandStatus = "ok" | "usage_error" | "error" | "not_a_project" | "diagnostics_error" | "selection_needed" | "config_error" | "verification_failed" | "git_precondition" | "not_found" | "run_failed" | "replay_diverged" | "task_not_ready" | "needs_human" | "eval_failed" | "not_replayable" | "attestation_failed";

/** What an anchored comment points at. */
export type CommentAnchor =
  /** A passage of a feature document. */
  | {
    kind: "passage";
    /** Project-relative path. */
    path: string;
    /** First line (1-based). */
    start_line: number;
    /** Last line. */
    end_line: number;
    /** The words selected. */
    quote: string;
    /** `sha256:<hex>` of the quote (filled when the draft is stored). */
    quote_sha256: string;
    /** Its section's heading path (`Requirements › Functional`). */
    section: string;
  }
  /** A line of the spec's changes since the last commit. */
  | {
    kind: "diff_line";
    /** Project-relative path. */
    path: string;
    /** `old` or `new`. */
    side: string;
    /** The line (1-based) on that side. */
    line: number;
    /** The line's text. */
    quote: string;
  }
  /** A line of a shown visible test. */
  | {
    kind: "test_line";
    /** The requirement. */
    req: string;
    /** The test's path. */
    path: string;
    /** The line (1-based). */
    line: number;
    /** The line's text. */
    quote: string;
  }
  /** A finding the review shows. */
  | {
    kind: "finding";
    /** `analyze` or `review`. */
    source: string;
    /** Its id. */
    id: string;
    /** Its text. */
    text: string;
  };

/** One comment in the tray. */
export interface CommentDraft {
  /** `c1`, `c2`, … */
  id: string;
  /** What it is about. */
  anchor: CommentAnchor;
  /** The person's words. */
  text: string;
  /** Who. */
  by: string;
  /** Unix seconds. */
  at: number;
  /** Where it is. */
  state: DraftState;
  /** The person kept it although its quote moved. */
  kept: boolean;
}

/** What a comment action does. */
export type CommentOp = "add" | "delete" | "keep";

/** Result of `speccoder review comments`: the tray and the last turn. */
export interface CommentsView {
  /** The feature. */
  feature: string;
  /** Every draft not yet sent, in order. */
  drafts: CommentDraft[];
  /** The last revision turn. */
  turn: RevisionView | null;
  /** The skill a send runs (`speckit-clarify`), when the format has one. */
  revise_with: string | null;
}

/** What changed relative to a baseline (pass-to-pass). */
export interface Comparison {
  /** Tests that passed in the baseline and now fail or are missing. */
  regressions: Regression[];
  /** Tests failing in both runs. */
  pre_existing: string[];
  /** Tests not in the baseline. */
  new_tests: string[];
  /** Tests collected in the baseline. */
  count_before: number;
  /** Tests collected now. */
  count_after: number;
  /** Checks failing now only because of tests that already failed in the baseline. */
  failing_as_before: string[];
}

/** The branch's conclusion. */
export type Conclusion = "success" | "failure" | "neutral";

/** One key: what it is, what it is set to, and what it is otherwise. */
export interface ConfigKey {
  /** The key, e.g. `max_turns`; dotted below a sub-table (`sigstore.identity`). */
  key: string;
  /**
   * `bool`, `integer`, `number`, `string`, `choice`, `duration`, `argv`,
   * `argv_list` or `string_list`.
   */
  kind: string;
  /** What it does. */
  about: string;
  /**
   * The value speccoder uses when the key is absent; none for keys
   * without a default (unset means off, or the key is required).
   */
  default: unknown;
  /** The value in the file, when set. */
  value: unknown;
  /** For `choice`: the allowed values. */
  choices: string[];
  /** For numbers: the smallest allowed. */
  min: number | null;
  /** For numbers: the largest allowed. */
  max: number | null;
  /** The section (or table) is refused without it. */
  required: boolean;
}

/**
 * Result of `speccoder config`: the project's configuration as a catalogue
 * of every key, each with its type, default and value (when set).
 */
export interface ConfigReport {
  /** The file, relative to the project root. */
  file: string;
  /** Whether it exists. */
  exists: boolean;
  /**
   * Whether it is committed as it is on disk (speccoder reads committed
   * configuration only, so an uncommitted change is not yet in force).
   */
  committed: boolean;
  /** Whether every parser accepts it. */
  valid: boolean;
  /** The first parser's complaint, when not valid. */
  error: string | null;
  /** Sections, in the reference's order. */
  sections: ConfigSection[];
  /** The uncommitted change, as a diff, when there is one. */
  diff: string | null;
}

/** One section of `speccoder.toml`. */
export interface ConfigSection {
  /** The table name, e.g. `model`, or `checks` for `[checks.<name>]`. */
  name: string;
  /** A title for people, e.g. `Model`. */
  title: string;
  /** What the section decides. */
  about: string;
  /**
   * Named tables (`[checks.<name>]`, `[mcp.<name>]`): `keys` describe
   * one, `tables` hold the configured ones.
   */
  named: boolean;
  /** The keys (for a named section: the keys of each table, unset). */
  keys: ConfigKey[];
  /** The configured named tables. */
  tables: ConfigTable[];
}

/** One configured named table, e.g. `[checks.test]`. */
export interface ConfigTable {
  /** Its name, e.g. `test`. */
  name: string;
  /** Its keys, with values. */
  keys: ConfigKey[];
}

/** The person's one confirmation, bound to what they saw. */
export interface Confirmation {
  /**
   * `sha256:<hex>` over the reproductions shown (their files and
   * statement ids).
   */
  digest: string;
  /** The triad's digest when confirmed: a different one lapses this. */
  triad_digest: string;
  /** Who. */
  by: string;
  /** Through what. */
  via: Via;
  /** Seen or named. */
  basis: Basis;
  /** Unix seconds. */
  at: number;
}

/** State of the project constitution. */
export type ConstitutionState = "present" | "unfilled" | "absent";

/** One part handed to a call. */
export interface ContextEntry {
  /** What it is. */
  kind: ContextKind;
  /**
   * Where it came from: `person`, `worktree`, `speccoder://<producer>/<name>`,
   * `github:<owner>/<repo>#<n>`.
   */
  source: string;
  /** A path, an id or a part pointer (`/bundle/plan`). */
  reference: string;
  /** Hex SHA-256 of the exact bytes given. */
  sha256: string;
  /** Their size. */
  bytes: number;
  /** Cut to a budget. */
  cut: boolean;
}

/** What a part is. A new kind is a new variant, never a free string. */
export type ContextKind = "constitution" | "plan" | "story" | "task" | "rejection" | "retry_digest" | "issue" | "scenario" | "note" | "comment" | "test_layout";

/** Every part of one call, sorted by kind and reference. */
export interface ContextManifest {
  /** The parts. */
  entries: ContextEntry[];
  /** Hex SHA-256 of [`ContextManifest::canonical`]. */
  digest: string;
}

/** Result of `accept`, `skip` and `nudge`. */
export interface ControlReport {
  /** Feature directory name. */
  feature: string;
  /** Task ID. */
  task: string;
  /** `accepted`, `skipped`, `unskipped`, `nudged` or `nudges_cleared`. */
  action: string;
  /** The person's reason or note. */
  text: string | null;
  /** The commit or ref that records it, when it lives in git. */
  record: string | null;
}

/** Result of `speccoder converge`. */
export interface ConvergeReport {
  /** Feature directory name. */
  feature: string;
  /** The agentplane run of the assessment, if one ran. */
  run_id: string | null;
  /** This round's number (1 = the first Convergence phase). */
  round: number;
  /**
   * `converged`, `tasks_appended`, `needs_review` (only unrequested
   * findings), `no_progress`, `round_limit` or `failed`.
   */
  outcome: string;
  /** Every finding as the model reported it (its claim, not evidence). */
  findings: unknown[];
  /** Tasks speccoder appended, as `T0NN text`. */
  appended: string[];
  /** `unrequested` findings: for a person, never turned into tasks. */
  unrequested: unknown[];
  /** Findings that repeat an earlier convergence task: no progress. */
  repeated: unknown[];
  /** The commit on `speccoder/<feature>` holding the appended section. */
  commit: string | null;
  /** Token usage of the assessment. */
  usage: unknown;
  /** Every tool call with the policy's decision. */
  tool_calls: unknown[];
  /** Extension hooks run, offered or skipped around it. */
  hooks: string[];
}

/** One convergence round, from its commit. */
export interface ConvergenceRound {
  /** Round number. */
  round: number;
  /** The commit that appended it. */
  commit: string;
  /** The appended tasks, as `T0NN text`. */
  tasks: string[];
}

/** Result of `speccoder report --cost`. */
export interface CostReport {
  /** Feature directory name. */
  feature: string;
  /** Per task, every attempt counted (passed and saved). */
  tasks: TaskCost[];
  /** Convergence assessments. */
  convergence: Usage;
  /** Everything. */
  total: Usage;
  /** The model the prices apply to, when prices are configured. */
  priced_model: string | null;
}

/** Rejected of judged, for one source. */
export interface Counts {
  /** Rejected (timeouts included). */
  rejected: number;
  /** Judged (unviable excluded). */
  judged: number;
}

/** Where one of the person's sentences went. */
export interface Coverage {
  /** The sentence, as the person wrote it. */
  sentence: string;
  /** The proposal's lines (0-based) that carry its words. */
  lines: number[];
  /** No line carries it. */
  dropped: boolean;
}

/** How calls of one argv were decided. */
export interface CoverageDecision {
  /** `allowed by rule`, `approved by a person`, `waiting`. */
  decision: string;
  /** How often. */
  count: number;
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
  /** Seconds since the epoch of the latest run that called it. */
  last_at: number | null;
  /** How it was decided then, with how often. */
  decided: CoverageDecision[];
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
  /** Task ID. */
  task: string;
  /** `approved` or `rejected`. */
  decision: string;
  /** What it covers. */
  covers: string;
  /** The reason, for a rejection. */
  reason: string | null;
  /** Who decided, as `name (via)`. */
  by: string | null;
  /** How far an approval reaches: `once`, `task` or `run`. */
  scope: string | null;
  /** An approval's note to the model. */
  note: string | null;
  /** The waiting call it decided, when it was one. */
  call: string | null;
  /** Who decided; `None` for an expiry. */
  operator: OperatorView | null;
  /**
   * Through what: an origin of `actions::Via` (`terminal`, `web`,
   * `acp`, …).
   */
  via: string | null;
  /** The editor decided in, over the Agent Client Protocol. */
  client?: string | null;
  /** Nobody decided: the deadline passed and the call was denied. */
  expired: boolean;
  /** The object it decided on. */
  object: ObjectView | null;
  /** The run it belongs to (a grant covers nothing after it). */
  run: string | null;
}

/** One requirement of a change's spec delta. */
export interface DeltaRequirement {
  /** The capability (`openspec/specs/<capability>/spec.md`). */
  capability: string;
  /** `added`, `modified`, `removed` or `renamed`. */
  operation: string;
  /** The requirement's name (the new name for a rename). */
  name: string;
  /** Its scenarios' names. */
  scenarios: string[];
  /** The delta file, relative to the project root. */
  path: string;
  /** Its heading's line. */
  line: number;
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
  /** How serious it is. */
  severity: Severity;
  /** Stable code, e.g. `tasks.duplicate-id`. */
  code: string;
  /** Workspace-relative file, if the problem is in one. */
  path: string | null;
  /** 1-based line, if known. */
  line: number | null;
  /** What is wrong. */
  message: string;
  /** What to do about it. */
  hint: string | null;
}

/**
 * The proposal's stretches (yours or added) and the person's (yours or
 * dropped).
 */
export interface Diff {
  /** Over the proposal. */
  proposal: Span[];
  /** Over the person's text. */
  original: Span[];
}

/** What the next attempt is told about the previous one. */
export interface Digest {
  /** The attempt this digest describes (1-based). */
  attempt: number;
  /** The verdict's reasons. */
  reasons: string[];
  /** Checks that did not pass. */
  checks: FailedCheck[];
  /** Paths the previous attempt changed; its changes were discarded. */
  discarded: string[];
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
  /** Pass, warn or fail. */
  level: DoctorLevel;
  /** What was found. */
  detail: string;
  /** The catalog code of a failure or warning. */
  code: string | null;
  /** How to resolve it. */
  fix: string[];
  /** The same lines, with how an interface takes each. */
  fix_steps: Step[];
  /** The fix speccoder can apply itself, on the person's yes. */
  remedy?: Remedy | null;
  /** The tasks a plan check names. */
  tasks?: string[];
}

/** How a doctor check came out. */
export type DoctorLevel = "pass" | "info" | "warn" | "fail";

/** Result of `speccoder doctor`. */
export interface DoctorReport {
  /** speccoder's version. */
  speccoder: string;
  /** `macos aarch64`. */
  os: string;
  /** Every check, in order. */
  checks: DoctorCheck[];
  /**
   * The agents that can make attempts over ACP, each ready or not
   * (`doctor --agents`; empty otherwise).
   */
  agents?: AgentCard[];
}

/** Where a draft comment is. */
export type DraftState = "draft" | "outdated" | "sent";

/**
 * What changed since a tick: the tick's commit holds the intent
 * it was judged against; the checkout's `HEAD` holds the intent now.
 */
export interface Drift {
  /**
   * The requirements the task cites whose line changed, appeared or went
   * away: the evidence no longer speaks for them. Empty: what changed is
   * elsewhere in the intent.
   */
  requirements: RequirementDrift[];
  /**
   * The intent files that differ (project-relative), task lists aside.
   * Empty when the intent is not in git (in place, ignored): it changed,
   * which file is not known.
   */
  files: string[];
}

/** A failed attempt that was undone and retried. */
export interface EarlierAttempt {
  /** The agentplane run of the attempt. */
  run_id: string;
  /** The attempt's status. */
  attempt_status: string;
  /** The verifier's verdict. */
  verdict: Verdict | null;
  /** The failure's fingerprint. */
  fingerprint: string;
  /** What the next attempt was told. */
  digest: Digest;
  /** Token usage of the attempt. */
  usage: unknown;
}

/**
 * The envelope as it is serialised — the schema of [`CommandResult`]
 * (whose fields are private and whose serialisation is written by hand).
 */
export interface Envelope {
  /** The envelope's version. */
  schema_version: number;
  /** The command. */
  command: string;
  /** The outcome class. */
  status: CommandStatus;
  /** The process exit status (always the status's). */
  exit_code: ExitCode;
  /** One line, for a person. */
  message: string;
  /**
   * The catalog code: present for every non-ok result and for a
   * degraded success.
   */
  code?: string | null;
  /** How to resolve it: commands, or `→ ` lines for a person. */
  fix?: string[] | null;
  /** The same lines, with how an interface takes each. */
  fix_steps?: Step[] | null;
  /** The code's section on the codes page. */
  docs?: string | null;
  /** The fix speccoder can apply itself, on the person's yes. */
  remedy?: Remedy | null;
  /** The steps that continue. */
  next?: NextStep[] | null;
  /** The name of `data`'s schema (`schemas/v2/<payload>.json`). */
  payload?: string | null;
  /** The command's payload. */
  data?: CommandData | null;
}

/** One judged case. */
export interface EvalCase {
  /** Directory name. */
  name: string;
  /**
   * What the case expects: `complete`, `stop` or `reject`; a bug case also
   * `already_satisfied` or `cannot_reproduce`.
   */
  expect: string;
  /** `correct`, `false_completion`, `not_completed`, `honest_stop`, `caught`, `cheat`, `invalid_case` or `error`. */
  classification: string;
  /** The run's status word and message. */
  outcome: string;
  /** The attempt's run id, when one ran. */
  run_id: string | null;
  /** The attempt status. */
  attempt_status: string | null;
  /** The verifier's verdict. */
  verdict: Verdict | null;
  /** Whether the held-out oracle passed (when it ran). */
  oracle_passed: boolean | null;
  /** Token usage of the attempt. */
  usage: unknown;
  /** The attempt's tool calls with the policy's decisions. */
  tool_calls: unknown[];
  /** The model's own closing report, or its reason for stopping. */
  model_said: string | null;
  /** Attempts made at the task, retries included. */
  attempts: number;
  /** The round of a repeated suite run (1-based). */
  round: number;
  /** Whether the gaming monitor flagged the attempt, when it ran. */
  monitor_flagged: boolean | null;
  /**
   * Whether the recorded attempt replays strictly from its journal;
   * `None` when there is no journaled run (external executors).
   */
  replays: boolean | null;
  /** The held-out tests' part (`acceptance.held_out` of the run report). */
  held_out: unknown;
  /** An attempt passed the visible checks and failed the held-out tests. */
  gap: boolean;
  /** The combined acceptance strength, when measured. */
  strength?: number | null;
  /** Stopped because a test rejected a permitted alternative. */
  over_strict: boolean;
}

/** Result of `speccoder eval`. */
export interface EvalReport {
  /** The model the suite ran with, `provider/model`. */
  model: string;
  /** One entry per case, in name order. */
  cases: EvalCase[];
  /** Counts per classification. */
  summary: EvalSummary;
}

/** Suite counts and metrics. */
export interface EvalSummary {
  /** Cases run. */
  cases: number;
  /** Completed and right. */
  correct: number;
  /** Completed with a passing verdict but wrong. */
  false_completions: number;
  /** Possible cases that stopped or failed verification. */
  not_completed: number;
  /** Impossible cases that stopped for a person. */
  honest_stops: number;
  /** Impossible cases that finished with green checks. */
  cheats: number;
  /** Cases that could not be judged. */
  invalid_or_error: number;
  /** correct / (correct + false completions), when defined. */
  precision: number | null;
  /** Input + output tokens per correct case, when any. */
  tokens_per_correct: number | null;
  /** How well the gaming monitor did, when it ran on any case. */
  monitor: MonitorScore | null;
  /** Recorded attempts checked by strict replay, and how many replayed. */
  replay_checked: number;
  /** Of those, the ones that replayed exactly. */
  replayed: number;
  /** `expect = "reject"` cases that were not ticked. */
  caught: number;
  /**
   * Attempts that passed visibly, and how many of them the held-out tests
   * failed; `None` when no held-out test ran.
   */
  held_out_gap: HeldOutGap | null;
  /** Ticked cases whose held-out tests passed, against the oracle. */
  held_out_oracle: HeldOutOracle | null;
  /**
   * The median combined acceptance strength over verified cases that
   * measured it; `None`: not measured.
   */
  median_acceptance_strength: number | null;
  /**
   * Cases stopped for a person because a test rejected a permitted
   * alternative.
   */
  over_strict_stops: number;
}

/** One line of the stream. */
export type EventLine = {
  /** The envelope version: the result envelope's `schema_version`. */
  v: number;
  /** 1, 2, 3, … — a gap means a lost line. */
  seq: number;
  /** RFC 3339 UTC, milliseconds. */
  ts: string;
} & (
  /** The first line: who is speaking, and what the stream supports. */
  | {
    type: "session.started";
    /** speccoder's version. */
    speccoder: string;
    /** The command. */
    command: string;
    /** Feature-detect on these, never on the version. */
    capabilities: string[];
  }
  /** A feature run begins, with its plan. */
  | {
    type: "run.started";
    /** Feature directory name. */
    feature: string;
    /** Every task, in document order. */
    tasks: PlannedTask[];
  }
  /** A task's run begins. */
  | {
    type: "task.started";
    /** Feature. */
    feature: string;
    /** Task ID. */
    task: string;
    /** Title. */
    title: string;
  }
  /** A stage begins. */
  | {
    type: "stage.started";
    /** Task ID. */
    task: string;
    /** The stage. */
    stage: Stage;
    /** `1/2`, a short commit id. */
    detail: string | null;
  }
  /** A stage ends (or does not apply). */
  | {
    type: "stage.finished";
    /** Task ID. */
    task: string;
    /** The stage. */
    stage: Stage;
    /** `passed`, `failed` or `skipped`. */
    state: StageState;
    /** Detail, or why it was skipped. */
    detail: string | null;
    /** How long it took. */
    elapsed_ms: number | null;
    /** The last lines of a failed check. */
    tail: string[];
  }
  /** A run picks up an attempt a killed process left behind. */
  | {
    type: "task.resumed";
    /** Task ID. */
    task: string;
    /** Where it was. */
    stage: Stage | null;
    /** The last tool call. */
    last_call: string | null;
  }
  /** A tool call and the policy's decision. */
  | {
    type: "tool_call";
    /** Task ID. */
    task: string;
    /** Model turn, 1-based. */
    turn: number;
    /** The tool. */
    tool: string;
    /** A path, a command, a pattern. */
    target: string;
    /** `allow`, `deny`, `approval`, `unknown`, `stop`. */
    decision: string;
    /** Whether an allowed call succeeded. */
    ok: boolean | null;
    /** The last lines of a failed command. */
    tail: string[];
  }
  /** A call waits for a person. */
  | {
    type: "approval.requested";
    /** Task ID. */
    task: string;
    /** The call id. */
    call: string;
    /** The tool. */
    tool: string;
    /** What waits. */
    what: string;
    /** The object's class: `write`, `command`, `network`, `mcp`. */
    class: string;
    /** The object's short form (path, `argv#…`, `tool@…`). */
    object: string;
  }
  /** A person decided a call. */
  | {
    type: "approval.decided";
    /** Task ID. */
    task: string;
    /** The call id. */
    call: string;
    /** `approve` or `reject`. */
    decision: string;
    /** `once`, `task`, `run`. */
    scope: string | null;
    /** Who. */
    by: string;
    /** `asserted`, `connected`, `expired`. */
    how: string;
    /** The object's class. */
    class: string;
    /** The object's short form. */
    object: string;
  }
  /** Tokens so far in the current attempt (at most once a second). */
  | {
    type: "usage";
    /** Task ID. */
    task: string;
    /** Input tokens. */
    input_tokens: number;
    /** Output tokens. */
    output_tokens: number;
    /** Input tokens read from the cache. */
    cache_read_tokens: number;
    /** Input tokens written to the cache. */
    cache_write_tokens: number;
    /** USD, when priced. */
    usd: number | null;
  }
  /** A task's run ended. */
  | {
    type: "task.finished";
    /** Task ID. */
    task: string;
    /** `verified`, `regression_only`, `failed`, `needs_approval`, … */
    outcome: string;
    /** The run's message. */
    message: string;
    /** The attempt's run id. */
    run: string | null;
    /** Input tokens. */
    input_tokens: number;
    /** Output tokens. */
    output_tokens: number;
    /** Cache reads. */
    cache_read_tokens: number;
    /** Cache writes. */
    cache_write_tokens: number;
    /** USD, when priced. */
    usd: number | null;
    /** A regression-only tick: the reason's name. */
    regression_reason?: string | null;
    /** A regression-only tick: the reason with its details and wording. */
    regression?: RegressionOnly | null;
  }
  /** The attempt's changed test files were checked against what the run collected. */
  | {
    type: "collection.checked";
    /** The task. */
    task: string;
    /** Each judged test file. */
    files: PublicCollectionFile[];
  }
  /** The search for the tick that made already-passing tests pass ended. */
  | {
    type: "regression.bisected";
    /** Task ID. */
    task: string;
    /** Acceptance runs it took. */
    runs: number;
    /** The implementing task, `before_feature`, or `null`. */
    result: string | null;
  }
  /**
   * An external agent makes the attempt: its name and permission mode
   * (`decides_alone`: it was not put into a mode that asks first).
   */
  | {
    type: "agent.started";
    /** Task ID. */
    task: string;
    /** The agent's name. */
    agent: string;
    /** Its permission mode, in a sentence. */
    mode: string;
    /** Calls it makes without asking never reach speccoder. */
    decides_alone: boolean;
    /** A recording says whether it asks. */
    known: boolean;
  }
  /** An external agent changed something without asking. */
  | {
    type: "agent.unasked";
    /** Task ID. */
    task: string;
    /** Calls so far. */
    calls: number;
  }
  /** An external agent's plan, as it last sent it: a claim. */
  | {
    type: "agent.plan";
    /** Task ID. */
    task: string;
    /** Its entries. */
    entries: AgentPlanEntry[];
  }
  /** What the model said in a turn: its account, a claim. */
  | {
    type: "said";
    /** Task ID. */
    task: string;
    /** Model turn (1-based). */
    turn: number;
    /** The words. */
    text: string;
  }
  /** The model's summary of its reasoning in a turn: its account, a claim. */
  | {
    type: "reasoning";
    /** Task ID. */
    task: string;
    /** Model turn (1-based). */
    turn: number;
    /** The summary. */
    text: string;
  }
  /** Anything else worth a line. */
  | {
    type: "notice";
    /** The line. */
    text: string;
  }
  /** A bug's triad was recorded. */
  | {
    type: "bug.recorded";
    /** Its task. */
    task: string;
    /** The home's kind. */
    home: string;
    /** The unit. */
    unit: string;
    /** The requirement. */
    requirement: string;
    /** The triad's digest. */
    triad_digest: string;
    /** Through what. */
    via: string;
  }
  /** One reproduce try ended. */
  | {
    type: "bug.reproduce_try";
    /** Its task. */
    task: string;
    /** The try. */
    n: number;
    /** Of at most. */
    of: number;
    /** Each statement's class. */
    per_statement: StatementClass[];
  }
  /** Every statement is reproduced. */
  | {
    type: "bug.reproduced";
    /** Its task. */
    task: string;
    /** The reproductions (never their content). */
    reproductions: ShownReproduction[];
  }
  /** The reproductions wait for the person. */
  | {
    type: "bug.confirm_waiting";
    /** Its task. */
    task: string;
    /** The digest to confirm. */
    digest: string;
    /** The command. */
    next: string;
  }
  /** The person confirmed them. */
  | {
    type: "bug.confirmed";
    /** Its task. */
    task: string;
    /** The digest confirmed. */
    digest: string;
    /** Who. */
    by: string;
    /** Through what. */
    via: string;
    /** `shown` or `named_test`. */
    basis: string;
  }
  /** The person rejected them. */
  | {
    type: "bug.confirm_rejected";
    /** Its task. */
    task: string;
    /** The digest rejected. */
    digest: string;
    /** The person's reason. */
    reason: string;
  }
  /** The pass-to-pass set was chosen. */
  | {
    type: "bug.unchanged_selected";
    /** Its task. */
    task: string;
    /** `selected` or `none_selected`. */
    state: string;
    /** Members. */
    count: number;
  }
  /** The revert and the special cases were judged. */
  | {
    type: "bug.discrimination";
    /** Its task. */
    task: string;
    /** `rejected`, `survived`, `unviable`. */
    revert: string;
    /** The special cases. */
    special_case: ShownSpecialCase[];
  }
  /** The bug's outcome. */
  | {
    type: "bug.outcome";
    /** Its task. */
    task: string;
    /** `awaiting_confirmation`, `cannot_reproduce`, `fixed`, … */
    outcome: string;
    /** The fix's verdict. */
    verdict: string | null;
    /** The stop's code. */
    code: string | null;
    /** What to do next. */
    next: string[];
  }
  /** Writing tests at review went one stage further. */
  | {
    type: "oracle.stage";
    /** The requirement. */
    requirement: string;
    /** `writing`, `comparing`, `checking_red`, `held_out`. */
    stage: string;
    /** This stage's step. */
    n: number;
    /** Of. */
    of: number;
  }
  /** An approval froze a requirement's tests. */
  | {
    type: "oracle.frozen";
    /** The unit. */
    unit: string;
    /** The requirement. */
    requirement: string;
    /** The visible set's manifest digest. */
    manifest_sha256: string;
    /** The approval's digest. */
    approval_sha256: string;
    /** Visible tests frozen. */
    visible_count: number;
    /** Held-out tests sealed with them. */
    held_out_count: number;
    /** Who. */
    by: string;
    /** Through what. */
    via: string;
  }
  /** A frozen oracle lapsed with its requirement's wording. */
  | {
    type: "oracle.lapsed";
    /** The unit. */
    unit: string;
    /** The requirement. */
    requirement: string;
    /** The wording it was frozen on. */
    frozen_wording_sha256: string;
    /** The wording now. */
    current_wording_sha256: string;
  }
  /** The last line: the command's result, as `--json` prints it. */
  | {
    type: "result";
    /** The result envelope. */
    result: CommandResult;
  }
  /** Plan through stopped where a person decides, at a failure or at the end. */
  | {
    type: "planning.stopped";
    /** The unit, once it exists. */
    unit: string | null;
    /** `questions`, `review`, `failed`, `done`. */
    kind: string;
    /** The skill a continuation runs next. */
    next_skill: string | null;
  }
  /** A step of a new project's plan began. */
  | {
    type: "scaffold.started";
    /** The step (`git_init`, `scaffold.1`, `format_init`, …). */
    step: string;
    /** Its command, exactly. */
    argv: string[];
    /** `local` or `network`. */
    access: string;
  }
  /** A step of a new project's plan ended. */
  | {
    type: "scaffold.finished";
    /** The step. */
    step: string;
    /** `done`, `skipped`, `failed`. */
    state: string;
    /** Its exit status, when it ran. */
    exit: number | null;
    /** How long it took. */
    duration_ms: number;
  }
  /** A new project was created and committed. */
  | {
    type: "project.created";
    /** SHA-256 of the folder's canonical path. */
    path_digest: string;
    /** `new_folder` or `existing_repository`. */
    mode: string;
    /** The stack. */
    stack: string;
    /** The spec format. */
    format: string;
    /** speccoder's commit. */
    commit: string;
    /** Through what. */
    via: string;
  }
  /** A project speccoder created was undone. */
  | {
    type: "project.undone";
    /** SHA-256 of the folder's canonical path. */
    path_digest: string;
    /** `trash` or `revert`. */
    kind: string;
    /** Through what. */
    via: string;
  });

/** Result of `speccoder attest tour` and `/api/tour`: one task's tour. */
export interface EvidenceTour {
  /** The unit. */
  feature: string;
  /** Task ID. */
  task: string;
  /** The task's title. */
  title: string;
  /** The verdict the evidence claims, when any. */
  verdict: string | null;
  /** The task commit, when one carries the task's evidence. */
  commit: string | null;
  /** The six steps, in order — only the first for an unverifiable tick. */
  steps: TourStep[];
  /** For an unverifiable tick: why the evidence does not count. */
  unverifiable: string | null;
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
  /** A bundled name, a local path or a URL, as given. */
  source: string;
  /** `bundled`, `path` or `url`. */
  kind: ExtensionKind;
}

/** What an extension source is. */
export type ExtensionKind = "bundled" | "path" | "url";

/** One check that did not pass. */
export interface FailedCheck {
  /** Name from `speccoder.toml` (or `acceptance`). */
  name: string;
  /** The command, as run. */
  argv: string[];
  /** What happened. */
  status: CheckStatus;
  /** Failing test IDs, when the check reports tests. */
  failing_tests: string[];
  /** Output from the first error line on, capped. */
  excerpt: string;
}

/** One feature (or change) of the project, as `speccoder feature` lists it. */
export interface FeatureEntry {
  /** 1-based position, the number `speccoder feature <n>` takes. */
  number: number;
  /** Directory name, e.g. `001-greet`. */
  name: string;
  /** The title its spec (or proposal) gives, when it has one. */
  title?: string | null;
  /** Repository-relative directory. */
  dir: string;
  /** The current one. */
  current: boolean;
  /** Ticked tasks. */
  done: number;
  /** All tasks (0: no task list yet). */
  tasks: number;
  /** Which planning artifacts exist: `spec`, `plan`, `tasks`. */
  artifacts: string[];
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
  /** Feature directory name. */
  feature: string;
  /** Every attempt, in order. */
  attempts: RunReport[];
  /** Why the run stopped. */
  stop: string;
  /** Convergence rounds between task runs (`run --converge`), in order. */
  converge: ConvergeReport[];
  /** Extension hooks run, offered or skipped around it. */
  hooks: string[];
  /** What parallel groups did: merged, conflicts, re-verification. */
  parallel: string[];
  /** The `full` check tier, run once after the run ticked the last task. */
  full_tier?: FullTier | null;
  /** How the run asked for a person: `ask` or `autopilot`. */
  mode: RunMode;
  /**
   * The calls an autopilot run refused instead of asking: listed for
   * the person.
   */
  autopilot_denied?: AutopilotDenial[];
}

/** The selected feature's artifacts and tasks. */
export interface FeatureSnapshot {
  /** The feature directory. */
  feature: FeatureRef;
  /** Which artifacts exist. */
  artifacts: Artifacts;
  /** The parsed task list, if `tasks.md` was readable. */
  tasks: ParsedTasks | null;
  /** Dependencies and states of the tasks; present iff `tasks` is. */
  schedule: Schedule | null;
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
  /** `feature` or `change`. */
  unit: string;
  /** The current one's name. */
  current: string | null;
  /**
   * How it is current: `argument`, `environment`, `recorded`,
   * `only_feature` or `none`.
   */
  reason: string;
  /** The same in words (`recorded in .specify/feature.json`). */
  reason_text: string;
  /** What this call did: `listed`, `switched` or `cleared`. */
  action: string;
  /** Every feature, in order. */
  features: FeatureEntry[];
}

/** One judged test file. */
export interface FileCollection {
  /** Workspace-relative path. */
  path: string;
  /** Collected, not collected, or unknown. */
  state: FileState;
  /** What decided it; `null` only when unknown. */
  decided_by: DecidedBy | null;
  /** Tests from it that ran (skipped and ignored ones excluded). */
  tests: number;
  /** The first 20 of their ids. */
  test_ids: string[];
  /** The runner's rule (not collected). */
  rule?: CollectionRule | null;
  /**
   * Not collected: which part of the rule the file breaks; unknown: why
   * speccoder cannot tell.
   */
  why?: string | null;
  /** The ways out, for this toolchain and file. */
  fixes?: string[];
}

/** Whether the run collected a test from a file. */
export type FileState = "collected" | "not_collected" | "unknown";

/** A reproduction on the final tree. */
export type FinalState = "passed" | "failed" | "not_run";

/** A writing-check hit. */
export interface Finding {
  /**
   * `vague`, `escape_clause`, `open_ended`, `no_obligation`,
   * `when_without_then`, `no_failure_path`.
   */
  rule: string;
  /** The words it is about (empty for the whole text). */
  words: string;
  /** Why it makes the requirement hard to test, in a line. */
  reason: string;
  /** What to write instead. */
  fix: string;
}

/** A spec-driven format. */
export type FormatKind = "speckit" | "openspec" | "kiro";

/** A spec format the plan can initialise, and its tool's state. */
export interface FormatOption {
  /** `speckit`, `openspec`. */
  id: string;
  /** `Spec Kit`, `OpenSpec`. */
  label: string;
  /** The tool's version when installed. */
  version: string | null;
  /** Why it cannot be chosen. */
  disabled: string | null;
}

/** The `full` check tier at the end of a feature. */
export interface FullTier {
  /** The checks that ran. */
  ran: string[];
  /** Every check passed (or none applied). */
  passed: boolean;
  /** The checks that failed. */
  failed: string[];
  /** Why nothing ran, or the record could not be written. */
  note: string | null;
}

/** The `min_strength` gate. */
export type Gate = "off" | "met" | "regenerated_met" | "failed";

/** The person's git identity. */
export interface GitIdentity {
  /** `user.name`. */
  name: string;
  /** `user.email`. */
  email: string;
}

/** A group of Needs You and how much is in it. */
export interface GroupCount {
  /** `blocked_on_you`, `ready_to_review`, `working` or `idle`. */
  group: string;
  /** As people read it. */
  title: string;
  /** Items, runs or features in it. */
  count: number;
}

/**
 * The hand-off of a stopped bug, composed by the engine from the journal
 * (never written by a model).
 */
export interface Handoff {
  /** The outcome it hands off. */
  outcome: BugOutcome;
  /** The tries made. */
  tries: ReproduceTry[];
  /** Per statement. */
  statements: StatementsShown;
  /** Tokens the tries spent. */
  cost?: unknown;
  /** How long reproduce took. */
  elapsed_ms: number;
  /** What the person can do. */
  next: NextStep[];
  /** The author's last words, labelled as its claim. */
  claim?: AuthorClaim | null;
}

/** Visible passes the held-out tests caught. */
export interface HeldOutGap {
  /** Attempts the held-out tests judged (they had passed visibly). */
  visible_passed: number;
  /** Of those, the ones the held-out tests failed. */
  held_out_failed: number;
  /** `held_out_failed / visible_passed`. */
  rate: number;
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
  /** `kept`, `absent` or `off` (`[acceptance] held_out = false`). */
  state: string;
  /** Tests kept. */
  count: number;
  /** The sealed manifest's digest, when kept. */
  manifest_sha256: string | null;
  /** Why none was kept (`not_red`, `none_generated`, …). */
  reason: string | null;
}

/** A held-out variant, as the payload shows it: never its content or path. */
export interface HeldOutVariant {
  /** The statement it covers. */
  statement: string;
  /** On the untouched code. */
  baseline: RedClass;
  /** On the final tree. */
  final: FinalState;
}

/** Where the format keeps the bug. */
export type HomeKind = "native_speckit_bug" | "native_kiro_bugfix" | "standing_speckit" | "standing_openspec";

/** How an interface takes a line. */
export type How =
  /** speccoder performs it (`actions::perform`). */
  | {
    how: "perform";
    /** What runs. */
    run: Runnable;
    /** What to ask for first (`Why it is skipped`); `None`: nothing. */
    input: string | null;
    /** The words the line gives already (a `--reason "…"` filled in). */
    text: string | null;
    /**
     * What to say in a confirmation first (it leaves the machine or
     * cannot be undone).
     */
    confirm: string | null;
    /** The feature the line names (`--feature F`); `None`: the current. */
    feature: string | null;
  }
  /** A view to open. */
  | {
    how: "view";
    /** Which. */
    view: View;
    /** What it is about: a code, a task. */
    subject: string | null;
  }
  /**
   * A command speccoder printed as a fix and can run for the person, in
   * the project folder and the OS sandbox (see `outside`) — never one
   * composed elsewhere. Several argvs run in order, each only after the last
   * one succeeded (`a && b`).
   */
  | {
    how: "command";
    /** The argvs, exactly. */
    argvs: string[][];
    /**
     * It reads the keyboard (a login): a terminal of the person's
     * own, not a panel.
     */
    interactive: boolean;
    /**
     * What it changes outside the project, for a command that runs
     * outside the sandbox by its nature (`changes your global git
     * config`); `None`: it runs in the sandbox, writes confined to
     * the project and the package caches.
     */
    outside?: string | null;
    /**
     * The host's proof that it issued exactly these argvs; only a
     * host that runs commands sets it.
     */
    token?: string | null;
  }
  /**
   * It must run in the shell speccoder starts from (`export NAME=…`):
   * nothing run elsewhere reaches that shell.
   */
  | {
    how: "shell";
    /** Why it cannot run here. */
    why: string;
    /** The setting that stands for it (`model`), if there is one. */
    setting: string | null;
  }
  /** Words for a person (`→ …`), or a command only they can complete. */
  | {
    how: "words";
    /** Why it is not taken here; `None` for words. */
    why: string | null;
  };

/** Which tick made already-passing acceptance tests pass. */
export type ImplementedBy =
  /** A tick on the feature branch. */
  | {
    /** Its task. */
    task: string;
    /** Its commit (short). */
    commit: string;
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
  /** Its group: `blocked_on_you` or `ready_to_review`. */
  group: string;
  /**
   * Ready to review: what the evidence leaves to the person, the
   * riskiest first; none for strong verified work.
   */
  risk?: RiskReason[];
  /**
   * An approval: the tool its call is for (approvals of one tool can be
   * decided together, each on its own object).
   */
  tool?: string | null;
  /** The feature. */
  feature: string;
  /** The task, when it is about one. */
  task: string | null;
  /** What to decide, in a line. */
  title: string;
  /** What it rests on: the call, the reason, the changed requirement. */
  detail: string;
  /** Waiting since (Unix seconds), when known. */
  since: number | null;
  /**
   * The decisions, the one that moves the work forward first; each runs
   * in the item's feature.
   */
  actions: Action[];
  /** `unverified`: the feature's ticks and why each regression-only one is. */
  unverified?: Unverified | null;
}

/**
 * Result of `speccoder inbox`: every decision that waits for a
 * person, across the project's features, the most urgent first — the
 * person's work once the agents write the code.
 */
export interface InboxReport {
  /** The items, the most urgent first (see [`InboxItem::kind`]). */
  items: InboxItem[];
  /** Features looked at. */
  features: number;
  /**
   * Every group with its count, in the order Needs You shows them:
   * `blocked_on_you`, `ready_to_review`, `working`, `idle`.
   */
  groups: GroupCount[];
  /** Runs in progress (or whose state is unknown). */
  working: WorkingRun[];
  /** Features with nothing running and nothing waiting. */
  idle: string[];
}

/** One detected check. */
export interface InitCheck {
  /** `[checks.<name>]`. */
  name: string;
  /** The command. */
  argv: string[];
  /** `fast` or `full`. */
  tier: string;
  /** Why it was proposed. */
  why: string;
}

/** The model key `init` looked for. */
export interface InitKey {
  /** `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `AWS_BEARER_TOKEN_BEDROCK`. */
  variable: string;
  /** Set in this environment (Bedrock: a credential source was found). */
  set: boolean;
  /**
   * Bedrock: where the AWS chain will take credentials from, by kind and
   * name (`aws login session (profile dev)`), never a value.
   */
  source?: string | null;
}

/** Result of `speccoder init`. */
export interface InitReport {
  /** The configuration file. */
  path: string;
  /** The checks it declares. */
  checks: InitCheck[];
  /** `provider/model`. */
  model: string;
  /** Files written, relative to the project root. */
  wrote: string[];
  /** The checks run on the untouched code (empty: not run). */
  ran: InitRun[];
  /** The repository instructions found (`AGENTS.md`, `CLAUDE.md`). */
  instructions: string[];
  /** The model key: the variable and whether it is set. */
  key: InitKey;
  /** The region written (Bedrock only). */
  region?: string | null;
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
  /** The checkout has a `.worktreeinclude`, which runs read already. */
  worktreeinclude: boolean;
}

/** One check `init` ran on the untouched code. */
export interface InitRun {
  /** `[checks.<name>]`. */
  name: string;
  /** Passed on the untouched code. */
  passed: boolean;
  /** `passed`, `failed`, `timed_out`, `skipped`, `error`. */
  status: string;
  /** How long it took. */
  duration_ms: number;
  /**
   * Why it did not pass: the last line of its output that says something
   * (`No module named pytest`), or the runner's note.
   */
  reason: string | null;
}

/** A repository instructions file given to an attempt. */
export interface InstructionFile {
  /** Repository-relative path. */
  path: string;
  /** sha256 of the file. */
  sha256: string;
  /** Its size. */
  bytes: number;
  /** Cut to fit the instructions budget. */
  cut: boolean;
}

/** One open issue. */
export interface IssueEntry {
  /** `#n`. */
  number: number;
  /** Its title. */
  title: string;
  /** Its address. */
  url: string;
  /** Label names. */
  labels: string[];
  /** Who it is assigned to (logins). */
  assignees: string[];
  /** Assigned to the person speccoder is signed in as. */
  mine: boolean;
  /** Last update, ISO 8601. */
  updated: string | null;
  /** The feature whose spec names this issue, when one does. */
  feature: string | null;
}

/** The issue a bug was recorded from. */
export interface IssueRef {
  /** Its number. */
  number: number;
  /** `owner/repo`, when known. */
  repo?: string | null;
  /**
   * The digest of its context-manifest entry (title and body as given
   * to the reproduction author).
   */
  context_digest?: string | null;
}

/**
 * Result of `speccoder issue`: the repository's open GitHub issues, read
 * through GitHub's API as the person.
 */
export interface IssuesReport {
  /** `owner/name`. */
  repo: string | null;
  /**
   * Open issues: assigned to the person first, then the most recently
   * updated.
   */
  issues: IssueEntry[];
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

/** A task's lane. */
export type Lane = "code" | "planning" | "yours";

/** An attempt that did not pass, as saved under `refs/speccoder/attempts/`. */
export interface LastAttempt {
  /** `verification_failed`, `blocked`, `needs_approval`, `not_demonstrated`, `interrupted`, … */
  outcome: string;
  /** Why. */
  reason: string;
  /** The agentplane run, when there was one. */
  run_id: string | null;
  /** The ref holding the attempt's tree. */
  reference: string;
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
  /** Cases the monitor judged. */
  judged: number;
  /** Flagged and really a cheat or false completion. */
  true_positives: number;
  /** Flagged but correct. */
  false_positives: number;
  /** Not flagged but a cheat or false completion. */
  false_negatives: number;
  /** true / (true + false positives), when anything was flagged. */
  precision: number | null;
  /** true positives / all positives, when there were any. */
  recall: number | null;
}

/** Mutants of the changed code, run against the acceptance command only. */
export interface Mutants {
  /** The tool. */
  tool: string;
  /** Caught by an assertion. */
  caught: number;
  /** Not caught. */
  missed: number;
  /** Caught by the deadline. */
  timeout: number;
  /** Did not build. */
  unviable: number;
  /** The missed ones, as the tool names them. */
  survivors: string[];
}

/** A model-written near miss: plausible, and wrong about one scenario. */
export interface NearMiss {
  /** `near-miss-1`, … */
  id: string;
  /** The scenario or requirement it gets wrong. */
  scenario: string;
  /** What it gets wrong, in one line (the author's claim). */
  rationale: string;
  /** Against the visible set. */
  visible: Outcome;
  /** Against the visible and the held-out set together. */
  combined: Outcome;
}

/** What the person chose; with the folder, everything the plan follows from. */
export interface NewChoices {
  /** What they are building, verbatim (kept for the first feature). */
  idea: string;
  /** The stack; `None`: the suggestion. */
  stack: Stack | null;
  /** The spec format (`speckit`, `openspec`); `None`: Spec Kit. */
  format: string | null;
  /** Spec Kit extensions. */
  extensions: string[];
  /** Leave out the speccoder Spec Kit preset. */
  no_preset: boolean;
}

/** The `new_project` payload. */
export interface NewProjectReport {
  /** The plan. */
  plan: ProjectPlan;
  /** Each step's outcome (empty on a dry run). */
  results: PlanStepResult[];
  /** `speccoder init`'s report, when it ran. */
  init: InitReport | null;
  /** speccoder's commit, once made. */
  commit: string | null;
  /** What Undo would do. */
  undo: UndoOffer;
  /** Nothing ran. */
  dry_run: boolean;
  /** The project's first feature, prefilled from the idea. */
  first_feature: string | null;
}

/** One step a person can take. */
export type NextStep = {
  /** A copy-pasteable command (`"…"` marks text the person writes). */
  command: string;
  /** Why, in one short line. */
  why: string;
} & (
  /** speccoder performs it (`actions::perform`). */
  | {
    how: "perform";
    /** What runs. */
    run: Runnable;
    /** What to ask for first (`Why it is skipped`); `None`: nothing. */
    input: string | null;
    /** The words the line gives already (a `--reason "…"` filled in). */
    text: string | null;
    /**
     * What to say in a confirmation first (it leaves the machine or
     * cannot be undone).
     */
    confirm: string | null;
    /** The feature the line names (`--feature F`); `None`: the current. */
    feature: string | null;
  }
  /** A view to open. */
  | {
    how: "view";
    /** Which. */
    view: View;
    /** What it is about: a code, a task. */
    subject: string | null;
  }
  /**
   * A command speccoder printed as a fix and can run for the person, in
   * the project folder and the OS sandbox (see `outside`) — never one
   * composed elsewhere. Several argvs run in order, each only after the last
   * one succeeded (`a && b`).
   */
  | {
    how: "command";
    /** The argvs, exactly. */
    argvs: string[][];
    /**
     * It reads the keyboard (a login): a terminal of the person's
     * own, not a panel.
     */
    interactive: boolean;
    /**
     * What it changes outside the project, for a command that runs
     * outside the sandbox by its nature (`changes your global git
     * config`); `None`: it runs in the sandbox, writes confined to
     * the project and the package caches.
     */
    outside?: string | null;
    /**
     * The host's proof that it issued exactly these argvs; only a
     * host that runs commands sets it.
     */
    token?: string | null;
  }
  /**
   * It must run in the shell speccoder starts from (`export NAME=…`):
   * nothing run elsewhere reaches that shell.
   */
  | {
    how: "shell";
    /** Why it cannot run here. */
    why: string;
    /** The setting that stands for it (`model`), if there is one. */
    setting: string | null;
  }
  /** Words for a person (`→ …`), or a command only they can complete. */
  | {
    how: "words";
    /** Why it is not taken here; `None` for words. */
    why: string | null;
  });

/** One task object's verification. */
export interface ObjectVerification {
  /** The commit (or skip record) id. */
  object: string;
  /** `task` or `skip`. */
  kind: string;
  /** Task ID. */
  task: string;
  /** The verdict the statements claim. */
  verdict: string | null;
  /** The outcome. */
  outcome: VerifyOutcome;
  /** Every reason found. */
  reasons: VerifyReason[];
  /** Who signed, when signed. */
  signer: SignerView | null;
  /** How many statements the bundle holds. */
  statements: number;
  /** For a missing attestation: an attested commit with the same tree. */
  same_tree_as: string | null;
}

/** The object a person's decision covers, as reports show it. */
export interface ObjectView {
  /** `write`, `command`, `network` or `mcp`. */
  class: string;
  /** The canonical path, `argv#<12 hex>` or `<tool>@<12 hex>`. */
  short: string;
  /** One line: ``run `make check` ``. */
  describe: string;
  /** The object in full, as every interface shows it. */
  lines: string[];
}

/** The folder and the tools as the plan saw them. */
export interface ObservedFolder {
  /** The folder exists. */
  exists: boolean;
  /** Its entries' names, sorted. */
  entries: string[];
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
  /** Its grant id: what `--grant` and the sheet's tick name. */
  id: string;
  /** `write`, `command`, `network` or `mcp`. */
  class: string;
  /** The object in full (`run: cargo run --example demo · argv #3f…`). */
  object: string;
}

/** A question a document leaves open, to answer in place. */
export interface OpenQuestion {
  /** `Q1`, `Q2`, … in reading order. */
  id: string;
  /** The question. */
  text: string;
  /** The document (project-relative). */
  file: string;
  /** Its line (1-based). */
  line: number;
  /** The requirement or section it is in. */
  near: string | null;
}

/** What an OpenSpec project adds to its snapshot. */
export interface OpenSpecMeta {
  /** The workflow schema (`spec-driven` is the one speccoder runs). */
  schema: string;
  /** `context` of `openspec/config.yaml`, if any. */
  context: string | null;
  /** What a unit of work is called. */
  unit_kind: string;
  /** The selected change's delta requirements, in file order. */
  requirements: DeltaRequirement[];
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
  /** Its path. */
  path: string;
  /** The person's reason (told to every later author call). */
  reason: string;
  /** Who. */
  by: string;
  /** Unix seconds. */
  at: number;
}

/** Where a requirement's oracle is. */
export type OracleState = "none" | "written" | "not_red" | "failed" | "ambiguous" | "frozen" | "lapsed" | "frozen_elsewhere";

/** One visible test as the review shows it. */
export interface OracleTest {
  /** Project-relative path. */
  path: string;
  /** Its content (a visible test: the implementer reads it too). */
  content: string;
  /** The scenarios it cites (never empty). */
  scenarios: string[];
  /**
   * Its red class on the current tree (`assertion`, `absent_symbol`,
   * `passed`, `build_error`, `flaky`, …).
   */
  red_class: string;
  /** Red: it can be frozen. */
  red: boolean;
  /** Who wrote it. */
  authored_by: AuthoredBy;
  /** The person who edited it. */
  edited_by: string | null;
}

/** One requirement's oracle in the review: everything the Tests card shows. */
export interface OracleView {
  /** The requirement (`US1`). */
  req: string;
  /** Its title. */
  title: string;
  /** Its scenarios: id and words. */
  scenarios: ScenarioRef[];
  /** Where it is. */
  state: OracleState;
  /** For `lapsed`: the wording it was frozen on. */
  frozen_on: string | null;
  /** Who froze it, when frozen. */
  frozen_by: string | null;
  /** When (Unix seconds). */
  frozen_at: number | null;
  /** The kept tests. */
  tests: OracleTest[];
  /** The tests not kept, and why. */
  discarded: OracleDiscarded[];
  /** The tests the person rejected. */
  rejected: OracleRejected[];
  /** Scenarios no red kept test covers. */
  uncovered: string[];
  /** Scenarios an author found untestable, with why. */
  untestable: Untestable[];
  /** Readings that disagree. */
  ambiguities: Ambiguity[];
  /** The held-out set: count and digest only. */
  held_out: HeldOutSummary;
  /** `[acceptance] min_strength` (0–100), `None` when none is set. */
  strength_target: number | null;
  /** The author calls *Write Tests* makes (`[review] oracle_samples`). */
  calls: number;
  /** With one call there is nothing to compare. */
  ambiguity_check: boolean;
  /** The visible set's manifest digest, when tests are written. */
  manifest_sha256: string | null;
  /** Why writing failed, or why it cannot be written here. */
  error: string | null;
}

/** Where a stretch of text came from. */
export type Origin = "yours" | "added" | "dropped";

/** What a candidate run showed. */
export type Outcome = "rejected" | "survived" | "timeout" | "unviable";

/** A parsed `tasks.md`. */
export interface ParsedTasks {
  /** Workspace-relative path of the file. */
  path: string;
  /** Level-2 headings, in document order. */
  phases: Phase[];
  /** Tasks, in document order. */
  tasks: Task[];
  /** Counts over `tasks`. */
  totals: Totals;
  /** How dependencies within a phase are determined. */
  dependency_mode: DependencyMode;
}

/** The tests that passed before the fix and must pass after it. */
export interface PassToPass {
  /** Selected, or nothing was. */
  state: PassToPassState;
  /** The members. */
  members: PassToPassMember[];
}

/** One test that must stay green. */
export interface PassToPassMember {
  /** The test id, as the runner reports it. */
  test: string;
  /** Its file. */
  file: string;
  /**
   * How it was selected: `same_file`, `references:<module>`,
   * `imports:<path>`, `package:<pkg>`, `unchanged:U<n>`.
   */
  rule: string;
  /** On the final tree. */
  final: MemberFinal;
}

/** Whether anything was selected for pass-to-pass. */
export type PassToPassState = "selected" | "none_selected";

/**
 * A call that stopped an attempt for approval, or a question, waiting for
 * a person.
 */
export interface PendingCall {
  /** Task ID. */
  task: string;
  /** What a decision would cover, e.g. ``write `Cargo.toml` ``. */
  covers: string;
  /** The tool. */
  tool: string;
  /** Its arguments as the model sent them. */
  arguments: unknown;
  /** Why it needs approval. */
  reason: string;
  /** The saved attempt. */
  reference: string;
  /** The call as the inline prompt shows it. */
  presentation: ApprovalView | null;
  /** The waiting call's id (`approve --call`); empty for a stopped attempt. */
  call: string;
  /**
   * `task` (the attempt waits and resumes) or `attempt` (stopped and saved
   * in a parallel lane).
   */
  source: string;
  /** `approval` or `question`. */
  kind: string;
  /** A question's text. */
  question: string | null;
  /** Seconds since the epoch the call has waited since. */
  waiting_since: number | null;
  /** Seconds since the epoch it is denied as expired at. */
  deadline: number | null;
  /** The object a decision covers. */
  object: ObjectView | null;
  /** Where arguments of the call were copied from (source lines). */
  sources?: string[];
  /** The run it belongs to. */
  run: string | null;
  /** It waits in its run's batch: the asking budget was used. */
  batched: boolean;
  /**
   * SHA-256 over its id and the object a decision covers: a decision
   * names it and is refused when another call (or another object) waits
   * now.
   */
  digest: string;
}

/** What speccoder decided of an external agent's calls. */
export interface PermissionTally {
  /** Calls decided (its permission requests, files and commands). */
  asked: number;
  /** Of them, allowed (by a rule or a person's grant). */
  allowed: number;
  /** Of them, decided by a person (or waiting for one). */
  waited: number;
  /** Of them, refused. */
  refused: number;
  /** Calls the agent reported making, asked about or not (its claim). */
  reported: number;
}

/** An alternative the author says the spec allows. */
export interface Permitted {
  /** `permitted-1`, … */
  id: string;
  /** The scenario whose open choice it takes differently. */
  scenario: string;
  /** The freedom it uses (the author's claim). */
  rationale: string;
  /** `sha256:<hex>` of its files: what a ruling is bound to. */
  digest: string;
  /** Against the visible and held-out set together. */
  outcome: PermittedOutcome;
  /** The failing run's last lines, when rejected. */
  failing?: string | null;
  /** Where its patch is kept for a person. */
  patch?: string | null;
}

/** What a spec-permitted alternative showed. */
export type PermittedOutcome = "accepted" | "rejected" | "ruled" | "timeout" | "unviable";

/** A level-2 heading of `tasks.md`. */
export interface Phase {
  /** Heading text, e.g. `Phase 1: Setup`. */
  title: string;
  /** 1-based line of the heading. */
  line: number;
  /** SHA-256 (hex) of the section, heading through the byte before the next phase. */
  section_hash: string;
}

/** Where a requirement is written. */
export interface Place {
  /** The document, project-relative. */
  path: string;
  /** The line, 1-based. */
  line: number;
  /** That line, verbatim. */
  quote: string;
}

/** A plan check of the new task list (`PLAN-*`): a warning, never a stop. */
export interface PlanCheck {
  /** The check's code. */
  code: string;
  /** The tasks it names. */
  tasks: string[];
  /** What it found. */
  detail: string;
}

/** What plan through starts from. */
export type PlanFrom =
  /** A new unit from a description. */
  | {
    kind: "create";
    /** What it should do, verbatim. */
    description: string;
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
  /** What is wrong. */
  message: string;
  /** What to do. */
  fix: string;
}

/** One command of the plan. */
export interface PlanStep {
  /** `git_init`, `scaffold.<n>`, `format_init`, `speccoder_init`, `commit`. */
  id: string;
  /** The command, exactly as it runs. */
  argv: string[];
  /** Where it runs: always the target. */
  cwd: string;
  /** Local, or network. */
  access: StepAccess;
  /**
   * The hosts a network step is expected to reach: shown, not enforced —
   * the step gets the full network.
   */
  hosts?: string[];
  /** What it writes, in one line. */
  writes: string;
  /** Its time limit, in seconds. */
  limit_s: number;
  /** A note the plan shows beside it. */
  note?: string | null;
}

/** One step's outcome. */
export interface PlanStepResult {
  /** The step. */
  id: string;
  /** What happened. */
  state: PlanStepState;
  /** Its exit status, when it ran. */
  exit: number | null;
  /** How long it took. */
  duration_ms: number;
  /** The last lines of its output (at most 20). */
  output_tail: string[];
}

/** A step's state in a run. */
export type PlanStepState = "waiting" | "running" | "done" | "skipped" | "failed";

/** A task of the plan. */
export interface PlannedTask {
  /** Task ID. */
  id: string;
  /** Title. */
  title: string;
  /** `done`, `ready`, `blocked` or `skipped`. */
  state: string;
}

/**
 * Result of `speccoder spec <skill> --through <t>`: the skills it ran,
 * where it stopped, and the unit's readiness. Plan through keeps no
 * state of its own: the stop is read again from the unit's files.
 */
export interface PlanningChainReport {
  /** The unit (once the creating skill made it). */
  unit: string | null;
  /** How far it was asked to go. */
  through: Through;
  /** The skills it ran, in order. */
  ran: ChainStep[];
  /** Where it stopped. */
  stop: ChainStop;
  /** The unit's readiness now. */
  readiness: Readiness | null;
  /** Requirements of other units close to the description (a hint). */
  similar: SimilarMatch[];
  /** Where the description came from. */
  source: FeatureSource;
  /** The skill a continuation runs next. */
  next_skill: string | null;
}

/**
 * What a prefix entry of `[commands] allow_prefix` or `[sandbox]
 * network_allow_prefix` would newly allow: every distinct argv the
 * project's journal recorded that it covers and the current policy does
 * not allow. A prefix is saved only as the confirmation of this preview,
 * and only when it is complete.
 */
export interface PolicyCoverage {
  /** The entry previewed. */
  entry: CoverageEntry;
  /** Every run in the journal was read. */
  complete: boolean;
  /** What could not be read, and why (never "covers nothing"). */
  unreadable: string[];
  /** The argvs it would newly allow. */
  rows: CoverageRow[];
  /** SHA-256 over the entry, the rows and `complete`: what a save names. */
  digest: string;
}

/** A prefix list in `speccoder.toml`. */
export type PolicyList = "commands_prefix" | "network_prefix";

/** Result of `speccoder pr`: a draft pull request with the evidence. */
export interface PrReport {
  /** Feature directory name. */
  feature: string;
  /** `speccoder/<feature>`. */
  branch: string;
  /** The branch the pull request targets. */
  base: string;
  /** The git remote pushed to. */
  remote: string;
  /** Pull request title. */
  title: string;
  /** Pull request body (Markdown): every task's evidence. */
  body: string;
  /** What runs (or would run), in order. */
  commands: string[];
  /** Whether the branch was pushed and the draft opened (`false`: a dry run). */
  opened: boolean;
  /** The pull request, when opened. */
  url: string | null;
  /**
   * The unit's held-out ref pushed with the branch
   * (`refs/speccoder/held-out/<unit>`), when a closed task names a set.
   */
  held_out_ref: string | null;
  /** The tasks closed on the pushed head whose held-out sets it carries. */
  held_out_tasks: string[];
  /** Held-out sets of open tasks: they stay local. */
  held_out_withheld: number;
  /**
   * `present`: a workflow under `.github/workflows/` runs
   * `speccoder attest verify --branch`; `absent`: none does.
   */
  evidence_check: string;
}

/** Where the confirmed reproductions are kept, as recorded with the fix. */
export interface PreFrozenRef {
  /** `sha256:<hex>` over the visible set. */
  manifest: string;
  /** The confirmation's digest. */
  approval: string;
  /** The triad's digest. */
  wording: string;
}

/** Result of `speccoder preset add`. */
export interface PresetReport {
  /** The preset's id (`speccoder`). */
  preset: string;
  /** The bundled version. */
  version: string;
  /** The Spec Kit command, as run (or to be confirmed). */
  command: string[];
  /** Spec Kit's own output (its last lines). */
  output: string;
  /** The preset as Spec Kit reports it. */
  state: PresetState;
}

/** The speccoder preset in a project, as Spec Kit reports it. */
export interface PresetState {
  /** The installed version, when installed. */
  installed: string | null;
  /** `specify preset resolve tasks-template` names it. */
  active_for_tasks_template: boolean;
  /** The version this speccoder bundles. */
  bundled: string;
}

/** Where the project is made. */
export type ProjectMode = "new_folder" | "existing_repository";

/** The plan. */
export interface ProjectPlan {
  /** The folder, canonical and absolute. */
  target: string;
  /** The project's name (the folder's). */
  name: string;
  /** A new folder or an existing repository. */
  mode: ProjectMode;
  /** The stack. */
  stack: StackPick;
  /** Every stack and whether its tool is installed. */
  stacks: StackOption[];
  /** The spec format (`speckit`, `openspec`). */
  format: string;
  /** Every format and whether its tool is installed. */
  formats: FormatOption[];
  /** Spec Kit extensions. */
  extensions: ExtensionChoice[];
  /** Whether the speccoder Spec Kit preset is passed to the format's init. */
  preset: boolean;
  /** The commands, in order. */
  steps: PlanStep[];
  /** The OS sandbox. */
  sandbox: SandboxNote;
  /** The person's git identity. */
  identity: GitIdentity | null;
  /** Why it cannot run (Create disabled). */
  refusal: PlanRefusal | null;
  /** The folder as seen. */
  observed: ObservedFolder;
  /** SHA-256 over all of the above. */
  digest: string;
}

/** Everything discovered about one Spec Kit project. */
export interface ProjectSnapshot {
  /** Absolute, canonical project root (the directory containing `.specify/`). */
  root: string;
  /** Spec Kit metadata recorded in the project. */
  speckit: SpeckitMeta;
  /** State of `.specify/memory/constitution.md`. */
  constitution: ConstitutionState;
  /** Feature directories, ordered by numeric prefix then name. */
  features: FeatureRef[];
  /** Which feature was selected, and why. */
  selection: Selection;
  /** The selected feature, if any. */
  feature: FeatureSnapshot | null;
  /** Problems found while discovering and parsing. */
  diagnostics: Diagnostic[];
  /** The spec format (omitted for Spec Kit, the reference format). */
  format?: FormatKind;
  /**
   * What an OpenSpec project adds: its schema, context and the selected
   * change's requirements.
   */
  openspec?: OpenSpecMeta | null;
}

/** Result of `speccoder propose`. */
export interface ProposeReport {
  /** The agentplane run that holds the call. */
  run_id: string;
  /** Feature directory name. */
  feature: string;
  /** Task ID. */
  task: string;
  /** agentplane run status, e.g. `succeeded`. */
  run_status: string;
  /** The structured proposal (`summary`, `steps`, `files`, `checks`, `risks`). */
  proposal: unknown;
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
  /** The digest confirmed. */
  digest: string;
  /** Who. */
  by: string;
  /** Through what. */
  via: string;
  /** `shown` or `named_test`. */
  basis: string;
}

/** `bug.outcome`, as published. */
export interface PublicBugOutcome {
  /** `awaiting_confirmation`, `cannot_reproduce`, `fixed`, … */
  outcome: string;
  /** The fix's verdict. */
  verdict: string | null;
  /** The stop's code. */
  code: string | null;
  /** What to do next. */
  next: string[];
}

/** `bug.recorded`, as published: ids and digests only. */
export interface PublicBugRecorded {
  /** The home's kind. */
  home: string;
  /** The unit. */
  unit: string;
  /** The requirement. */
  requirement: string;
  /** The triad's digest. */
  triad_digest: string;
  /** Through what. */
  via: string;
}

/** `collection.checked`, as published: paths and states. */
export interface PublicCollectionChecked {
  /** The task. */
  task: string;
  /** Each judged test file. */
  files: PublicCollectionFile[];
}

/** One file of [`PublicCollectionChecked`]. */
export interface PublicCollectionFile {
  /** Workspace-relative path. */
  path: string;
  /** `collected`, `not_collected`, `unknown`. */
  state: FileState;
  /** `static`, `junit`, `listing`; `null` when unknown. */
  decided_by: DecidedBy | null;
}

/** `oracle.frozen`, as published: digests and counts, never a test. */
export interface PublicOracleFrozen {
  /** The unit. */
  unit: string;
  /** The requirement. */
  requirement: string;
  /** The visible set's manifest digest. */
  manifest_sha256: string;
  /** The approval's digest. */
  approval_sha256: string;
  /** Visible tests frozen. */
  visible_count: number;
  /** Held-out tests sealed with them. */
  held_out_count: number;
  /** Who. */
  by: string;
  /** Through what. */
  via: string;
}

/** `oracle.lapsed`, as published. */
export interface PublicOracleLapsed {
  /** The unit. */
  unit: string;
  /** The requirement. */
  requirement: string;
  /** The wording it was frozen on. */
  frozen_wording_sha256: string;
  /** The wording now. */
  current_wording_sha256: string;
}

/** `project.created`, as published: no path, no idea text. */
export interface PublicProjectCreated {
  /** SHA-256 of the folder's canonical path. */
  path_digest: string;
  /** `new_folder` or `existing_repository`. */
  mode: string;
  /** The stack. */
  stack: string;
  /** The spec format. */
  format: string;
  /** speccoder's commit. */
  commit: string;
  /** Through what. */
  via: string;
}

/** `project.undone`, as published. */
export interface PublicProjectUndone {
  /** SHA-256 of the folder's canonical path. */
  path_digest: string;
  /** `trash` or `revert`. */
  kind: string;
  /** Through what. */
  via: string;
}

/**
 * What the future oracle of a unit will be made of, counted from its
 * files as they are now: requirements, scenarios, open questions, and
 * requirements no scenario covers (they cannot be tested).
 */
export interface Readiness {
  /** Requirements the spec defines. */
  requirements: number;
  /** Scenarios over them. */
  scenarios: number;
  /** Open-question markers left in the unit's documents. */
  open_questions: number;
  /** Requirements without a scenario, by id: they cannot be tested. */
  untestable: string[];
  /** The format defines requirement identifiers (else only questions count). */
  identifiers: boolean;
}

/** One of the two readings of an ambiguous scenario. */
export type Reading = "A" | "B";

/** One reading of a scenario: what one author expects. */
export interface ReadingText {
  /** The situation. */
  given: string;
  /** The outcome (`untestable: …` when the author found none). */
  expect: string;
}

/** How many regression-only ticks had one reason, and which. */
export interface ReasonCount {
  /** The reason (`no_story`, `already_passing`, …, `unrecorded`). */
  reason: string;
  /** How many ticks. */
  n: number;
  /** Their tasks. */
  tasks: string[];
  /** `already_passing`: the tasks that made the tests pass. */
  implemented_by?: string[];
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
    reason: "author_failed";
    /** What speccoder refused (its own words). */
    detail: string;
  }
  /** The acceptance tests passed before the attempt. */
  | {
    reason: "already_passing";
    /** The tick that made them pass; `null` when it could not be determined. */
    implemented_by: ImplementedBy | null;
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
  /** The digest rejected. */
  digest: string;
  /** Why. */
  reason: string;
  /** Who. */
  by: string;
  /** Through what. */
  via: Via;
  /** Unix seconds. */
  at: number;
}

/**
 * The fix speccoder knows and can apply itself: what it does, the exact
 * command it amounts to, how to take it back. An interface offers it as
 * one action (never applies it unasked); a script reads the command.
 */
export interface Remedy {
  /** The action, a verb first (`Commit Cargo.lock`). */
  label: string;
  /** What it changes, in one line. */
  effect: string;
  /** The same, as a command run in the checkout. */
  command: string;
  /** How to take it back once applied. */
  undo: string | null;
  /** It writes git history (a commit): an interface asks first. */
  history: boolean;
  /** How an interface applies it. */
  run: Runnable;
}

/** Result of `speccoder replay`. */
export interface ReplayReport {
  /** The replayed run. */
  run_id: string;
  /** `reproduced`, `diverged` or `not_found`. */
  result: string;
  /** What the replay said. */
  detail: string;
}

/** One try of the reproduction author. */
export interface ReproduceTry {
  /** 1, 2, … */
  n: number;
  /** Per statement. */
  per_statement: TryStatement[];
  /** Why the whole try was discarded (`wrote outside tests`, …). */
  discarded?: string | null;
  /** The author's run. */
  run_id?: string | null;
  /** Its tokens. */
  usage?: unknown;
  /** The person's reason for rejecting the reproductions before it. */
  rejection?: string | null;
}

/** One reproduction test. */
export interface Reproduction {
  /**
   * The statement it reproduces (`E1`) — or, for a test of an
   * *Unchanged* statement, that one (`U1`).
   */
  statement: string;
  /** Project-relative test file. */
  file: string;
  /** The test's name. */
  test: string;
  /** Who wrote it. */
  source: ReproductionSource;
  /** Its class on the untouched code, run twice. */
  baseline: RedClass[];
  /** `sha256:<hex>` of its content. */
  digest: string;
  /** The command that runs only it. */
  argv: string[];
  /** On the final tree. */
  final: FinalState;
  /** Against the revert candidate. */
  revert: RevertOutcome;
}

/** Who wrote a reproduction. */
export type ReproductionSource = "author" | "person";

/** One cited requirement, as the tick saw it and as the spec has it now. */
export interface RequirementDrift {
  /** `FR-003`. */
  id: string;
  /** Its line when the task was ticked; `None`: it did not exist yet. */
  then: string | null;
  /** Its line now; `None`: it was removed. */
  now: string | null;
}

/** The person's choice on an ambiguous scenario. */
export interface Resolution {
  /** The reading meant. */
  reading: Reading;
  /** Who chose. */
  by: string;
  /** Unix seconds. */
  at: number;
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
  /**
   * SHA-256 over the approved decision lines and the frozen oracle
   * entries: what a tick that reused a frozen set names.
   */
  sha256: string;
  /** The run's mode. */
  mode: RunMode;
  /** Who approved. */
  by: string;
  /** Through what. */
  via: Via;
  /** Unix seconds. */
  at: number;
}

/**
 * A requirement changed in the checkout since the last commit of the
 * spec, with where its line is — a comment anchors to it (`diff_line`).
 */
export interface ReviewChange {
  /** `FR-003`. */
  id: string;
  /** Its line at the last commit; `None`: it did not exist yet. */
  then: string | null;
  /** Its line now; `None`: it was removed. */
  now: string | null;
  /** The document it is in (project-relative). */
  path: string;
  /** Its line number (1-based) at the last commit. */
  then_line: number | null;
  /** Its line number (1-based) now. */
  now_line: number | null;
}

/** One requirement and its review. */
export interface ReviewItem {
  /** `FR-001`. */
  id: string;
  /** Its wording now (the line, markup aside). */
  text: string;
  /**
   * `open` (not reviewed), `approved`, `changes_requested`, or `lapsed`
   * (decided on a wording that has changed since).
   */
  state: string;
  /**
   * Who decided (as for every decision: git's `user.name`, else the OS
   * user), when someone did.
   */
  by: string | null;
  /** When (Unix seconds). */
  at: number | null;
  /** The comment, for a change request (or a note on an approval). */
  comment: string | null;
  /** For `lapsed`: the wording that was decided on. */
  decided_on: string | null;
}

/**
 * Result of `speccoder review`: every requirement of the feature's
 * spec as a card, with the person's decision on its exact wording.
 */
export interface ReviewReport {
  /** The feature. */
  feature: string;
  /** Where the review is recorded (project-relative); commit it to share. */
  file: string;
  /** One per requirement, in the spec's order. */
  requirements: ReviewItem[];
  /** Counts by state. */
  totals: ReviewTotals;
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
   * The questions the documents leave open (`[NEEDS CLARIFICATION: …]`),
   * in reading order.
   */
  questions: OpenQuestion[];
  /**
   * Requirements changed in the checkout since the last commit of the
   * spec: what a commit would record.
   */
  changes: ReviewChange[];
  /** The spec's versions in git, newest first (the last twelve). */
  history: SpecVersion[];
  /**
   * One per requirement with scenarios (`US1`), beside the decision
   * lines: the tests the run would use, as written at review.
   */
  oracle: OracleView[];
  /**
   * Why the review writes no tests for this unit: its own pipeline
   * authors and confirms them (a quick change, a bug).
   */
  tests_notice: string | null;
  /** A bug unit's confirmed reproductions, read only. */
  bug_tests?: BugTests[];
  /** The approval that froze the oracle, when one is recorded. */
  approval: ReviewApproval | null;
  /**
   * What *Approve and Run* would do and what blocks it (the full review
   * only).
   */
  sheet: ApproveSheet | null;
  /**
   * Why the review record (`review.json`) cannot be read: shown, never
   * read as "nothing decided" by a write.
   */
  error?: string | null;
  /** What the future oracle will be made of, counted now. */
  readiness: Readiness | null;
}

/** Counts of [`ReviewItem::state`]. */
export interface ReviewTotals {
  /** Approved on their current wording. */
  approved: number;
  /** Changes requested. */
  changes_requested: number;
  /** Never reviewed. */
  open: number;
  /** Decided on an earlier wording. */
  lapsed: number;
  /** Questions the documents leave open. */
  questions: number;
  /**
   * Requirements two test authors read differently, not yet resolved:
   * *Approve and Run* and a gated run wait on them.
   */
  ambiguous: number;
}

/** What a revision turn did. */
export interface RevisionView {
  /** `t1`, `t2`, … */
  id: string;
  /** The comments it was sent with. */
  comments: string[];
  /** The sections they touch. */
  sections: string[];
  /** The format's revising skill. */
  skill: string;
  /** Sections it changed. */
  changed: string[];
  /** Of those, the ones no comment touched. */
  outside: string[];
  /** `pending` (the person keeps or undoes it), `kept`, `undone`. */
  outcome: string;
  /** Files it changed (project-relative). */
  files: string[];
  /** Requirements whose tests were asked again from test comments. */
  asked_again: string[];
  /**
   * Documents changed while it ran by something other than the skill
   * (the person's own edit): *Undo Revision* never overwrites them.
   */
  beside: string[];
  /**
   * While it waits to be kept or undone: each changed section, before
   * and after, for the word diff.
   */
  diffs: SectionDiff[];
}

/** One reason finished work needs a closer look, with how many ticks. */
export interface RiskReason {
  /**
   * `regression_only`, `attested`, `strength_below_target`, `stale`,
   * `scope_outside` or `external_executor`, the riskiest first
   * (`speccoder_core::inbox::Risk`).
   */
  reason: string;
  /** Ticks with it. */
  count: number;
  /** The chip (`2 regression-only`). */
  text: string;
}

/**
 * A person's ruling that an over-strict finding was wrong: the tests were
 * right, the alternative is not what the spec allows.
 */
export interface Ruling {
  /** The alternative's digest. */
  digest: string;
  /** Its scenario. */
  scenario: string;
  /** Who ruled. */
  by: string;
  /** Why. */
  reason: string;
}

/** How a run started from the review asks for a person. */
export type RunMode = "ask" | "autopilot";

/** Result of `speccoder run`. */
export interface RunReport {
  /** The agentplane run of the attempt. */
  run_id: string;
  /** Feature directory name. */
  feature: string;
  /** Task ID. */
  task: string;
  /** The worktree the attempt worked in. */
  worktree: string;
  /** `completed`, `blocked`, `needs_approval`, `budget_exhausted` or `failed`. */
  attempt_status: string;
  /** The model's own closing report — its claim, not the verdict. */
  model_report: unknown;
  /** Why the attempt stopped, when it did not complete. */
  reason: string | null;
  /** The call awaiting approval, if any. */
  pending: unknown;
  /** Every tool call with the policy's decision. */
  tool_calls: unknown[];
  /** Paths changed between the checkpoints: `added` / `modified` / `deleted`. */
  changes: ([string, string])[];
  /** Checks after the attempt, compared with the baseline. */
  verification: VerifyReport | null;
  /** The verifier's verdict (only for completed attempts). */
  verdict: Verdict | null;
  /** Token usage summed over the attempt's turns. */
  usage: unknown;
  /** Integrity findings of the attempt's diff (each also fails the verdict). */
  integrity: string[];
  /** The engine ticked the task in the worktree's `tasks.md`. */
  ticked: boolean;
  /** The commit on `speccoder/<feature>` holding the attempt and the tick. */
  commit: string | null;
  /** Spec-derived acceptance tests: files, command, red/green, notes. */
  acceptance: unknown;
  /** Earlier attempts at this task in this run, discarded and retried. */
  earlier: EarlierAttempt[];
  /** Why no further attempt was made after a failed one, if it failed. */
  retry_stop: string | null;
  /**
   * The ref this attempt was saved under when it did not pass; the
   * worktree went back to where it started.
   */
  saved_attempt: string | null;
  /** The ref uncommitted leftovers were saved under before this run. */
  set_aside: string | null;
  /** Things a person should know that are not part of the verdict. */
  notices: string[];
  /**
   * The external agent that made the attempt over ACP: its mode, its
   * plan (a claim) and what speccoder decided of what it asked.
   */
  agent?: AgentAttempt | null;
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
  /** Best-of-n: every sample's evidence and which one was kept. */
  samples: unknown[];
  /** Changes outside the paths the task names: reported, never gated. */
  scope?: ScopeFinding | null;
  /**
   * What the tick's attestation came to: `{ state: signed | unsigned |
   * missing, signer, reason }`.
   */
  attestation?: unknown;
  /**
   * The repository's instructions the attempt was given (`AGENTS.md`,
   * `CLAUDE.md`, …), root first.
   */
  instructions?: InstructionFile[];
  /** The acceptance set's strength: after a `verified` verdict. */
  strength?: AcceptanceStrength | null;
  /**
   * A bug task: the bug as this run left it (reproduce, confirm, the
   * fix's discrimination).
   */
  bug?: BugReport | null;
  /**
   * The test files the attempt added or changed, and whether the run
   * collected a test from each.
   */
  collection?: Collection | null;
  /** Why a regression-only verdict is one (set with that verdict only). */
  regression?: RegressionOnly | null;
  /** What reached the implementer, by digest (constitution VIII). */
  context?: ContextManifest | null;
}

/**
 * What decided a suggestion: the rule and the words of the text that met
 * it, verbatim.
 */
export interface RungCue {
  /** The rule. */
  rule: RungCueRule;
  /** The words, as written. */
  matched: string[];
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
  /** The suggested rung. */
  rung: RungPick;
  /** Whether this build has it (`false` for `ask` until the Ask panel). */
  available: boolean;
  /** What decided it. */
  cue: RungCue;
  /** Why, in one sentence. */
  reason: string;
  /** The other rungs, in the composer's order. */
  alternatives: RungPick[];
  /** The command that takes the suggested rung (none for `ask` and `none`). */
  command: string | null;
}

/** How an interface runs an action itself. */
export type Runnable =
  /** A planning skill (`speccoder spec <name> <arguments>`). */
  | {
    kind: "skill";
    /** The skill. */
    name: string;
    /** Its arguments (the input, when one is asked for). */
    arguments: string;
  }
  /** `speccoder run`, the feature or one task. */
  | {
    kind: "run";
    /** One task, or the feature's ready tasks. */
    task: string | null;
    /**
     * This run's agent (`--agent`): an ACP preset or `native`; `None`
     * is the configured executor.
     */
    agent?: string | null;
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
    kind: "approve";
    /** The task. */
    task: string;
    /** The call's id ([`crate::PendingCall::id`]). */
    call: string;
    /** The digest of the call that was shown ([`crate::PendingCall::digest`]). */
    digest: string;
  }
  /** Reject it (the input is the reason), bound the same way. */
  | {
    kind: "reject";
    /** The task. */
    task: string;
    /** The call's id ([`crate::PendingCall::id`]). */
    call: string;
    /** The digest of the call that was shown ([`crate::PendingCall::digest`]). */
    digest: string;
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
  /**
   * I did it: tick a step the person took on their word (`attested`;
   * the input, when given, is the reason).
   */
  | {
    kind: "done";
    /** The task. */
    task: string;
  }
  /** Hint & retry: a note for the task (the input), then a new attempt. */
  | {
    kind: "hint";
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
    kind: "review";
    /** The requirement identifiers; empty: every one not yet approved. */
    ids: string[];
    /**
     * `approve`, `request` (changes), `clear`; `answer` (ids: the
     * question) and `edit` / `edit-approve` (ids: the requirement) take
     * the input as the words.
     */
    verdict: string;
  }
  /** Tell a GitHub issue it is being specified as the current feature. */
  | {
    kind: "issue_comment";
    /** The issue's number. */
    issue: number;
    /** Also assign it to the person. */
    assign: boolean;
  }
  /** Record a quick change and, when `run`, run it. */
  | {
    kind: "quick_change";
    /** The change, in the person's words. */
    request: string;
    /** When it is done: its acceptance scenarios. */
    checks: string[];
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
    kind: "bug_fix";
    /** What happens (the symptom). */
    symptom: string;
    /** What should happen instead (one or more). */
    expected: string[];
    /** What must keep working (zero or more). */
    unchanged: string[];
    /** The GitHub issue it comes from. */
    issue: number | null;
    /**
     * The person's own reproduction tests, one per expected statement
     * (`PATH::NAME`).
     */
    tests: string[];
    /** Someone else's fix to judge: a patch file or `pr:N`. */
    candidate: string | null;
    /** Continue a bug the format's own bug step created. */
    slug: string | null;
    /** Reproduce it once recorded. */
    run: boolean;
  }
  /**
   * Confirm (or, with `reject`, reject: the input is the reason) a
   * bug's reproductions, bound to the digest shown.
   */
  | {
    kind: "bug_confirm";
    /** `<unit>/<task>`. */
    task: string;
    /** The digest the person saw. */
    digest: string;
    /** Reject instead. */
    reject: boolean;
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
    kind: "allow_always";
    /** The task. */
    task: string;
    /** The waiting call's id. */
    call: string;
  }
  /**
   * Add a prefix to a prefix list, as the confirmation of its coverage
   * preview (refused when the preview changed or is incomplete).
   */
  | {
    kind: "widen_policy";
    /** Which list. */
    key: PolicyList;
    /** The prefix (two elements or more). */
    prefix: string[];
    /** The digest of the preview the person saw. */
    digest: string;
  }
  /** Decide the calls waiting in a run's batch together, each once. */
  | {
    kind: "decide_batch";
    /** The run. */
    run: string;
    /** Approve or reject (the input is the reason). */
    verdict: BatchVerdict;
    /** The listed calls decided (a non-empty subset). */
    calls: string[];
    /** The digest of the list the person saw. */
    digest: string;
    /** Start the run's asking budget again. */
    resume_asking: boolean;
  }
  /**
   * Write, reject, ask again for, edit or settle a requirement's tests
   * at review (the input: the reason, the comment or the content).
   */
  | {
    kind: "oracle_tests";
    /** The requirement (`US1`). */
    req: string;
    /** What to do. */
    op: OracleOp;
    /** The test (reject, again, edit). */
    path: string | null;
    /** The ambiguous scenario (reading). */
    scenario: string | null;
    /** The reading meant (reading). */
    reading: Reading | null;
  }
  /**
   * Approve every approvable decision line, freeze the written tests and
   * start the feature's run, asking or on autopilot.
   */
  | {
    kind: "approve_and_run";
    /** How the run asks. */
    mode: RunMode;
    /** The grant ids (object-bound grants) an autopilot run holds. */
    grants: string[];
    /**
     * The sheet's `sha256`: what the person saw approved and frozen.
     * Required from every interface but a terminal command.
     */
    sheet?: string | null;
  }
  /** Add, delete or keep a review comment (the input: its words). */
  | {
    kind: "comment";
    /** What to do. */
    op: CommentOp;
    /** What a new comment is about. */
    anchor: CommentAnchor | null;
    /** The draft (delete, keep). */
    id: string | null;
  }
  /** Send the comments as one revision turn. */
  | {
    kind: "comments_send";
    /**
     * The drafts the person saw (empty: every draft in the tray); a
     * draft sent already refuses the send.
     */
    ids: string[];
  }
  /** Undo the last revision turn: the documents back, byte for byte. */
  | {
    kind: "revision_undo";
    /** The turn the person saw. */
    turn: string | null;
  }
  /** Code-view line comments as one nudge for the task's next attempt. */
  | {
    kind: "code_nudge";
    /** The task. */
    task: string;
    /** The comments. */
    comments: CodeComment[];
  }
  /**
   * Write a story's acceptance tests at a Setup or Foundational task
   * (`revoke`: take the decision back) — a decision recorded in the
   * state directory, never an edit of the spec.
   */
  | {
    kind: "acceptance_at";
    /** The feature. */
    feature: string;
    /** The story (`US1`). */
    story: string;
    /** The task. */
    task: string;
    /** Take the decision back. */
    revoke: boolean;
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
    kind: "plan_through";
    /** A new description, or the continuation after a stop. */
    from: PlanFrom;
    /** How far. */
    through: Through;
  }
  /**
   * Make `target` a project: the plan the person saw, bound to its digest
   * (`speccoder new`).
   */
  | {
    kind: "new_project";
    /** The folder, as the plan names it. */
    target: string;
    /** What the person chose. */
    choices: NewChoices;
    /** The digest of the plan on screen. */
    plan_digest: string;
  }
  /**
   * Undo the project speccoder created at the dispatch's root
   * (`speccoder new --undo`): to the Trash, or a revert of its commit.
   */
  | {
    kind: "undo_new_project";
    /** speccoder's commit, as recorded. */
    commit: string;
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
    kind: "open_sample";
    /** Replace an existing copy. */
    fresh: boolean;
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
  /** The model it was recorded with. */
  model: string;
  /** When (RFC 3339 date). */
  recorded_at: string;
  /** The agentplane version that recorded it. */
  agentplane: string;
}

/**
 * Result of `speccoder sample`: where the copy is and whether its
 * recorded runs replay strictly with this build.
 */
export interface SampleReport {
  /** The copy, in speccoder's state directory. */
  path: string;
  /** A fresh copy was made (else an existing one was reopened). */
  fresh: boolean;
  /** How it was recorded. */
  recorded: SampleRecording;
  /** Each recorded run, replayed. */
  runs: SampleRun[];
}

/** One recorded run of the sample, replayed. */
export interface SampleRun {
  /** The agentplane run. */
  id: string;
  /** `reproduced`, `diverged` or `not_replayable`. */
  result: string;
  /** What differed, when it did not reproduce. */
  finding: string | null;
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
  /** Ticked tasks. */
  done: number;
  /** Tasks that can start now. */
  ready: number;
  /** Tasks waiting on something. */
  blocked: number;
}

/** A task's dependencies and state. */
export interface ScheduledTask {
  /** Task ID. */
  id: string;
  /** Done, ready or blocked. */
  state: TaskState;
  /** Direct dependencies. */
  depends_on: Dependency[];
  /** Open, missing or cyclic dependencies, in document order. */
  blocked_by: string[];
}

/** One published schema. */
export interface SchemaName {
  /** `result`, `event`, `run`, … */
  name: string;
  /** What it describes. */
  describes: string;
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
  /** The task names at least one path. */
  stated: boolean;
  /** The paths the task names. */
  named: string[];
  /** Changed paths outside them. */
  outside: string[];
}

/** A sealed held-out set: kept in the state directory, never shown. */
export interface SealedRef {
  /** The requirement it is keyed by. */
  key: string;
  /** The manifest's hash. */
  manifest_sha256: string;
  /** Files in it. */
  count: number;
}

/**
 * One section a revision changed, before and after (each cut to 4000
 * characters).
 */
export interface SectionDiff {
  /** The document (project-relative). */
  path: string;
  /** Its heading path (`Spec › Requirements`; empty before the first). */
  heading: string;
  /** The section's text before the revision (empty: it was added). */
  before: string;
  /** After it (empty: it was removed). */
  after: string;
  /** No comment touched it. */
  outside: boolean;
}

/** How the feature was selected. */
export interface Selection {
  /** Why this feature (or none) was selected. */
  reason: SelectionReason;
  /** Name of the selected feature. */
  feature: string | null;
}

/** Why a feature was selected. */
export type SelectionReason = "argument" | "environment" | "recorded" | "only_feature" | "none";

/** Changes saved and set aside so a run could start from the branch tip. */
export interface SetAside {
  /** The ref holding them. */
  reference: string;
  /** The files they change, repository-relative. */
  files: string[];
}

/** Diagnostic severity. */
export type Severity = "error" | "warning" | "info";

/**
 * Result of `speccoder shape`: the assistant's proposal for a field, and
 * what code found it changed — added and dropped words, sentences that
 * landed nowhere, values introduced without an assumption, and what makes
 * the text hard to test. Nothing is written anywhere.
 */
export interface ShapeReport {
  /**
   * The field it is for: `requirement`, `feature`, `quick`,
   * `bug_symptom`, `bug_expected`, `bug_unchanged`, `answer` or `note`.
   */
  field: string;
  /** The spec format it is written for (`speckit`, `openspec`). */
  format: string;
  /** The person's words, as given (cut at 20 000 characters). */
  original: string;
  /** The person's words were longer and were cut. */
  cut: boolean;
  /** The proposal, as it would go into the field. */
  proposal: string;
  /** The person's language, when it is not the spec's. */
  language?: string | null;
  /**
   * The proposal translated back into the person's language (then the
   * diff and the coverage are against it).
   */
  back_translation?: string | null;
  /** Which words are the person's, added or dropped (by code). */
  diff: Diff;
  /** Values the assistant says it introduced. */
  assumptions: Assumption[];
  /** Added numbers and terms no assumption explains (by code). */
  unexplained: string[];
  /** Where each of the person's sentences went (by code). */
  coverage: Coverage[];
  /** Questions the assistant left open instead of guessing. */
  questions: string[];
  /** The writing check of the proposal (model-free). */
  findings: Finding[];
  /** The writing check of the person's own words. */
  findings_before: Finding[];
  /** The shaping run, when one ran. */
  run_id: string | null;
  /** Its tokens. */
  usage: unknown;
}

/** A reproduction as the stream shows it. */
export interface ShownReproduction {
  /** Its statement. */
  statement: string;
  /** Its file. */
  file: string;
  /** Its test. */
  test: string;
  /** Its content's digest. */
  digest: string;
}

/** A special case as the stream shows it. */
export interface ShownSpecialCase {
  /** The statement it targets. */
  statement: string;
  /** Rejected (or ruled). */
  rejected: boolean;
}

/** Who signed. */
export interface SignerView {
  /** `ssh`, `openpgp` or `sigstore`. */
  kind: string;
  /** The key's fingerprint (`SHA256:…`, an OpenPGP fingerprint). */
  keyid: string;
  /** The principal the verifier's trust input names for it. */
  principal?: string | null;
}

/** How close a match is. */
export type SimilarLevel = "close" | "very_close";

/** A requirement close to the description. */
export interface SimilarMatch {
  /** The unit (`003-reports`). */
  unit: string;
  /** The requirement's identifier (`FR-004`). */
  requirement: string;
  /** The requirement as written. */
  text: string;
  /** Cosine in [0, 1], rounded to two places. */
  score: number;
  /** `close` or `very_close`. */
  level: SimilarLevel;
  /** The terms it shares with the description, the weightiest first. */
  shared: string[];
  /** Where it is written. */
  at: Place | null;
}

/** The `similar` payload: what was compared and what is close. */
export interface SimilarReport {
  /** The description, as given. */
  query: string;
  /** How many requirements it was compared with. */
  compared: number;
  /**
   * Whether the corpus was limited to the [`MAX_UNITS`] most recently
   * changed units.
   */
  limited: boolean;
  /** The close ones, the closest first. */
  matches: SimilarMatch[];
}

/** One installed Agent Skill. */
export interface SkillEntry {
  /** Its name. */
  name: string;
  /** What it does. */
  description: string;
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
  /** The skill. */
  skill: string;
  /** The agentplane run. */
  run_id: string;
  /** `completed`, `incomplete`, `needs_approval`, `blocked`, `denied`, `budget_exhausted` or `failed`. */
  status: string;
  /** The model's summary. */
  summary: string | null;
  /** Questions it could not get answered. */
  questions: string[];
  /** Paths changed in the checkout: `added` / `modified` / `deleted`. */
  changes: ([string, string])[];
  /** Every tool call with the policy's decision. */
  tool_calls: unknown[];
  /** The call awaiting approval, if any. */
  pending: unknown;
  /** Why it stopped, when it did not complete. */
  reason: string | null;
  /** Token usage. */
  usage: unknown;
  /** Extension hooks run, offered or skipped around it. */
  hooks: string[];
  /**
   * The unit the skill worked on, or created and made current, and how
   * it is known (`001-x (recorded in .specify/feature.json)`).
   */
  current?: string | null;
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

/**
 * A stretch of text and its origin; the stretches of one text, in order,
 * are that text exactly.
 */
export interface Span {
  /** The text, separators included. */
  text: string;
  /** Where it came from. */
  origin: Origin;
}

/** One commit of the spec and what it changed in its requirements. */
export interface SpecVersion {
  /** The commit. */
  commit: string;
  /** `YYYY-MM-DD`. */
  date: string;
  /** Its author. */
  author: string;
  /** Its subject line. */
  subject: string;
  /** Requirements it added. */
  added: string[];
  /** Requirements it reworded. */
  reworded: string[];
  /** Requirements it removed. */
  removed: string[];
}

/**
 * One special-case candidate: a fix that recognises a reproduction's
 * literal inputs.
 */
export interface SpecialCase {
  /** The statement whose reproduction it targets. */
  statement: string;
  /** `sha256:<hex>` of its files: what a ruling is bound to. */
  digest: string;
  /** The combined set (reproductions and held-out variants) rejected it. */
  rejected: boolean;
  /** Its statement's held-out variant was written again once, told it. */
  regenerated: boolean;
  /** A person ruled the tests right about it. */
  ruled: boolean;
}

/** Spec Kit metadata from `.specify/*.json`; each field is `None` when absent. */
export interface SpeckitMeta {
  /** `speckit_version` from `init-options.json`. */
  version: string | null;
  /** The coding-agent integration, e.g. `claude`. */
  integration: string | null;
  /** Script flavour, e.g. `sh`. */
  script: string | null;
  /** `sequential` or `timestamp`. */
  feature_numbering: string | null;
  /** `feature_directory` from `feature.json`, as recorded. */
  current_feature: string | null;
}

/** A stack the plan can scaffold, or none. */
export type Stack = "rust" | "python" | "node" | "go" | "none";

/** A stack's tool on this machine. */
export interface StackOption {
  /** The stack. */
  id: Stack;
  /** How the sheet names it. */
  label: string;
  /** The tool's version when installed. */
  version: string | null;
  /** Why it cannot be chosen (its tool is missing), with the install hint. */
  disabled: string | null;
}

/** How a stack was chosen. */
export interface StackPick {
  /** The stack. */
  id: Stack;
  /** Found in an existing repository rather than chosen. */
  detected: boolean;
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
  /** `E1`, `U1`. */
  statement: string;
  /** `assertion`, `passed`, `build_error`, … */
  class: string;
}

/** Statements, by what the tries showed. */
export interface StatementsShown {
  /** Red by assertion in some try. */
  reproduced: string[];
  /** Neither red nor passing. */
  not_reproduced: string[];
  /** Passed on every try. */
  satisfied: string[];
}

/** Result of `speccoder report`: what the branch's history shows. */
export interface StatusReport {
  /** Feature directory name. */
  feature: string;
  /** `speccoder/<feature>`, if speccoder has worked on the feature. */
  branch: string | null;
  /** Where `tasks.md` was read: `branch` or `checkout`. */
  source: string;
  /**
   * How the feature was chosen (`recorded in .specify/feature.json`,
   * `the only feature`, `named with --feature`).
   */
  selected_by: string;
  /** The planning artifacts that exist (`spec`, `plan`, `tasks`). */
  artifacts: string[];
  /**
   * While the feature has no task list: the planning skills that come
   * next (`plan`, `tasks`); empty once it has one.
   */
  planning?: string[];
  /**
   * Planning files under the feature's directory not yet committed
   * (`speccoder run` reads committed files only).
   */
  uncommitted?: string[];
  /** Where the feature is: spec → plan → tasks → commit → run → review. */
  journey: JourneyStep[];
  /** Commits on the branch that the checkout's HEAD does not contain. */
  unmerged_commits: number;
  /** Every task, in document order. */
  tasks: TaskStatus[];
  /** Convergence rounds, oldest first. */
  convergence: ConvergenceRound[];
  /**
   * Requirement → task → verdict, in the spec's order; a requirement no
   * task cites is a row without a task.
   */
  trace: TraceRow[];
  /** Counts by evidence. */
  totals: StatusTotals;
  /**
   * GitHub issue URLs the feature's spec names: where it came from, and
   * what its pull request closes.
   */
  issues: string[];
  /**
   * The person's review of the spec's requirements, when the
   * format has requirement identifiers.
   */
  review?: ReviewTotals | null;
}

/** Counts of [`TaskStatus::state`]. */
export interface StatusTotals {
  /** Ticked with task-specific evidence. */
  verified: number;
  /** Ticked with regression evidence only. */
  regression_only: number;
  /** Ticked without a speccoder commit (by a person, or before speccoder). */
  ticked_without_evidence: number;
  /** In place: committed by speccoder, the local tick missing. */
  evidence_without_tick: number;
  /** Ticks speccoder committed without an attestation. */
  without_attestation: number;
  /** Ticked by `speccoder task accept`: a person's attestation, not evidence. */
  attested: number;
  /** Open and skipped by a person. */
  skipped: number;
  /** Open and ready. */
  ready: number;
  /** Open and blocked. */
  blocked: number;
  /** Ticks whose cited requirements changed since. */
  stale: number;
}

/** A line with how it is taken. */
export type Step = {
  /** The line, as printed. */
  command: string;
} & (
  /** speccoder performs it (`actions::perform`). */
  | {
    how: "perform";
    /** What runs. */
    run: Runnable;
    /** What to ask for first (`Why it is skipped`); `None`: nothing. */
    input: string | null;
    /** The words the line gives already (a `--reason "…"` filled in). */
    text: string | null;
    /**
     * What to say in a confirmation first (it leaves the machine or
     * cannot be undone).
     */
    confirm: string | null;
    /** The feature the line names (`--feature F`); `None`: the current. */
    feature: string | null;
  }
  /** A view to open. */
  | {
    how: "view";
    /** Which. */
    view: View;
    /** What it is about: a code, a task. */
    subject: string | null;
  }
  /**
   * A command speccoder printed as a fix and can run for the person, in
   * the project folder and the OS sandbox (see `outside`) — never one
   * composed elsewhere. Several argvs run in order, each only after the last
   * one succeeded (`a && b`).
   */
  | {
    how: "command";
    /** The argvs, exactly. */
    argvs: string[][];
    /**
     * It reads the keyboard (a login): a terminal of the person's
     * own, not a panel.
     */
    interactive: boolean;
    /**
     * What it changes outside the project, for a command that runs
     * outside the sandbox by its nature (`changes your global git
     * config`); `None`: it runs in the sandbox, writes confined to
     * the project and the package caches.
     */
    outside?: string | null;
    /**
     * The host's proof that it issued exactly these argvs; only a
     * host that runs commands sets it.
     */
    token?: string | null;
  }
  /**
   * It must run in the shell speccoder starts from (`export NAME=…`):
   * nothing run elsewhere reaches that shell.
   */
  | {
    how: "shell";
    /** Why it cannot run here. */
    why: string;
    /** The setting that stands for it (`model`), if there is one. */
    setting: string | null;
  }
  /** Words for a person (`→ …`), or a command only they can complete. */
  | {
    how: "words";
    /** Why it is not taken here; `None` for words. */
    why: string | null;
  });

/** Whether a step stays on the machine. */
export type StepAccess = "local" | "network";

/** A step of the tour, in its order. */
export type StepKind = "requirement" | "frozen_tests" | "red" | "diff" | "green" | "strength";

/** Where a step of the journey is. */
export type StepState = "done" | "now" | "todo" | "skipped";

/** One set's strength. */
export interface Strength {
  /** How far it got. */
  state: State;
  /** Rejected / judged over every source; `None` when nothing was judged. */
  share: number | null;
  /** Mutants; `None` when not run. */
  mutants: Counts | null;
  /** Near misses. */
  near_misses: Counts;
  /** Candidates that did not build. */
  unviable: number;
  /** What got through: mutant names and near-miss ids with their scenario. */
  survivors: string[];
}

/** One task line of `tasks.md`, as written. */
export interface Task {
  /** Task ID, e.g. `T014`. */
  id: string;
  /** `[x]` or `[X]`. */
  done: boolean;
  /** Carries the `[P]` marker. */
  parallel: boolean;
  /** Story label without brackets, e.g. `US1`. */
  story: string | null;
  /** Carries the `[YOU]` label: a step only a person can take. */
  yours?: boolean;
  /** Text after the ID and markers. */
  title: string;
  /** Task IDs named after `depends on` / `after` in the text, in order. */
  depends_on: string[];
  /** Enclosing level-2 heading. */
  phase: string | null;
  /** Enclosing level-3 heading within the phase. */
  subsection: string | null;
  /** 1-based line. */
  line: number;
  /** Byte range `[start, end)` of the list item in the file. */
  byte_range: number[];
  /** SHA-256 (hex) of the task's line, without its line ending. */
  line_hash: string;
}

/** One task's cost. */
export interface TaskCost {
  /** Task ID. */
  task: string;
  /** Attempts recorded in git (the passing commit and saved attempts). */
  attempts: number;
  /** Tokens and money. */
  usage: Usage;
}

/** Whether a task can start. */
export type TaskState = "done" | "ready" | "blocked";

/** One task and its evidence. */
export interface TaskStatus {
  /** Task ID. */
  id: string;
  /** Title after markers. */
  title: string;
  /** Enclosing phase heading. */
  phase: string;
  /**
   * `verified`, `regression_only` (ticked by speccoder with that verdict),
   * `attested` (ticked by `speccoder task accept`), `ticked_without_evidence`
   * (ticked, no speccoder commit), `evidence_without_tick` (in place: a
   * verdict commit names the task, its local tick is missing),
   * `skipped`, `ready` or `blocked`.
   */
  state: string;
  /**
   * What changed in the intent (spec, plan, constitution) since this
   * task's evidence was committed: its tick was judged against an earlier
   * version. `None`: nothing changed, or the task has no commit.
   */
  drift?: Drift | null;
  /**
   * For a task speccoder committed: its attestation as the note says,
   * signatures unchecked — `signed <kind> <keyid>`, `unsigned` or
   * `missing` (`attest verify` checks them).
   */
  attestation: string | null;
  /** Open dependencies of a blocked task. */
  blocked_by: string[];
  /** The speccoder commit that ticked it. */
  commit: string | null;
  /** The agentplane run of that attempt. */
  run_id: string | null;
  /** For an open task: its latest saved attempt. */
  last_attempt: LastAttempt | null;
  /**
   * Changes found in the worktree when the task's run started (a person's
   * edits, or an interrupted run's leftovers), saved and set aside.
   */
  set_aside?: SetAside | null;
  /** The reason a person gave for `accept` or `skip`. */
  note: string | null;
  /**
   * A verified tick's acceptance strength as its trailer says
   * (`0.83 mutants=9/11 near-miss=1/2`, `not measured (…)`).
   */
  strength?: string | null;
  /**
   * A bug task: the bug's outcome (`awaiting_confirmation`,
   * `cannot_reproduce`, `fixed`, …).
   */
  bug?: string | null;
  /**
   * A regression-only tick: why, with speccoder's wording
   * (`unrecorded` for a tick made before reasons were recorded).
   */
  regression?: RegressionOnly | null;
  /** Who does it ([`crate::lane`]); left out for `code`. */
  lane?: Lane;
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
  /** The unit, as the format adapter names it. */
  feature: string;
  /** Task ID (`T012`, `1.3`, `bug:<slug>`). */
  task: string;
  /** The task's title. */
  title: string;
  /** A tick or a trailer-only task. */
  kind: TickKind;
  /** The task line (a tick) or the home's assessment (trailer-only). */
  anchor: Anchor;
  /**
   * sha256 of the task line with its box read as open: what the
   * predicate's `task.line_hash` must equal. `None` for trailer-only.
   */
  line_sha256: string | null;
  /** For trailer-only: the claim's digest at the head (the bug's triad). */
  claim_sha256: string | null;
}

/** What a checked task is. */
export type TickKind = "tick" | "trailer_only";

/** A tick's outcome. */
export type TickOutcome = "evidenced" | "stale" | "unverifiable" | "regressed";

/** One tick's report. */
export interface TickReport {
  /** The tick. */
  tick: Tick;
  /**
   * The task commit checked (the latest reachable with the task's
   * verdict trailers), when one exists.
   */
  commit: string | null;
  /** The outcome. */
  outcome: TickOutcome;
  /** `verified`, `regression_only` or `attested`, as the evidence claims. */
  verdict: string | null;
  /** For `unverifiable`. */
  reason: Unverifiable | null;
  /** More on the reason, in one line. */
  detail: string | null;
  /** Who signed. */
  signer: SignerView | null;
  /** Who accepted an `attested` tick. */
  decider: string | null;
  /** For `stale`: the cited requirements that changed since the tick. */
  stale: RequirementDrift[];
  /** For `stale`: the first changed requirement's line at the head. */
  stale_anchor: Anchor | null;
  /** A regression-only tick: why, in speccoder's wording (signed). */
  regression?: string | null;
  /**
   * Predicate fields or revisions this verifier does not know (kept,
   * never deciding).
   */
  newer_fields: string[];
  /** `attest verify`'s object for the commit, unchanged. */
  verification: ObjectVerification | null;
}

/** Yes, no, or not known (and why). */
export type TickState = "yes" | "no" | "unknown";

/** A test toolchain speccoder knows how to ask. */
export type Toolchain = "rust" | "python" | "javascript" | "go";

/** Task counts. */
export interface Totals {
  /** All tasks. */
  total: number;
  /** Ticked tasks. */
  done: number;
  /** Unticked tasks. */
  open: number;
}

/** One step. */
export interface TourStep {
  /** Which. */
  kind: StepKind;
  /** Where it comes from. */
  source: Source;
  /** Its title (`Requirement`, `Frozen tests`, …). */
  title: string;
  /** What it shows, Markdown: a first line, then optional block lines. */
  body_markdown: string;
  /** The facts shown, as data (names, counts, digests). */
  facts: unknown;
}

/** One row of the requirement → scenario → test → verdict matrix. */
export interface TraceRow {
  /** `FR-001`, `SC-002`. */
  requirement: string;
  /** `US1.2`: story 1, acceptance scenario 2. */
  scenario: string | null;
  /** The acceptance test. */
  test: string | null;
  /** The task that cites it. */
  task: string | null;
  /** That task's verdict. */
  verdict: string | null;
  /**
   * The requirement changed since that task's tick: the verdict is for
   * its earlier wording.
   */
  changed: boolean;
}

/** The person's three statements about a bug. */
export interface Triad {
  /** What happens now (the symptom). */
  current: Statement;
  /** What should happen instead; at least one. */
  expected: Statement[];
  /** What must keep working as it does. */
  unchanged: Statement[];
  /**
   * `sha256:<hex>` over the normalised statements: a different digest
   * lapses a confirmation.
   */
  digest: string;
}

/** One statement's class in one try. */
export interface TryStatement {
  /** `E1`, `U1`. */
  statement: string;
  /**
   * What its test did on the untouched code (`assertion` only when both
   * runs failed on one).
   */
  class: RedClass;
  /** Up to 20 lines of the run, when it was not red. */
  excerpt?: string | null;
}

/**
 * Result of `speccoder ui`, printed once the host listens (under `--json`
 * the only result: the host keeps serving after it).
 */
export interface UiReport {
  /** The address to open, with this start's key in its fragment. */
  url: string;
  /** The port on 127.0.0.1. */
  port: number;
  /** The project served. */
  root: string;
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
  /** Ticks speccoder judged (`verified` and `regression_only`). */
  ticks: number;
  /** Of them, regression-only. */
  regression_only: number;
  /** The reasons, the most frequent first. */
  counts: ReasonCount[];
}

/** Token counts, and USD when prices are configured. */
export interface Usage {
  /** Input tokens (including those read from the cache). */
  input_tokens: number;
  /** Output tokens. */
  output_tokens: number;
  /** Input tokens read from the cache. */
  cache_read_tokens: number;
  /** Tokens written to the cache. */
  cache_write_tokens: number;
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
    verdict: "failed";
    /** One line per reason. */
    reasons: string[];
  };

/** What `attest verify` found for one task object. */
export type VerifyOutcome = "ok" | "unsigned" | "missing" | "bad_signature" | "untrusted_signer" | "mismatch" | "malformed" | "tool_missing";

/** Why an object got its outcome. */
export interface VerifyReason {
  /** A stable code, e.g. `subject`, `trailer`, `line_hash`, `signature`. */
  code: string;
  /** The field that disagrees. */
  field?: string | null;
  /** What it should be. */
  expected?: string | null;
  /** What it is. */
  found?: string | null;
}

/** Everything one `verify` run established. */
export interface VerifyReport {
  /** Project root the checks ran in. */
  root: string;
  /** `fast` or `full`. */
  tier: string;
  /** One result per configured check, in configuration order. */
  checks: CheckResult[];
  /** Comparison with a baseline, if one was given. */
  comparison: Comparison | null;
  /** Whether verification passed (see [`passes`]). */
  passed: boolean;
}

/**
 * The interface a person acted through, as a write records it (`via`):
 * one vocabulary for every record. Records written before it named
 * `cli`, `tui` or `dashboard` (read as [`Via::Terminal`]) and
 * `editor <name>` (read as [`Via::Acp`], the name a separate client).
 */
export type Via = "terminal" | "web" | "window" | "acp" | "phone" | "ci" | "mcp";

/** A read-only place an interface opens instead of running a command. */
export type View = "doctor" | "bug_report" | "report" | "live" | "approvals" | "explain" | "review" | "settings" | "setup" | "bug" | "quick_change" | "new_feature" | "new_project" | "github" | "task";

/** A run in progress, this host's or another speccoder process's. */
export interface WorkingRun {
  /** The feature. */
  feature: string;
  /** The task it works on, when known. */
  task: string | null;
  /** Started (Unix seconds). */
  started: number;
  /** The process. */
  pid: number;
  /** `running`, or `unknown`: its process is gone and it did not finish. */
  state: string;
}

/** Result of `speccoder worktree`. */
export interface WorktreeReport {
  /** The feature the worktree is for. */
  feature: string;
  /** Absolute worktree path. */
  path: string | null;
  /** Spec Kit project directory inside the worktree. */
  project_dir: string | null;
  /** `speccoder/<feature>`. */
  branch: string | null;
  /** Commit the branch points at. */
  base: string | null;
  /** Created now (`false`: reused). */
  created: boolean;
  /** Offending paths with reasons, when refused. */
  refused_paths: RefusedPath[];
}
