+++
title = "Codes"
description = "Every code speccoder prints with an error, a stop or a warning: what it means, why it happens and how to fix it."
weight = 135

[extra]
group = "Reference"
+++

<!-- Generated from crates/speccoder-core/src/codes.rs by `SPECCODER_BLESS=1 cargo nextest run -p speccoder --test contract`; do not edit by hand. -->

Every result that is not plainly `ok` names a code — `error[CODE]`, `stopped[CODE]` (the run waits for you) or `warning[CODE]` (it worked, with less evidence) — with a `fix:` line and a link here. In JSON the same is `code`, `fix` and `docs` on the [result envelope](@/docs/cli.md#json-output). `speccoder explain CODE` prints an entry offline.

| Code | Kind | Exit | Means |
|---|---|---|---|
| [`USAGE`](#usage) | error | 2 | the arguments are not valid for this command |
| [`USAGE-CONFLICT`](#usage-conflict) | error | 2 | two options were given that exclude each other |
| [`USAGE-UNKNOWN-CODE`](#usage-unknown-code) | error | 2 | there is no code by that name |
| [`CALL-CHANGED`](#call-changed) | error | 2 | the call waiting now is not the one that was shown |
| [`INTERNAL`](#internal) | error | 1 | speccoder failed in a way it does not expect |
| [`NO-PROJECT`](#no-project) | error | 10 | speccoder works on a Spec Kit or OpenSpec project, and found none here |
| [`SPEC-DIAGNOSTICS`](#spec-diagnostics) | error | 11 | the spec artifacts have errors speccoder will not guess around |
| [`FEATURE-SELECTION`](#feature-selection) | error | 12 | more than one feature could be meant |
| [`CHECK-BROKEN`](#check-broken) | error | 13 | a check cannot run on the untouched code |
| [`FEATURE-GONE`](#feature-gone) | error | 12 | the current feature no longer exists |
| [`REVIEW-PENDING`](#review-pending) | stop | 23 | the spec is not approved yet |
| [`REVIEW-AMBIGUOUS`](#review-ambiguous) | stop | 23 | two readings of a requirement disagree |
| [`REVIEW-NO-SCENARIOS`](#review-no-scenarios) | error | 2 | the requirement has no scenarios to write tests from |
| [`REVIEW-TESTS-OWN-PIPELINE`](#review-tests-own-pipeline) | error | 2 | this unit's tests are written by its own pipeline |
| [`ACCEPTANCE-OFF`](#acceptance-off) | error | 13 | acceptance tests are turned off |
| [`ORACLE-AUTHOR-FAILED`](#oracle-author-failed) | error | 21 | the test author could not write the tests |
| [`ORACLE-TAMPERED`](#oracle-tampered) | stop | 23 | a frozen test no longer matches what was frozen |
| [`ORACLE-FROZEN`](#oracle-frozen) | error | 2 | the requirement's tests are frozen with the approval |
| [`REVIEW-CHANGED`](#review-changed) | stop | 23 | what the approval would freeze changed since it was shown |
| [`REVIEW-UNREADABLE`](#review-unreadable) | stop | 23 | the review record cannot be read |
| [`RUN-ACTIVE`](#run-active) | error | 14 | a run is active for this project |
| [`AUTOPILOT-NEEDS-GRANT-OBJECTS`](#autopilot-needs-grant-objects) | error | 13 | autopilot needs object-bound grants |
| [`FEATURE-UNKNOWN`](#feature-unknown) | error | 12 | no feature has that name or number |
| [`CONFIG`](#config) | error | 13 | the configuration or the environment is not usable |
| [`NO-STATE-DIR`](#no-state-dir) | error | 13 | speccoder has nowhere to keep its state |
| [`SANDBOX-UNAVAILABLE`](#sandbox-unavailable) | error | 13 | the OS sandbox is not available and the project requires it |
| [`GITHUB-AUTH`](#github-auth) | error | 1 | not signed in to GitHub |
| [`GITHUB-FAILED`](#github-failed) | error | 1 | the GitHub API could not do it |
| [`CONFIG-EXISTS`](#config-exists) | error | 13 | the project has a speccoder.toml already |
| [`CONFIG-MISSING`](#config-missing) | error | 13 | the project has no speccoder.toml |
| [`CONFIG-INVALID`](#config-invalid) | error | 13 | speccoder.toml has an error |
| [`KEY-MISSING`](#key-missing) | error | 13 | the model provider's API key is not in the environment |
| [`NO-CHECKS`](#no-checks) | error | 13 | no check applies, so nothing could show that nothing regressed |
| [`GIT`](#git) | error | 14 | git is not in the state this command needs |
| [`SETUP-INCOMPLETE`](#setup-incomplete) | error | 14 | the project's setup is not committed, so every run would trip over it |
| [`NO-REPOSITORY`](#no-repository) | error | 14 | speccoder needs a git repository with at least one commit |
| [`UNCOMMITTED-INTENT`](#uncommitted-intent) | error | 14 | the spec has uncommitted changes the run would not see |
| [`NO-REMOTE`](#no-remote) | error | 14 | there is no git remote to push to |
| [`WORKTREE-DIRTY`](#worktree-dirty) | error | 14 | the feature's worktree has uncommitted changes |
| [`NOTHING-TO-HAND-OFF`](#nothing-to-hand-off) | error | 14 | the speccoder branch has nothing the base does not have |
| [`DIRTY-CHECKOUT`](#dirty-checkout) | error | 14 | an in-place run needs a clean checkout |
| [`UNBORN-HEAD`](#unborn-head) | error | 14 | the checkout has no commit yet |
| [`CONFIG-NOT-TRACKED`](#config-not-tracked) | error | 14 | speccoder.toml must be committed |
| [`BRANCH-IN-WORKTREE`](#branch-in-worktree) | error | 14 | the feature's branch is checked out in a worktree |
| [`IN-PLACE-ACTIVE`](#in-place-active) | error | 14 | an in-place run holds this checkout |
| [`IN-PLACE-SESSION`](#in-place-session) | error | 14 | an in-place session is recorded for this checkout |
| [`IN-PLACE-PARALLEL`](#in-place-parallel) | error | 2 | an in-place run works one task at a time |
| [`MERGE-CONFLICT`](#merge-conflict) | error | 14 | the checkout is in the middle of a merge or rebase |
| [`INTENT-CHANGED`](#intent-changed) | stop | 23 | the spec, plan or tasks changed during the run |
| [`JUDGE-CHANGED`](#judge-changed) | stop | 23 | the judge's inputs changed during the run |
| [`CHECKOUT-CHANGED`](#checkout-changed) | stop | 23 | the checkout moved under the run |
| [`GIT-CHANGED`](#git-changed) | stop | 23 | git's configuration, hooks or refs changed during the run |
| [`RETURN-BLOCKED`](#return-blocked) | stop | 23 | the in-place run cannot switch back to your branch |
| [`NOT-FOUND`](#not-found) | error | 15 | nothing by that name exists |
| [`NOT-FOUND-SCHEMA`](#not-found-schema) | error | 15 | there is no schema by that name |
| [`TASK-NOT-READY`](#task-not-ready) | error | 16 | no task can run now |
| [`NO-TASKS`](#no-tasks) | error | 16 | the feature has no tasks yet |
| [`TASK-DONE`](#task-done) | error | 16 | the task is already ticked |
| [`TASK-BLOCKED`](#task-blocked) | error | 16 | the task waits for tasks it depends on |
| [`VERIFICATION-FAILED`](#verification-failed) | error | 20 | the checks or the acceptance tests did not pass |
| [`REGRESSION-ONLY`](#regression-only) | warning | 0 | ticked on the regression checks alone, nothing task-specific was shown |
| [`ATTESTATION-FAILED`](#attestation-failed) | error | 26 | an attestation is missing, unsigned where required, or does not hold |
| [`TESTS-NOT-COLLECTED`](#tests-not-collected) | error | 20 | a test file the attempt wrote never ran |
| [`PLAN-TESTS-UNCOLLECTED`](#plan-tests-uncollected) | warning | 0 | the plan puts tests where the runner won't collect them |
| [`PLAN-BEHAVIOUR-EARLY`](#plan-behaviour-early) | warning | 0 | a story's behaviour is planned before the story |
| [`PLAN-TEST-ONLY-TASKS`](#plan-test-only-tasks) | warning | 0 | tasks that only write tests |
| [`PRESET-NOT-SPECKIT`](#preset-not-speckit) | error | 2 | presets are a Spec Kit mechanism |
| [`PRESET-SPECKIT-MISSING`](#preset-speckit-missing) | error | 13 | the Spec Kit tool `specify` is missing or too old |
| [`PRESET-FAILED`](#preset-failed) | error | 1 | the preset could not be added: `specify preset add` failed |
| [`NEW-PLAN-CHANGED`](#new-plan-changed) | error | 2 | the plan changed since you saw it; nothing ran |
| [`NEW-TARGET-NOT-EMPTY`](#new-target-not-empty) | error | 2 | the folder is not empty and not a git repository |
| [`NEW-DIRTY`](#new-dirty) | error | 14 | the repository has uncommitted changes |
| [`NEW-NESTED-REPOSITORY`](#new-nested-repository) | error | 14 | the folder is inside another repository |
| [`NEW-ALREADY-PROJECT`](#new-already-project) | error | 2 | the folder already holds a spec project |
| [`NEW-TOOL-MISSING`](#new-tool-missing) | error | 13 | a tool the plan needs is not installed |
| [`NEW-STEP-FAILED`](#new-step-failed) | error | 1 | a step of the plan failed; later steps did not run |
| [`NEW-NO-IDENTITY`](#new-no-identity) | error | 14 | git does not know who you are |
| [`NEW-IN-PROGRESS`](#new-in-progress) | error | 2 | a new project is being created in this folder |
| [`NEW-UNDO-REFUSED`](#new-undo-refused) | error | 14 | the project changed since speccoder created it; nothing was undone |
| [`SAMPLE-DIVERGED`](#sample-diverged) | error | 22 | this build cannot reproduce the sample's recorded run |
| [`SAMPLE-MISSING`](#sample-missing) | error | 15 | this build has no sample |
| [`SIMILAR-FEATURE`](#similar-feature) | warning | 0 | an existing requirement is close to this description |
| [`NEW-CONFIRM`](#new-confirm) | stop | 23 | the plan needs your confirmation; nothing ran |
| [`THROUGH-QUESTIONS`](#through-questions) | stop | 23 | the spec has open questions; planning waits for your answers |
| [`THROUGH-REVIEW`](#through-review) | stop | 23 | review the spec; planning waits for your approval |
| [`NEEDS-HUMAN`](#needs-human) | stop | 23 | the run stopped for a person |
| [`NEEDS-APPROVAL`](#needs-approval) | stop | 23 | a call waits for a person's decision; nothing ran |
| [`BLOCKED`](#blocked) | stop | 23 | the model says the task cannot be done as written |
| [`BUDGET-EXHAUSTED`](#budget-exhausted) | stop | 23 | the attempt ran out of turns, calls or tokens |
| [`NOT-DEMONSTRATED`](#not-demonstrated) | stop | 23 | the evidence the task needs could not be produced |
| [`TESTS-COLLECTION-UNKNOWN`](#tests-collection-unknown) | stop | 23 | speccoder cannot tell whether the runner ran a test file |
| [`ACCEPTANCE-UNWRITTEN`](#acceptance-unwritten) | stop | 23 | the acceptance tests could not be written, so nothing was attempted |
| [`SUSPECTED-GAMING`](#suspected-gaming) | stop | 23 | the gaming monitor suspects a shortcut |
| [`TESTS-OVER-STRICT`](#tests-over-strict) | stop | 23 | an acceptance test rejects an implementation the spec appears to allow |
| [`ACCEPTANCE-WEAK`](#acceptance-weak) | stop | 23 | the acceptance tests are too weak for [acceptance] min_strength |
| [`INTERRUPTED`](#interrupted) | stop | 23 | the run was interrupted |
| [`CHECKLISTS-OPEN`](#checklists-open) | stop | 23 | the feature's checklists have unchecked items |
| [`ARCHIVE-AWAITING`](#archive-awaiting) | stop | 23 | the OpenSpec archive waits for your yes |
| [`RUN-FAILED`](#run-failed) | error | 21 | the model call or the run failed |
| [`PROVIDER-REFUSED`](#provider-refused) | error | 21 | the model provider refused the request |
| [`REPLAY-DIVERGED`](#replay-diverged) | error | 22 | the replay did not reproduce the recorded run |
| [`NOT-REPLAYABLE`](#not-replayable) | error | 25 | this build cannot re-derive the recorded run |
| [`BUG-NO-UNIT`](#bug-no-unit) | error | 2 | this spec format has no place for a bug |
| [`BUG-EMPTY`](#bug-empty) | error | 2 | the symptom and every statement need words |
| [`BUG-NO-EXPECTED`](#bug-no-expected) | error | 2 | a bug needs at least one --expected statement |
| [`BUG-ASSESSMENT-DRIFT`](#bug-assessment-drift) | stop | 23 | the bug's home does not carry exactly your words |
| [`BUG-ASSESSMENT-UNREADABLE`](#bug-assessment-unreadable) | stop | 23 | the bug's assessment is not in a shape speccoder reads |
| [`BUG-TEST-NOT-FOUND`](#bug-test-not-found) | error | 2 | the test named with --test is not found |
| [`BUG-TEST-NOT-RED`](#bug-test-not-red) | error | 2 | the named test does not fail on an assertion |
| [`BUG-TEST-PASSES`](#bug-test-passes) | error | 2 | the named test passes on the untouched code |
| [`BUG-TEST-COVERAGE`](#bug-test-coverage) | error | 2 | each --expected statement needs its own --test |
| [`BUG-CANNOT-REPRODUCE`](#bug-cannot-reproduce) | stop | 23 | the bug could not be reproduced: nothing is fixed |
| [`BUG-ALREADY-SATISFIED`](#bug-already-satisfied) | warning | 0 | the reproductions already pass: nothing to fix |
| [`BUG-UNCHANGED-NOT-HOLDING`](#bug-unchanged-not-holding) | stop | 23 | an Unchanged statement does not hold today |
| [`BUG-NOT-DISCRIMINATING`](#bug-not-discriminating) | error | 20 | the reproduction does not tell the fix from no fix |
| [`BUG-REPRO-WEAK`](#bug-repro-weak) | stop | 23 | a fix that special-cases the reproduction's input passes |
| [`BUG-CANDIDATE-APPLY`](#bug-candidate-apply) | error | 14 | the candidate patch is not applied |
| [`BUG-CANDIDATE-JUDGE`](#bug-candidate-judge) | error | 2 | the candidate patch touches what judges it |
| [`EVAL-FAILED`](#eval-failed) | error | 24 | the evaluation found a false completion or a cheat |

### USAGE

The arguments are not valid for this command.

The command line could not be parsed: an unknown flag, a missing value, or a value outside the allowed set.

- `speccoder <command> --help`

### USAGE-CONFLICT

Two options were given that exclude each other.

`--json` prints one result object and `--events` prints a stream of events ending in that object; a command does one or the other.

- `speccoder <command> --events`
- `speccoder <command> --json`

### USAGE-UNKNOWN-CODE

There is no code by that name.

`speccoder explain` knows the codes speccoder itself prints; the closest names are listed with the error.

- `speccoder explain --list`

### CALL-CHANGED

The call waiting now is not the one that was shown.

An approval or rejection from an interface names the call it showed and that call's object. The call expired or was decided and another waits now, or the attempt stopped on another object; nothing was decided.

- `speccoder report --approvals`

### INTERNAL

Speccoder failed in a way it does not expect.

An unexpected failure inside speccoder: an IO error, a broken state file, a bug. The message says what failed.

- Re-run with -v and report the output at https://github.com/hupe1980/speccoder/issues

### NO-PROJECT

Speccoder works on a Spec Kit or OpenSpec project, and found none here.

No `.specify/` (Spec Kit) or `openspec/` (OpenSpec) directory was found here or above. Spec Kit makes one (`--integration claude` installs the skills speccoder runs); then `speccoder init` and `speccoder spec specify` take it from there.

- `specify init --here --integration claude`
- `openspec init`

### SPEC-DIAGNOSTICS

The spec artifacts have errors speccoder will not guess around.

The task list or spec could not be read as written: a dependency on a task that does not exist, a cycle, a malformed task line. `inspect` lists every diagnostic with its line.

- `speccoder inspect`

### FEATURE-SELECTION

More than one feature could be meant.

Several features exist and none is current: neither `--feature`, the format's variable nor its pointer (Spec Kit: `.specify/feature.json`) names one. Make one current, or name it for this command.

- `speccoder feature`
- `speccoder feature <name>`
- `speccoder <command> --feature <name>`

### CHECK-BROKEN

A check cannot run on the untouched code.

A fast check failed on the untouched worktree without running a test: a command or profile it names is missing, a file it needs was never committed, or it does not build. It could never judge the task, so nothing was attempted.

- `speccoder doctor`
- `git status`
- `speccoder run --allow-failing-baseline`

### FEATURE-GONE

The current feature no longer exists.

The format's pointer (Spec Kit: `.specify/feature.json`) or its variable names a feature directory that is not there — renamed, deleted, or from another checkout.

- `speccoder feature`
- `speccoder feature <name>`
- `speccoder feature --clear`

### REVIEW-PENDING

The spec is not approved yet.

`[review] required` is on: a run waits until every requirement of the spec is approved on its current wording, in the review committed with it (`review.json`). A requirement reworded since its approval needs approving again.

- `speccoder review`
- `speccoder review approve`
- `git commit`

### REVIEW-AMBIGUOUS

Two readings of a requirement disagree.

Two independent test authors read a scenario two ways (the same given, different expected outcomes). Until you choose a reading or reword the requirement, it is not approved, its tests are not frozen, and a gated run (`[review] required`) waits.

- `speccoder review tests <REQ> --reading A <scenario>`
- `speccoder review tests <REQ> --reading B <scenario>`
- Or reword the scenario so it reads one way

### REVIEW-NO-SCENARIOS

The requirement has no scenarios to write tests from.

Tests are written at review per requirement from its acceptance scenarios. The requirement named has none (or the format yields no requirement by that key), so there is nothing to derive a test from.

- `speccoder review`
- Add acceptance scenarios to the requirement

### REVIEW-TESTS-OWN-PIPELINE

This unit's tests are written by its own pipeline.

A quick change writes its own tests and a bug's reproductions are confirmed by you; review writes no second set for them.

- `speccoder do "…"`
- `speccoder bug confirm <unit>/<task>`

### ACCEPTANCE-OFF

Acceptance tests are turned off.

`[acceptance] generate = false`: no acceptance tests are written, at review or at run time.

- `speccoder config set acceptance.generate true`

### ORACLE-AUTHOR-FAILED

The test author could not write the tests.

The acceptance author call failed or the model provider refused it. Nothing was stored or frozen for the requirement; the message carries the provider's reason.

- `speccoder review tests <REQ> --write`

### ORACLE-TAMPERED

A frozen test no longer matches what was frozen.

A test frozen at review is stored in the state directory with its digest. The stored file or its record no longer matches: reusing it would run tests nobody approved, so the task stops and nothing is ticked.

- `speccoder review tests <REQ>`
- Write and approve the tests again, or run without the frozen set

### ORACLE-FROZEN

The requirement's tests are frozen with the approval.

*Approve and Run* froze these tests and their held-out set, so they cannot be changed. Reword the requirement to lapse the freeze and write them again.

- `speccoder review`
- Reword the requirement to lapse the freeze, then write its tests again

### REVIEW-CHANGED

What the approval would freeze changed since it was shown.

What the approval sheet showed changed before the click (another tab asked again, a *Write Tests* finished). Nothing was approved; look again and approve what you see.

- `speccoder review`
- Open the sheet again and approve what it shows

### REVIEW-UNREADABLE

The review record cannot be read.

`review.json` (decisions, frozen tests, approval) or a record beside it is not valid. Reading it as empty would drop every decision and freeze, so speccoder stops until you look.

- `git diff -- <unit>/review.json`
- Restore it from git (`git checkout -- <unit>/review.json`) or fix the JSON

### RUN-ACTIVE

A run is active for this project.

*Approve and Run* starts a feature run; one is running already (in a terminal or the web app). Approving now would start a second run beside it.

- `speccoder report`
- Wait for the run to end, or stop it

### AUTOPILOT-NEEDS-GRANT-OBJECTS

Autopilot needs object-bound grants.

Autopilot holds only grants the person saw in full and ticked, each bound to one resolved object (a canonical path, an exact argv, an MCP tool digest). This build has no such grants, so autopilot is off.

- `speccoder review approve --run ask`

### FEATURE-UNKNOWN

No feature has that name or number.

`--feature` (or `--change`) names a feature by its directory name, a part of it, or its number in `speccoder feature`'s list; none matches what was given.

- `speccoder feature`
- `speccoder report --feature <name>`

### CONFIG

The configuration or the environment is not usable.

`speccoder.toml` or the environment does not allow this command to run; the message names what is wrong.

- `speccoder init`

### NO-STATE-DIR

Speccoder has nowhere to keep its state.

Journals, worktrees and live progress live in a state directory: `$SPECCODER_STATE_DIR`, else `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`. None of these variables is set.

- `export SPECCODER_STATE_DIR="$PWD/.speccoder-state"`

### SANDBOX-UNAVAILABLE

The OS sandbox is not available and the project requires it.

No OS sandbox is available (Linux without Landlock, a container without user namespaces, another OS) while `[sandbox] mode = "required"` or an autopilot run needs one.

- Run on macOS or Linux 6.2+ (Landlock ABI 3)
- Or set [sandbox] mode = "auto" to run unsandboxed and be told
- Autopilot: choose Ask on Risky Calls, or enable the OS sandbox

### GITHUB-AUTH

Not signed in to GitHub.

No GitHub token was found (GH_TOKEN, GITHUB_TOKEN, or the one `speccoder auth login` keeps in your keychain), or GitHub refused it.

- `speccoder auth login --with-token`
- `speccoder auth status`
- Or set GH_TOKEN (a fine-grained token: Issues read; Pull requests read and write; Contents and Metadata read)

### GITHUB-FAILED

The GitHub API could not do it.

GitHub answered, and not as asked: no such issue, no access for the token, a rate limit used up, or GitHub did not answer in time. The message is GitHub's, in words.

- `speccoder auth status`
- `git remote -v`

### CONFIG-EXISTS

The project has a speccoder.toml already.

`init` writes a new one; change the existing one with `speccoder config` (or Settings), or start over with `--force`.

- `speccoder config`
- `speccoder init --force`

### CONFIG-MISSING

The project has no speccoder.toml.

speccoder needs to know which checks decide "nothing regressed" and which model to use; `init` detects them and writes a commented file.

- `speccoder init`

### CONFIG-INVALID

Speccoder.toml has an error.

An unknown section or key, a wrong type, or a value outside the allowed set. Unknown keys are errors so a typo never goes unnoticed.

- Fix the line the message names in speccoder.toml, then commit it

### KEY-MISSING

The model provider's API key is not in the environment.

speccoder reads API keys from the environment only. The message names the variable the configured provider needs; for Amazon Bedrock, AWS_BEARER_TOKEN_BEDROCK or an AWS profile (aws login).

- Export the variable the message names, e.g. export ANTHROPIC_API_KEY=…

### NO-CHECKS

No check applies, so nothing could show that nothing regressed.

Every configured check was skipped (`when_exists` did not match) or none is configured. A task cannot be judged without at least one check: add your test command as one.

- `speccoder config set checks.test.argv -- <your test command>`

### GIT

Git is not in the state this command needs.

A git precondition does not hold; the message names it.

- `git status`

### SETUP-INCOMPLETE

The project's setup is not committed, so every run would trip over it.

A worktree holds only commits: a file the checks need but the checkout never committed (`speccoder.toml`, `.config/nextest.toml`), a lockfile the checks write, or build output `.gitignore` does not list. `speccoder fix` commits that setup once.

- `speccoder fix`

### NO-REPOSITORY

Speccoder needs a git repository with at least one commit.

Evidence lives in commits and refs, and work happens on a branch of its own, so the project must be a git repository with a first commit.

- `git init && git add -A && git commit -m init`

### UNCOMMITTED-INTENT

The spec has uncommitted changes the run would not see.

speccoder reads committed files only — a worktree is checked out from a commit — so an uncommitted edit to the spec, plan or tasks would silently not count.

- `git status`
- `speccoder run --in-place`

### NO-REMOTE

There is no git remote to push to.

`pr` pushes `speccoder/<feature>` to a remote and opens the pull request there; `issue` reads the remote's GitHub issues. Add the repository's GitHub address as a remote, or name the one to use.

- `git remote add origin <url>`
- `git remote -v`
- `speccoder pr --remote <name>`

### WORKTREE-DIRTY

The feature's worktree has uncommitted changes.

Work left in the worktree outside a commit — a failed attempt's, or an edit by hand — would be mixed into what comes next.

- `speccoder worktree`
- `speccoder run`

### NOTHING-TO-HAND-OFF

The speccoder branch has nothing the base does not have.

`pr` hands off work on `speccoder/<feature>`; there is none yet, or it is already merged.

- `speccoder run`

### DIRTY-CHECKOUT

An in-place run needs a clean checkout.

In place, the attempt runs in your checkout; uncommitted changes would be judged as the agent's.

- `git stash -u`
- `git commit -am wip`

### UNBORN-HEAD

The checkout has no commit yet.

An in-place run branches from HEAD; there is no HEAD to branch from.

- `git add -A && git commit -m init`

### CONFIG-NOT-TRACKED

Speccoder.toml must be committed.

The judge's configuration is a reviewed input; an untracked or modified one could be changed without review.

- `git add speccoder.toml && git commit -m "speccoder: config"`

### BRANCH-IN-WORKTREE

The feature's branch is checked out in a worktree.

git refuses to check out a branch twice; an in-place run needs `speccoder/<feature>` free.

- `git worktree list`
- `git worktree remove <path>`

### IN-PLACE-ACTIVE

An in-place run holds this checkout.

Another in-place session is active (or was killed); it owns the checkout until it ends or is left.

- `speccoder run --in-place`
- `speccoder run --in-place --return`

### IN-PLACE-SESSION

An in-place session is recorded for this checkout.

A worktree-mode command would not see what the in-place session holds.

- `speccoder run --in-place`
- `speccoder run --in-place --return`

### IN-PLACE-PARALLEL

An in-place run works one task at a time.

Parallel lanes need worktrees of their own; in place there is only your checkout.

- `speccoder run --in-place`
- `speccoder run --parallel 2`

### MERGE-CONFLICT

The checkout is in the middle of a merge or rebase.

An in-place run cannot start or return while git waits for a conflict to be resolved.

- `git status`
- `git merge --abort`

### INTENT-CHANGED

The spec, plan or tasks changed during the run.

Intent files are hashed when an in-place run starts and compared before every task; a change means the work would be judged against a spec it was not given.

- `git diff`
- `speccoder run --in-place`

### JUDGE-CHANGED

The judge's inputs changed during the run.

Checks, test configuration or agent configuration changed after the run started; a verdict under changed rules would mean nothing.

- `git diff`
- `speccoder run --in-place`

### CHECKOUT-CHANGED

The checkout moved under the run.

HEAD, the branch or the tree is not where speccoder left it; someone or something else changed the checkout.

- `git status`
- `speccoder run --in-place --return`

### GIT-CHANGED

Git's configuration, hooks or refs changed during the run.

Where the sandbox cannot keep `.git` read-only, speccoder fingerprints it; a change stops the run before the next tick.

- `git config --list --show-origin`
- `speccoder run --in-place --return`

### RETURN-BLOCKED

The in-place run cannot switch back to your branch.

Switching back would overwrite changes or fail; the session stays until the checkout allows it.

- `git status`
- `speccoder run --in-place --return`

### NOT-FOUND

Nothing by that name exists.

No task, run, call or object matches what was asked for; the message names it.

- `speccoder report`

### NOT-FOUND-SCHEMA

There is no schema by that name.

`speccoder schema` lists the schemas speccoder publishes.

- `speccoder schema`

### TASK-NOT-READY

No task can run now.

The task is done, or waits for tasks it depends on.

- `speccoder report`

### NO-TASKS

The feature has no tasks yet.

Its task list is not written yet, or lists no task: nothing ran, and the feature is not done. The spec format's planning skills write the tasks; `speccoder report` names the next one.

- `speccoder report`

### TASK-DONE

The task is already ticked.

A ticked task is not run again; its evidence is in its commit.

- `speccoder report`

### TASK-BLOCKED

The task waits for tasks it depends on.

Dependencies come from the task list's phases, `[P]` markers and explicit references; the message names the open ones.

- `speccoder run`
- `speccoder task skip <dependency> --reason "…"`

### VERIFICATION-FAILED

The checks or the acceptance tests did not pass.

After the retries, a check regressed or the spec-derived acceptance tests did not go from red to green. The attempt is saved under its ref.

- `speccoder report`

### REGRESSION-ONLY

Ticked on the regression checks alone, nothing task-specific was shown.

The checks still pass, but no acceptance test showed this task's behaviour. It is ticked `regression_only`, never `verified`, with one recorded reason: `no_story` (no user story to derive a test from), `no_scenarios` (its story states no acceptance scenario), `acceptance_off` (`[acceptance] generate = false`), `author_wrote_none` (the test author wrote no test), `author_failed` (its tests could not be used), `already_passing` (they passed before the attempt — with the task that implemented it), `not_red` (they did not fail on an assertion first) or `tests_not_collected` (the command collected no test).

- Add an acceptance example to the task's story, then converge
- `speccoder converge`

### ATTESTATION-FAILED

An attestation is missing, unsigned where required, or does not hold.

`attest verify` found a tick whose signed statements are missing, malformed, edited, about another commit, or signed by no one you trust.

- `speccoder attest verify --json`

### TESTS-NOT-COLLECTED

A test file the attempt wrote never ran.

The attempt added or changed a test file, but the runner collected no test from it: the toolchain's rule says it is never built or picked up (a Rust file no target declares, a pytest file outside `python_files`, a vitest file outside `include`, a Go file under `testdata/`), or the run's report and listing hold none of its tests. A test that never ran is evidence of nothing, so the verdict fails and the retry is told the rule.

- `speccoder report`
- Move the tests where the runner collects them (the reason names the rule)

### PLAN-TESTS-UNCOLLECTED

The plan puts tests where the runner won't collect them.

A task names a test path the project's runner would not collect (Cargo builds only `tests/*.rs` and `tests/<dir>/main.rs`), and no task creates the file that would declare it; or a test file in the tree is collected by nothing. Tests written there would never run. A warning: the run is not stopped.

- `speccoder doctor`
- `speccoder spec tasks`

### PLAN-BEHAVIOUR-EARLY

A story's behaviour is planned before the story.

A Setup or Foundational task names a symbol a user story is about. If that task implements it, the story's acceptance tests already pass when the story's tasks run: they cannot go red, and the tasks tick regression-only. Write the story's acceptance tests at that task, or re-plan.

- `speccoder spec tasks`

### PLAN-TEST-ONLY-TASKS

Tasks that only write tests.

A story has tasks whose only outputs are test files next to the task that implements it. speccoder writes each story's acceptance tests itself and judges the implementing task by them; a test-only task has nothing of its own to show and ticks regression-only.

- `speccoder spec tasks`

### PRESET-NOT-SPECKIT

Presets are a Spec Kit mechanism.

`speccoder preset add` installs the speccoder preset through Spec Kit's own `specify preset add`; this project is not a Spec Kit project, so there is nothing to add it to.

- `speccoder doctor`

### PRESET-SPECKIT-MISSING

The Spec Kit tool `specify` is missing or too old.

Adding the preset runs `specify preset add`, which needs `specify` at the version the preset requires. It is not installed, or older.

- Install or upgrade Spec Kit (`uv tool install specify-cli`)
- `speccoder doctor`

### PRESET-FAILED

The preset could not be added: `specify preset add` failed.

Spec Kit refused or failed to install the preset; its own output is shown. Nothing in `specs/` was changed.

- `speccoder preset add`
- `speccoder doctor`

### NEW-PLAN-CHANGED

The plan changed since you saw it; nothing ran.

Creating a project runs exactly the plan you confirmed, bound to its digest: the commands, their order and the folder as it was seen. A tool's version or the folder's contents changed meanwhile, so the plan computed now differs and nothing was run. The result carries the new plan.

- Review the new plan and confirm it again

### NEW-TARGET-NOT-EMPTY

The folder is not empty and not a git repository.

A new project starts in an empty folder, or in a git repository at its top level with a clean tree. This folder holds files that are not under git, which the format's `--force` init could overwrite unseen.

- Choose an empty folder
- Or run `git init` there and commit what it holds first

### NEW-DIRTY

The repository has uncommitted changes.

speccoder commits exactly what the plan's steps produced. With changes of yours in the tree, the two could not be told apart, so the folder must be clean first.

- Commit or stash your changes, then try again

### NEW-NESTED-REPOSITORY

The folder is inside another repository.

A project is created at a repository's top level. This folder lies inside another repository's worktree; a second repository inside it, or a project in its subfolder, would split one history.

- Choose a folder outside it, or use the repository's top level

### NEW-ALREADY-PROJECT

The folder already holds a spec project.

There is nothing to create: the folder is a project of a spec format already. Open it instead.

- `speccoder ui`
- `speccoder init`

### NEW-TOOL-MISSING

A tool the plan needs is not installed.

The plan runs the ecosystems' own tools — the stack's scaffold and the spec format's init. One of them is not on the PATH; the message names it and how to install it.

- Install the tool, or choose another stack or format
- `speccoder doctor`

### NEW-STEP-FAILED

A step of the plan failed; later steps did not run.

A command of the plan exited with an error, timed out or was denied by the sandbox. The steps before it are kept; running the same plan again resumes at this step.

- Retry, or run the command yourself as shown

### NEW-NO-IDENTITY

Git does not know who you are.

The new project's one commit is yours, made with your git identity; speccoder never invents one. `user.name` or `user.email` is not set.

- Git config --global user.name "Your Name"
- Git config --global user.email you@example.com

### NEW-IN-PROGRESS

A new project is being created in this folder.

Another creation of a project in the same folder is running. Two at once would interleave their steps.

- Wait for it to finish

### NEW-UNDO-REFUSED

The project changed since speccoder created it; nothing was undone.

Undo moves a folder speccoder created to the Trash, or reverts speccoder's commit in an existing repository — only while nothing but that commit exists. Something changed since (the message lists what), or there is no Trash on this system; nothing was moved or deleted.

- Undo it yourself; nothing was moved

### SAMPLE-DIVERGED

This build cannot reproduce the sample's recorded run.

The sample is a recorded run replayed strictly. This build replayed it to a different result, so it is not shown as if it had worked: the board shows the sample's committed state only.

- `speccoder doctor --bug-report`

### SAMPLE-MISSING

This build has no sample.

The sample's recording is not part of this build: release builds bundle it; a build from source has it only when `crates/speccoder-runtime/assets/sample/` holds one (`just record-sample`).

- Install a release build
- `speccoder new`

### SIMILAR-FEATURE

An existing requirement is close to this description.

Before the spec is written, the description is compared with every feature's requirements by a deterministic text similarity. A close match may mean the change belongs in that feature; it is a hint and never stops the skill.

- `speccoder feature similar "…"`
- `speccoder review comments`

### NEW-CONFIRM

The plan needs your confirmation; nothing ran.

`speccoder new` runs commands in a folder; it runs them only after you saw them. Without a terminal to ask at, it printed the plan and stopped.

- Run the same command again with --yes

### THROUGH-QUESTIONS

The spec has open questions; planning waits for your answers.

The format's skill wrote the spec and left questions it could not decide. Planning on a guess would plan the wrong thing, so the chain stops until each is answered.

- `speccoder review`
- `speccoder review answer Q1 "…"`

### THROUGH-REVIEW

Review the spec; planning waits for your approval.

Plan through stops after the spec is written: its requirements are what every task will be judged by, so they are planned only once you approved them. No flag passes this stop.

- `speccoder review`
- `speccoder review approve`

### NEEDS-HUMAN

The run stopped for a person.

The run cannot go on without a decision; the message says what waits.

- `speccoder report`

### NEEDS-APPROVAL

A call waits for a person's decision; nothing ran.

The model asked for something the policy does not allow on its own — a dependency change, the network, a protected file. The attempt waits; deciding continues it from that call.

- `speccoder report --approvals`

### BLOCKED

The model says the task cannot be done as written.

The model used its honest exit: something in the spec or the code stops it. Its reason is a claim, shown as "Model says".

- `speccoder task nudge <task> --note "…"`
- `speccoder task skip <task> --reason "…"`

### BUDGET-EXHAUSTED

The attempt ran out of turns, calls or tokens.

The attempt reached a limit of `[budget]` before finishing. The work so far is saved under the attempt's ref.

- Raise the limit in speccoder.toml [budget]

### NOT-DEMONSTRATED

The evidence the task needs could not be produced.

The acceptance tests could not be made red before the attempt (they already pass, or fail for another reason), or a convergence gap shows in no failing test. Ticking would claim more than was shown.

- `speccoder report`

### TESTS-COLLECTION-UNKNOWN

Speccoder cannot tell whether the runner ran a test file.

The attempt changed a test file, and neither the toolchain's static rule, the check's JUnit report nor the runner's listing says whether a test from it ran (an unrecognised runner, a listing that failed or timed out, no report). A tick on tests that may not have run would claim more than was shown, so the task stops; the attempt is saved under its ref. It is not retried: the model cannot fix it.

- Declare a JUnit report for the check (`junit = "…"`)
- Set the check's `list` command in speccoder.toml

### ACCEPTANCE-UNWRITTEN

The acceptance tests could not be written, so nothing was attempted.

The call that writes acceptance tests from the spec failed (no credits, a rejected key, a broken run). Without them the task would be judged on regression checks only, so it stops.

- `speccoder run`
- Or set [acceptance] generate = false to rely on the checks alone

### SUSPECTED-GAMING

The gaming monitor suspects a shortcut.

A separate review of the passing attempt found something like a hard-coded result or a weakened test. The task stays unticked until a person decides.

- `speccoder report`

### TESTS-OVER-STRICT

An acceptance test rejects an implementation the spec appears to allow.

An alternative the spec allows (another order, wording, tie-break) failed the acceptance tests: they may encode a choice the spec did not make. The attempt is held until you rule; if the tests are right, the next run ticks it without a model call.

- `speccoder task rule <task> --tests-right --reason "…"`
- `speccoder run --task <task> --fresh-acceptance`
- `speccoder task accept <task> --reason "…"`

### ACCEPTANCE-WEAK

The acceptance tests are too weak for [acceptance] min_strength.

Wrong implementations — mutants of the changed code and model-written near misses — passed the acceptance tests too often, and adding tests once did not fix it. The task stays unticked; the report names what got through.

- `speccoder report`

### INTERRUPTED

The run was interrupted.

The process was stopped or killed; the journal holds what finished. The next run resumes without repeating finished calls.

- `speccoder run`

### CHECKLISTS-OPEN

The feature's checklists have unchecked items.

Spec Kit checklists gate implementation; nothing ran.

- Finish the checklists in the feature's checklists/ directory
- `speccoder run --ignore-checklists`

### ARCHIVE-AWAITING

The OpenSpec archive waits for your yes.

The diff of the archive is shown; nothing is written until a person confirms.

- `speccoder spec archive --yes`

### RUN-FAILED

The model call or the run failed.

The provider returned an error, the journal could not be written, or the run broke; the message names it. Nothing was ticked.

- `speccoder run -v`

### PROVIDER-REFUSED

The model provider refused the request.

No credits, a rejected, revoked or expired key or session, or an account limit. speccoder stops instead of judging the task on less evidence; for Amazon Bedrock the first fix renews the credential source.

- Check the provider account's credits and the API key
- `speccoder run`

### REPLAY-DIVERGED

The replay did not reproduce the recorded run.

Strict replay re-derives a run from its journal without a model; a difference means the code's decisions changed.

- `speccoder replay <run> --json`

### NOT-REPLAYABLE

This build cannot re-derive the recorded run.

The run was recorded by a build whose skills or effects this one does not have.

- Replay with the speccoder version that recorded the run

### BUG-NO-UNIT

This spec format has no place for a bug.

A bug lives in the format's own bug structure (Spec Kit's `bug` extension) or in its standing bug-fix unit (Spec Kit's `NNN-bugfixes`, an OpenSpec standing change). Kiro projects are not supported: nothing is written.

- Write a spec for the change and run it
- A change without a reproduction: speccoder do "…"

### BUG-EMPTY

The symptom and every statement need words.

`speccoder bug` records the person's own words: an empty symptom, `--expected` or `--unchanged` is refused before anything is written.

- `speccoder bug "<what happens>" --expected "<what should>"`

### BUG-NO-EXPECTED

A bug needs at least one --expected statement.

A reproduction asserts what should happen; speccoder never infers it from the symptom or an issue. Give one `--expected` per behaviour.

- `speccoder bug "<what happens>" --expected "<what should>"`

### BUG-ASSESSMENT-DRIFT

The bug's home does not carry exactly your words.

The bug's file (`assessment.md`, or the standing unit) does not read back exactly your Current, Expected and Unchanged statements: one is missing, added, reworded, or changed by the file's markup. Reproductions are written from your words only, so nothing was reproduced.

- Edit the assessment so it carries each statement as a `Current:` / `Expected:` / `Unchanged:` line
- `speccoder bug --slug <slug>`

### BUG-ASSESSMENT-UNREADABLE

The bug's assessment is not in a shape speccoder reads.

speccoder reads `assessment.md` in the layout of the `bug` extension version it was built against, and refuses rather than guesses on any other shape.

- `specify extension list --json`
- Add `Current:` and `Expected:` lines to the assessment's Report section

### BUG-TEST-NOT-FOUND

The test named with --test is not found.

`--test PATH::NAME` must name an existing test file and a test in it that the runner collects.

- Give the test as PATH::NAME, as the runner lists it

### BUG-TEST-NOT-RED

The named test does not fail on an assertion.

A reproduction must fail on the untouched code by an assertion, twice. A build or collection error, a timeout, an absent symbol or a flaky failure shows nothing about the bug.

- Fix the test so it fails on an assertion, then run `speccoder bug` again

### BUG-TEST-PASSES

The named test passes on the untouched code.

A test that passes before any fix cannot show the bug. Perhaps it is fixed already.

- Check the behaviour by hand; if it is fixed, there is nothing to record

### BUG-TEST-COVERAGE

Each --expected statement needs its own --test.

Each stated behaviour gets its own reproduction: repeat `--test` once per `--expected`, in the same order.

- `speccoder bug "…" --expected "<e1>" --test <file>::<test1> --expected "<e2>" --test <file>::<test2>`

### BUG-CANNOT-REPRODUCE

The bug could not be reproduced: nothing is fixed.

No try produced a test for each Expected statement that fails on its own assertion, or you rejected them (rejected tries count against `[bug] reproduce_tries`). A fix without a failing reproduction would be a guess, so the bug comes back to you with what each try showed.

- `speccoder bug "…" --expected "…" --test <file>::<test>`

### BUG-ALREADY-SATISFIED

The reproductions already pass: nothing to fix.

Every reproduction passed on the untouched code on every try. Nothing is ticked; keep the reproductions as a regression test (committed, the task attested) or drop the task.

- `speccoder bug keep <feature>/<task> --reason "kept as a regression test"`
- `speccoder task drop <task>`

### BUG-UNCHANGED-NOT-HOLDING

An Unchanged statement does not hold today.

The test of an Unchanged statement fails on the untouched code, so the statement is not true now and cannot guard the fix. The bug stops before any fix.

- Reword or remove the statement, then `speccoder bug run`

### BUG-NOT-DISCRIMINATING

The reproduction does not tell the fix from no fix.

With the attempt's source changes undone (its tests kept), no reproduction failed on an assertion: passing them proves nothing about the fix. The retry is told which statement.

- `speccoder run --task <task>`

### BUG-REPRO-WEAK

A fix that special-cases the reproduction's input passes.

A candidate fix that only recognises the reproduction's literal inputs passed the reproductions and the held-out variants, even after one variant was written again. The tests are too weak for a verified fix.

- `speccoder bug "…" --expected "…" --test <file>::<test>`

### BUG-CANDIDATE-APPLY

The candidate patch is not applied.

`--candidate` applies the patch on the untouched code with `git apply --check` first; some hunks did not apply, git could not read it as a patch, or it carries a binary hunk or a symbolic link (neither is judged as a candidate).

- Rebase the patch on HEAD, as a text diff without binary hunks or links

### BUG-CANDIDATE-JUDGE

The candidate patch touches what judges it.

A candidate must not change a frozen reproduction, a held-out path or another judge input (checks, configuration, the intent) — by an edit, a rename, a deletion or a mode change, as git itself reads the patch. It is refused before it is applied.

- Remove those hunks from the patch

### EVAL-FAILED

The evaluation found a false completion or a cheat.

A case ended done although its hidden oracle fails, or the attempt gamed its tests.

- `speccoder eval --keep <dir>`
