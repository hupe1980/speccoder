+++
title = "GitHub"
description = "Start a feature from an issue, see its pull request beside the evidence, and open or update the draft — through GitHub's API, as you, with nothing else to install."
weight = 82

[extra]
group = "Use"
+++

speccoder talks to GitHub's API as you, on github.com and GitHub Enterprise alike: the host is your remote's. Reading (issues, a pull request's state) happens as you look; writing (a comment, a push, a pull request) only when you confirm it.

## Sign in

```sh
speccoder auth login      # a code, approved on GitHub — no token to copy
speccoder auth status     # who speccoder is there, and where its token comes from
speccoder auth logout     # forget the stored token
```

Each takes `--host` (default: the remote's host, else github.com).

`auth login` signs you in through speccoder's GitHub App, [speccoder-app](https://github.com/apps/speccoder-app): it shows a code (already on your clipboard) and opens GitHub's device page, where you enter it and approve. If the app cannot see this repository, speccoder prints the [install link](https://github.com/apps/speccoder-app/installations/new); install it on your account or organisation, for all repositories or selected ones. In the [web interface](@/docs/web.md), the GitHub card's **Sign In** does the same.

The app's repository permissions:

| Permission | Access |
|---|---|
| Issues, Pull requests | read and write |
| Contents, Metadata, Checks, Commit statuses | read |

A token works instead: `speccoder auth login --with-token < token.txt` reads a [fine-grained token](https://github.com/settings/personal-access-tokens/new) with these permissions from stdin; `gh auth token | speccoder auth login --with-token` uses the GitHub CLI's.

Where the token comes from, in order:

1. `GH_TOKEN`, then `GITHUB_TOKEN` (an Enterprise host: `GH_ENTERPRISE_TOKEN`, then `GITHUB_ENTERPRISE_TOKEN`) — in Actions, `GITHUB_TOKEN` is provided;
2. the token `auth login` kept in the system's credential store (macOS Keychain, Windows Credential Manager, the Secret Service on Linux; service `speccoder`, account `github:<host>`).

speccoder never writes a token to a file or prints it, and sends it only to its host. Without a credential store, sign-in says so; set `GH_TOKEN` instead. `speccoder doctor` shows whether a token is there and where from, without calling GitHub.

**GitHub Enterprise.** speccoder's app lives on github.com. On your server, register a GitHub App with the permissions above, **Enable Device Flow** on and **Expire user authorization tokens** off (a command-line tool cannot keep the secret renewal needs), and set its Client ID in `SPECCODER_GITHUB_CLIENT_ID`. Or sign in with a token.

## A feature from an issue

```sh
speccoder issue                 # open issues: assigned to you first, then most recently updated
speccoder issue start 42        # specify a feature from #42
speccoder issue comment 42      # tell #42 the current feature is being specified from it
```

`issue start` runs your spec tool's specify skill with the issue's title, text and address as the description; the skill writes the link into the spec. `42`, `#42` and the issue's address all work.

Once the feature exists, speccoder asks "Comment on #42 that it is being specified? [Y/n]". The comment names the feature, its spec and its branch, and is posted once per feature.

| Flag (`issue start`) | Does |
|---|---|
| `--comment` / `--no-comment` | Decide without asking |
| `--assign` | Also assign the issue to you (also on `issue comment`) |
| `--no-prompt` | Never ask; the skill leaves questions as `[NEEDS CLARIFICATION]` |

In the web interface, the overview lists the open issues with **Start Feature**; an issue a spec already names shows that feature and **Comment** (confirmed first).

## Its pull request

```sh
speccoder pr --dry-run          # title, description and commands; nothing pushed
speccoder pr                    # shows that, asks, then pushes and opens the draft
speccoder pr --yes --base main  # without asking
```

`speccoder pr` pushes `speccoder/<feature>`, the attestation notes (`refs/notes/speccoder`) and the held-out tests of the tasks closed on the pushed head (`refs/speccoder/held-out/<feature>`) to `--remote` (default `origin`), none of them forced, and opens a **draft** pull request against your checkout's branch or `--base`. It is the only command that pushes to a remote. Without a terminal and without `--yes` it is a dry run; it never merges or marks a pull request ready.

The held-out ref carries exactly the [held-out sets](@/docs/evidence.md) whose manifest digest a task closed on the pushed head names in its signed evidence — the same rule for a feature, a standing unit that never finishes (quick changes, a bug-fix unit) or an OpenSpec change. Sets of open tasks stay local (the dry run counts them: `held-out tests of 2 open task(s) stay local`): a later task's implementer could otherwise read them. Each publication is parented on the branch and on the previous one, so the ref only grows and every push is a fast-forward; a rejected push is reported with the command to compare, and the pull request opens anyway. The dry run also says whether a workflow of the repository runs `speccoder-verify`, or links [how to add one](#evidence-check).

The description is the evidence:

- the totals, and a table of every task: its verdict, mutation results, attestation and commit;
- tokens, and convergence rounds, if any;
- `Closes` for every issue the spec names, so merging closes them;
- how a reviewer verifies the attestations, when there are any;
- each ticked task's [evidence tour](@/docs/verifier.md#the-evidence-tour), collapsed (dropped last-first, counted, within GitHub's 65 536 characters);
- what `verified` does not mean.

Run `pr` again after more tasks are ticked: the open pull request is updated (branch and notes pushed, title and description refreshed), never opened twice. When the branch's full-tier checks failed, `pr` refuses; `--force` hands off anyway and says so in the description. Nothing is pushed when no pull request can be opened: no GitHub remote, or nobody signed in.

A pull request is opened in the repository `--remote` names, from a branch of that repository; speccoder does not open pull requests from a fork.

In the web interface, the report shows the pull request as GitHub has it (draft, open or merged; the review decision; the checks) beside the evidence. **Open Draft PR** and **Update** show the branch, remote, steps, title and description first.

## Evidence check

The description is the author's account. The **`speccoder / evidence`** check is the maintainer's: the [`speccoder-verify` action](https://github.com/hupe1980/speccoder/tree/main/packaging/speccoder-verify-action) runs [the verifier](@/docs/verifier.md) on GitHub's runner, on every push, with the trust of your base branch, and publishes a check run with the verdict bar, the requirement trace, an annotation on each tick that is not plainly verified, and every task's tour. It needs no model key and no speccoder, builds nothing and runs nothing from the pull request; the binary it downloads is refused unless its build provenance (signed by speccoder's release workflow for that version) and its sha256 check out.

1. **Trust.** Commit the keys that may sign evidence to the default branch, in [ssh-keygen's allowed-signers format](https://man.openbsd.org/ssh-keygen#ALLOWED_SIGNERS):

   ```text
   # .github/speccoder/allowed_signers
   ada@example.com namespaces="in-toto" ssh-ed25519 AAAAC3Nza…
   ```

   The action reads it from the pull request's **base** commit — a pull request that adds its own key to the file is not trusted by it. For Sigstore, set `certificate-identity` and `certificate-oidc-issuer` instead.

2. **The workflow**, for pull requests from branches of the repository:

   ```yaml
   # .github/workflows/evidence.yml
   name: evidence
   on: pull_request
   permissions:
     contents: read
     checks: write        # creates the check run (the workflow token is a GitHub App token)
     pull-requests: read
     attestations: read   # verifies the verifier's build provenance
   jobs:
     evidence:
       runs-on: ubuntu-latest
       steps:
         - uses: hupe1980/speccoder-verify@v0.2.0
   ```

   No checkout: the action fetches the head and its notes as git objects into a scratch repository. A `pull_request` run from a **fork** gets a read-only token: the action then writes the same summary to the job summary and the annotations to the job, says that the named check was not published and why, and the **job**'s status carries the outcome — require the job, or use the fork workflow below.

3. **Forks** — the same, from the base branch's workflow, with a write token:

   ```yaml
   on: pull_request_target   # the workflow and its inputs come from the base branch
   permissions:
     contents: read
     checks: write
     pull-requests: read
     attestations: read
   jobs:
     evidence:
       runs-on: ubuntu-latest
       steps:
         # Never add actions/checkout of the head here, nor any build: under
         # pull_request_target that would run the fork's code with a write token.
         - uses: hupe1980/speccoder-verify@v0.2.0
   ```

   The verifier reads the fork's notes and objects and executes nothing from them; the action refuses to run if the head is checked out in the workspace. If your policy forbids `pull_request_target`, a two-stage `workflow_run` (the first workflow uploads the event, the second runs the action) gives the same guarantees.

4. **Make it required.** In the branch's protection rule (or ruleset), require the status check `speccoder / evidence`, source **GitHub Actions** (the workflow token creates it). GitHub counts a `neutral` required check as passing; a pull request that touches no unit has nothing to check, so the action publishes that as `failure`, titled `neutral: no ticks checked`, unless you set `allow-neutral: true`.

5. **Merge with merge commits** for verified features. A squash or rebase merge makes new commits: the attestations name the old ones, which are no longer reachable, and on the default branch those ticks become `commit_unreachable`. Push the notes too (`speccoder pr` does; `git push origin refs/notes/speccoder` otherwise).

| Input | Default | |
|---|---|---|
| `version` | the action's release | the verifier to download (provenance and sha256 checked) |
| `allowed-signers` | `.github/speccoder/allowed_signers` | read from the base revision |
| `certificate-identity`, `certificate-oidc-issuer` | — | Sigstore trust; installs `cosign` |
| `require-signed` | `true` | unsigned ticks are unverifiable |
| `require-verified` | `false` | `regression_only` and `attested` fail too |
| `allow-neutral` | `false` | nothing to check passes as `neutral` |
| `mode` | `touched` on pull requests, `all` on push | which units are checked |
| `check-name` | `speccoder / evidence` | |
| `badge-branch` | — | see below |

Outputs: `conclusion`, `verified`, `regression-only`, `attested`, `stale`, `unverifiable`, `share`, `report` (the JSON report's path).

### A badge

On a push to the default branch, `badge-branch: speccoder-badge` (with `contents: write` on that workflow) commits `speccoder-evidence.svg` and a shields.io endpoint file to that branch; nothing is published from a pull request, and there is no hosted service. Link it from your README:

```markdown
![evidence](https://raw.githubusercontent.com/<owner>/<repo>/speccoder-badge/speccoder-evidence.svg)
```

The workflow's own status badge is the zero-setup alternative (pass or fail only).

## When it does not work

| Code | What is wrong | Fix |
|---|---|---|
| [`GITHUB-AUTH`](@/docs/codes.md#github-auth) | no token, or GitHub refused it (expired, revoked) | `speccoder auth login`, or set `GH_TOKEN` |
| [`GITHUB-FAILED`](@/docs/codes.md#github-failed) | GitHub answered otherwise: no such issue, no access, rate limit used up, no answer in time | the message says which; `speccoder auth status` |
| [`NO-REMOTE`](@/docs/codes.md#no-remote) | no remote, or one that names no GitHub repository | `git remote add origin <url>` |

`SPECCODER_GITHUB_API` sends every call, with your token, to another address (a proxy you run); set it only to one you trust.
