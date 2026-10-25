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

`speccoder pr` pushes `speccoder/<feature>` (and the attestation notes, `refs/notes/speccoder`) to `--remote` (default `origin`) and opens a **draft** pull request against your checkout's branch or `--base`. It is the only command that pushes to a remote. Without a terminal and without `--yes` it is a dry run; it never merges or marks a pull request ready.

The description is the evidence:

- the totals, and a table of every task: its verdict, mutation results, attestation and commit;
- tokens, and convergence rounds, if any;
- `Closes` for every issue the spec names, so merging closes them;
- how a reviewer verifies the attestations, when there are any;
- what `verified` does not mean.

Run `pr` again after more tasks are ticked: the open pull request is updated (branch and notes pushed, title and description refreshed), never opened twice. When the branch's full-tier checks failed, `pr` refuses; `--force` hands off anyway and says so in the description. Nothing is pushed when no pull request can be opened: no GitHub remote, or nobody signed in.

A pull request is opened in the repository `--remote` names, from a branch of that repository; speccoder does not open pull requests from a fork.

In the web interface, the report shows the pull request as GitHub has it (draft, open or merged; the review decision; the checks) beside the evidence. **Open Draft PR** and **Update** show the branch, remote, steps, title and description first.

## When it does not work

| Code | What is wrong | Fix |
|---|---|---|
| [`GITHUB-AUTH`](@/docs/codes.md#github-auth) | no token, or GitHub refused it (expired, revoked) | `speccoder auth login`, or set `GH_TOKEN` |
| [`GITHUB-FAILED`](@/docs/codes.md#github-failed) | GitHub answered otherwise: no such issue, no access, rate limit used up, no answer in time | the message says which; `speccoder auth status` |
| [`NO-REMOTE`](@/docs/codes.md#no-remote) | no remote, or one that names no GitHub repository | `git remote add origin <url>` |

`SPECCODER_GITHUB_API` sends every call, with your token, to another address (a proxy you run); set it only to one you trust.
