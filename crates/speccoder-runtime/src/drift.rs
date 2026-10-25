//! Drift: a tick is judged against the intent its commit holds; the
//! spec may have changed since. What changed, per task, from git alone: the
//! tick commit's tree then, the checkout's `HEAD` now — committed files
//! only, as every run reads them. Two batched `git cat-file` calls however
//! many tasks there are, plus one `ls-tree`: the report is read often.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use speccoder_core::{Drift, RequirementDrift, TaskStatus};

use crate::format::Adapter;
use crate::git::Git;
use crate::tools::paths::Class;
use crate::tools::policy::Policy;

/// Sets `drift` on every task with a commit whose intent differs from
/// `HEAD`'s. `feature_dir` is project-relative; `prefix` is the project's
/// path in the repository (`""` at the top, else ending in `/`). `false`, nothing marked:
/// `HEAD` does not hold the feature's intent (it is not in git, or not on
/// this branch), so git cannot say what the spec is now.
pub(crate) fn mark(
    root: &Path,
    format: &dyn Adapter,
    feature_dir: &str,
    prefix: &str,
    tasks: &mut [TaskStatus],
) -> bool {
    if tasks.iter().all(|t| t.commit.is_none()) {
        return true;
    }
    let git = Git::at(root);
    let now = intent_now(&git, format, feature_dir, prefix);
    let unit = format!("{}/", feature_dir.trim_end_matches('/'));
    if !now.keys().any(|p| p.starts_with(&unit)) {
        return false;
    }
    let commits: BTreeSet<&str> = tasks.iter().filter_map(|t| t.commit.as_deref()).collect();
    let then = intent_then(&git, &commits, &now, prefix);
    let spec = format.requirements_file().map(|f| format!("{unit}{f}"));
    // The requirements file's text, then and now, read once per blob.
    let mut blobs: BTreeSet<String> = BTreeSet::new();
    if let Some(spec) = &spec {
        blobs.extend(now.get(spec).cloned());
        for files in then.values() {
            blobs.extend(files.get(spec).cloned());
        }
    }
    let texts = contents(&git, &blobs);
    let requirements = |blob: Option<&String>| -> BTreeMap<String, String> {
        blob.and_then(|b| texts.get(b))
            .map(|t| format.requirements_in(t).into_iter().collect())
            .unwrap_or_default()
    };
    let now_reqs = requirements(spec.as_ref().and_then(|s| now.get(s)));
    for t in tasks.iter_mut() {
        let Some(files) = t.commit.as_deref().and_then(|c| then.get(c)) else {
            continue;
        };
        let changed: Vec<String> = now
            .iter()
            .filter(|(path, blob)| files.get(*path) != Some(*blob))
            .map(|(path, _)| path.clone())
            .collect();
        if changed.is_empty() {
            continue;
        }
        let then_reqs = requirements(spec.as_ref().and_then(|s| files.get(s)));
        let ids: BTreeSet<&String> = then_reqs.keys().chain(now_reqs.keys()).collect();
        let requirements: Vec<RequirementDrift> = ids
            .into_iter()
            .filter(|id| crate::attest::cites(&t.title, id))
            .filter(|id| {
                then_reqs.get(*id).map(|l| l.trim()) != now_reqs.get(*id).map(|l| l.trim())
            })
            .map(|id| RequirementDrift {
                id: id.clone(),
                then: then_reqs.get(id).cloned(),
                now: now_reqs.get(id).cloned(),
            })
            .collect();
        t.drift = Some(Drift {
            requirements,
            files: changed,
        });
    }
    true
}

/// The intent files at `HEAD` that speak for the feature (its own
/// documents and the format's shared ones, such as a constitution; not the
/// task list, not other units, not the current-unit pointer), each with its
/// blob, by project-relative path.
fn intent_now(
    git: &Git,
    format: &dyn Adapter,
    feature_dir: &str,
    prefix: &str,
) -> BTreeMap<String, String> {
    let Ok(out) = git.run(&["ls-tree", "-r", "-z", "--full-name", "HEAD", "--", "."]) else {
        return BTreeMap::new();
    };
    let rules = format.rules();
    let policy = Policy::new(&[], &[]).rules(rules);
    let tasks_file = format.messages().tasks_file;
    let pointer = format.current_pointer();
    let unit = format!("{}/", feature_dir.trim_end_matches('/'));
    let units = feature_dir
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| format!("{parent}/"));
    String::from_utf8_lossy(&out)
        .split('\0')
        .filter_map(|entry| {
            // `<mode> blob <oid>\t<path>`
            let (meta, path) = entry.split_once('\t')?;
            let blob = meta.split(' ').nth(2)?;
            let path = path.strip_prefix(prefix)?;
            let speaks = if path.starts_with(&unit) {
                path.rsplit('/').next() != Some(tasks_file)
            } else {
                units.as_deref().is_none_or(|u| !path.starts_with(u))
                    && (rules.authoring_path)(path)
                    && Some(path) != pointer
            };
            (speaks && policy.write_protected(path) == Some(Class::Intent))
                .then(|| (path.to_owned(), blob.to_owned()))
        })
        .collect()
}

/// For each commit, the blob of each of `now`'s paths in its tree (absent:
/// the file did not exist then).
fn intent_then(
    git: &Git,
    commits: &BTreeSet<&str>,
    now: &BTreeMap<String, String>,
    prefix: &str,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let queries: Vec<(&str, &str)> = commits
        .iter()
        .flat_map(|c| now.keys().map(move |p| (*c, p.as_str())))
        .collect();
    let input: String = queries
        .iter()
        .map(|(c, p)| format!("{c}:{prefix}{p}\n"))
        .collect();
    let out = git
        .run_with_stdin(
            &["cat-file", "--batch-check=%(objectname)"],
            input.as_bytes(),
        )
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut then: BTreeMap<String, BTreeMap<String, String>> = commits
        .iter()
        .map(|c| ((*c).to_owned(), BTreeMap::new()))
        .collect();
    for ((commit, path), line) in queries.iter().zip(out.lines()) {
        // A missing object prints `<query> missing`.
        if !line.ends_with(" missing")
            && let Some(files) = then.get_mut(*commit)
        {
            files.insert((*path).to_owned(), line.trim().to_owned());
        }
    }
    then
}

/// The text of each blob.
fn contents(git: &Git, blobs: &BTreeSet<String>) -> BTreeMap<String, String> {
    if blobs.is_empty() {
        return BTreeMap::new();
    }
    let input: String = blobs.iter().map(|b| format!("{b}\n")).collect();
    let Ok(out) = git.run_with_stdin(&["cat-file", "--batch"], input.as_bytes()) else {
        return BTreeMap::new();
    };
    // `<oid> blob <size>\n<content>\n`, one after another.
    let mut texts = BTreeMap::new();
    let mut rest = out.as_slice();
    while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
        let header = String::from_utf8_lossy(&rest[..nl]).into_owned();
        rest = &rest[nl + 1..];
        let mut fields = header.split(' ');
        let (Some(oid), Some("blob"), Some(size)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(size) = size.parse::<usize>() else {
            break;
        };
        let Some(body) = rest.get(..size) else {
            break;
        };
        texts.insert(oid.to_owned(), String::from_utf8_lossy(body).into_owned());
        rest = rest.get(size + 1..).unwrap_or_default();
    }
    texts
}
