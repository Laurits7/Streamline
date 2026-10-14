//! Checklists inside a task (D-71): "Do laundry" → sort, wash, hang, fold. The task is
//! planned and completed as one; ticking the last item completes it (D-80).

use serde::{Deserialize, Serialize};

pub const MAX_ITEMS: usize = 50;
pub const MAX_TEXT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Stable within the list (client-chosen), so ticks survive edits and reordering.
    pub id: String,
    pub text: String,
    pub done: bool,
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
        let id_ok = !it.id.is_empty()
            && it.id.len() <= 40
            && it
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !id_ok {
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
            ..i.clone()
        })
        .collect()
}

/// A changed list applied to an existing task: items that are still there keep their
/// ticks, new ones start unticked.
pub fn keep_ticks(new: &[Item], old: &[Item]) -> Vec<Item> {
    new.iter()
        .map(|i| Item {
            done: old.iter().any(|o| o.id == i.id && o.done),
            ..i.clone()
        })
        .collect()
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
        }
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
