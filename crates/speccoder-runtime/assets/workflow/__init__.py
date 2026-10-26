"""speccoder as a Spec Kit workflow step type.

Written by `speccoder workflow install`. Runs `speccoder --json run` (or
`speccoder spec <skill>`) in the project and maps speccoder's exit code onto
the step's status:

- 0  -> completed
- 23 -> paused: speccoder stopped for a person (an approval, a blocked or
        undemonstrated task, a suspected shortcut). Decide with
        `speccoder inbox`, `speccoder task approve|nudge|accept`, then
        `specify workflow resume <run>` runs this step again.
- anything else -> failed, with speccoder's message.

Config:
  skill:     a Spec Kit skill to host (`specify`, `plan`, `tasks`, ...) instead of `run`
  args:      the skill's arguments (expressions like "{{ inputs.spec }}" allowed)
  converge:  true -> `run --converge`
  feature:   the feature (name or number)
  max_tasks: at most this many tasks
  timeout:   seconds (default: no limit; a feature run takes as long as it takes)
  executable: path to speccoder (default: `speccoder` on PATH)
  bug:       a bug's slug (Spec Kit's `bug` extension): `speccoder bug run --slug`
             — reproduce, the person's confirmation (paused), the fix
"""

from __future__ import annotations

import json
import math
import shutil
import subprocess
from typing import Any

from specify_cli.workflows.base import StepBase, StepContext, StepResult, StepStatus
from specify_cli.workflows.expressions import evaluate_expression

NEEDS_HUMAN = 23


def _value(raw: Any, context: StepContext) -> Any:
    if isinstance(raw, str) and "{{" in raw:
        return evaluate_expression(raw, context)
    return raw


class SpeccoderStep(StepBase):
    """Run speccoder (implement with evidence, or host a planning skill)."""

    type_key = "speccoder"

    def argv(self, config: dict[str, Any], context: StepContext) -> list[str]:
        exe = config.get("executable") or shutil.which("speccoder") or "speccoder"
        root = context.project_root or "."
        bug = _value(config.get("bug"), context)
        if bug:
            return [exe, "-C", root, "--json", "bug", "run", "--no-prompt", "--slug", str(bug)]
        skill = config.get("skill")
        if skill:
            args = _value(config.get("args", ""), context)
            argv = [exe, "-C", root, "--json", "spec", str(skill), "--no-prompt"]
            if args:
                argv.append(str(args))
            return argv
        argv = [exe, "-C", root, "--json", "run", "--no-prompt"]
        if config.get("converge") is True:
            argv.append("--converge")
        feature = _value(config.get("feature"), context)
        if feature:
            argv += ["--feature", str(feature)]
        if config.get("max_tasks"):
            argv += ["--max-tasks", str(int(config["max_tasks"]))]
        return argv

    def execute(self, config: dict[str, Any], context: StepContext) -> StepResult:
        argv = self.argv(config, context)
        try:
            proc = subprocess.run(  # noqa: S603 -- an argv, no shell
                argv,
                capture_output=True,
                text=True,
                cwd=context.project_root or ".",
                timeout=config.get("timeout"),
            )
        except FileNotFoundError:
            return StepResult(
                status=StepStatus.FAILED,
                error=f"speccoder not found ({argv[0]}); install it or set `executable`",
            )
        except subprocess.TimeoutExpired:
            return StepResult(
                status=StepStatus.FAILED,
                error=f"speccoder did not finish within {config.get('timeout')} s",
            )
        try:
            result = json.loads(proc.stdout)
        except json.JSONDecodeError:
            result = {"message": (proc.stdout or proc.stderr)[-2000:]}
        output = {
            "exit_code": proc.returncode,
            "status": result.get("status"),
            "message": result.get("message"),
            "data": result.get("data"),
        }
        if proc.returncode == 0:
            return StepResult(status=StepStatus.COMPLETED, output=output)
        data = result.get("data") or {}
        if (
            proc.returncode == NEEDS_HUMAN
            and config.get("skill")
            and data.get("status") == "incomplete"
            and data.get("changes")
        ):
            # A planning skill that wrote its artifacts but left questions
            # open: Spec Kit's own flow continues to the review gate, where
            # the person sees them (they are in the output and the spec).
            return StepResult(status=StepStatus.COMPLETED, output=output)
        if proc.returncode == NEEDS_HUMAN:
            return StepResult(
                status=StepStatus.PAUSED,
                output=output,
                error=f"speccoder needs a person: {result.get('message')} -- "
                "decide (speccoder inbox; speccoder task approve / nudge / accept), "
                "then `specify workflow resume`",
            )
        return StepResult(
            status=StepStatus.FAILED,
            output=output,
            error=f"speccoder exited {proc.returncode}: {result.get('message')}",
        )

    def validate(self, config: dict[str, Any]) -> list[str]:
        errors = super().validate(config)
        sid = config.get("id", "?")
        if "converge" in config and not isinstance(config["converge"], bool):
            errors.append(f"speccoder step {sid!r}: 'converge' must be true or false")
        if "max_tasks" in config and (
            isinstance(config["max_tasks"], bool) or not isinstance(config["max_tasks"], int)
            or config["max_tasks"] < 1
        ):
            errors.append(f"speccoder step {sid!r}: 'max_tasks' must be a positive integer")
        if "timeout" in config:
            t = config["timeout"]
            if isinstance(t, bool) or not isinstance(t, (int, float)) or not math.isfinite(t) or t <= 0:
                errors.append(f"speccoder step {sid!r}: 'timeout' must be a positive number of seconds")
        if "bug" in config and not isinstance(config["bug"], str):
            errors.append(f"speccoder step {sid!r}: 'bug' must be a slug (or an expression)")
        if "skill" in config and not isinstance(config["skill"], str):
            errors.append(f"speccoder step {sid!r}: 'skill' must be a string")
        return errors
