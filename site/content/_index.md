+++
title = "speccoder"
description = "speccoder implements GitHub Spec Kit features task by task, in an isolated git worktree and an OS sandbox, and ticks a task only when tests derived from your spec go from red to green."
template = "index.html"

[extra]
faq = [
  { q = "What is speccoder?", a = "A command-line coding agent for GitHub Spec Kit projects. It works through a feature's tasks one at a time and ticks a task only when evidence shows it is done: tests derived from your spec that fail before the change and pass after it, your own checks, and an audit of the diff." },
  { q = "How is it different from a coding agent in my editor?", a = "An editor agent decides when it is done. speccoder never takes the model's word: a task is decided by tests, checks and a diff audit, every attempt is recorded in git, and a person decides whatever is uncertain. You can even let Claude Code or Codex make the attempts and keep speccoder's judgement." },
  { q = "Which models does it support?", a = "Anthropic and OpenAI models, any OpenAI-compatible server (Ollama, vLLM, llama.cpp, LM Studio), and Claude Code, Codex or any command as an external agent. API keys are read from the environment only." },
  { q = "Is it safe to run on my code?", a = "speccoder works in its own git worktree and never touches your checkout. Every command it starts runs in an OS sandbox (Seatbelt on macOS, Landlock and seccomp on Linux) that writes only to the worktree, cannot read credentials and has no network beyond loopback. Every tool call is classified before it runs: allowed, needing your approval, or denied." },
  { q = "Do I need GitHub Spec Kit?", a = "Yes. speccoder reads Spec Kit's spec.md, plan.md and tasks.md, and supports projects created by Spec Kit 0.13 to 1.2. It can also run Spec Kit's planning skills (specify, clarify, plan, tasks, analyze) with its own model." },
  { q = "What does verified mean?", a = "The spec-derived acceptance tests went from red to green and none of your checks regressed. It does not mean the change is correct, complete or safe. Review the speccoder branch like any contribution before you merge it." },
]
+++
