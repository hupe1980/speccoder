+++
title = "speccoder"
description = "speccoder implements Spec Kit features and OpenSpec changes task by task, in an isolated git worktree and an OS sandbox, and ticks a task only when tests derived from your spec go from red to green."
template = "index.html"

[extra]
faq = [
  { q = "What is speccoder?", a = "A coding agent for spec-driven projects (GitHub Spec Kit and OpenSpec), with a terminal dashboard and an app. It works through a feature's tasks one at a time and ticks a task only when evidence shows it is done: tests derived from your spec that fail before the change and pass after it, your own checks, and an audit of the diff." },
  { q = "How is it different from a coding agent in my editor?", a = "An editor agent decides when it is done. speccoder never takes the model's word: a task is decided by tests, checks and a diff audit, every attempt is recorded in git, and a person decides whatever is uncertain. You can even let GitHub Copilot, OpenCode, Claude Code, Codex or Gemini CLI make the attempts: speccoder decides every call they make and keeps the judgement." },
  { q = "Which models does it support?", a = "Anthropic and OpenAI models, Amazon Bedrock (a Bedrock API key or an aws login session), any OpenAI-compatible server (Ollama, vLLM, llama.cpp, LM Studio), and GitHub Copilot, OpenCode, Claude Code, Codex or Gemini CLI as executors over ACP (or any command). API keys are read from the environment only." },
  { q = "Is it safe to run on my code?", a = "speccoder works in its own git worktree and never touches your checkout. Every command it starts runs in an OS sandbox (Seatbelt on macOS, Landlock and seccomp on Linux) that writes only to the worktree, cannot read credentials, has no network beyond loopback and cannot reach daemon sockets such as Docker's. Every tool call is classified before it runs: allowed, needing your approval, or denied." },
  { q = "Which spec formats does it support?", a = "GitHub Spec Kit (projects created by 0.13 to 1.2) and OpenSpec (1.0 and newer). speccoder reads their files as they write them and runs their planning skills (Spec Kit: specify, clarify, plan, tasks, analyze; OpenSpec: propose, explore, archive) with its own model. Kiro specs are recognised and refused." },
  { q = "Can a reviewer check the evidence without trusting the author?", a = "Yes. speccoder attest verify --branch checks every ticked task of a branch against its signed evidence using the reviewer's own trust, with no model key and no network. Its GitHub Action posts the result on each pull request as the speccoder / evidence check." },
  { q = "What does verified mean?", a = "The spec-derived acceptance tests went from red to green and none of your checks regressed. It does not mean the change is correct, complete or safe. Review the speccoder branch like any contribution before you merge it." },
]
+++
