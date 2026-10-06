//! Auto-suggest a day plan (SPEC §6.8, D-19: suggest only, on request). Deterministic
//! and explainable: tasks are ranked by a documented score, then placed greedily into
//! the earliest free time of the best-matching block, never on top of busy time. Every
//! placement carries the reasons the UI shows.
//!
//! Score (higher first): overdue 100 · due today 90 · expires today 80 · due tomorrow 60 ·
//! due in 2–3 days 30 · due within a week 10; plus urgency × 10 and importance × 8 (0–3
//! each). Ties: harder first (so hard work lands earlier), then by id.
//!
//! Block choice: hard tasks (difficulty 3) prefer "hard" blocks, easy ones (1) prefer
//! "easy" blocks, others prefer "medium" or unthemed blocks; within a preference, the
//! earliest block wins. Without blocks, the day's window is used.

use serde::{Deserialize, Serialize};

use crate::conflicts::next_free_slot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Energy {
    Hard,
    Medium,
    Easy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: String,
    pub start: u32,
    pub end: u32,
    pub energy: Option<Energy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    /// Estimated minutes; `None` = unknown (30 is assumed).
    pub estimate: Option<u32>,
    /// 1 (easy) – 3 (hard).
    pub difficulty: Option<u8>,
    pub importance: u8,
    pub urgency: u8,
    /// Days until the due date (negative = overdue).
    pub due_in_days: Option<i64>,
    /// An "expires" task that's only doable today.
    pub expires_today: bool,
    /// Waiting for a prerequisite.
    pub blocked: bool,
}

pub struct Input {
    pub blocks: Vec<Block>,
    /// Busy time: events and already scheduled tasks.
    pub busy: Vec<(u32, u32)>,
    pub tasks: Vec<Candidate>,
    /// The user's available part of the day.
    pub window: (u32, u32),
    /// Don't place anything before this (now, for today).
    pub from: u32,
}

/// Why a task was placed where it was. Rendered by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Reason {
    Overdue,
    DueToday,
    DueTomorrow,
    DueSoon { days: i64 },
    ExpiresToday,
    Urgent,
    Important,
    HardInHardBlock,
    EasyInEasyBlock,
    EarliestFreeTime,
    EstimateAssumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placement {
    pub task_id: String,
    /// `None` when the day has no blocks.
    pub block_id: Option<String>,
    pub start: u32,
    pub minutes: u32,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Leftover {
    /// Waiting for a prerequisite.
    Blocked,
    /// No free time of this length in any block.
    NoRoom { minutes: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub placements: Vec<Placement>,
    pub leftovers: Vec<(String, Leftover)>,
}

pub const ASSUMED_ESTIMATE: u32 = 30;

pub fn score(c: &Candidate) -> i64 {
    let due = match c.due_in_days {
        Some(d) if d < 0 => 100,
        Some(0) => 90,
        Some(1) => 60,
        Some(2..=3) => 30,
        Some(4..=7) => 10,
        _ => 0,
    };
    let expires = if c.expires_today { 80 } else { 0 };
    due.max(expires) + i64::from(c.urgency.min(3)) * 10 + i64::from(c.importance.min(3)) * 8
}

fn deadline_reasons(c: &Candidate) -> Vec<Reason> {
    let mut r = vec![];
    match c.due_in_days {
        Some(d) if d < 0 => r.push(Reason::Overdue),
        Some(0) => r.push(Reason::DueToday),
        Some(1) => r.push(Reason::DueTomorrow),
        Some(d @ 2..=7) => r.push(Reason::DueSoon { days: d }),
        _ => {}
    }
    if c.expires_today {
        r.push(Reason::ExpiresToday);
    }
    if c.urgency >= 2 {
        r.push(Reason::Urgent);
    }
    if c.importance >= 2 {
        r.push(Reason::Important);
    }
    r
}

/// Lower is a better match of task difficulty to block energy.
fn preference(difficulty: Option<u8>, energy: Option<Energy>) -> u8 {
    use Energy::*;
    match (difficulty, energy) {
        (Some(3), Some(Hard)) | (Some(1), Some(Easy)) => 0,
        (Some(3), Some(Medium) | None) | (Some(1), Some(Medium) | None) => 1,
        (Some(3), Some(Easy)) | (Some(1), Some(Hard)) => 2,
        (_, Some(Medium) | None) => 0,
        (_, Some(Hard) | Some(Easy)) => 1,
    }
}

pub fn suggest(input: &Input) -> Plan {
    let mut order: Vec<&Candidate> = input.tasks.iter().collect();
    order.sort_by(|a, b| {
        score(b)
            .cmp(&score(a))
            .then(b.difficulty.unwrap_or(2).cmp(&a.difficulty.unwrap_or(2)))
            .then(a.id.cmp(&b.id))
    });
    let mut busy = input.busy.clone();
    let mut plan = Plan {
        placements: vec![],
        leftovers: vec![],
    };
    let window_block = [Block {
        id: String::new(),
        start: input.window.0,
        end: input.window.1,
        energy: None,
    }];
    let blocks: &[Block] = if input.blocks.is_empty() {
        &window_block
    } else {
        &input.blocks
    };
    for c in order {
        if c.blocked {
            plan.leftovers.push((c.id.clone(), Leftover::Blocked));
            continue;
        }
        let minutes = c.estimate.filter(|&m| m > 0).unwrap_or(ASSUMED_ESTIMATE);
        let mut choices: Vec<&Block> = blocks.iter().collect();
        choices.sort_by_key(|b| (preference(c.difficulty, b.energy), b.start, b.id.clone()));
        let found = choices.iter().find_map(|b| {
            let lo = b.start.max(input.from);
            next_free_slot(&busy, minutes, lo, (lo, b.end)).map(|s| (*b, s))
        });
        let Some((block, start)) = found else {
            plan.leftovers
                .push((c.id.clone(), Leftover::NoRoom { minutes }));
            continue;
        };
        busy.push((start, start + minutes));
        let mut reasons = deadline_reasons(c);
        match (c.difficulty, block.energy) {
            (Some(3), Some(Energy::Hard)) => reasons.push(Reason::HardInHardBlock),
            (Some(1), Some(Energy::Easy)) => reasons.push(Reason::EasyInEasyBlock),
            _ => {}
        }
        reasons.push(Reason::EarliestFreeTime);
        if c.estimate.is_none() {
            reasons.push(Reason::EstimateAssumed);
        }
        plan.placements.push(Placement {
            task_id: c.id.clone(),
            block_id: (!block.id.is_empty()).then(|| block.id.clone()),
            start,
            minutes,
            reasons,
        });
    }
    plan.placements
        .sort_by_key(|p| (p.start, p.task_id.clone()));
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str) -> Candidate {
        Candidate {
            id: id.into(),
            estimate: Some(60),
            difficulty: None,
            importance: 0,
            urgency: 0,
            due_in_days: None,
            expires_today: false,
            blocked: false,
        }
    }
    const H: u32 = 60;

    #[test]
    fn golden_day() {
        // Deep work 08–12 (hard), admin 13–15 (easy); a standup 09:00–09:30.
        let input = Input {
            blocks: vec![
                Block {
                    id: "deep".into(),
                    start: 8 * H,
                    end: 12 * H,
                    energy: Some(Energy::Hard),
                },
                Block {
                    id: "admin".into(),
                    start: 13 * H,
                    end: 15 * H,
                    energy: Some(Energy::Easy),
                },
            ],
            busy: vec![(9 * H, 9 * H + 30)],
            tasks: vec![
                Candidate {
                    difficulty: Some(3),
                    due_in_days: Some(1),
                    estimate: Some(90),
                    ..task("report")
                },
                Candidate {
                    difficulty: Some(1),
                    estimate: Some(30),
                    ..task("email")
                },
                Candidate {
                    difficulty: Some(3),
                    importance: 3,
                    ..task("design")
                },
                Candidate {
                    difficulty: Some(1),
                    urgency: 2,
                    estimate: None,
                    ..task("invoice")
                },
                Candidate {
                    blocked: true,
                    ..task("dry")
                },
            ],
            window: (8 * H, 18 * H),
            from: 0,
        };
        let plan = suggest(&input);
        let got: Vec<_> = plan
            .placements
            .iter()
            .map(|p| {
                (
                    p.task_id.as_str(),
                    p.block_id.as_deref(),
                    p.start,
                    p.minutes,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                // "report" (due tomorrow: 60) goes first: 08:00 is free for 60 min only → after the standup.
                ("design", Some("deep"), 8 * H, 60),
                ("report", Some("deep"), 9 * H + 30, 90),
                ("invoice", Some("admin"), 13 * H, 30),
                ("email", Some("admin"), 13 * H + 30, 30),
            ]
        );
        let report = plan
            .placements
            .iter()
            .find(|p| p.task_id == "report")
            .unwrap();
        assert_eq!(
            report.reasons,
            [
                Reason::DueTomorrow,
                Reason::HardInHardBlock,
                Reason::EarliestFreeTime
            ]
        );
        let invoice = plan
            .placements
            .iter()
            .find(|p| p.task_id == "invoice")
            .unwrap();
        assert_eq!(
            invoice.reasons,
            [
                Reason::Urgent,
                Reason::EasyInEasyBlock,
                Reason::EarliestFreeTime,
                Reason::EstimateAssumed
            ]
        );
        assert_eq!(plan.leftovers, [("dry".into(), Leftover::Blocked)]);
    }

    #[test]
    fn never_on_busy_time_and_leftovers_when_full() {
        let input = Input {
            blocks: vec![],
            busy: vec![(8 * H, 12 * H)],
            tasks: vec![
                Candidate {
                    estimate: Some(4 * H),
                    ..task("a")
                },
                Candidate {
                    estimate: Some(5 * H),
                    ..task("b")
                },
            ],
            window: (8 * H, 16 * H),
            from: 0,
        };
        let plan = suggest(&input);
        assert_eq!(plan.placements.len(), 1);
        assert_eq!(
            (
                plan.placements[0].start,
                plan.placements[0].block_id.clone()
            ),
            (12 * H, None)
        );
        assert_eq!(
            plan.leftovers,
            [("b".into(), Leftover::NoRoom { minutes: 300 })]
        );
    }

    #[test]
    fn deterministic_and_respects_now() {
        let input = Input {
            blocks: vec![],
            busy: vec![],
            tasks: vec![task("b"), task("a"), task("c")],
            window: (8 * H, 18 * H),
            from: 14 * H + 10,
        };
        let a = suggest(&input);
        assert_eq!(a, suggest(&input));
        assert_eq!(
            a.placements
                .iter()
                .map(|p| (p.task_id.as_str(), p.start))
                .collect::<Vec<_>>(),
            [("a", 850), ("b", 910), ("c", 970)]
        );
    }

    #[test]
    fn scores() {
        assert_eq!(
            score(&Candidate {
                due_in_days: Some(-2),
                ..task("x")
            }),
            100
        );
        assert_eq!(
            score(&Candidate {
                expires_today: true,
                urgency: 1,
                ..task("x")
            }),
            90
        );
        assert_eq!(
            score(&Candidate {
                due_in_days: Some(5),
                importance: 3,
                ..task("x")
            }),
            34
        );
    }
}
