+++
title = "GitHub"
description = "Start a feature from an issue, see its pull request beside the evidence, and open or update the draft — through GitHub's API, as you, with nothing else to install."
weight = 82

[extra]
group = "Use"
+++

speccoder talks to GitHub through its API, as you — github.com and GitHub Enterprise alike: the host is your remote's. Nothing else to install. Reading (issues, a pull request's state) happens as you look; writing (a push, a pull request) happens only when you confirm it.

## Sign in

```sh
speccoder auth login --with-token < token.txt   # once; the token goes to your keychain
speccoder auth status                           # who speccoder is there, and where its token comes from
speccoder auth logout                           # forget it
```

[Create a fine-grained token](https://github.com/settings/personal-access-tokens/new) for your repository with **Issues: read**, **Pull requests: read and write**, **Contents** and **Metadata: read** (a classic token: `repo`, `read:org`). Already use the GitHub CLI? `gh auth token | speccoder auth login --with-token` signs in with its token — speccoder does not need `gh` afterwards. In the [app](@/docs/web.md), the GitHub card's **Sign In** takes the token in a password field.

Where the token comes from, in order:

1. `GH_TOKEN`, then `GITHUB_TOKEN` (an Enterprise host: `GH_ENTERPRISE_TOKEN`) — for CI, where Actions provides `GITHUB_TOKEN`;
2. the token `auth login` kept in the system's credential store (macOS Keychain, Windows Credential Manager, the Secret Service on Linux; service `speccoder`, account `github:<host>`).

speccoder never writes a token to a file, never prints it, and sends it only to its own host. Without a credential store, sign-in says so — set `GH_TOKEN` instead. `doctor` shows whether a token is there and where from, without calling GitHub.

**Device flow.** With an OAuth App's client ID in `SPECCODER_GITHUB_CLIENT_ID` (your Enterprise server's own app, for example), `speccoder auth login` opens GitHub's device page and signs in with a code — no token to copy.

## A feature from an issue

```sh
speccoder issue                 # the open issues: yours first, then the freshest
speccoder issue start 42        # specify a feature from #42
```

`issue start` runs your spec tool's own skill (Spec Kit's specify) with the issue's title, text and address as the description. The skill writes it into the spec, so the link lives where the feature does — no state of speccoder's own. `42`, `#42` and the issue's address all work.

In the [web interface](@/docs/web.md), the overview lists the open issues with **Start Feature**; an issue a feature's spec already names shows that feature instead.

## Its pull request

```sh
speccoder pr --dry-run          # what would be pushed, where, and the description
speccoder pr                    # asks, then pushes and opens the draft
```

The draft's description carries the evidence — every task's verdict, its acceptance strength, its attestation and how to verify it, how much of the spec is approved — and `Closes` for every issue the spec names, so merging closes them. Run `pr` again after more tasks are ticked: an open pull request is updated — the branch and its attestation notes pushed, its title and description brought up to date with the evidence — never opened twice. Nothing is pushed unless the pull request can be opened: a remote that names no GitHub repository, or nobody signed in, is said first.

In the web interface, the report shows the pull request as GitHub has it — draft or open or merged, the review decision, the checks — beside speccoder's own evidence. **Open Draft PR** and **Update** show a confirmation first: the branch, the remote, the steps, the title and the description.

## When it does not work

| Code | What is wrong | Fix |
|---|---|---|
| [`GITHUB-AUTH`](@/docs/codes.md#github-auth) | no token, or GitHub refused it (expired, revoked) | `speccoder auth login --with-token`, or set `GH_TOKEN` |
| [`GITHUB-FAILED`](@/docs/codes.md#github-failed) | GitHub answered otherwise: no such issue, no access for the token, a rate limit used up, no answer in time | the message says which; `speccoder auth status` |
| [`NO-REMOTE`](@/docs/codes.md#no-remote) | no remote, or one that names no GitHub repository | `git remote add origin <url>` |

`SPECCODER_GITHUB_API` sends every call — with your token — to another address (a proxy you run); set it only to one you trust. A pull request is opened in the repository `--remote` names (default `origin`), from a branch of that repository; pull requests from a fork are not opened by speccoder.
