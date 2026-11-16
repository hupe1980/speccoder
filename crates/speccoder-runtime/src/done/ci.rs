//! The CI line: what GitHub's checks say of this commit. Read only by
//! `speccoder done`; at `pr` the new head has no checks yet.

use std::path::Path;

use speccoder_core::done::{DoneFinding, DoneLine, DonePart, DoneState};

use crate::git::Git;
use crate::github::{self, CheckState};

/// The CI line for HEAD at `root`.
#[must_use]
pub fn line(root: &Path) -> DoneLine {
    let cant = |why: &str, fix: &str| {
        DoneLine::new(DonePart::Ci, DoneState::CantTell, why).with_fixes([fix.to_owned()])
    };
    let git = Git::at(root);
    let Ok(Some(sha)) = git.head() else {
        return cant("no commit yet", "git commit");
    };
    let repo = match github::repo(root, None) {
        Ok(r) => r,
        Err(e) => return cant(&e, "git remote add origin <GitHub repository>"),
    };
    let pushed = git
        .line(&["branch", "-r", "--contains", &sha])
        .is_ok_and(|b| !b.trim().is_empty());
    if !pushed {
        return cant("HEAD is not pushed: CI has not seen it", "git push");
    }
    match github::commit_checks(&github::Ctx::from_env(), &repo, &sha) {
        Err(e) => cant(&e, "speccoder auth login"),
        Ok(checks) => of(&checks),
    }
}

/// The line for these checks.
#[must_use]
pub fn of(checks: &[(String, CheckState)]) -> DoneLine {
    let n = checks.len();
    let named = |s: CheckState| -> Vec<&str> {
        checks
            .iter()
            .filter(|(_, c)| *c == s)
            .map(|(n, _)| n.as_str())
            .collect()
    };
    let (failed, running) = (named(CheckState::Failed), named(CheckState::Running));
    if n == 0 {
        DoneLine::new(
            DonePart::Ci,
            DoneState::CantTell,
            "no checks reported for HEAD yet",
        )
    } else if !failed.is_empty() {
        let findings = failed
            .iter()
            .map(|f| DoneFinding {
                path: (*f).to_owned(),
                line: None,
                what: "failed".to_owned(),
            })
            .collect();
        DoneLine::new(
            DonePart::Ci,
            DoneState::NotDone,
            format!("{} of {n} checks failed", failed.len()),
        )
        .with_findings(findings)
        .with_fixes([format!(
            "fix what {} reports, then push",
            failed.first().copied().unwrap_or("the check")
        )])
    } else if !running.is_empty() {
        DoneLine::new(
            DonePart::Ci,
            DoneState::CantTell,
            format!("{} of {n} checks still running", running.len()),
        )
    } else {
        DoneLine::new(
            DonePart::Ci,
            DoneState::Done,
            format!("all {n} checks passed"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn githubs_rollup_becomes_a_line_naming_what_failed() {
        let data = json!({"repository": {"object": {"statusCheckRollup": {"contexts": {"nodes": [
            {"__typename": "CheckRun", "name": "just ci (ubuntu)", "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"__typename": "CheckRun", "name": "sandbox", "status": "COMPLETED", "conclusion": "FAILURE"},
            {"__typename": "StatusContext", "context": "netlify", "state": "SUCCESS"}
        ]}}}}});
        let l = of(&github::checks_from(&data));
        assert_eq!(l.state, DoneState::NotDone);
        assert_eq!(l.findings[0].path, "sandbox");
        assert_eq!(l.summary, "1 of 3 checks failed");
        // Past the first 100, the rollup's state speaks for the rest.
        let many = json!({"repository": {"object": {"statusCheckRollup": {"state": "FAILURE", "contexts": {"totalCount": 101, "nodes": [
            {"__typename": "CheckRun", "name": "a", "status": "COMPLETED", "conclusion": "SUCCESS"}
        ]}}}}});
        let l = of(&github::checks_from(&many));
        assert_eq!(l.state, DoneState::NotDone, "{l:?}");
        let running = of(&[
            ("a".into(), CheckState::Passed),
            ("b".into(), CheckState::Running),
        ]);
        assert_eq!(running.state, DoneState::CantTell);
        assert_eq!(
            of(&[("a".into(), CheckState::Passed)]).state,
            DoneState::Done
        );
        assert_eq!(of(&[]).state, DoneState::CantTell);
    }
}
