//! Best-of-n: choosing among independent samples by evidence.

use crate::worktree;

/// One sample's evidence: verdict, surviving mutants, diff size, tokens.
pub(crate) fn sample_summary(
    report: &speccoder_core::RunReport,
    status: &str,
    patch: &str,
) -> serde_json::Value {
    use speccoder_core::verify::Verdict;
    let rank = match (status, &report.verdict) {
        ("completed", Some(Verdict::Verified)) => 2,
        ("completed", Some(Verdict::RegressionOnly)) => 1,
        _ => 0,
    };
    let changed = patch
        .lines()
        .filter(|l| {
            (l.starts_with('+') && !l.starts_with("+++"))
                || (l.starts_with('-') && !l.starts_with("---"))
        })
        .count();
    let tokens = report.usage.as_ref().map_or(0, |u| {
        u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0)
    });
    serde_json::json!({
        "run_id": report.run_id,
        "attempt_status": status,
        "verdict": report.verdict,
        "rank": rank,
        "survivors": report.mutation.as_ref().and_then(|m| m["missed"].as_u64()),
        "changed_lines": changed,
        "tokens": tokens,
    })
}

/// The best sample: passing first (verified over regression-only), then
/// fewer surviving mutants, a smaller diff, fewer tokens; ties keep the earlier.
pub(crate) fn best_sample(
    samples: &[(
        speccoder_core::RunReport,
        String,
        worktree::Checkpoint,
        serde_json::Value,
    )],
) -> usize {
    let key = |s: &serde_json::Value| {
        (
            std::cmp::Reverse(s["rank"].as_u64().unwrap_or(0)),
            s["survivors"].as_u64().unwrap_or(0),
            s["changed_lines"].as_u64().unwrap_or(0),
            s["tokens"].as_u64().unwrap_or(0),
        )
    };
    (0..samples.len())
        .min_by_key(|&i| (key(&samples[i].3), i))
        .unwrap_or(0)
}
