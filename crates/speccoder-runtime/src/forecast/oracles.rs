//! The frozen oracles a forecast judges with, and their tests' runs on a
//! tree: in a scratch worktree at that tree, inside the OS sandbox, one
//! test command at a time, each result kept by tree and test digest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git::Git;
use crate::oracle::FrozenTest;
use crate::{sandbox, worktree};

/// Results kept per feature.
const KEPT: usize = 500;
/// Output kept per failing test.
const OUTPUT: usize = 1024;

/// One story's frozen oracle as the forecast uses it.
#[derive(Debug, Clone)]
pub(crate) struct Oracle {
    /// The unit.
    pub(crate) unit: String,
    /// The story (`US1`).
    pub(crate) story: String,
    /// The forecast's own unit.
    pub(crate) mine: bool,
    /// The tests (paths project-relative), with the scenarios each cites.
    pub(crate) tests: Vec<FrozenTest>,
    /// `given → must hold`, by scenario.
    pub(crate) contracts: BTreeMap<String, String>,
}

impl Oracle {
    /// Its scenarios, in the order its tests cite them.
    pub(crate) fn scenarios(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.tests
            .iter()
            .flat_map(|t| t.scenarios.iter())
            .filter(|s| seen.insert((*s).clone()))
            .cloned()
            .collect()
    }
}

/// A test's run on a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Outcome {
    pub(crate) passed: bool,
    /// Neither passed nor failed on an assertion or a build: it timed out,
    /// ran no test, ended unclassified. Decides nothing; never kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) undecided: Option<String>,
    /// Failed once, passed when run again.
    #[serde(default)]
    pub(crate) flaky: bool,
    /// The end of its output when it failed.
    #[serde(default)]
    pub(crate) output: String,
}

/// The results kept for a feature, oldest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Cache {
    #[serde(default)]
    pub(crate) results: BTreeMap<String, Outcome>,
    #[serde(default)]
    pub(crate) order: Vec<String>,
}

impl Cache {
    fn put(&mut self, key: String, o: Outcome) {
        if self.results.insert(key.clone(), o).is_none() {
            self.order.push(key);
        }
        while self.order.len() > KEPT {
            let old = self.order.remove(0);
            self.results.remove(&old);
        }
    }
}

/// A test's digest: its path, its frozen content and its command.
pub(crate) fn digest(t: &FrozenTest) -> String {
    crate::integrity::sha256_hex(
        format!("{}\0{}\0{}", t.path, t.sha256, t.argv.join("\0")).as_bytes(),
    )
}

/// The frozen oracles of `unit` this project's state holds whose
/// `Oracle:` commit is on `rev` and whose tests are on it byte for byte;
/// the others are named in `skipped`.
pub(crate) fn of_unit(
    plane: &Path,
    top: &Path,
    prefix: &str,
    unit: &str,
    rev: &str,
    mine: bool,
    skipped: &mut Vec<String>,
) -> Vec<Oracle> {
    let Ok(dirs) = std::fs::read_dir(plane.join(crate::oracle::DIR).join(unit)) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = dirs
        .flatten()
        .filter(|d| d.path().join("frozen.json").is_file())
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .collect();
    keys.sort();
    let git = Git::at(top);
    let mut out = Vec::new();
    for key in keys {
        let Some(frozen) = crate::oracle::load_frozen(plane, unit, &key) else {
            continue;
        };
        let on_branch = git
            .line(&[
                "log",
                "-n1",
                "--format=%H",
                &format!("--grep=^{}: {key}$", crate::pipeline::story::ORACLE),
                rev,
                "--",
            ])
            .is_ok_and(|c| !c.is_empty());
        if !on_branch || frozen.tests.is_empty() {
            continue;
        }
        if let Some(t) = frozen
            .tests
            .iter()
            .find(|t| blob_sha(top, rev, prefix, &t.path).as_deref() != Some(t.sha256.as_str()))
        {
            skipped.push(format!(
                "{unit}'s {key} oracle: {} on its branch is not the frozen test",
                t.path
            ));
            continue;
        }
        out.push(Oracle {
            unit: unit.to_owned(),
            story: key.clone(),
            mine,
            tests: frozen.tests,
            contracts: crate::pipeline::story::contracts_at(top, rev, &key),
        });
    }
    out
}

/// `sha256:<hex>` of a project file at a tree or commit, `None` when absent.
pub(crate) fn blob_sha(top: &Path, rev: &str, prefix: &str, path: &str) -> Option<String> {
    let spec = format!("{rev}:{prefix}{path}");
    Git::at(top)
        .run(&["cat-file", "blob", &spec])
        .ok()
        .map(|b| format!("sha256:{}", crate::integrity::sha256_hex(&b)))
}

/// Where tests run: what a scratch worktree needs.
pub(crate) struct Bench<'a> {
    pub(crate) top: &'a Path,
    pub(crate) prefix: &'a str,
    /// What else decides an outcome (the project's `[sandbox]` and
    /// `[worktree]`): part of every result's key.
    pub(crate) salt: String,
    pub(crate) tmp: PathBuf,
    pub(crate) state: &'a Path,
    pub(crate) sandbox: &'a sandbox::Config,
    pub(crate) worktree: &'a crate::prepare::Config,
}

/// Why tests did not run on a tree.
#[derive(Debug)]
pub(crate) enum Failed {
    /// The tree does not set up: its dependencies do not install, its
    /// lockfile disagrees.
    Setup(String),
    /// Anything else (no scratch worktree, no sandbox).
    Other(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup(m) | Self::Other(m) => f.write_str(m),
        }
    }
}

impl From<String> for Failed {
    fn from(m: String) -> Self {
        Self::Other(m)
    }
}

/// Counts of what ran and what was reused.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    pub(crate) ran: usize,
    pub(crate) reused: usize,
}

impl Bench<'_> {
    /// Each test's outcome on `tree` (a tree object; checked out over the
    /// commit `base`), by test path. A failure runs once more: passing then,
    /// it is flaky. Uncached tests run in one scratch worktree; none
    /// uncached, none made. No untracked file of the checkout is copied in:
    /// the tree may hold another person's code.
    pub(crate) fn run(
        &self,
        base: &str,
        tree: &str,
        tests: &[&FrozenTest],
        cache: &mut Cache,
        tally: &mut Tally,
    ) -> Result<BTreeMap<String, Outcome>, Failed> {
        let key = |t: &FrozenTest| format!("{tree}:{}:{}", self.salt, digest(t));
        let mut out = BTreeMap::new();
        let mut todo = Vec::new();
        for t in tests {
            match cache.results.get(&key(t)) {
                Some(o) => {
                    tally.reused += 1;
                    out.insert(t.path.clone(), o.clone());
                }
                None => todo.push(*t),
            }
        }
        if todo.is_empty() {
            return Ok(out);
        }
        let scratch = worktree::scratch_at(self.top, &self.tmp, base)
            .map_err(|e| format!("the scratch worktree could not be made: {}", e.0))?;
        let base_tree = Git::at(self.top)
            .line(&["rev-parse", &format!("{base}^{{tree}}")])
            .unwrap_or_default();
        if base_tree != tree {
            Git::at(&scratch.path)
                .run(&["read-tree", "-m", "-u", "HEAD", tree])
                .map_err(|e| format!("the merged tree could not be checked out: {}", e.0))?;
        }
        let project = if self.prefix.is_empty() {
            scratch.path.clone()
        } else {
            scratch.path.join(self.prefix.trim_end_matches('/'))
        };
        let (runner, notice) = sandbox::runner(&scratch.path, self.sandbox)?;
        if !runner.is_sandboxed() {
            return Err(Failed::Other(
                notice.unwrap_or_else(|| "commands would run unsandboxed".to_owned()),
            ));
        }
        if self.worktree.prepare && crate::prepare::needed(&project) {
            crate::prepare::prepare(&crate::prepare::Prepare {
                worktree: &scratch.path,
                project: &project,
                config: self.worktree,
                runner: &runner,
                state: Some(self.state),
                shared_caches: false,
            })
            .map_err(|f| {
                // Paths inside the scratch worktree, as the project names them.
                let mut m = f.message;
                for root in [
                    Some(scratch.path.clone()),
                    std::fs::canonicalize(&scratch.path).ok(),
                ]
                .into_iter()
                .flatten()
                {
                    m = m.replace(&format!("{}/", root.display()), "");
                }
                Failed::Setup(m)
            })?;
        }
        for t in todo {
            use speccoder_core::verify::RedClass;
            let once = || {
                let (r, summary) = crate::pipeline::acceptance_run(&project, &t.argv, &runner);
                let class = speccoder_core::verify::classify_red(&r, summary.as_ref());
                let passed = r.status == speccoder_core::verify::CheckStatus::Passed
                    && class == RedClass::Passed;
                // A test that ran and exited non-zero failed; one that timed
                // out, collected nothing or never exited decides nothing.
                let undecided = match class {
                    _ if passed => None,
                    RedClass::Timeout => Some("timed out"),
                    RedClass::NoTests => Some("ran no test"),
                    _ if r.exit_code.is_none() => Some("ended without an exit code"),
                    _ => None,
                };
                (
                    passed,
                    undecided,
                    format!("{}\n{}", r.stdout_tail, r.stderr_tail),
                )
            };
            tally.ran += 1;
            let (first, first_undecided, text) = once();
            let o = if first {
                Outcome {
                    passed: true,
                    undecided: None,
                    flaky: false,
                    output: String::new(),
                }
            } else {
                tally.ran += 1;
                let (second, second_undecided, _) = once();
                Outcome {
                    passed: second,
                    undecided: if second {
                        None
                    } else {
                        first_undecided.or(second_undecided).map(str::to_owned)
                    },
                    flaky: second,
                    output: if second { String::new() } else { tail(&text) },
                }
            };
            if o.undecided.is_none() {
                cache.put(key(t), o.clone());
            }
            out.insert(t.path.clone(), o);
        }
        Ok(out)
    }
}

fn tail(text: &str) -> String {
    let t = text.trim();
    if t.len() <= OUTPUT {
        return t.to_owned();
    }
    let mut start = t.len() - OUTPUT;
    while !t.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &t[start..])
}

/// Scenarios green on a run: every test citing it passed.
pub(crate) fn green(oracle: &Oracle, runs: &BTreeMap<String, Outcome>) -> BTreeSet<String> {
    oracle
        .scenarios()
        .into_iter()
        .filter(|s| {
            oracle
                .tests
                .iter()
                .filter(|t| t.scenarios.contains(s))
                .all(|t| runs.get(&t.path).is_some_and(|o| o.passed))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(path: &str, scenarios: &[&str]) -> FrozenTest {
        FrozenTest {
            path: path.into(),
            sha256: "sha256:x".into(),
            argv: vec!["sh".into(), path.into()],
            scenarios: scenarios.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn a_scenario_is_green_only_when_every_test_citing_it_passed() {
        let o = Oracle {
            unit: "001".into(),
            story: "US1".into(),
            mine: true,
            tests: vec![test("a", &["US1-1", "US1-2"]), test("b", &["US1-2"])],
            contracts: BTreeMap::new(),
        };
        let pass = |p: bool| Outcome {
            passed: p,
            undecided: None,
            flaky: false,
            output: String::new(),
        };
        let runs = BTreeMap::from([("a".to_owned(), pass(true)), ("b".to_owned(), pass(false))]);
        assert_eq!(green(&o, &runs), BTreeSet::from(["US1-1".to_owned()]));
        assert_eq!(o.scenarios(), ["US1-1", "US1-2"]);
    }

    #[test]
    fn the_cache_keeps_the_newest_results_and_a_test_changes_its_digest() {
        let mut c = Cache::default();
        for i in 0..KEPT + 3 {
            c.put(
                format!("k{i}"),
                Outcome {
                    passed: true,
                    undecided: None,
                    flaky: false,
                    output: String::new(),
                },
            );
        }
        assert_eq!(c.results.len(), KEPT);
        assert!(!c.results.contains_key("k0"));
        let a = test("a", &[]);
        let mut b = a.clone();
        b.argv.push("-v".into());
        assert_ne!(digest(&a), digest(&b));
    }

    #[test]
    fn output_is_cut_from_the_end_on_a_char_boundary() {
        let long = "é".repeat(OUTPUT);
        let t = tail(&long);
        assert!(t.starts_with('…') && t.len() <= OUTPUT + 4);
    }
}
