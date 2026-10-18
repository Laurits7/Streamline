//! Long-term goals (SPEC §6.10, D-21: derived progress with a manual override), the goal
//! tree's progress roll-up (D-112) and when the periodic review is due.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};

/// What a goal's progress is derived from: its milestones and its linked work (linked
/// tasks plus every task in linked projects). Skipped / won't-do items don't count.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub milestones_done: u32,
    pub milestones: u32,
    pub tasks_done: u32,
    pub tasks: u32,
}

/// 0.0–1.0. The override wins; otherwise the share of done items (`None` = nothing to
/// derive from yet).
pub fn progress(c: Counts, manual: Option<f64>) -> Option<f64> {
    if let Some(m) = manual {
        return Some(m.clamp(0.0, 1.0));
    }
    let total = c.milestones + c.tasks;
    (total > 0).then(|| f64::from(c.milestones_done + c.tasks_done) / f64::from(total))
}

/// A linked routine's part: done ÷ (done + missed) of its recent occurrences (skipped ones
/// are neutral). `None` = nothing decided yet.
pub fn routine_rate(done: u32, missed: u32) -> Option<f64> {
    (done + missed > 0).then(|| f64::from(done) / f64::from(done + missed))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// Reached once: milestones, linked work, sub-goals, routines.
    #[default]
    Outcome,
    /// Kept day by day; its progress is its habit score (D-105).
    Habit,
    /// A rule for oneself ("eat less junk"): no daily question and no progress (D-112).
    Rule,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "outcome" => Self::Outcome,
            "habit" => Self::Habit,
            "rule" => Self::Rule,
            _ => return None,
        })
    }
}

/// One goal's inputs to the tree roll-up.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub parent: Option<String>,
    pub kind: Kind,
    /// `active`, `paused`, `achieved` or `dropped`.
    pub status: String,
    /// Its own linked work (milestones, tasks, projects) as one part.
    pub own: Option<f64>,
    /// Each linked routine's rate ([`routine_rate`]), one part each.
    pub routines: Vec<Option<f64>>,
    /// Habits: their score.
    pub habit: Option<f64>,
    pub manual: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rolled {
    /// From the parts, even when overridden.
    pub derived: Option<f64>,
    /// What's shown: the override, else `derived`.
    pub progress: Option<f64>,
}

/// Progress of every goal of a tree (D-112). An outcome's derived progress is the average
/// of its parts: its own linked work, each linked routine, and each sub-goal (achieved =
/// 100 %; dropped ones and rules left out; parts without a value left out). Habits keep
/// their habit score; rules have none. The manual override wins.
pub fn roll_up(nodes: &HashMap<String, Node>) -> HashMap<String, Rolled> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, n) in nodes {
        if let Some(p) = n.parent.as_deref().filter(|p| nodes.contains_key(*p)) {
            children.entry(p).or_default().push(id.as_str());
        }
    }
    fn visit<'a>(
        id: &'a str,
        nodes: &'a HashMap<String, Node>,
        children: &HashMap<&'a str, Vec<&'a str>>,
        out: &mut HashMap<String, Rolled>,
        seen: &mut HashSet<&'a str>,
    ) -> Rolled {
        if let Some(r) = out.get(id) {
            return *r;
        }
        // A cycle in stored data: count the goal as nothing here instead of looping.
        if !seen.insert(id) {
            return Rolled::default();
        }
        let n = &nodes[id];
        let derived = match n.kind {
            Kind::Rule => None,
            Kind::Habit => n.habit,
            Kind::Outcome => {
                let mut parts: Vec<f64> = n.own.into_iter().collect();
                parts.extend(n.routines.iter().flatten());
                for c in children.get(id).into_iter().flatten() {
                    let r = visit(c, nodes, children, out, seen);
                    let child = &nodes[*c];
                    parts.extend(match (child.kind, child.status.as_str()) {
                        (Kind::Rule, _) | (_, "dropped") => None,
                        (_, "achieved") => Some(1.0),
                        _ => r.progress,
                    });
                }
                (!parts.is_empty()).then(|| parts.iter().sum::<f64>() / parts.len() as f64)
            }
        };
        let progress = match n.kind {
            Kind::Rule => None,
            _ => n.manual.map(|m| m.clamp(0.0, 1.0)).or(derived),
        };
        let r = Rolled { derived, progress };
        out.insert(id.to_string(), r);
        r
    }
    let mut out = HashMap::new();
    for id in nodes.keys() {
        visit(id, nodes, &children, &mut out, &mut HashSet::new());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Off,
    Weekly,
    Monthly,
}

impl Cadence {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            _ => return None,
        })
    }
}

/// The current review period's first day: the week's first day (`week_start`, ISO
/// weekday) or the 1st of the month.
pub fn period_start(cadence: Cadence, today: NaiveDate, week_start: u32) -> Option<NaiveDate> {
    match cadence {
        Cadence::Off => None,
        Cadence::Weekly => {
            let ws = week_start.clamp(1, 7);
            let offset = (today.weekday().number_from_monday() + 7 - ws) % 7;
            Some(today - Duration::days(i64::from(offset)))
        }
        Cadence::Monthly => today.with_day(1),
    }
}

/// A review is due once per period, from its first day on, unless one was done in it.
pub fn review_due(
    cadence: Cadence,
    last_review: Option<NaiveDate>,
    today: NaiveDate,
    week_start: u32,
) -> bool {
    match period_start(cadence, today, week_start) {
        None => false,
        Some(start) => last_review.is_none_or(|l| l < start),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn derived_and_overridden() {
        let c = Counts {
            milestones_done: 1,
            milestones: 2,
            tasks_done: 3,
            tasks: 6,
        };
        assert_eq!(progress(c, None), Some(0.5));
        assert_eq!(progress(c, Some(0.9)), Some(0.9));
        assert_eq!(progress(c, Some(1.4)), Some(1.0));
        assert_eq!(progress(Counts::default(), None), None);
    }

    fn node(parent: Option<&str>, kind: Kind, status: &str, own: Option<f64>) -> Node {
        Node {
            parent: parent.map(String::from),
            kind,
            status: status.into(),
            own,
            ..Default::default()
        }
    }

    fn nodes(v: Vec<(&str, Node)>) -> HashMap<String, Node> {
        v.into_iter().map(|(k, n)| (k.to_string(), n)).collect()
    }

    #[test]
    fn routine_rates() {
        assert_eq!(routine_rate(3, 1), Some(0.75));
        assert_eq!(routine_rate(0, 2), Some(0.0));
        assert_eq!(routine_rate(0, 0), None);
    }

    #[test]
    fn tree_roll_up() {
        // life > money (40 %), health (paused; own 0 %, running 3 of 4) > junk rule;
        // an old dropped goal and an empty one.
        let mut n = nodes(vec![
            ("life", node(None, Kind::Outcome, "active", None)),
            (
                "money",
                node(Some("life"), Kind::Outcome, "active", Some(0.4)),
            ),
            (
                "health",
                node(Some("life"), Kind::Outcome, "paused", Some(0.0)),
            ),
            ("junk", node(Some("health"), Kind::Rule, "active", None)),
            (
                "old",
                node(Some("life"), Kind::Outcome, "dropped", Some(0.9)),
            ),
            ("empty", node(Some("life"), Kind::Outcome, "active", None)),
        ]);
        n.get_mut("health").unwrap().routines = vec![routine_rate(3, 1), None];
        let r = roll_up(&n);
        // Health: own 0 and running 0.75; the undecided routine and the rule left out.
        assert_eq!(r["health"].progress, Some(0.375));
        assert_eq!(r["junk"], Rolled::default(), "rules have no progress");
        assert_eq!(r["empty"].progress, None, "nothing to measure");
        // Life: money and (paused) health; the dropped and the empty one left out.
        assert_eq!(r["life"].progress, Some((0.4 + 0.375) / 2.0));

        // Achieved counts fully; an override wins and feeds the parent.
        n.get_mut("money").unwrap().status = "achieved".into();
        n.get_mut("health").unwrap().manual = Some(0.5);
        let r = roll_up(&n);
        assert_eq!(r["health"].derived, Some(0.375));
        assert_eq!(r["health"].progress, Some(0.5));
        assert_eq!(r["life"].progress, Some(0.75));

        // Habits keep their score; a manual value on a rule is ignored.
        n.insert(
            "run".into(),
            Node {
                habit: Some(0.6),
                own: Some(0.0),
                ..node(Some("life"), Kind::Habit, "active", None)
            },
        );
        n.get_mut("junk").unwrap().manual = Some(1.0);
        let r = roll_up(&n);
        assert_eq!(r["run"].progress, Some(0.6));
        assert_eq!(r["junk"].progress, None);
        assert_eq!(r["life"].progress, Some((1.0 + 0.5 + 0.6) / 3.0));
    }

    #[test]
    fn roll_up_survives_cycles_and_orphans() {
        let n = nodes(vec![
            ("a", node(Some("b"), Kind::Outcome, "active", Some(0.5))),
            ("b", node(Some("a"), Kind::Outcome, "active", Some(1.0))),
            ("c", node(Some("gone"), Kind::Outcome, "active", Some(0.2))),
        ]);
        let r = roll_up(&n);
        assert_eq!(r.len(), 3);
        assert_eq!(r["c"].progress, Some(0.2), "a missing parent: top level");
        assert!(r["a"].progress.is_some() && r["b"].progress.is_some());
    }

    #[test]
    fn reviews() {
        // Week starting Monday: Wed 7 Oct belongs to the week of Mon 5 Oct.
        assert_eq!(
            period_start(Cadence::Weekly, d("2026-10-07"), 1),
            Some(d("2026-10-05"))
        );
        assert_eq!(
            period_start(Cadence::Weekly, d("2026-10-07"), 7),
            Some(d("2026-10-04"))
        );
        assert_eq!(
            period_start(Cadence::Monthly, d("2026-10-07"), 1),
            Some(d("2026-10-01"))
        );
        assert!(review_due(Cadence::Weekly, None, d("2026-10-07"), 1));
        assert!(
            review_due(Cadence::Weekly, Some(d("2026-10-04")), d("2026-10-07"), 1),
            "last week's review"
        );
        assert!(!review_due(
            Cadence::Weekly,
            Some(d("2026-10-05")),
            d("2026-10-07"),
            1
        ));
        assert!(!review_due(
            Cadence::Monthly,
            Some(d("2026-10-02")),
            d("2026-10-31"),
            1
        ));
        assert!(review_due(
            Cadence::Monthly,
            Some(d("2026-10-31")),
            d("2026-11-01"),
            1
        ));
        assert!(!review_due(Cadence::Off, None, d("2026-10-07"), 1));
    }
}
