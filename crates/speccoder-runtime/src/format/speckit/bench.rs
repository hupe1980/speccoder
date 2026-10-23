//! The Spec Kit project `speccoder bench import` writes around an issue.

use std::fs;
use std::path::{Path, PathBuf};

use crate::bench::Issue;

/// Writes a Spec Kit project into `project`: the issue as the user story of
/// `spec.md`, a plan naming the test command, and one task to resolve it.
pub fn scaffold(
    project: &Path,
    slug: &str,
    issue: &Issue<'_>,
    test_cmd: &[String],
) -> Result<(), String> {
    let feature = format!("001-{slug}");
    let fdir = project.join("specs").join(&feature);
    fs::create_dir_all(&fdir).map_err(|e| e.to_string())?;
    fs::create_dir_all(project.join(".specify/memory")).map_err(|e| e.to_string())?;
    let write = |p: PathBuf, text: String| fs::write(p, text).map_err(|e| e.to_string());
    write(
        project.join(".specify/init-options.json"),
        "{\"speckit_version\": \"1.2.3\", \"integration\": \"claude\", \"script\": \"sh\"}\n"
            .to_owned(),
    )?;
    write(
        project.join(".specify/memory/constitution.md"),
        "# Constitution\n\n### I. Minimal\nChange only what the issue needs; keep existing behaviour and tests.\n\n### II. Honest\nA requirement that cannot be met as written is reported, never worked around.\n".to_owned(),
    )?;
    let first_line = issue
        .statement
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Resolve the issue");
    let title: String = first_line
        .trim()
        .trim_start_matches('#')
        .trim()
        .chars()
        .take(70)
        .collect();
    write(
        fdir.join("spec.md"),
        format!(
            "# Feature Specification: {}\n\n## User Scenarios & Testing\n\n### User Story 1 - {title} (Priority: P1)\n\nAn issue reported against `{}`:\n\n{}\n\n## Requirements\n\n- **FR-001**: The behaviour the issue asks for works.\n- **FR-002**: Existing behaviour and tests are unchanged.\n",
            issue.id,
            issue.repo,
            issue.statement.trim()
        ),
    )?;
    write(
        fdir.join("plan.md"),
        format!(
            "# Implementation Plan: {}\n\nExisting repository `{}` at `{}`. Find the code the issue concerns and change it there; do not rewrite unrelated code. Tests: `{}`.\n",
            issue.id,
            issue.repo,
            issue.base_commit,
            test_cmd.join(" ")
        ),
    )?;
    write(
        fdir.join("tasks.md"),
        format!(
            "# Tasks: {}\n\n## Phase 1: User Story 1 - {title} (Priority: P1)\n\n- [ ] T001 [US1] Resolve the issue described in spec.md, with a test that fails before the change and passes after\n",
            issue.id
        ),
    )?;
    Ok(())
}
