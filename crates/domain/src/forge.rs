//! Tasks from GitHub, GitLab and Gitea/Forgejo (7.6, D-97): which items become tasks, which
//! tasks are finished upstream, and which project a repository's tasks go to.
//!
//! An item is an issue or a PR/MR, keyed `owner/repo#12` (GitLab MRs `group/proj!3`). It is
//! "yours" for one or more reasons ([`ISSUE`], [`PR`], [`REVIEW`], [`MENTION`]). Each item
//! becomes a task once; the reasons it was last seen for are kept, so that when it drops out
//! of what's yours the task is completed only if one of those reasons is still switched on
//! (turning mentions off doesn't "complete" mention tasks).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Reasons an item is yours (bit flags).
pub const ISSUE: u8 = 1;
/// A PR/MR assigned to you.
pub const PR: u8 = 2;
/// Your review is requested.
pub const REVIEW: u8 = 4;
/// You were mentioned.
pub const MENTION: u8 = 8;

/// An open item that is currently yours.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// `owner/repo#12` or `group/proj!3`.
    pub key: String,
    /// `owner/repo` (GitLab: the full path, subgroups included).
    pub repo: String,
    pub title: String,
    pub url: String,
    pub kinds: u8,
}

/// Merge the answers of several queries: one item per key, with all its reasons.
pub fn merge(lists: impl IntoIterator<Item = Item>) -> Vec<Item> {
    let mut out: Vec<Item> = vec![];
    let mut at: HashMap<String, usize> = HashMap::new();
    for it in lists {
        match at.get(&it.key) {
            Some(&i) => out[i].kinds |= it.kinds,
            None => {
                at.insert(it.key.clone(), out.len());
                out.push(it);
            }
        }
    }
    out
}

/// An item seen before (it has, or had, a task).
#[derive(Debug, Clone, PartialEq)]
pub struct Known {
    pub key: String,
    /// The reasons it was last seen for.
    pub kinds: u8,
    /// Its task is still open (not done, not deleted).
    pub open: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Indices into the current items: new, make a task.
    pub create: Vec<usize>,
    /// Keys whose task is finished upstream: complete it.
    pub complete: Vec<String>,
    /// Keys seen again with different reasons: remember the new ones.
    pub reasons: Vec<(String, u8)>,
}

/// What to do after a poll. `whole`: every query answered in full; without it nothing is
/// completed, since a missing item may only be missing from a cut-off answer.
pub fn reconcile(current: &[Item], known: &[Known], enabled: u8, whole: bool) -> Plan {
    let known_by: HashMap<&str, &Known> = known.iter().map(|k| (k.key.as_str(), k)).collect();
    let current_keys: HashSet<&str> = current.iter().map(|i| i.key.as_str()).collect();
    let mut plan = Plan::default();
    for (i, it) in current.iter().enumerate() {
        match known_by.get(it.key.as_str()) {
            None => plan.create.push(i),
            Some(k) if k.kinds != it.kinds => plan.reasons.push((it.key.clone(), it.kinds)),
            Some(_) => {}
        }
    }
    if whole {
        plan.complete = known
            .iter()
            .filter(|k| k.open && k.kinds & enabled != 0 && !current_keys.contains(k.key.as_str()))
            .map(|k| k.key.clone())
            .collect();
    }
    plan
}

/// A repository rule: tasks from repositories matching `repo` go to `project_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// `owner/repo`, with `*` matching any text: `owner/*`, `*/docs`, `*`.
    pub repo: String,
    pub project_id: String,
}

/// The project for a repository: the first matching rule's, else none (the inbox).
pub fn project_for<'a>(rules: &'a [Rule], repo: &str) -> Option<&'a str> {
    rules
        .iter()
        .find(|r| glob(&r.repo, repo))
        .map(|r| r.project_id.as_str())
}

/// Case-insensitive match where `*` stands for any text (slashes included, so `group/*`
/// covers GitLab subgroups).
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.trim().to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    // Classic wildcard matching with backtracking to the last star.
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// The task title: review requests say so, everything else is the item's own title.
pub fn task_title(it: &Item) -> String {
    if it.kinds & (ISSUE | PR) == 0 && it.kinds & REVIEW != 0 {
        format!("Review: {}", it.title)
    } else {
        it.title.clone()
    }
}

/// Why the item is yours, for the task's notes.
pub fn reasons_text(kinds: u8) -> String {
    let mut parts = vec![];
    if kinds & ISSUE != 0 {
        parts.push("assigned issue");
    }
    if kinds & PR != 0 {
        parts.push("assigned pull request");
    }
    if kinds & REVIEW != 0 {
        parts.push("review requested");
    }
    if kinds & MENTION != 0 {
        parts.push("you were mentioned");
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(key: &str, kinds: u8) -> Item {
        Item {
            key: key.into(),
            repo: key.split(['#', '!']).next().unwrap().into(),
            title: format!("Title of {key}"),
            url: format!("https://example.org/{key}"),
            kinds,
        }
    }

    fn known(key: &str, kinds: u8, open: bool) -> Known {
        Known {
            key: key.into(),
            kinds,
            open,
        }
    }

    const ALL: u8 = ISSUE | PR | REVIEW | MENTION;

    #[test]
    fn merge_unions_reasons() {
        let m = merge([
            item("a/b#1", ISSUE),
            item("a/b#2", PR),
            item("a/b#1", MENTION),
        ]);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].kinds, ISSUE | MENTION);
        assert_eq!(m[1].kinds, PR);
    }

    #[test]
    fn new_items_are_created_once() {
        let cur = [item("a/b#1", ISSUE), item("a/b#2", ISSUE)];
        let p = reconcile(&cur, &[known("a/b#1", ISSUE, true)], ALL, true);
        assert_eq!(p.create, vec![1]);
        assert!(p.complete.is_empty());
        // A deleted or completed task is not brought back.
        let p = reconcile(
            &cur,
            &[known("a/b#1", ISSUE, false), known("a/b#2", ISSUE, false)],
            ALL,
            true,
        );
        assert!(p.create.is_empty());
        assert!(p.complete.is_empty());
    }

    #[test]
    fn gone_upstream_completes() {
        let p = reconcile(
            &[],
            &[known("a/b#1", ISSUE, true), known("a/b#2", ISSUE, false)],
            ALL,
            true,
        );
        assert_eq!(p.complete, vec!["a/b#1".to_string()]);
    }

    #[test]
    fn partial_answers_complete_nothing() {
        let p = reconcile(&[], &[known("a/b#1", ISSUE, true)], ALL, false);
        assert!(p.complete.is_empty());
    }

    #[test]
    fn switched_off_reasons_dont_complete() {
        // Only a mention, and mentions are now off: it's missing for that reason, not done.
        let k = [
            known("a/b#1", MENTION, true),
            known("a/b#2", ISSUE | MENTION, true),
        ];
        let p = reconcile(&[], &k, ALL & !MENTION, true);
        assert_eq!(p.complete, vec!["a/b#2".to_string()]);
    }

    #[test]
    fn changed_reasons_are_remembered() {
        let p = reconcile(
            &[item("a/b#1", MENTION)],
            &[known("a/b#1", ISSUE | MENTION, true)],
            ALL,
            true,
        );
        assert_eq!(p.reasons, vec![("a/b#1".to_string(), MENTION)]);
        assert!(p.complete.is_empty() && p.create.is_empty());
    }

    #[test]
    fn globs() {
        assert!(glob("owner/repo", "Owner/Repo"));
        assert!(glob("owner/*", "owner/repo"));
        assert!(glob("group/*", "group/sub/proj"));
        assert!(glob("*/docs", "anyone/docs"));
        assert!(glob("*", "x/y"));
        assert!(glob("a*c*e", "abcde"));
        assert!(!glob("owner/*", "other/repo"));
        assert!(!glob("owner/repo", "owner/repo2"));
        assert!(!glob("", "owner/repo"));
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = vec![
            Rule {
                repo: "acme/website".into(),
                project_id: "web".into(),
            },
            Rule {
                repo: "acme/*".into(),
                project_id: "work".into(),
            },
        ];
        assert_eq!(project_for(&rules, "acme/website"), Some("web"));
        assert_eq!(project_for(&rules, "acme/api"), Some("work"));
        assert_eq!(project_for(&rules, "me/dotfiles"), None);
    }

    #[test]
    fn titles_and_notes() {
        assert_eq!(task_title(&item("a/b#1", REVIEW)), "Review: Title of a/b#1");
        assert_eq!(task_title(&item("a/b#1", REVIEW | PR)), "Title of a/b#1");
        assert_eq!(task_title(&item("a/b#1", MENTION)), "Title of a/b#1");
        assert_eq!(
            reasons_text(ISSUE | MENTION),
            "assigned issue, you were mentioned"
        );
    }
}
