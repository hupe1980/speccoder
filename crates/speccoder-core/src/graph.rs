//! Task dependencies from what `tasks.md` documents, and which tasks are ready.
//!
//! Edges come only from structure and from IDs written in task text; nothing
//! is inferred:
//!
//! - **Phases.** Foundational waits on all earlier phases; each user-story
//!   phase waits on the earlier *non-story* phases only, so stories are
//!   independent of each other; polish, convergence and unrecognised phases
//!   wait on all earlier phases.
//! - **Subsections.** A `###` subsection waits on the earlier subsections of
//!   its phase (tests before implementation).
//! - **`[P]` runs.** A `[P]` task waits on the previous non-`[P]` task; a
//!   non-`[P]` task waits on the whole preceding `[P]` run, or on the previous
//!   task when there is no run.
//! - **Explicit.** `depends on T012` / `after T012` in the task text.
//!
//! In **declared** mode (the Format line documents `(depends on …)`) the
//! written references are the complete order within a phase:
//! subsection and `[P]` edges are dropped, phase order is kept.
//!
//! In **chain** mode (OpenSpec) a group waits on every earlier group and a
//! task on the one before it in its group; written references are added.
//!
//! Dangling references and cycles are errors; the tasks involved are blocked.

use std::collections::{BTreeSet, HashMap};

use crate::snapshot::{
    Dependency, DependencyKind, DependencyMode, Diagnostic, ParsedTasks, Schedule, ScheduleTotals,
    ScheduledTask, Severity, Task, TaskState,
};

/// What a phase heading says the phase is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseKind {
    /// Shared infrastructure.
    Setup,
    /// Blocking prerequisites of every story.
    Foundational,
    /// One user story; independent of the other stories.
    Story,
    /// Cross-cutting work after the stories.
    Polish,
    /// Tasks appended by `/speckit-converge`.
    Convergence,
    /// Anything else; waits on everything before it.
    Other,
}

/// Classifies a phase by its heading and whether its tasks carry story labels.
#[must_use]
pub fn classify(title: &str, has_story_labels: bool) -> PhaseKind {
    let t = title.to_lowercase();
    if t.contains("convergence") {
        PhaseKind::Convergence
    } else if t.contains("polish") {
        PhaseKind::Polish
    } else if has_story_labels || names_a_user_story(&t) {
        PhaseKind::Story
    } else if t.contains("foundational") {
        PhaseKind::Foundational
    } else if t.contains("setup") {
        PhaseKind::Setup
    } else {
        PhaseKind::Other
    }
}

fn names_a_user_story(lower: &str) -> bool {
    lower.match_indices("user story").any(|(i, m)| {
        lower[i + m.len()..]
            .trim_start()
            .starts_with(|c: char| c.is_ascii_digit())
    })
}

/// A run of consecutive tasks sharing a phase heading.
struct Group {
    kind: PhaseKind,
    /// Task indices, split into consecutive subsection blocks.
    blocks: Vec<Vec<usize>>,
}

/// While grouping: (phase title, subsection blocks, current subsection).
type OpenGroup<'t> = (Option<&'t str>, Vec<Vec<usize>>, Option<&'t str>);

fn groups(tasks: &[Task]) -> Vec<Group> {
    let mut out: Vec<OpenGroup<'_>> = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        let phase = t.phase.as_deref();
        let sub = t.subsection.as_deref();
        match out.last_mut() {
            Some((p, blocks, last_sub)) if *p == phase => {
                if *last_sub == sub {
                    if let Some(b) = blocks.last_mut() {
                        b.push(i);
                    }
                } else {
                    blocks.push(vec![i]);
                    *last_sub = sub;
                }
            }
            _ => out.push((phase, vec![vec![i]], sub)),
        }
    }
    out.into_iter()
        .map(|(phase, blocks, _)| {
            let labelled = blocks.iter().flatten().any(|&i| tasks[i].story.is_some());
            Group {
                kind: classify(phase.unwrap_or(""), labelled),
                blocks,
            }
        })
        .collect()
}

/// Builds the schedule of `parsed`, with `graph.*` diagnostics.
#[must_use]
pub fn schedule(parsed: &ParsedTasks) -> (Schedule, Vec<Diagnostic>) {
    let tasks = &parsed.tasks;
    let mut diags = Vec::new();
    let index: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .rev() // the first occurrence of a duplicate wins
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();

    // Structural dependencies, as task indices.
    let mut structural: Vec<Vec<usize>> = vec![Vec::new(); tasks.len()];
    let declared = parsed.dependency_mode == DependencyMode::Declared;
    let chain = parsed.dependency_mode == DependencyMode::Chain;
    let groups = groups(tasks);
    for (g, group) in groups.iter().enumerate() {
        if chain {
            let earlier: Vec<usize> = groups[..g]
                .iter()
                .flat_map(|e| e.blocks.iter().flatten().copied())
                .collect();
            let mut previous: Option<usize> = None;
            for &i in group.blocks.iter().flatten() {
                let mut deps = earlier.clone();
                deps.extend(previous);
                structural[i] = deps;
                previous = Some(i);
            }
            continue;
        }
        let phase_barrier: Vec<usize> = groups[..g]
            .iter()
            .filter(|earlier| group.kind != PhaseKind::Story || earlier.kind != PhaseKind::Story)
            .flat_map(|earlier| earlier.blocks.iter().flatten().copied())
            .collect();
        for (b, block) in group.blocks.iter().enumerate() {
            let sub_barrier: Vec<usize> = group.blocks[..b].iter().flatten().copied().collect();
            let mut last_serial: Option<usize> = None;
            let mut run: Vec<usize> = Vec::new();
            for &i in block {
                let mut deps = phase_barrier.clone();
                if declared {
                    structural[i] = deps;
                    continue;
                }
                deps.extend(&sub_barrier);
                if tasks[i].parallel {
                    deps.extend(last_serial);
                    run.push(i);
                } else {
                    if run.is_empty() {
                        deps.extend(last_serial);
                    } else {
                        deps.append(&mut run);
                    }
                    last_serial = Some(i);
                }
                structural[i] = deps;
            }
        }
    }

    // Direct dependencies with kinds; explicit ones resolved by ID.
    let mut direct: Vec<Vec<Dependency>> = Vec::with_capacity(tasks.len());
    let mut edges: Vec<Vec<usize>> = Vec::with_capacity(tasks.len());
    for (i, t) in tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
        let mut deps = Vec::new();
        let mut resolved = Vec::new();
        for &d in &structural[i] {
            if seen.insert(tasks[d].id.as_str()) {
                deps.push(Dependency {
                    id: tasks[d].id.clone(),
                    kind: DependencyKind::Structural,
                    missing: false,
                });
                resolved.push(d);
            }
        }
        for id in &t.depends_on {
            if !seen.insert(id.as_str()) {
                continue;
            }
            match index.get(id.as_str()) {
                Some(&d) => resolved.push(d),
                None => diags.push(
                    Diagnostic::new(
                        Severity::Error,
                        "graph.dangling",
                        format!("{} depends on {id}, which does not exist", t.id),
                    )
                    .at(&parsed.path)
                    .line(t.line)
                    .hint("fix the task ID or remove the reference"),
                ),
            }
            deps.push(Dependency {
                id: id.clone(),
                kind: DependencyKind::Explicit,
                missing: !index.contains_key(id.as_str()),
            });
        }
        direct.push(deps);
        edges.push(resolved);
    }

    // Cycles: strongly connected components with more than one task, or a self-loop.
    let component = strongly_connected(&edges);
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &c) in component.iter().enumerate() {
        members.entry(c).or_default().push(i);
    }
    let in_cycle = |i: usize| members[&component[i]].len() > 1 || edges[i].contains(&i);
    let mut reported = BTreeSet::new();
    for i in 0..tasks.len() {
        if in_cycle(i) && reported.insert(component[i]) {
            let ids: Vec<&str> = members[&component[i]]
                .iter()
                .map(|&m| tasks[m].id.as_str())
                .collect();
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "graph.cycle",
                    format!("dependency cycle: {}", ids.join(" → ")),
                )
                .at(&parsed.path)
                .line(tasks[i].line)
                .hint("remove one of the `depends on` references"),
            );
        }
    }

    // States.
    let mut out = Vec::with_capacity(tasks.len());
    let mut totals = ScheduleTotals::default();
    for (i, t) in tasks.iter().enumerate() {
        let mut blockers: Vec<(usize, String)> = Vec::new();
        for (dep, &d) in direct[i]
            .iter()
            .zip(resolved_or_missing(&direct[i], &index).iter())
        {
            match d {
                None => blockers.push((usize::MAX, dep.id.clone())),
                Some(d) => {
                    let cyclic = in_cycle(i) && component[d] == component[i];
                    if !tasks[d].done || cyclic {
                        blockers.push((d, dep.id.clone()));
                    }
                }
            }
        }
        blockers.sort();
        blockers.dedup();
        let state = if t.done {
            totals.done += 1;
            TaskState::Done
        } else if blockers.is_empty() && !in_cycle(i) {
            totals.ready += 1;
            TaskState::Ready
        } else {
            totals.blocked += 1;
            TaskState::Blocked
        };
        out.push(ScheduledTask {
            id: t.id.clone(),
            state,
            depends_on: direct[i].clone(),
            blocked_by: if t.done {
                Vec::new()
            } else {
                blockers.into_iter().map(|(_, id)| id).collect()
            },
        });
    }
    (Schedule { tasks: out, totals }, diags)
}

fn resolved_or_missing(deps: &[Dependency], index: &HashMap<&str, usize>) -> Vec<Option<usize>> {
    deps.iter()
        .map(|d| index.get(d.id.as_str()).copied())
        .collect()
}

/// Iterative Tarjan: the component number of every node.
fn strongly_connected(edges: &[Vec<usize>]) -> Vec<usize> {
    const UNSEEN: usize = usize::MAX;
    let n = edges.len();
    let mut order = vec![UNSEEN; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut component = vec![UNSEEN; n];
    let mut stack = Vec::new();
    let mut next_order = 0;
    let mut next_component = 0;

    for root in 0..n {
        if order[root] != UNSEEN {
            continue;
        }
        // (node, index of the next edge to visit)
        let mut work = vec![(root, 0usize)];
        while let Some(&mut (v, ref mut e)) = work.last_mut() {
            if *e == 0 && order[v] == UNSEEN {
                order[v] = next_order;
                low[v] = next_order;
                next_order += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if let Some(&w) = edges[v].get(*e) {
                *e += 1;
                if order[w] == UNSEEN {
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(order[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == order[v] {
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    component[w] = next_component;
                    if w == v {
                        break;
                    }
                }
                next_component += 1;
            }
        }
    }
    component
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::Totals;

    /// `spec`: (id, phase, subsection, parallel, story, done, explicit deps)
    type Spec<'a> = (
        &'a str,
        &'a str,
        Option<&'a str>,
        bool,
        Option<&'a str>,
        bool,
        &'a [&'a str],
    );

    fn parsed(spec: &[Spec<'_>]) -> ParsedTasks {
        parsed_in(DependencyMode::Structural, spec)
    }

    fn parsed_in(dependency_mode: DependencyMode, spec: &[Spec<'_>]) -> ParsedTasks {
        let tasks: Vec<Task> = spec
            .iter()
            .enumerate()
            .map(|(n, &(id, phase, sub, parallel, story, done, deps))| Task {
                id: id.to_owned(),
                done,
                parallel,
                story: story.map(str::to_owned),
                yours: false,
                title: String::new(),
                depends_on: deps.iter().map(|d| (*d).to_owned()).collect(),
                phase: Some(phase.to_owned()),
                subsection: sub.map(str::to_owned),
                line: u32::try_from(n + 1).unwrap(),
                byte_range: [0, 0],
                line_hash: String::new(),
            })
            .collect();
        ParsedTasks {
            path: "tasks.md".to_owned(),
            phases: Vec::new(),
            totals: Totals::default(),
            tasks,
            dependency_mode,
        }
    }

    fn states(s: &Schedule) -> Vec<(&str, TaskState, Vec<&str>)> {
        s.tasks
            .iter()
            .map(|t| {
                (
                    t.id.as_str(),
                    t.state,
                    t.blocked_by.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    }

    use TaskState::{Blocked, Done, Ready};

    const SETUP: &str = "Phase 1: Setup";
    const FOUND: &str = "Phase 2: Foundational";
    const US1: &str = "Phase 3: User Story 1 - A";
    const US2: &str = "Phase 4: User Story 2 - B";
    const POLISH: &str = "Phase 5: Polish & Cross-Cutting Concerns";

    #[test]
    fn classifies_spec_kit_headings() {
        assert_eq!(
            classify("Phase 1: Setup (Shared Infrastructure)", false),
            PhaseKind::Setup
        );
        assert_eq!(
            classify("Phase 2: Foundational (Blocking Prerequisites)", false),
            PhaseKind::Foundational
        );
        assert_eq!(
            classify("Phase 3: User Story 1 - Login (Priority: P1) 🎯 MVP", false),
            PhaseKind::Story
        );
        assert_eq!(
            classify("Phase 9: Polish & Cross-Cutting Concerns", false),
            PhaseKind::Polish
        );
        assert_eq!(
            classify("Phase 10: Convergence", false),
            PhaseKind::Convergence
        );
        assert_eq!(classify("Phase 4: Login", true), PhaseKind::Story);
        assert_eq!(classify("User stories overview", false), PhaseKind::Other);
    }

    #[test]
    fn open_setup_blocks_everything_after_it() {
        let p = parsed(&[
            ("T001", SETUP, None, false, None, false, &[]),
            ("T002", FOUND, None, false, None, false, &[]),
            ("T003", US1, None, false, Some("US1"), false, &[]),
        ]);
        let (s, d) = schedule(&p);
        assert!(d.is_empty());
        assert_eq!(
            states(&s),
            [
                ("T001", Ready, vec![]),
                ("T002", Blocked, vec!["T001"]),
                ("T003", Blocked, vec!["T001", "T002"]),
            ]
        );
    }

    #[test]
    fn stories_are_independent_and_polish_waits_for_all() {
        let p = parsed(&[
            ("T001", SETUP, None, false, None, true, &[]),
            ("T002", FOUND, None, false, None, true, &[]),
            ("T003", US1, None, false, Some("US1"), false, &[]),
            ("T004", US2, None, false, Some("US2"), false, &[]),
            ("T005", POLISH, None, false, None, false, &[]),
        ]);
        let (s, _) = schedule(&p);
        assert_eq!(
            states(&s),
            [
                ("T001", Done, vec![]),
                ("T002", Done, vec![]),
                ("T003", Ready, vec![]),
                ("T004", Ready, vec![]),
                ("T005", Blocked, vec!["T003", "T004"]),
            ]
        );
        assert_eq!(
            s.totals,
            ScheduleTotals {
                done: 2,
                ready: 2,
                blocked: 1
            }
        );
    }

    #[test]
    fn parallel_runs_fork_and_join() {
        let p = parsed(&[
            ("T001", SETUP, None, false, None, true, &[]),
            ("T002", SETUP, None, true, None, false, &[]),
            ("T003", SETUP, None, true, None, false, &[]),
            ("T004", SETUP, None, false, None, false, &[]),
        ]);
        let (s, _) = schedule(&p);
        assert_eq!(
            states(&s),
            [
                ("T001", Done, vec![]),
                ("T002", Ready, vec![]),
                ("T003", Ready, vec![]),
                ("T004", Blocked, vec!["T002", "T003"]),
            ]
        );
    }

    #[test]
    fn tests_subsection_comes_before_implementation() {
        let p = parsed(&[
            (
                "T001",
                US1,
                Some("Tests for User Story 1"),
                true,
                Some("US1"),
                false,
                &[],
            ),
            (
                "T002",
                US1,
                Some("Implementation for User Story 1"),
                true,
                Some("US1"),
                false,
                &[],
            ),
        ]);
        let (s, _) = schedule(&p);
        assert_eq!(states(&s)[1], ("T002", Blocked, vec!["T001"]));
    }

    #[test]
    fn explicit_dependency_crosses_stories() {
        let p = parsed(&[
            ("T001", US1, None, false, Some("US1"), false, &[]),
            ("T002", US2, None, false, Some("US2"), false, &["T001"]),
        ]);
        let (s, _) = schedule(&p);
        assert_eq!(states(&s)[1], ("T002", Blocked, vec!["T001"]));
        assert_eq!(s.tasks[1].depends_on[0].kind, DependencyKind::Explicit);
    }

    #[test]
    fn declared_mode_keeps_phase_order_but_drops_structure_within_a_phase() {
        let p = parsed_in(
            DependencyMode::Declared,
            &[
                ("T001", SETUP, None, false, None, true, &[]),
                ("T002", SETUP, None, false, None, false, &["T001"]),
                ("T003", SETUP, None, true, None, false, &["T001"]),
                (
                    "T004",
                    US1,
                    Some("Tests"),
                    true,
                    Some("US1"),
                    false,
                    &["T002"],
                ),
                (
                    "T005",
                    US1,
                    Some("Implementation"),
                    true,
                    Some("US1"),
                    false,
                    &[],
                ),
            ],
        );
        let (s, d) = schedule(&p);
        assert!(d.is_empty());
        assert_eq!(
            states(&s),
            [
                ("T001", Done, vec![]),
                ("T002", Ready, vec![]),
                ("T003", Ready, vec![]),
                ("T004", Blocked, vec!["T002", "T003"]),
                ("T005", Blocked, vec!["T002", "T003"]),
            ]
        );
    }

    #[test]
    fn dangling_reference_is_an_error_and_blocks() {
        let p = parsed(&[("T001", SETUP, None, false, None, false, &["T099"])]);
        let (s, d) = schedule(&p);
        assert_eq!(d[0].code, "graph.dangling");
        assert_eq!(d[0].line, Some(1));
        assert_eq!(states(&s)[0], ("T001", Blocked, vec!["T099"]));
        assert!(s.tasks[0].depends_on[0].missing);
    }

    #[test]
    fn cycles_and_self_references_are_errors_and_never_ready() {
        // T002 waits on T001 structurally; T001 explicitly waits on T002.
        let p = parsed(&[
            ("T001", SETUP, None, false, None, false, &["T002"]),
            ("T002", SETUP, None, false, None, true, &[]),
            ("T003", POLISH, None, false, None, false, &["T003"]),
        ]);
        let (s, d) = schedule(&p);
        let codes: Vec<_> = d
            .iter()
            .map(|d| (d.code.as_str(), d.message.as_str()))
            .collect();
        assert_eq!(
            codes,
            [
                ("graph.cycle", "dependency cycle: T001 → T002"),
                ("graph.cycle", "dependency cycle: T003"),
            ]
        );
        assert_eq!(s.tasks[0].state, Blocked);
        assert_eq!(s.tasks[2].state, Blocked);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        /// Owned form of [`Spec`] while generating.
        type OwnedSpec = (
            String,
            &'static str,
            Option<String>,
            bool,
            Option<&'static str>,
            bool,
            Vec<String>,
        );

        const PHASES: [&str; 6] = [SETUP, FOUND, US1, US2, "Phase 6: Story notes", POLISH];

        /// Random task lists: phases in template order, subsections, `[P]`,
        /// done flags, and explicit references only to earlier tasks.
        fn task_lists() -> impl Strategy<Value = ParsedTasks> {
            prop::collection::vec(
                (
                    0..PHASES.len(),
                    0..3usize,
                    any::<bool>(),
                    any::<bool>(),
                    prop::option::of(0..64usize),
                ),
                1..40,
            )
            .prop_map(|mut rows| {
                rows.sort_by_key(|r| (r.0, r.1));
                let ids: Vec<String> = (1..=rows.len()).map(|n| format!("T{n:03}")).collect();
                let spec: Vec<OwnedSpec> = rows
                    .iter()
                    .enumerate()
                    .map(|(i, &(ph, sub, par, done, back))| {
                        let story = match ph {
                            2 => Some("US1"),
                            3 => Some("US2"),
                            _ => None,
                        };
                        let deps = back
                            .filter(|_| i > 0)
                            .map(|b| vec![ids[b % i].clone()])
                            .unwrap_or_default();
                        (
                            ids[i].clone(),
                            PHASES[ph],
                            Some(format!("S{sub}")),
                            par,
                            story,
                            done,
                            deps,
                        )
                    })
                    .collect();
                let owned: Vec<Vec<&str>> = spec
                    .iter()
                    .map(|r| r.6.iter().map(String::as_str).collect())
                    .collect();
                let rows: Vec<Spec<'_>> = spec
                    .iter()
                    .zip(&owned)
                    .map(|(r, d)| {
                        (
                            r.0.as_str(),
                            r.1,
                            r.2.as_deref(),
                            r.3,
                            r.4,
                            r.5,
                            d.as_slice(),
                        )
                    })
                    .collect();
                parsed(&rows)
            })
        }

        proptest! {
            #[test]
            fn ready_means_every_dependency_is_done(p in task_lists()) {
                let (s, _) = schedule(&p);
                let done: HashMap<&str, bool> = p.tasks.iter().map(|t| (t.id.as_str(), t.done)).collect();
                for t in &s.tasks {
                    if t.state == Ready {
                        prop_assert!(t.depends_on.iter().all(|d| done[d.id.as_str()]));
                    }
                }
            }

            #[test]
            fn open_tasks_with_done_dependencies_are_ready(p in task_lists()) {
                let (s, _) = schedule(&p);
                let done: HashMap<&str, bool> = p.tasks.iter().map(|t| (t.id.as_str(), t.done)).collect();
                for (t, task) in s.tasks.iter().zip(&p.tasks) {
                    if !task.done && t.depends_on.iter().all(|d| done[d.id.as_str()]) {
                        prop_assert_eq!(t.state, Ready);
                    }
                }
            }

            #[test]
            fn stories_never_depend_structurally_on_each_other(p in task_lists()) {
                let (s, _) = schedule(&p);
                let story: HashMap<&str, Option<&str>> =
                    p.tasks.iter().map(|t| (t.id.as_str(), t.story.as_deref())).collect();
                for t in &s.tasks {
                    for d in t.depends_on.iter().filter(|d| d.kind == DependencyKind::Structural) {
                        if let (Some(a), Some(b)) = (story[t.id.as_str()], story[d.id.as_str()]) {
                            prop_assert_eq!(a, b);
                        }
                    }
                }
            }

            #[test]
            fn backward_references_never_create_cycles(p in task_lists()) {
                let (s, d) = schedule(&p);
                prop_assert!(d.is_empty(), "{:?}", d);
                prop_assert_eq!(s.totals.done + s.totals.ready + s.totals.blocked, p.tasks.len());
            }
        }
    }
}
