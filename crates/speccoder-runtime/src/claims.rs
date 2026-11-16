//! Story claims kept as refs on the shared remote. Every change
//! is a compare-and-swap: a push with `--force-with-lease=<ref>:<expected>`
//! (an empty expectation: the ref must not exist yet), or `update-ref`
//! with the old value when there is no remote. A lost race names the winner.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use speccoder_core::claims::{
    self as core, Claim, ClaimChange, ClaimState, ClaimVerb, ClaimView, ClaimsReport,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::git::Git;

const COMMAND: &str = "claim";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new(COMMAND, status, message).with_code(code)
}

/// Never a prompt: a key with a passphrase or an unknown host fails.
fn ssh() -> std::ffi::OsString {
    std::env::var_os("GIT_SSH_COMMAND").unwrap_or_else(|| "ssh -o BatchMode=yes".into())
}

/// Who the person is to git.
#[derive(Debug, Clone)]
pub(crate) struct Me {
    name: String,
    email: String,
}

fn me(git: &Git) -> Option<Me> {
    let email = git
        .line(&["config", "--get", "user.email"])
        .ok()
        .filter(|e| !e.is_empty())?;
    let name = git
        .line(&["config", "--get", "user.name"])
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or(&email).to_owned());
    Some(Me { name, email })
}

/// Where a repository's claims live.
pub(crate) struct Store {
    git: Git,
    /// The shared remote; `None`: this repository only.
    remote: Option<String>,
}

/// One story's ref, as read: its value, and the claim when it reads as one.
type Read = BTreeMap<String, (String, Option<Claim>)>;

/// Why a change did not land.
enum Put {
    /// Someone changed the ref since it was read.
    Lost,
    /// git or the remote failed.
    Failed(String),
}

impl Store {
    pub(crate) fn at(dir: &Path) -> Option<Self> {
        let git = Git::at(dir);
        let top = git.toplevel().ok().flatten()?;
        let git = Git::at(&top);
        let remotes: Vec<String> = git
            .line(&["remote"])
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        let remote = if remotes.iter().any(|r| r == "origin") {
            Some("origin".to_owned())
        } else if remotes.len() == 1 {
            remotes.into_iter().next()
        } else {
            None
        };
        Some(Self { git, remote })
    }

    fn namespace(feature: &str) -> String {
        format!("{}/{feature}/", core::NAMESPACE)
    }

    /// The claims of `feature` as this repository holds them.
    fn local(&self, feature: &str) -> Read {
        let out = self
            .git
            .run(&[
                "for-each-ref",
                "--format=%(objectname) %(refname)%00%(contents)%00",
                &Self::namespace(feature),
            ])
            .unwrap_or_default();
        let text = String::from_utf8_lossy(&out);
        let mut fields = text.split('\0');
        let mut read = Read::new();
        while let (Some(head), Some(body)) = (fields.next(), fields.next()) {
            let head = head.trim_start_matches('\n');
            let Some((id, name)) = head.split_once(' ') else {
                continue;
            };
            let Some(story) = name.strip_prefix(&Self::namespace(feature)) else {
                continue;
            };
            read.insert(
                story.to_owned(),
                (id.to_owned(), Claim::parse(feature, story, body)),
            );
        }
        read
    }

    /// The claims of `feature` as the remote holds them now (one fetch,
    /// which also refreshes this repository's copy); without a remote, the
    /// repository's own.
    fn live(&self, feature: &str) -> Result<Read, String> {
        if let Some(r) = &self.remote {
            let ns = Self::namespace(feature);
            let spec = format!("+{ns}*:{ns}*");
            let ssh = ssh();
            // Read where claims are written: a remote with its own push
            // address keeps them there.
            let push = self.git.line(&["remote", "get-url", "--push", r]).ok();
            let fetch = self.git.line(&["remote", "get-url", r]).ok();
            let from = match (push, fetch) {
                (Some(p), Some(f)) if !p.is_empty() && p != f => p,
                _ => r.clone(),
            };
            self.git
                .quiet(
                    &["fetch", "--prune", "--no-tags", "--quiet", &from, &spec],
                    &[("GIT_SSH_COMMAND", ssh.as_os_str() as &OsStr)],
                )
                .map_err(|e| {
                    e.0.lines()
                        .rfind(|l| !l.trim().is_empty())
                        .unwrap_or("git fetch failed")
                        .trim_start_matches("fatal: ")
                        .to_owned()
                })?;
        }
        Ok(self.local(feature))
    }

    /// The claims refspec on the remote, once; the notice when added.
    fn refspec(&self) -> Option<String> {
        let r = self.remote.as_ref()?;
        let spec = format!("+{0}/*:{0}/*", core::NAMESPACE);
        let key = format!("remote.{r}.fetch");
        let have = self
            .git
            .line(&["config", "--get-all", &key])
            .unwrap_or_default();
        if have.lines().any(|l| l.trim() == spec) {
            return None;
        }
        self.git.run(&["config", "--add", &key, &spec]).ok()?;
        Some(format!(
            "claims now come with every `git fetch` from {r} ({key})"
        ))
    }

    /// Writes `claim` (or removes the ref, `None`) where the ref still is
    /// `expected` (`None`: must not exist).
    fn put(
        &self,
        feature: &str,
        story: &str,
        expected: Option<&str>,
        claim: Option<&Claim>,
    ) -> Result<(), Put> {
        let name = core::ref_of(feature, story);
        let id = match claim {
            Some(c) => {
                let tree = self
                    .git
                    .run_with_stdin(&["mktree"], b"")
                    .map(|o| String::from_utf8_lossy(&o).trim().to_owned())
                    .map_err(|e| Put::Failed(e.0))?;
                Some(
                    self.git
                        .run_with_stdin(&["commit-tree", &tree, "-F", "-"], c.message().as_bytes())
                        .map(|o| String::from_utf8_lossy(&o).trim().to_owned())
                        .map_err(|e| Put::Failed(e.0))?,
                )
            }
            None => None,
        };
        let local = |git: &Git| -> Result<(), Put> {
            let old = expected.unwrap_or("");
            let args: Vec<&str> = match &id {
                Some(new) => vec!["update-ref", &name, new, old],
                None => vec!["update-ref", "-d", &name, old],
            };
            match git.outcome(&args) {
                Ok((Some(0), _, _)) => Ok(()),
                Ok((_, _, err))
                    if err.contains("expected")
                        || err.contains("exists")
                        || err.contains("unable to resolve") =>
                {
                    Err(Put::Lost)
                }
                Ok((_, _, err)) => Err(Put::Failed(err)),
                Err(e) => Err(Put::Failed(e.0)),
            }
        };
        let Some(r) = &self.remote else {
            return local(&self.git);
        };
        let lease = format!("--force-with-lease={name}:{}", expected.unwrap_or(""));
        let target = match &id {
            Some(new) => format!("{new}:{name}"),
            None => format!(":{name}"),
        };
        let ssh = ssh();
        match self.git.quiet(
            &["push", "--quiet", "--no-verify", &lease, r, &target],
            &[("GIT_SSH_COMMAND", ssh.as_os_str() as &OsStr)],
        ) {
            Ok(_) => {
                // This repository's copy follows; a lost local update is
                // fixed by the next fetch.
                let _ = match &id {
                    Some(new) => self.git.run(&["update-ref", &name, new]),
                    None => self.git.run(&["update-ref", "-d", &name]),
                };
                Ok(())
            }
            Err(e) => {
                // Lost when the ref moved away from what was expected —
                // whatever words git or the remote chose for it.
                let now = self
                    .live(feature)
                    .ok()
                    .map(|r| r.get(story).map(|(id, _)| id.clone()));
                match now {
                    Some(now) if now.as_deref() != expected => Err(Put::Lost),
                    _ => Err(Put::Failed(
                        e.0.lines()
                            .rfind(|l| !l.trim().is_empty())
                            .unwrap_or("git push failed")
                            .trim_start_matches("fatal: ")
                            .trim_start_matches("error: ")
                            .to_owned(),
                    )),
                }
            }
        }
    }

    /// The base a done claim's commit is merged into.
    fn merged(&self, commit: &str) -> bool {
        crate::forecast::base_of(&self.git).is_some_and(|base| {
            self.git
                .succeeds(&["merge-base", "--is-ancestor", commit, &base])
                .unwrap_or(false)
        })
    }

    /// Removes done claims whose work reached the base; the stories removed.
    fn prune(&self, feature: &str, read: &mut Read) -> Vec<String> {
        let gone: Vec<(String, String)> = read
            .iter()
            .filter_map(|(story, (id, c))| {
                let done = c.as_ref()?.done.as_deref()?;
                self.merged(done).then(|| (story.clone(), id.clone()))
            })
            .collect();
        gone.into_iter()
            .filter(|(story, id)| self.put(feature, story, Some(id), None).is_ok())
            .map(|(story, _)| {
                read.remove(&story);
                story
            })
            .collect()
    }
}

fn views(read: &Read, me: Option<&str>, at: u64) -> Vec<ClaimView> {
    read.iter()
        .map(|(story, (_, c))| ClaimView {
            story: story.clone(),
            state: c
                .as_ref()
                .map_or(ClaimState::Unreadable, |c| c.state(me, at)),
            claim: c.clone(),
        })
        .collect()
}

/// What `claim` is asked to do.
#[derive(Debug, Clone, Default)]
pub struct Options<'a> {
    /// The feature (name or number).
    pub feature: Option<&'a str>,
    /// Stories to claim or release; none lists.
    pub stories: &'a [String],
    /// Release instead of claim.
    pub release: bool,
    /// Take over, or remove, another person's claim.
    pub force: bool,
}

/// The feature's root, name, and the stories its tasks name.
fn feature_of(
    path: &Path,
    feature: Option<&str>,
) -> Result<(PathBuf, String, Vec<String>), Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(s)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried(COMMAND, &inspected)));
    };
    let Some(f) = s.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            COMMAND,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let mut stories: Vec<String> = f
        .tasks
        .iter()
        .flat_map(|t| &t.tasks)
        .filter_map(|t| t.story.clone())
        .collect();
    stories.sort_by_key(|s| {
        s.trim_start_matches("US")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    stories.dedup();
    Ok((PathBuf::from(&s.root), f.feature.name.clone(), stories))
}

fn report(store: &Store, feature: &str, me: Option<&Me>, read: &Read, live: bool) -> ClaimsReport {
    let at = now();
    ClaimsReport {
        feature: feature.to_owned(),
        remote: store.remote.clone(),
        live,
        me: me.map(|m| m.email.clone()),
        claims: views(read, me.map(|m| m.email.as_str()), at),
        changed: Vec::new(),
        notices: Vec::new(),
        at,
    }
}

/// The feature's claims as this repository last fetched them: no network
/// (the web app's view).
#[must_use]
pub fn local(path: &Path, feature: Option<&str>) -> CommandResult {
    let (root, feature, _) = match feature_of(path, feature) {
        Ok(f) => f,
        Err(r) => return *r,
    };
    let Some(store) = Store::at(&root) else {
        return refuse(
            CommandStatus::GitPrecondition,
            "NO-REPOSITORY",
            "not a git repository",
        );
    };
    let read = store.local(&feature);
    let r = report(&store, &feature, me(&store.git).as_ref(), &read, false);
    let message = summary(&r);
    CommandResult::new(COMMAND, CommandStatus::Ok, &message)
        .with_data(CommandData::Claims(Box::new(r)))
}

fn summary(r: &ClaimsReport) -> String {
    // One line per holder: "Dana has US1, US2".
    let mut by: Vec<(&str, Vec<&str>)> = Vec::new();
    for c in r.others() {
        match by.iter_mut().find(|(h, _)| *h == c.holder) {
            Some((_, s)) => s.push(&c.story),
            None => by.push((&c.holder, vec![&c.story])),
        }
    }
    let others: Vec<String> = by
        .into_iter()
        .map(|(h, s)| format!("{h} has {}", s.join(", ")))
        .collect();
    let mine: Vec<&str> = r
        .claims
        .iter()
        .filter(|c| c.state == ClaimState::Mine)
        .map(|c| c.story.as_str())
        .collect();
    let mut parts = Vec::new();
    if !mine.is_empty() {
        parts.push(format!("you have {}", mine.join(", ")));
    }
    parts.extend(others);
    if parts.is_empty() {
        format!("no story of {} is claimed", r.feature)
    } else {
        parts.join(" · ")
    }
}

/// `speccoder claim`: list, claim, release or take over.
#[must_use]
pub fn claim(path: &Path, opts: &Options<'_>) -> CommandResult {
    let (root, feature, known) = match feature_of(path, opts.feature) {
        Ok(f) => f,
        Err(r) => return *r,
    };
    // A story the feature has: `US<n>`, one its tasks name when they name any.
    let wanted: Vec<String> = opts
        .stories
        .iter()
        .map(|s| s.trim().to_uppercase())
        .collect();
    if let Some(bad) = wanted.iter().find(|s| {
        let numbered = s
            .strip_prefix("US")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        !numbered || (!known.is_empty() && !known.contains(s))
    }) {
        let have = if known.is_empty() {
            "a story is named US1, US2, …".to_owned()
        } else {
            format!("its stories: {}", known.join(", "))
        };
        return CommandResult::new(
            COMMAND,
            CommandStatus::UsageError,
            &format!("{feature} has no story {bad}; {have}"),
        );
    }
    let Some(store) = Store::at(&root) else {
        return refuse(
            CommandStatus::GitPrecondition,
            "NO-REPOSITORY",
            "not a git repository",
        );
    };
    let who = me(&store.git);
    let mut read = match store.live(&feature) {
        Ok(r) => r,
        Err(why) => {
            return refuse(
                CommandStatus::Error,
                "CLAIM-UNREACHABLE",
                &format!(
                    "cannot read claims from {}: {why}",
                    store.remote.as_deref().unwrap_or("this repository")
                ),
            );
        }
    };
    let mut notices = Vec::new();
    let pruned = store.prune(&feature, &mut read);
    if !pruned.is_empty() {
        notices.push(format!(
            "{} merged: {} claim removed",
            pruned.join(", "),
            if pruned.len() == 1 { "its" } else { "their" }
        ));
    }
    let mut changed = Vec::new();
    let mut refused: Option<CommandResult> = None;
    if !opts.stories.is_empty() {
        let Some(me) = who.clone() else {
            return refuse(
                CommandStatus::ConfigError,
                "CLAIM-NO-IDENTITY",
                "git does not know who you are: a claim names its holder by user.email",
            )
            .with_fix(vec!["git config user.email you@example.com".to_owned()]);
        };
        notices.extend(store.refspec());
        let branch = format!("speccoder/{feature}");
        for story in &wanted {
            let story = story.clone();
            let outcome = if opts.release {
                release(&store, &feature, &story, &me, &read, opts.force)
            } else {
                take(&store, &feature, &story, &me, &branch, &read, opts.force)
            };
            match outcome {
                Ok(c) => changed.extend(c),
                Err(r) => {
                    refused.get_or_insert(*r);
                }
            }
        }
        read = store.local(&feature);
    }
    let mut r = report(&store, &feature, who.as_ref(), &read, true);
    r.changed = changed;
    r.notices = notices;
    if let Some(stop) = refused {
        return stop.with_data(CommandData::Claims(Box::new(r)));
    }
    let message = match r.changed.as_slice() {
        [] => summary(&r),
        c => c
            .iter()
            .map(|c| change_line(c, &r))
            .collect::<Vec<_>>()
            .join("; "),
    };
    CommandResult::new(COMMAND, CommandStatus::Ok, &message)
        .with_data(CommandData::Claims(Box::new(r)))
}

fn change_line(c: &ClaimChange, r: &ClaimsReport) -> String {
    let until = r
        .claims
        .iter()
        .find(|v| v.story == c.story)
        .and_then(|v| v.claim.as_ref())
        .map(|x| {
            format!(
                " for {}",
                core::relative(x.until, r.at).trim_start_matches("in ")
            )
        })
        .unwrap_or_default();
    let from = c.from.as_deref().unwrap_or("someone");
    match c.verb {
        ClaimVerb::Claimed => format!("{} is yours{until}", c.story),
        ClaimVerb::Renewed => format!("{} stays yours{until}", c.story),
        ClaimVerb::Took if c.stale => {
            format!("{} is yours{until}, after {from}'s claim ran out", c.story)
        }
        ClaimVerb::Took => format!("took {} over from {from}{until}", c.story),
        ClaimVerb::Released => format!("released {}", c.story),
        ClaimVerb::Removed => format!("removed {from}'s claim on {}", c.story),
    }
}

fn held(feature: &str, story: &str, c: &Claim) -> Box<CommandResult> {
    let fix = vec![
        format!("talk to {} <{}>", c.holder, c.email),
        format!("speccoder claim {story} --force --feature {feature}"),
    ];
    Box::new(
        refuse(
            CommandStatus::NeedsHuman,
            "CLAIMED",
            &core::held_line(c, now()),
        )
        .with_fix(fix),
    )
}

/// Claims `story` (or renews the person's own); another's live or done
/// claim only with `force`.
fn take(
    store: &Store,
    feature: &str,
    story: &str,
    me: &Me,
    branch: &str,
    read: &Read,
    force: bool,
) -> Result<Option<ClaimChange>, Box<CommandResult>> {
    take_once(store, feature, story, me, branch, read, force, true)
}

#[allow(clippy::too_many_arguments)]
fn take_once(
    store: &Store,
    feature: &str,
    story: &str,
    me: &Me,
    branch: &str,
    read: &Read,
    force: bool,
    retry: bool,
) -> Result<Option<ClaimChange>, Box<CommandResult>> {
    let at = now();
    let current = read.get(story);
    let state = current.map(|(_, c)| {
        c.as_ref()
            .map_or(ClaimState::Unreadable, |c| c.state(Some(&me.email), at))
    });
    if let (Some(ClaimState::Theirs | ClaimState::Done), Some((_, Some(c)))) = (state, current)
        && !force
    {
        return Err(held(feature, story, c));
    }
    let mine = state == Some(ClaimState::Mine);
    let previous = current.and_then(|(_, c)| c.as_ref());
    let claim = Claim {
        feature: feature.to_owned(),
        story: story.to_owned(),
        holder: me.name.clone(),
        email: me.email.clone(),
        since: previous.filter(|_| mine).map_or(at, |p| p.since),
        until: at + core::WINDOW_S,
        branch: previous
            .filter(|_| mine)
            .map_or_else(|| branch.to_owned(), |p| p.branch.clone()),
        done: previous.filter(|_| mine).and_then(|p| p.done.clone()),
    };
    match store.put(
        feature,
        story,
        current.map(|(id, _)| id.as_str()),
        Some(&claim),
    ) {
        Ok(()) => Ok(Some(ClaimChange {
            story: story.to_owned(),
            verb: match state {
                None => ClaimVerb::Claimed,
                Some(ClaimState::Mine) => ClaimVerb::Renewed,
                Some(_) => ClaimVerb::Took,
            },
            from: previous.filter(|_| !mine).map(|p| p.holder.clone()),
            stale: state == Some(ClaimState::Stale),
        })),
        Err(Put::Lost) => {
            // Lost to whom? Another person: their claim stands. Free, stale
            // or the person's own (another machine of theirs): once more.
            let again = store.live(feature).ok();
            let blocks = again
                .as_ref()
                .and_then(|r| r.get(story))
                .and_then(|(_, c)| c.as_ref())
                .is_some_and(|c| c.state(Some(&me.email), now()).blocks());
            match again {
                Some(r) if !blocks && retry => {
                    take_once(store, feature, story, me, branch, &r, force, false)
                }
                _ => Err(lost(store, feature, story)),
            }
        }
        Err(Put::Failed(why)) => Err(Box::new(refuse(
            CommandStatus::Error,
            "CLAIM-UNREACHABLE",
            &format!("cannot claim {story}: {why}"),
        ))),
    }
}

/// A race lost: who won.
fn lost(store: &Store, feature: &str, story: &str) -> Box<CommandResult> {
    let winner = store
        .live(feature)
        .ok()
        .and_then(|r| r.get(story).and_then(|(_, c)| c.clone()));
    match winner {
        Some(c) => held(feature, story, &c),
        None => Box::new(refuse(
            CommandStatus::NeedsHuman,
            "CLAIMED",
            &format!("{story} changed while it was being claimed; look again: speccoder claim"),
        )),
    }
}

fn release(
    store: &Store,
    feature: &str,
    story: &str,
    me: &Me,
    read: &Read,
    force: bool,
) -> Result<Option<ClaimChange>, Box<CommandResult>> {
    let Some((id, c)) = read.get(story) else {
        return Ok(None);
    };
    let mine = c.as_ref().is_some_and(|c| c.is_of(&me.email));
    if !mine
        && !force
        && let Some(c) = c
    {
        let state = c.state(Some(&me.email), now());
        if state.blocks() {
            return Err(Box::new(
                refuse(
                    CommandStatus::NeedsHuman,
                    "CLAIMED",
                    &format!("{story} is {}'s, not yours to release", c.holder),
                )
                .with_fix(vec![format!(
                    "speccoder claim {story} --release --force --feature {feature}"
                )]),
            ));
        }
    }
    match store.put(feature, story, Some(id), None) {
        Ok(()) => Ok(Some(ClaimChange {
            story: story.to_owned(),
            verb: if mine {
                ClaimVerb::Released
            } else {
                ClaimVerb::Removed
            },
            from: c.as_ref().filter(|_| !mine).map(|c| c.holder.clone()),
            stale: false,
        })),
        Err(Put::Lost) => Err(lost(store, feature, story)),
        Err(Put::Failed(why)) => Err(Box::new(refuse(
            CommandStatus::Error,
            "CLAIM-UNREACHABLE",
            &format!("cannot release {story}: {why}"),
        ))),
    }
}

/// A run's claims: the stories it holds, each renewed at most once an hour.
pub(crate) struct Run {
    store: Option<Store>,
    me: Option<Me>,
    feature: String,
    /// Story → when this run last claimed or renewed it.
    held: BTreeMap<String, u64>,
    /// Stories marked done.
    done: std::collections::BTreeSet<String>,
    /// Said once: offline or no identity.
    told: bool,
    /// Claims could not be read or written: not tried again this run.
    off: bool,
}

/// What a run learns before a story's task.
pub(crate) enum Hold {
    /// Go on; a line to say, once.
    Go(Option<String>),
    /// Another person holds it: the stop.
    Stop(Box<CommandResult>),
}

impl Run {
    pub(crate) fn new(dir: &Path, feature: &str) -> Self {
        let store = Store::at(dir);
        let me = store.as_ref().and_then(|s| me(&s.git));
        Self {
            store,
            me,
            feature: feature.to_owned(),
            held: BTreeMap::new(),
            done: std::collections::BTreeSet::new(),
            told: false,
            off: false,
        }
    }

    /// Whether this run holds `story`.
    pub(crate) fn holds(&self, story: &str) -> bool {
        self.held.contains_key(story)
    }

    fn once(&mut self, line: String) -> Hold {
        self.off = true;
        if std::mem::replace(&mut self.told, true) {
            Hold::Go(None)
        } else {
            Hold::Go(Some(line))
        }
    }

    /// Claims or renews `story` before its task.
    pub(crate) fn hold(&mut self, story: &str) -> Hold {
        let at = now();
        if self
            .held
            .get(story)
            .is_some_and(|t| at.saturating_sub(*t) < core::RENEW_AFTER_S)
        {
            return Hold::Go(None);
        }
        if self.off {
            return Hold::Go(None);
        }
        let Some(store) = &self.store else {
            return Hold::Go(None);
        };
        let Some(me) = self.me.clone() else {
            return self.once(format!(
                "claims: git has no user.email here — {story} not claimed"
            ));
        };
        let read = match store.live(&self.feature) {
            Ok(r) => r,
            Err(why) => {
                let from = store.remote.clone().unwrap_or_default();
                return self.once(format!(
                    "claims: {from} unreachable ({why}) — {story} not claimed"
                ));
            }
        };
        let first = !self.held.contains_key(story);
        let branch = format!("speccoder/{}", self.feature);
        let mut notice = if store.remote.is_some() && first {
            store.refspec()
        } else {
            None
        };
        match take(store, &self.feature, story, &me, &branch, &read, false) {
            Ok(change) => {
                self.held.insert(story.to_owned(), at);
                // Said where others see it; a claim on this machine only is
                // quiet (one person, one checkout: nothing to tell).
                let line =
                    change
                        .filter(|_| first)
                        .and_then(|c| match (c.verb, c.from, &store.remote) {
                            (ClaimVerb::Took, Some(from), _) => {
                                Some(format!("{story} claimed, after {from}'s claim ran out"))
                            }
                            (ClaimVerb::Claimed, _, Some(r)) => {
                                Some(format!("{story} claimed on {r}"))
                            }
                            _ => None,
                        });
                notice = match (notice, line) {
                    (Some(a), Some(b)) => Some(format!("{b}; {a}")),
                    (a, b) => b.or(a),
                };
                Hold::Go(notice)
            }
            Err(stop) if stop.status() == CommandStatus::NeedsHuman => Hold::Stop(stop),
            Err(stop) => self.once(format!("claims: {} — {story} not claimed", stop.message())),
        }
    }

    /// Marks the person's claims on `stories` (every task ticked) done at
    /// `commit`: once per story and run, and only when this repository's
    /// copy shows one of them still open — no network otherwise.
    pub(crate) fn done(&mut self, stories: &[&str], commit: &str) {
        let (Some(store), Some(me)) = (&self.store, &self.me) else {
            return;
        };
        let open = |read: &Read, s: &str| {
            read.get(s)
                .and_then(|(_, c)| c.as_ref())
                .is_some_and(|c| c.is_of(&me.email) && c.done.is_none())
        };
        let local = store.local(&self.feature);
        let due: Vec<&str> = stories
            .iter()
            .copied()
            .filter(|s| !self.done.contains(*s) && open(&local, s))
            .collect();
        if due.is_empty() {
            return;
        }
        let Ok(read) = store.live(&self.feature) else {
            return;
        };
        for story in due {
            let Some((id, Some(c))) = read.get(story).filter(|_| open(&read, story)) else {
                self.done.insert(story.to_owned());
                continue;
            };
            // Held until merged — or, merged by squash or rebase (no
            // ancestry to see), until the done window ends.
            let done = Claim {
                done: Some(commit.to_owned()),
                until: now() + core::DONE_WINDOW_S,
                ..c.clone()
            };
            if store
                .put(&self.feature, story, Some(id), Some(&done))
                .is_ok()
            {
                self.done.insert(story.to_owned());
            }
        }
    }
}

/// Other people's claims on stories in `stories` (live or done, unmerged),
/// from this repository's copy: what `pr` refuses.
#[must_use]
pub(crate) fn others_on(dir: &Path, feature: &str, stories: &[String]) -> Vec<Claim> {
    let Some(store) = Store::at(dir) else {
        return Vec::new();
    };
    let me = me(&store.git).map(|m| m.email);
    let at = now();
    let mut read = store.live(feature).unwrap_or_else(|_| store.local(feature));
    store.prune(feature, &mut read);
    read.into_values()
        .filter_map(|(_, c)| c)
        .filter(|c| stories.contains(&c.story))
        .filter(|c| c.state(me.as_deref(), at).blocks())
        .collect()
}

/// Other people's claims on `feature`, from this repository's copy (no
/// network): Needs You and the board.
#[must_use]
pub fn others(dir: &Path, feature: &str) -> Vec<Claim> {
    let Some(store) = Store::at(dir) else {
        return Vec::new();
    };
    let me = me(&store.git).map(|m| m.email);
    let at = now();
    store
        .local(feature)
        .into_values()
        .filter_map(|(_, c)| c)
        .filter(|c| c.state(me.as_deref(), at).blocks())
        .filter(|c| c.done.as_deref().is_none_or(|d| !store.merged(d)))
        .collect()
}

#[cfg(test)]
mod tests {
    // Test code: panicking on unexpected input is the failure report.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[test]
    fn a_run_holds_its_story_quietly_here_and_marks_it_done_for_two_weeks() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        git(r, &["init", "-q"]);
        git(r, &["config", "user.name", "Dana"]);
        git(r, &["config", "user.email", "dana@example.com"]);
        git(r, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let head = git(r, &["rev-parse", "HEAD"]);
        let mut run = Run::new(r, "001-x");
        // No remote: claimed, and nothing said.
        assert!(matches!(run.hold("US1"), Hold::Go(None)));
        assert!(run.holds("US1"));
        // Within the hour: nothing read again.
        assert!(matches!(run.hold("US1"), Hold::Go(None)));
        run.done(&["US1"], &head);
        let store = Store::at(r).unwrap();
        let read = store.local("001-x");
        let c = read["US1"].1.clone().unwrap();
        assert_eq!(c.done.as_deref(), Some(head.as_str()));
        assert!(c.until > now() + core::WINDOW_S, "done holds past a day");
        assert!(c.until <= now() + core::DONE_WINDOW_S);
        // Another person meets a done claim as one that stands.
        assert_eq!(c.state(Some("sam@example.com"), now()), ClaimState::Done);
    }
}
