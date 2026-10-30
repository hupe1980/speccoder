+++
title = "GitHub"
description = "Start a feature from an issue, see its pull request beside the evidence, and open or update the draft — through GitHub's API, as you, with nothing else to install."
weight = 82

[extra]
group = "Use"
+++

speccoder talks to GitHub's API as you, on github.com and GitHub Enterprise (the host is your remote's). It reads issues and a pull request's state as you look, and writes (a comment, a push, a pull request) only when you confirm.

## Sign in

```sh
speccoder auth login      # a code, approved on GitHub — no token to copy
speccoder auth status     # who speccoder is there, and where its token comes from
speccoder auth logout     # forget the stored token (an environment variable stays yours)
```

Each takes `--host` (default: the remote's host, else github.com). `auth login` signs you in through speccoder's GitHub App, [speccoder-app](https://github.com/apps/speccoder-app): it copies a code to your clipboard and opens GitHub's device page. If the app cannot see this repository, speccoder prints the [install link](https://github.com/apps/speccoder-app/installations/new). In the [web interface](@/docs/web.md), the GitHub card's **Sign In** does the same.

| Permission | Access |
|---|---|
| Issues, Pull requests | read and write |
| Contents, Metadata, Checks, Commit statuses | read |

A token works instead: `speccoder auth login --with-token < token.txt` reads a [fine-grained token](https://github.com/settings/personal-access-tokens/new) with these permissions from stdin (`gh auth token | speccoder auth login --with-token` uses the GitHub CLI's). The token is taken from, in order:

1. `GH_TOKEN`, then `GITHUB_TOKEN` (an Enterprise host: `GH_ENTERPRISE_TOKEN`, then `GITHUB_ENTERPRISE_TOKEN`);
2. the system's credential store, where `auth login` keeps it (macOS Keychain, Windows Credential Manager, the Secret Service on Linux; service `speccoder`, account `github:<host>`).

speccoder never writes a token to a file or prints it, and sends it only to its host. Without a credential store, set `GH_TOKEN`. `speccoder doctor` shows where a token comes from, without calling GitHub.

**GitHub Enterprise.** speccoder's app lives on github.com. On your server, register a GitHub App with the permissions above, **Enable Device Flow** on and **Expire user authorization tokens** off, and set its Client ID in `SPECCODER_GITHUB_CLIENT_ID`; or sign in with a token.

## A feature from an issue

```sh
speccoder issue                 # open issues: assigned to you first, then most recently updated
speccoder issue start 42        # specify a feature from #42 (also #42 or the issue's address)
speccoder issue comment 42      # tell #42 the current feature is being specified from it
```

`issue start` runs your spec tool's specify skill with the issue's title, text and address; the skill writes the link into the spec. Once the feature exists, speccoder asks whether to comment on the issue; the comment names the feature, its spec and its branch, once per feature.

| Flag (`issue start`) | Does |
|---|---|
| `--comment` / `--no-comment` | Decide without asking |
| `--assign` | Also assign the issue to you (also on `issue comment`) |
| `--no-prompt` | Never ask; the skill leaves questions as `[NEEDS CLARIFICATION]` |

## Its pull request

```sh
speccoder pr --dry-run          # title, description and commands; nothing pushed
speccoder pr                    # shows that, asks, then pushes and opens the draft
speccoder pr --yes --base main  # without asking
```

`speccoder pr` is the only command that pushes. To `--remote` (default `origin`), none of them forced, it pushes the branch `speccoder/<feature>`, the attestation notes `refs/notes/speccoder`, and the [held-out tests](@/docs/evidence.md#where-they-are-kept) of the tasks closed on the pushed head (`refs/speccoder/held-out/<feature>`). It then opens a **draft** pull request against your checkout's branch or `--base`. Without a terminal and without `--yes` it is a dry run. It never merges or marks a pull request ready.

- Held-out sets of open tasks stay local (the dry run counts them); a set that changed since it judged is left out and named.
- A rejected push of the held-out ref or of the notes is reported with the commands to reconcile (`git notes --ref=speccoder merge` for diverged notes); the pull request opens anyway.
- The dry run says whether a workflow under `.github/workflows/` runs the evidence check (the action, or `speccoder attest verify --branch`), or links [how to add one](#evidence-check).
- Run `pr` again after more tasks are ticked: the open pull request is updated, never opened twice.
- It opens in the repository `--remote` names, from a branch of that repository; not from a fork.

The description is the evidence: the totals and a table of every task (verdict, mutation results, attestation, commit); tokens and convergence rounds; `Closes` for every issue the spec names; `Fixes #N` for every issue a [bug fix](@/docs/bugs.md) on the branch came from, while its task is `verified` or `attested`; how to verify the attestations; each ticked task's [evidence tour](@/docs/verifier.md#the-evidence-tour), collapsed, within GitHub's 65 536 characters (dropped last-first, counted); and what `verified` does not mean.

`pr` refuses, pushing nothing, when the branch's full-tier checks failed (`--force` hands off anyway and says so in the description), when the branch has nothing the base lacks ([`NOTHING-TO-HAND-OFF`](@/docs/codes.md#nothing-to-hand-off)), or when there is no such remote, it names no GitHub repository, or nobody is signed in.

## Evidence check

The description is the author's account; the **`speccoder / evidence`** check is the maintainer's. The [evidence action](https://github.com/hupe1980/speccoder/tree/main/packaging/speccoder-verify-action) (`speccoder-verify-action`) downloads the `speccoder` release and runs [`speccoder attest verify --branch`](@/docs/verifier.md) with the trust of your base branch and publishes a check run with the verdict bar, the requirement trace, an annotation on each tick that is not plainly verified, and every task's tour.

- It needs no model key and no speccoder install, and builds and runs nothing from the pull request.
- The `speccoder` archive it downloads runs only if its build provenance (signed by speccoder's release workflow for that version) and its sha256 check out. Builds exist for Linux x64, Linux ARM64 and macOS ARM64 runners.
- speccoder runs with no token in its environment.

1. **Trust.** Commit the keys that may sign evidence to the default branch, in [ssh-keygen's allowed-signers format](https://man.openbsd.org/ssh-keygen#ALLOWED_SIGNERS):

   ```text
   # .github/speccoder/allowed_signers
   ada@example.com namespaces="in-toto" ssh-ed25519 AAAAC3Nza…
   ```

   The action never reads this file from the commit it checks, so a change that adds its own key is not trusted:

   | Event | Trust from |
   |---|---|
   | pull request | the base commit |
   | push to the default branch | the commit before the push (`before`) |
   | any other push or event | the default branch's tip |

   Where there is no such commit, or it is the checked commit itself (the push that creates the default branch, a branch pushed at the default branch's tip), the job fails, `allow-neutral` or not. A key takes effect for the pushes after the merge that adds it. For Sigstore, set `certificate-identity` and `certificate-oidc-issuer` instead.

2. **The workflow**, for pull requests from branches of the repository:

   ```yaml
   # .github/workflows/evidence.yml
   name: evidence
   on: pull_request
   permissions:
     contents: read
     checks: write        # creates the check run
     pull-requests: read
     attestations: read   # verifies speccoder's build provenance
   jobs:
     evidence:
       runs-on: ubuntu-latest
       steps:
         - uses: hupe1980/speccoder/packaging/speccoder-verify-action@v0.5.1
   ```

   No checkout: the action fetches the head and its notes as git objects into a scratch repository. A `pull_request` run from a **fork** gets a read-only token: the action then writes the summary and annotations to the job, says the named check was not published, and the **job**'s status carries the outcome. Require the job, or use the fork workflow below.

3. **Forks**: the same with `on: pull_request_target` (the workflow and its inputs come from the base branch, with a write token). Never add a checkout of the head or any build to that workflow: it would run the fork's code with a write token. The action refuses to run if the head is checked out in the workspace.

4. **Make it required.** In the branch's protection rule (or ruleset), require the status check `speccoder / evidence`, source **GitHub Actions**. GitHub counts a `neutral` required check as passing, so a pull request with nothing to check is published as `failure`, titled `neutral: no ticks checked`, unless you set `allow-neutral: true`. A refusal (the check cannot read the branch) fails the job even then.

5. **Merge with merge commits** for verified features. A squash or rebase merge makes new commits: the attestations name the old ones, and on the default branch those ticks become `commit_unreachable`. Push the notes too (`speccoder pr` does; `git push origin refs/notes/speccoder` otherwise).

| Input | Default | |
|---|---|---|
| `version` | the action's release (`0.5.1`) | the `speccoder` release to download (provenance and sha256 checked) |
| `allowed-signers` | `.github/speccoder/allowed_signers` | read from the trusted commit (above), never the checked one |
| `certificate-identity`, `certificate-oidc-issuer` | — | Sigstore trust; installs `cosign` |
| `require-signed` | `true` | unsigned ticks are unverifiable |
| `require-verified` | `false` | `regression_only` and `attested` fail too |
| `allow-neutral` | `false` | nothing to check passes as `neutral` |
| `mode` | `touched` on pull requests, `all` otherwise | which units are checked |
| `check-name` | `speccoder / evidence` | the check run's name |
| `badge-branch` | — | see [a badge](#a-badge) |
| `token` | `github.token` | for the action's own git and API calls; never passed to speccoder |

Outputs: `conclusion`, `verified`, `regression-only`, `attested`, `stale`, `unverifiable`, `share`, `report` (the JSON report's path, payload `branch_verify`).

### A badge

On a push to the default branch, `badge-branch: speccoder-badge` (with `contents: write` on that workflow) commits `speccoder-evidence.svg` and a shields.io endpoint file, `speccoder-evidence.json`, to that branch. Nothing is published from a pull request, and there is no hosted service:

```markdown
![evidence](https://raw.githubusercontent.com/<owner>/<repo>/speccoder-badge/speccoder-evidence.svg)
```

## When it does not work

| Code | What is wrong | Fix |
|---|---|---|
| [`GITHUB-AUTH`](@/docs/codes.md#github-auth) | no token, or GitHub refused it (expired, revoked) | `speccoder auth login`, or set `GH_TOKEN` |
| [`GITHUB-FAILED`](@/docs/codes.md#github-failed) | GitHub answered otherwise: no such issue, no access, rate limit used up, no answer in time | the message says which; `speccoder auth status` |
| [`NO-REMOTE`](@/docs/codes.md#no-remote) | no remote, or one that names no GitHub repository | `git remote add origin <url>` |

`SPECCODER_GITHUB_API` sends every call, with your token, to another address (a proxy you run); set it only to one you trust.
