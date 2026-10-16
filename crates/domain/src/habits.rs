//! Habit goals (D-105): intentions kept over a period ("eat less junk"), answered per day
//! in the day reflection with yes / partly / no, and judged per week in the review.

use chrono::{Duration, NaiveDate};

/// How long the score on the Goals page looks back.
pub const SCORE_DAYS: i64 = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Yes,
    Partly,
    No,
}

impl Answer {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "yes" => Self::Yes,
            "partly" => Self::Partly,
            "no" => Self::No,
            _ => return None,
        })
    }

    /// Yes counts fully, partly half, no not at all.
    pub fn weight(self) -> f64 {
        match self {
            Self::Yes => 1.0,
            Self::Partly => 0.5,
            Self::No => 0.0,
        }
    }
}

/// Whether a habit applies on `day`: from its start (if any) to its end (if any).
pub fn in_period(start: Option<NaiveDate>, end: Option<NaiveDate>, day: NaiveDate) -> bool {
    start.is_none_or(|s| s <= day) && end.is_none_or(|e| day <= e)
}

/// The last `days` days up to `today` (or the habit's end, if earlier), not before its
/// start. `None` when the habit hasn't started yet.
pub fn window(
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    today: NaiveDate,
    days: i64,
) -> Option<(NaiveDate, NaiveDate)> {
    let to = end.map_or(today, |e| e.min(today));
    let mut from = to - Duration::days(days - 1);
    if let Some(s) = start {
        from = from.max(s);
    }
    (from <= to).then_some((from, to))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub yes: u32,
    pub partly: u32,
    pub no: u32,
    /// Days in the window without an answer.
    pub unanswered: u32,
    /// 0–1 over the answered days (partly = half); `None` = nothing answered.
    pub score: Option<f64>,
}

/// Answers within `from..=to` (other days are ignored).
pub fn stats(answers: &[(NaiveDate, Answer)], from: NaiveDate, to: NaiveDate) -> Stats {
    let mut s = Stats::default();
    let mut sum = 0.0;
    let mut seen = std::collections::HashSet::new();
    for (d, a) in answers {
        if *d < from || *d > to || !seen.insert(*d) {
            continue;
        }
        sum += a.weight();
        match a {
            Answer::Yes => s.yes += 1,
            Answer::Partly => s.partly += 1,
            Answer::No => s.no += 1,
        }
    }
    let answered = s.yes + s.partly + s.no;
    let days = ((to - from).num_days() + 1).max(0) as u32;
    s.unanswered = days.saturating_sub(answered);
    s.score = (answered > 0).then(|| sum / f64::from(answered));
    s
}

/// Days in a row answered "yes", up to today (or yesterday, while today is unanswered).
pub fn streak(answers: &[(NaiveDate, Answer)], today: NaiveDate) -> u32 {
    let yes: std::collections::HashSet<NaiveDate> = answers
        .iter()
        .filter(|(_, a)| *a == Answer::Yes)
        .map(|(d, _)| *d)
        .collect();
    let answered_today = answers.iter().any(|(d, _)| *d == today);
    let mut day = if answered_today {
        today
    } else {
        today - Duration::days(1)
    };
    let mut n = 0;
    while yes.contains(&day) {
        n += 1;
        day -= Duration::days(1);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn parsing_and_weights() {
        assert_eq!(Answer::parse("partly"), Some(Answer::Partly));
        assert_eq!(Answer::parse("maybe"), None);
        assert_eq!(Answer::Partly.weight(), 0.5);
    }

    #[test]
    fn periods_and_windows() {
        let today = d("2026-10-15");
        assert!(in_period(None, None, today));
        assert!(!in_period(Some(d("2026-10-16")), None, today));
        assert!(!in_period(None, Some(d("2026-10-14")), today));
        assert_eq!(window(None, None, today, 7), Some((d("2026-10-09"), today)));
        assert_eq!(
            window(Some(d("2026-10-13")), None, today, 7),
            Some((d("2026-10-13"), today))
        );
        assert_eq!(
            window(None, Some(d("2026-10-10")), today, 7),
            Some((d("2026-10-04"), d("2026-10-10")))
        );
        assert_eq!(window(Some(d("2026-10-20")), None, today, 7), None);
    }

    #[test]
    fn scores_and_counts() {
        let a = [
            (d("2026-10-12"), Answer::Yes),
            (d("2026-10-13"), Answer::Partly),
            (d("2026-10-14"), Answer::No),
            (d("2026-10-15"), Answer::Yes),
            (d("2026-10-01"), Answer::No), // outside
        ];
        let s = stats(&a, d("2026-10-09"), d("2026-10-15"));
        assert_eq!((s.yes, s.partly, s.no, s.unanswered), (2, 1, 1, 3));
        assert_eq!(s.score, Some(2.5 / 4.0));
        assert_eq!(stats(&[], d("2026-10-09"), d("2026-10-15")).score, None);
    }

    #[test]
    fn streaks() {
        let a = [
            (d("2026-10-12"), Answer::No),
            (d("2026-10-13"), Answer::Yes),
            (d("2026-10-14"), Answer::Yes),
        ];
        // Today not answered yet: counts up to yesterday.
        assert_eq!(streak(&a, d("2026-10-15")), 2);
        let mut b = a.to_vec();
        b.push((d("2026-10-15"), Answer::Partly));
        assert_eq!(streak(&b, d("2026-10-15")), 0, "partly breaks the run");
        b.pop();
        b.push((d("2026-10-15"), Answer::Yes));
        assert_eq!(streak(&b, d("2026-10-15")), 3);
    }
}
