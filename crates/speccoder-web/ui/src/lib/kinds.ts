// What each kind of Needs You item is called, its symbol and tone, and
// why it waits: one table for Needs You and the view across projects.

export const KINDS: Record<string, { title: string; icon: string; tone: string; why: string }> = {
  unknown: { title: "Could Not Be Read", icon: "exclaim", tone: "needs_approval", why: "Another speccoder holds the journal — calls may be waiting for you that cannot be listed now." },
  approval: { title: "Waiting for Approval", icon: "hand", tone: "needs_approval", why: "A run is paused until you decide." },
  bug_confirm: { title: "Confirm a Bug's Reproduction", icon: "exclaim", tone: "needs_approval", why: "It fails on the untouched code by an assertion — does it show the bug? Nothing is fixed before you say so." },
  bug_cannot_reproduce: { title: "Could Not Reproduce", icon: "xoctagon", tone: "failed", why: "No test of every expected behaviour failed on an assertion; nothing was fixed. Bring a test, reword, or try again." },
  bug_unchanged_not_holding: { title: "Unchanged Does Not Hold", icon: "exclaim", tone: "failed", why: "A statement of what must keep working is not true today." },
  bug_repro_weak: { title: "Reproduction Too Weak", icon: "shield", tone: "needs_approval", why: "A fix that only recognises the reproduction's input passes it." },
  bug_already_satisfied: { title: "Already Fixed", icon: "check", tone: "verified", why: "Every reproduction passes: keep one as a regression test, or drop it." },
  approval_batch: { title: "Calls Waiting Together", icon: "hand", tone: "needs_approval", why: "The run used its approval budget: further calls wait here, decided together — each once." },
  ruling: { title: "Rule on the Tests", icon: "shield", tone: "needs_approval", why: "The tests reject an alternative the spec may allow." },
  gaming: { title: "Suspected Shortcuts", icon: "exclaim", tone: "failed", why: "The monitor saw what looks like gaming the tests." },
  unverified: { title: "Little Was Verified", icon: "loop", tone: "regression_only", why: "Most ticks rest on the regression checks alone — why, by reason, and what changes it." },
  stale: { title: "The Spec Changed", icon: "exclaim", tone: "needs_approval", why: "Evidence proves an earlier wording." },
  ambiguous: { title: "Read Two Ways", icon: "exclaim", tone: "needs_approval", why: "Two test authors read a requirement differently — say which reading is meant, or reword it. Approve and Run waits." },
  question: { title: "Open Questions", icon: "exclaim", tone: "needs_approval", why: "The spec leaves these undecided — answer before it is built on." },
  review: { title: "Review the Spec", icon: "doc", tone: "ready", why: "The result will be judged against these words." },
  revise: { title: "Change Requests", icon: "sparkle", tone: "needs_approval", why: "Revise the spec as you asked, then review it again." },
  yours: { title: "Yours to Do", icon: "hand", tone: "needs_approval", why: "A step only you can take — do it, give the agent a hint, or skip it." },
  claimed: { title: "Someone Else's Story", icon: "lock", tone: "needs_approval", why: "The next task belongs to a story another person claimed. Talk to them, or take it over." },
  autopilot_denied: { title: "Refused on Autopilot", icon: "shield", tone: "ready", why: "Calls the autopilot run would have asked you about were refused, not asked: look at each, and allow one when you run again." },
  gate_blocked: { title: "Blocked by a Gate", icon: "shield", tone: "failed", why: "Your gate did not let the tick through — its own words say why. Fix that, then run again." },
  hook_untrusted: { title: "Gates to Trust", icon: "shield", tone: "needs_approval", why: "A gate runs only after you trusted exactly what it runs; until then it blocks every tick." },
  extension_untrusted: { title: "Extensions to Trust", icon: "shield", tone: "needs_approval", why: "Nothing of an extension runs before you trusted one digest over every file it holds." },
  checkout_untrusted: { title: "Agent Files to Trust", icon: "shield", tone: "needs_approval", why: "What a checkout tells agents can steer them; agents read only the agent files you trusted, as you saw them." },
  stuck: { title: "Did Not Pass", icon: "xoctagon", tone: "failed", why: "Decide: run again, leave a note, skip or accept." },
  aside: { title: "Changes Set Aside", icon: "tray", tone: "needs_approval", why: "Found in the worktree when a run started — keep them on the branch, or let them go." },
  commit: { title: "Plans to Commit", icon: "doc", tone: "ready", why: "A run reads committed files only." },
  promote: { title: "Worth a Spec", icon: "sparkle", tone: "ready", why: "Measured, not guessed: it touched more than one requirement's worth. Start a spec from it — its tick is inherited, not verified — or not now." },
  request: { title: "Asked of This Project", icon: "tray", tone: "ready", why: "Another project you depend on is asked for something — take it up, record it as a bug, or decline it." },
  feedback: { title: "Feedback Answered", icon: "check", tone: "ready", why: "A project you asked something of fixed or declined it." },
  handoff: { title: "Ready to Hand Off", icon: "arrowup", tone: "verified", why: "Every task is done — review the branch." },
};

/** A kind's title; one the page does not know, by its own name. */
export const kindTitle = (kind: string) => KINDS[kind]?.title ?? kind.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

/** Who reads an agent file, as a person names it. */
export const READERS: Record<string, string> = { speccoder: "speccoder", claude: "Claude Code", codex: "Codex", gemini: "Gemini CLI", opencode: "OpenCode", copilot: "Copilot" };
