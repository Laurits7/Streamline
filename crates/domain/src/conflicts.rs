//! Double-booking (SPEC §6.11): which scheduled things overlap on a day, and where the next
//! free slot is. Times are minutes from local midnight; intervals are half-open, so items
//! that only touch (09:00–10:00 and 10:00–11:00) don't conflict. Mirrored in
//! `web/src/lib/conflicts.ts`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A scheduled task (a day entry with a time).
    Task,
    /// A busy calendar event.
    Event,
    /// A time block of the day plan (tasks are meant to go inside them).
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub a: String,
    pub a_kind: Kind,
    pub b: String,
    pub b_kind: Kind,
    /// Overlap in minutes.
    pub minutes: u32,
}

/// Whether two kinds of items may not overlap: tasks inside blocks are fine, and events
/// overlapping each other are the calendar's business.
fn clash(a: Kind, b: Kind) -> bool {
    !matches!(
        (a, b),
        (Kind::Task, Kind::Block) | (Kind::Block, Kind::Task) | (Kind::Event, Kind::Event)
    )
}

/// Every clashing pair that overlaps, earliest first. Zero-length items never conflict.
pub fn find(items: &[Item]) -> Vec<Conflict> {
    let mut sorted: Vec<&Item> = items.iter().filter(|i| i.end > i.start).collect();
    sorted.sort_by(|x, y| (x.start, x.end, &x.id).cmp(&(y.start, y.end, &y.id)));
    let mut out = vec![];
    for (i, a) in sorted.iter().enumerate() {
        for b in &sorted[i + 1..] {
            if b.start >= a.end {
                break;
            }
            if clash(a.kind, b.kind) {
                out.push(Conflict {
                    a: a.id.clone(),
                    a_kind: a.kind,
                    b: b.id.clone(),
                    b_kind: b.kind,
                    minutes: a.end.min(b.end) - b.start,
                });
            }
        }
    }
    out
}

/// The earliest start at or after `from` where `minutes` fit inside `window` without
/// touching any busy interval. Free time is tried at `from` and right after each busy end.
pub fn next_free_slot(
    busy: &[(u32, u32)],
    minutes: u32,
    from: u32,
    window: (u32, u32),
) -> Option<u32> {
    let mut candidates: Vec<u32> = std::iter::once(from.max(window.0))
        .chain(
            busy.iter()
                .map(|b| b.1)
                .filter(|&e| e >= from.max(window.0)),
        )
        .collect();
    candidates.sort_unstable();
    candidates.into_iter().find(|&s| {
        s + minutes <= window.1
            && busy
                .iter()
                .all(|&(bs, be)| be <= bs || s + minutes <= bs || s >= be)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: &str, kind: Kind, start: u32, end: u32) -> Item {
        Item {
            id: id.into(),
            kind,
            start,
            end,
        }
    }

    #[test]
    fn touching_is_fine_overlapping_is_not() {
        let items = [
            it("standup", Kind::Event, 540, 570),
            it("write", Kind::Task, 570, 630), // starts as the standup ends
            it("call", Kind::Task, 600, 660),  // overlaps "write" by 30
            it("lunch", Kind::Event, 650, 700), // overlaps "call" by 10
        ];
        let c = find(&items);
        assert_eq!(
            c,
            [
                Conflict {
                    a: "write".into(),
                    a_kind: Kind::Task,
                    b: "call".into(),
                    b_kind: Kind::Task,
                    minutes: 30
                },
                Conflict {
                    a: "call".into(),
                    a_kind: Kind::Task,
                    b: "lunch".into(),
                    b_kind: Kind::Event,
                    minutes: 10
                },
            ]
        );
    }

    #[test]
    fn what_may_overlap() {
        let items = [
            it("deep", Kind::Block, 480, 720),
            it("task", Kind::Task, 500, 560), // inside the block: fine
            it("meet", Kind::Event, 600, 660), // event inside a block: conflict
            it("meet2", Kind::Event, 630, 690), // overlaps "meet", which is not ours to judge
            it("admin", Kind::Block, 700, 780), // blocks overlapping: conflict
            it("point", Kind::Event, 510, 510), // zero length: ignored
        ];
        let pairs: Vec<_> = find(&items)
            .iter()
            .map(|c| (c.a.clone(), c.b.clone(), c.minutes))
            .collect();
        assert_eq!(
            pairs,
            [
                ("deep".into(), "meet".into(), 60),
                ("deep".into(), "meet2".into(), 60),
                ("deep".into(), "admin".into(), 20)
            ]
        );
    }

    #[test]
    fn all_day_and_late_items() {
        // A blocking all-day event covers the whole day; an item running past midnight
        // is clipped at 24:00 by the caller and still conflicts.
        let items = [
            it("trip", Kind::Event, 0, 1440),
            it("late", Kind::Task, 1380, 1440),
        ];
        assert_eq!(find(&items).len(), 1);
        assert!(find(&[it("a", Kind::Task, 1380, 1440), it("b", Kind::Task, 0, 60)]).is_empty());
    }

    #[test]
    fn free_slots() {
        let busy = [(540, 570), (600, 660)];
        let day = (480, 1080);
        assert_eq!(
            next_free_slot(&busy, 30, 540, day),
            Some(570),
            "right after the first busy item"
        );
        assert_eq!(
            next_free_slot(&busy, 45, 540, day),
            Some(660),
            "the 30-minute gap is too short"
        );
        assert_eq!(
            next_free_slot(&busy, 30, 400, day),
            Some(480),
            "not before the window"
        );
        assert_eq!(
            next_free_slot(&busy, 60, 1030, day),
            None,
            "doesn't fit before the window ends"
        );
        assert_eq!(next_free_slot(&[], 60, 1020, day), Some(1020));
    }
}
