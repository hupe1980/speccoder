//! Feedback between projects (spec 125): an outbox file a project's agent
//! (or its person) writes for another project it depends on — open items
//! as `### AP-43 — title` under `## Open` — and each item's status as the
//! other project decided it. speccoder reads the file as written and
//! writes it only through the agent's tools.

use serde::{Deserialize, Serialize};

/// One open item of an outbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Item {
    /// `AP-43`, or `FB-3` for a heading without an id of its own.
    pub id: String,
    /// The heading's words after the id.
    pub title: String,
    /// The item's text, up to the next heading.
    pub text: String,
    /// Over title and text: a change since a decision shows.
    pub digest: String,
}

fn digest(title: &str, text: &str) -> String {
    // FNV-1a, 64 bits: stable across builds, enough to see a change.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title
        .bytes()
        .chain(std::iter::once(b'\n'))
        .chain(text.bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `AP-43` at the start of a heading: letters, a dash, digits.
fn leading_id(heading: &str) -> Option<(String, String)> {
    let token: String = heading
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ':')
        .collect();
    let (prefix, number) = token.split_once('-')?;
    let ok = !prefix.is_empty()
        && prefix.len() <= 8
        && prefix.chars().all(|c| c.is_ascii_uppercase())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return None;
    }
    let rest = heading[token.len()..]
        .trim_start()
        .trim_start_matches(['—', '–', '-', ':'])
        .trim()
        .to_owned();
    Some((token, rest))
}

/// A heading outside code fences: its line, level and words.
struct Heading {
    line: usize,
    level: usize,
    words: String,
}

/// The headings of `lines`, skipping fenced code blocks.
fn headings(lines: &[&str]) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        let marker = if t.starts_with("```") {
            Some("```")
        } else if t.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        match (fence, marker) {
            (None, Some(m)) => {
                fence = Some(m);
                continue;
            }
            (Some(f), Some(m)) if f == m => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            _ => {}
        }
        let level = l.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && l[level..].starts_with(' ') {
            out.push(Heading {
                line: i,
                level,
                words: l[level..].trim().to_owned(),
            });
        }
    }
    out
}

/// An item's place: its heading line and the line after its last.
struct Span {
    start: usize,
    end: usize,
    item: Item,
}

/// The open items with their places: the `### ` headings under `## Open`
/// (up to the next `#`/`##`), else every `### `, else every `## ` heading.
fn spans(text: &str) -> (Vec<Span>, Option<(usize, usize)>) {
    let lines: Vec<&str> = text.lines().collect();
    let hs = headings(&lines);
    let end_of = |h: usize| -> usize {
        let level = hs[h].level;
        hs[h + 1..]
            .iter()
            .find(|x| x.level <= level)
            .map_or(lines.len(), |x| x.line)
    };
    let open = hs
        .iter()
        .position(|h| h.level == 2 && h.words.eq_ignore_ascii_case("open"))
        .map(|i| (hs[i].line, end_of(i)));
    let pick: Vec<usize> = match open {
        Some((from, to)) => (0..hs.len())
            .filter(|&i| hs[i].level == 3 && hs[i].line > from && hs[i].line < to)
            .collect(),
        None => {
            let deep: Vec<usize> = (0..hs.len()).filter(|&i| hs[i].level == 3).collect();
            if deep.is_empty() {
                (0..hs.len()).filter(|&i| hs[i].level == 2).collect()
            } else {
                deep
            }
        }
    };
    let mut seen = std::collections::BTreeSet::new();
    let out = pick
        .into_iter()
        .map(|i| {
            let end = end_of(i);
            let body = lines[hs[i].line + 1..end].join("\n").trim().to_owned();
            let (mut id, title) = leading_id(&hs[i].words).unwrap_or_else(|| {
                // No id of its own: one from its words, stable as items move.
                let d = digest(&hs[i].words, "");
                (format!("FB-{}", &d[..6]), hs[i].words.clone())
            });
            if !seen.insert(id.clone()) {
                id = format!("{id}-{}", hs[i].line + 1);
                seen.insert(id.clone());
            }
            Span {
                start: hs[i].line,
                end,
                item: Item {
                    digest: digest(&title, &body),
                    id,
                    title,
                    text: body,
                },
            }
        })
        .collect();
    (out, open)
}

/// The open items of an outbox: the `### ` headings under `## Open`, else
/// every `### `, else every `## ` heading; headings in code fences are text.
#[must_use]
pub fn parse_outbox(text: &str) -> Vec<Item> {
    spans(text).0.into_iter().map(|s| s.item).collect()
}

/// The id a new item gets: the next number of the file's own series
/// (`AP-44` after `AP-43`, over every heading), else `FB-1`, `FB-2`, ….
#[must_use]
pub fn next_id(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut best: Option<(String, u64)> = None;
    for h in headings(&lines) {
        if let Some((id, _)) = leading_id(&h.words)
            && let Some((prefix, n)) = id.split_once('-')
            && let Ok(n) = n.parse::<u64>()
            && best.as_ref().is_none_or(|(_, b)| n > *b)
        {
            best = Some((prefix.to_owned(), n));
        }
    }
    match best {
        Some((prefix, n)) => format!("{prefix}-{}", n + 1),
        None => "FB-1".to_owned(),
    }
}

/// Text an agent wrote, made safe as an item's body: no line can open a
/// heading or a code fence of the file.
fn inert(body: &str) -> String {
    body.replace('\r', "")
        .lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with('#') || t.starts_with("```") || t.starts_with("~~~") {
                format!("\\{l}")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` with a new item appended where the open items are: at the end of
/// `## Open`; after the last item of a file without one; else under a new
/// `## Open` at the end.
#[must_use]
pub fn append(text: &str, id: &str, title: &str, body: &str, evidence: &str) -> String {
    let title = title.replace(['\r', '\n'], " ");
    let item = format!(
        "### {id} — {}\n\n{}\n\n_{evidence}_",
        title.trim(),
        inert(body.trim())
    );
    let lines: Vec<&str> = text.lines().collect();
    let (items, open) = spans(text);
    let at = match (open, items.last()) {
        (Some((_, end)), _) => Some(end),
        (None, Some(last)) => Some(last.end),
        (None, None) => None,
    };
    let mut out = match at {
        Some(at) => {
            let mut head: Vec<&str> = lines[..at].to_vec();
            while head.last().is_some_and(|l| l.trim().is_empty()) {
                head.pop();
            }
            let tail = &lines[at..];
            let mut s = head.join("\n");
            s.push_str("\n\n");
            s.push_str(&item);
            if !tail.is_empty() {
                s.push_str("\n\n");
                s.push_str(&tail.join("\n"));
            }
            s
        }
        None => {
            let mut s = text.trim_end().to_owned();
            if !s.is_empty() {
                s.push_str("\n\n");
            }
            s.push_str("## Open\n\n");
            s.push_str(&item);
            s
        }
    };
    out.push('\n');
    out
}

/// `text` without the open item `id` (its heading and text, nothing else).
#[must_use]
pub fn remove(text: &str, id: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let span = spans(text).0.into_iter().find(|s| s.item.id == id)?;
    let mut kept: Vec<&str> = lines[..span.start].to_vec();
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    let rest = &lines[span.end..];
    let mut out = kept.join("\n");
    if !rest.is_empty() {
        out.push_str("\n\n");
        out.push_str(&rest.join("\n"));
    }
    out.push('\n');
    Some(out)
}

/// Where an item stands, as the other project decided it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// The other project has not decided on it.
    NotSeen,
    /// Taken up as a spec (a unit).
    TakenUp {
        /// The unit.
        unit: String,
    },
    /// Recorded as a bug task.
    Bug {
        /// `unit/task`.
        task: String,
    },
    /// Fixed in a commit of the other project.
    Fixed {
        /// The commit.
        commit: String,
        /// An agent said so (not a person, not a tick).
        by_agent: bool,
    },
    /// Declined, with the reason given.
    Declined {
        /// Why.
        reason: String,
    },
    /// Filed as a GitHub issue.
    Filed {
        /// `owner/name`.
        repo: String,
        /// Its number.
        number: u64,
        /// `open`, `closed`, or unknown.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        issue_state: Option<String>,
    },
    /// The other project could not be read.
    Unknown {
        /// Why.
        why: String,
    },
}

/// An item with where it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Tracked {
    /// The item.
    #[serde(flatten)]
    pub item: Item,
    /// Its status.
    pub status: Status,
    /// It changed since the other project decided on it.
    #[serde(default)]
    pub changed: bool,
}

/// An upstream a project declares, and what it asks of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Upstream {
    /// Its name (`[upstream.<name>]`).
    pub name: String,
    /// Its folder, when local.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Its GitHub repository (`owner/name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The outbox file, project-relative.
    pub outbox: String,
    /// The outbox exists.
    pub exists: bool,
    /// Its open items.
    pub items: Vec<Tracked>,
}

/// What another project asks of this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Request {
    /// The asking project's name.
    pub source: String,
    /// Its folder.
    pub path: String,
    /// The item, as that project wrote it.
    #[serde(flatten)]
    pub item: Item,
    /// What this project decided, if anything.
    pub status: Status,
    /// It changed since this project decided.
    #[serde(default)]
    pub changed: bool,
}

/// `speccoder feedback`: what this project asks of others, and what others
/// ask of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeedbackReport {
    /// The upstreams this project declares.
    pub upstreams: Vec<Upstream>,
    /// Open items other recent projects ask of this one.
    pub requests: Vec<Request>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Feedback for agentplane\n\n**From:** speccoder\n\n## Decisions\n\n- **No MCP notifications (AP-32, refused).** An observer …\n\n## Open\n\n### AP-43 — a stop that halts without unwinding\n\n`Runtime::request_cancel` stops a running agent.\n\nAsk: a halt.\n\n### AP-44: journal export\n\nExport it.\n";

    #[test]
    fn the_open_items_are_read_as_the_file_has_them() {
        let items = parse_outbox(FILE);
        assert_eq!(items.len(), 2, "{items:#?}");
        assert_eq!(items[0].id, "AP-43");
        assert_eq!(items[0].title, "a stop that halts without unwinding");
        assert!(items[0].text.contains("Ask: a halt."));
        assert_eq!(items[1].id, "AP-44");
        assert_eq!(items[1].title, "journal export");
        // Decisions are not items.
        assert!(!items.iter().any(|i| i.id == "AP-32"));
        // A file without `## Open`: its headings, ids made up.
        let plain = parse_outbox("# x\n\n### Retry on 429\n\nPlease.\n\n### Pagination\n");
        assert_eq!(plain.len(), 2);
        // Made-up ids follow the words, not the position.
        let moved = parse_outbox("# x\n\n### New first\n\n### Retry on 429\n\nPlease.\n");
        assert_eq!(moved[1].id, plain[0].id);
    }

    #[test]
    fn a_new_item_continues_the_files_series_under_open() {
        assert_eq!(next_id(FILE), "AP-45");
        assert_eq!(next_id(""), "FB-1");
        // An empty Open section takes the item (it is listed).
        let tmpl = append("# Feedback\n\n## Open\n", "FB-1", "x", "y", "e");
        assert_eq!(parse_outbox(&tmpl).len(), 1, "{tmpl}");
        // A file of its own headings keeps them all listed.
        let own = append(
            "# x\n\n### Retry\n\nPlease.\n",
            "FB-1",
            "Pagination",
            "z",
            "e",
        );
        assert_eq!(parse_outbox(&own).len(), 2, "{own}");
        // An agent's text cannot open a heading or a fence.
        let sneaky = append(FILE, "AP-45", "t\r", "## Notes\n### AP-99 — fake\n```", "e");
        let ids: Vec<String> = parse_outbox(&sneaky).into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["AP-43", "AP-44", "AP-45"], "{sneaky}");
        let out = append(
            FILE,
            "AP-45",
            "retry budget",
            "Make it so.",
            "Reported by an agent · speccoder@4be81c2",
        );
        let items = parse_outbox(&out);
        assert_eq!(items.last().unwrap().id, "AP-45");
        assert!(
            items
                .last()
                .unwrap()
                .text
                .contains("_Reported by an agent · speccoder@4be81c2_")
        );
        assert!(out.contains("## Decisions"), "{out}");
        let fresh = append("# Feedback\n", "FB-1", "x", "y", "e");
        assert!(fresh.contains("## Open\n\n### FB-1 — x"), "{fresh}");
        // An item's digest follows its words.
        let changed = parse_outbox(&FILE.replace("Ask: a halt.", "Ask: a halt, please."));
        assert_ne!(changed[0].digest, parse_outbox(FILE)[0].digest);
    }

    #[test]
    fn an_item_is_removed_with_its_text_only() {
        let out = remove(FILE, "AP-43").unwrap();
        let items = parse_outbox(&out);
        assert_eq!(
            items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["AP-44"]
        );
        assert!(out.contains("## Decisions") && out.contains("## Open"));
        assert!(remove(FILE, "AP-99").is_none());
        // A heading in a code fence is no item; a Decisions entry is never removed.
        let fenced = "## Decisions\n\n### AP-43 — old\n\n## Open\n\n### AP-43 — a stop\n\n```sh\n# a comment\n### not an item\n```\nmore\n";
        let items = parse_outbox(fenced);
        assert_eq!(items.len(), 1);
        assert!(items[0].text.contains("more"), "{:?}", items[0].text);
        let out = remove(fenced, "AP-43").unwrap();
        assert!(
            out.contains("### AP-43 — old") && !out.contains("a stop"),
            "{out}"
        );
    }
}
