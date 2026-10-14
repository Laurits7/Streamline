//! Checklists inside a task (D-71): "Do laundry" → sort, wash, hang, fold. The task is
//! planned and completed as one; ticking the last item completes it (D-80). Each tick
//! records when and by whom, so the day log shows steps on the day they were done (D-84).

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub const MAX_ITEMS: usize = 50;
pub const MAX_TEXT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Stable within the list (client-chosen), so ticks survive edits and reordering.
    pub id: String,
    pub text: String,
    pub done: bool,
    /// When it was ticked (UTC, RFC 3339 with milliseconds), set by the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<String>,
    /// Who ticked it (user id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_by: Option<String>,
}

/// Validate and tidy a list: texts trimmed, empty items dropped; ids must be short,
/// plain and unique; at most `MAX_ITEMS` items of at most `MAX_TEXT` characters.
pub fn normalize(items: Vec<Item>) -> Result<Vec<Item>, &'static str> {
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for mut it in items {
        it.text = it.text.trim().to_string();
        if it.text.is_empty() {
            continue;
        }
        if it.text.chars().count() > MAX_TEXT {
            return Err("checklist items must be at most 200 characters");
        }
        if !valid_id(&it.id) {
            return Err("checklist item ids must be 1-40 letters, digits, - or _");
        }
        if out.iter().any(|o| o.id == it.id) {
            return Err("checklist item ids must be unique");
        }
        out.push(it);
    }
    if out.len() > MAX_ITEMS {
        return Err("a checklist can have at most 50 items");
    }
    Ok(out)
}

/// The same items, all unticked (a new task from a routine or a default task).
pub fn fresh(items: &[Item]) -> Vec<Item> {
    items
        .iter()
        .map(|i| Item {
            done: false,
            done_at: None,
            done_by: None,
            ..i.clone()
        })
        .collect()
}

/// A changed list applied to an existing task: items that are still there keep their
/// ticks, new ones start unticked.
pub fn keep_ticks(new: &[Item], old: &[Item]) -> Vec<Item> {
    new.iter()
        .map(|i| match old.iter().find(|o| o.id == i.id && o.done) {
            Some(o) => Item {
                done: true,
                done_at: o.done_at.clone(),
                done_by: o.done_by.clone(),
                ..i.clone()
            },
            None => Item {
                done: false,
                done_at: None,
                done_by: None,
                ..i.clone()
            },
        })
        .collect()
}

/// Record who ticked what and when (D-84) for a list replacing `old`: items that were
/// already ticked keep their record, unticked ones lose it, and newly ticked ones get
/// `user` and the time the client sent (an offline tick replayed later), or `now` if it
/// sent none, a malformed one or one in the future.
pub fn stamp(old: &[Item], new: Vec<Item>, now: DateTime<Utc>, user: &str) -> Vec<Item> {
    let fmt = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::Millis, true);
    new.into_iter()
        .map(|mut i| {
            if !i.done {
                i.done_at = None;
                i.done_by = None;
            } else if let Some(o) = old.iter().find(|o| o.id == i.id && o.done) {
                i.done_at = o.done_at.clone();
                i.done_by = o.done_by.clone();
            } else {
                let sent = i
                    .done_at
                    .as_deref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|t| t.with_timezone(&Utc))
                    .filter(|t| *t <= now);
                i.done_at = Some(fmt(sent.unwrap_or(now)));
                i.done_by = Some(user.to_string());
            }
            i
        })
        .collect()
}

/// A kind of a default task (D-86): "Laundry" → linen, colours; picking kinds adds one
/// task per kind, with the kind's steps, or the default task's when it has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kind {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub checklist: Vec<Item>,
}

pub const MAX_KINDS: usize = 20;

/// Validate a default task's kinds: names trimmed and at most 80 characters, unnamed
/// kinds dropped, ids as for items and unique, at most `MAX_KINDS`; steps as `normalize`,
/// stored unticked.
pub fn normalize_kinds(kinds: Vec<Kind>) -> Result<Vec<Kind>, &'static str> {
    let mut out: Vec<Kind> = Vec::with_capacity(kinds.len());
    for mut k in kinds {
        k.name = k.name.trim().to_string();
        if k.name.is_empty() {
            continue;
        }
        if k.name.chars().count() > 80 {
            return Err("kind names must be at most 80 characters");
        }
        if !valid_id(&k.id) || out.iter().any(|o| o.id == k.id) {
            return Err("kind ids must be unique, 1-40 letters, digits, - or _");
        }
        k.checklist = fresh(&normalize(k.checklist)?);
        out.push(k);
    }
    if out.len() > MAX_KINDS {
        return Err("a default task can have at most 20 kinds");
    }
    Ok(out)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// (done, total)
pub fn progress(items: &[Item]) -> (usize, usize) {
    (items.iter().filter(|i| i.done).count(), items.len())
}

fn all_done(items: &[Item]) -> bool {
    !items.is_empty() && items.iter().all(|i| i.done)
}

/// The status a task moves to when its checklist changes from `old` to `new` (D-80):
/// an open task whose list becomes all ticked is done; a done task with an item
/// unticked is open again. `None` leaves the status alone (also for missed, skipped
/// and won't-do tasks, and for edits that keep the list all ticked).
pub fn status_after(old: &[Item], new: &[Item], status: &str) -> Option<&'static str> {
    match status {
        "open" if all_done(new) && !all_done(old) => Some("done"),
        "done"
            if new
                .iter()
                .any(|n| !n.done && old.iter().any(|o| o.id == n.id && o.done)) =>
        {
            Some("open")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: &str, text: &str, done: bool) -> Item {
        Item {
            id: id.into(),
            text: text.into(),
            done,
            done_at: None,
            done_by: None,
        }
    }

    #[test]
    fn normalizes_kinds() {
        let k = |id: &str, name: &str, steps: Vec<Item>| Kind {
            id: id.into(),
            name: name.into(),
            checklist: steps,
        };
        let got = normalize_kinds(vec![
            k("l", " Linen ", vec![it("w", "Wash", true)]),
            k("x", "  ", vec![]),
            k("c", "Colours", vec![]),
        ])
        .unwrap();
        assert_eq!(
            got,
            vec![
                k("l", "Linen", vec![it("w", "Wash", false)]),
                k("c", "Colours", vec![])
            ]
        );
        assert!(normalize_kinds(vec![k("a", "A", vec![]), k("a", "B", vec![])]).is_err());
        assert!(normalize_kinds(vec![k("a b", "A", vec![])]).is_err());
        assert!(normalize_kinds(vec![k("a", &"x".repeat(81), vec![])]).is_err());
        assert!(
            normalize_kinds(vec![k(
                "a",
                "A",
                vec![it("s", "", false), it("s", "x", false), it("s", "y", false)]
            )])
            .is_err()
        );
        let many: Vec<Kind> = (0..21).map(|i| k(&i.to_string(), "x", vec![])).collect();
        assert!(normalize_kinds(many).is_err());
    }

    #[test]
    fn stamps_ticks() {
        let now: DateTime<Utc> = "2026-10-14T10:00:00Z".parse().unwrap();
        let at = |i: &Item| (i.done_at.clone(), i.done_by.clone());
        let mut old = vec![it("a", "Wash", true), it("b", "Fold", false)];
        old[0].done_at = Some("2026-10-13T08:00:00.000Z".into());
        old[0].done_by = Some("u1".into());
        // Already ticked: keeps its record even if the client sends another.
        let mut next = vec![it("a", "Wash", true), it("b", "Fold", true)];
        next[0].done_at = Some("2026-10-14T09:00:00Z".into());
        let got = stamp(&old, next, now, "u2");
        assert_eq!(
            at(&got[0]),
            (Some("2026-10-13T08:00:00.000Z".into()), Some("u1".into()))
        );
        // Newly ticked without a time: now, by the caller.
        assert_eq!(
            at(&got[1]),
            (Some("2026-10-14T10:00:00.000Z".into()), Some("u2".into()))
        );
        // An offline tick keeps its (past) time, normalised to UTC.
        let mut next = vec![it("b", "Fold", true)];
        next[0].done_at = Some("2026-10-13T23:30:00+02:00".into());
        assert_eq!(
            stamp(&old, next, now, "u2")[0].done_at.as_deref(),
            Some("2026-10-13T21:30:00.000Z")
        );
        // Future or malformed times become now.
        for bad in ["2026-10-15T00:00:00Z", "yesterday"] {
            let mut next = vec![it("b", "Fold", true)];
            next[0].done_at = Some(bad.into());
            assert_eq!(
                stamp(&old, next, now, "u2")[0].done_at.as_deref(),
                Some("2026-10-14T10:00:00.000Z")
            );
        }
        // Unticking clears it.
        let mut next = vec![it("a", "Wash", false)];
        next[0].done_by = Some("u1".into());
        assert_eq!(at(&stamp(&old, next, now, "u2")[0]), (None, None));
        // Fresh copies and kept ticks carry the record along or drop it.
        assert_eq!(at(&fresh(&old)[0]), (None, None));
        assert_eq!(
            at(&keep_ticks(&[it("a", "Wash!", false)], &old)[0]),
            at(&old[0])
        );
    }

    #[test]
    fn normalizes() {
        let got = normalize(vec![
            it("a", " Sort ", true),
            it("b", "   ", false),
            it("c", "Wash", false),
        ])
        .unwrap();
        assert_eq!(got, vec![it("a", "Sort", true), it("c", "Wash", false)]);
        assert!(normalize(vec![it("a", "x", false), it("a", "y", false)]).is_err());
        assert!(normalize(vec![it("a b", "x", false)]).is_err());
        assert!(normalize(vec![it("", "x", false)]).is_err());
        assert!(normalize(vec![it("a", &"x".repeat(201), false)]).is_err());
        let many: Vec<Item> = (0..51).map(|i| it(&i.to_string(), "x", false)).collect();
        assert!(normalize(many).is_err());
    }

    #[test]
    fn fresh_copies_and_kept_ticks() {
        let old = vec![it("a", "Sort", true), it("b", "Wash", true)];
        assert_eq!(
            fresh(&old),
            vec![it("a", "Sort", false), it("b", "Wash", false)]
        );
        let new = vec![it("b", "Wash (40°)", false), it("c", "Hang", false)];
        assert_eq!(
            keep_ticks(&new, &old),
            vec![it("b", "Wash (40°)", true), it("c", "Hang", false)]
        );
        assert_eq!(progress(&old), (2, 2));
        assert_eq!(progress(&new), (0, 2));
    }

    #[test]
    fn checklist_completes_and_reopens() {
        let half = vec![it("a", "Sort", true), it("b", "Wash", false)];
        let all = vec![it("a", "Sort", true), it("b", "Wash", true)];
        // Ticking the last item completes an open task.
        assert_eq!(status_after(&half, &all, "open"), Some("done"));
        // Removing the last unticked item does too.
        assert_eq!(status_after(&half, &all[..1], "open"), Some("done"));
        // Unticking an item reopens a done task.
        assert_eq!(status_after(&all, &half, "done"), Some("open"));
        // Nothing else changes the status.
        assert_eq!(
            status_after(&all, &all, "open"),
            None,
            "reopened by hand, list still ticked"
        );
        assert_eq!(status_after(&[], &[], "open"), None);
        assert_eq!(status_after(&half, &all, "skipped"), None);
        assert_eq!(status_after(&half, &all, "missed"), None);
        assert_eq!(status_after(&all, &half, "wont_do"), None);
        // A new unticked item on a done task isn't an untick.
        let more = vec![
            it("a", "Sort", true),
            it("b", "Wash", true),
            it("c", "Fold", false),
        ];
        assert_eq!(status_after(&all, &more, "done"), None);
        assert_eq!(status_after(&half, &half, "done"), None);
    }
}
