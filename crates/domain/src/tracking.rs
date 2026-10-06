//! Tracking (SPEC §6.2b): combining a metric's entries into one value per day, trend
//! buckets for week/month/year charts, and reading CSV imports (D-17).

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Latest,
    Average,
    Sum,
    Max,
}

impl Aggregate {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "latest" => Self::Latest,
            "average" => Self::Average,
            "sum" => Self::Sum,
            "max" => Self::Max,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub date: NaiveDate,
    /// When it was logged (RFC 3339; sorts in time order).
    pub at: String,
    pub value: f64,
}

/// One value per day.
pub fn daily(entries: &[Entry], agg: Aggregate) -> BTreeMap<NaiveDate, f64> {
    let mut by_day: BTreeMap<NaiveDate, Vec<&Entry>> = BTreeMap::new();
    for e in entries {
        by_day.entry(e.date).or_default().push(e);
    }
    by_day
        .into_iter()
        .map(|(d, mut es)| {
            es.sort_by(|a, b| a.at.cmp(&b.at));
            let vals = es.iter().map(|e| e.value);
            let v = match agg {
                Aggregate::Latest => es.last().map(|e| e.value).unwrap_or(0.0),
                Aggregate::Average => vals.clone().sum::<f64>() / es.len() as f64,
                Aggregate::Sum => vals.sum(),
                Aggregate::Max => vals.fold(f64::MIN, f64::max),
            };
            (d, v)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    /// First day of the bucket.
    pub start: NaiveDate,
    /// `None` = nothing logged.
    pub value: Option<f64>,
}

/// Chart points for the range ending on `end`: one per day for a week (7) or month (30),
/// one per week (the average of the days with values) for a year (52).
pub fn trend(daily: &BTreeMap<NaiveDate, f64>, end: NaiveDate, range: Range) -> Vec<Point> {
    let (count, step) = match range {
        Range::Week => (7, 1),
        Range::Month => (30, 1),
        Range::Year => (52, 7),
    };
    let first = end - Duration::days(count * step - 1);
    (0..count)
        .map(|i| {
            let start = first + Duration::days(i * step);
            let vals: Vec<f64> = (0..step)
                .filter_map(|d| daily.get(&(start + Duration::days(d))).copied())
                .collect();
            Point {
                start,
                value: (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64),
            }
        })
        .collect()
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim().trim_matches('"');
    for f in ["%Y-%m-%d", "%d.%m.%Y", "%Y/%m/%d", "%d/%m/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
    }
    // A timestamp: use its date part.
    s.get(..10)
        .and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok())
}

/// CSV rows `date, value[, note]` (a header row is skipped). Separators: `,` `;` or tab;
/// with `;` or tab, a decimal comma is fine (`72,5`). Dates: `YYYY-MM-DD`, `DD.MM.YYYY`.
pub fn parse_csv(text: &str) -> Result<Vec<(NaiveDate, f64, String)>, String> {
    let mut out = vec![];
    for (i, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        let sep = if line.contains('\t') {
            '\t'
        } else if line.contains(';') {
            ';'
        } else {
            ','
        };
        let cols: Vec<&str> = line.splitn(3, sep).map(str::trim).collect();
        let date = cols.first().and_then(|c| parse_date(c));
        let value = cols.get(1).and_then(|v| {
            let v = v.trim_matches('"');
            let v = if sep == ',' {
                v.to_string()
            } else {
                v.replace(',', ".")
            };
            v.parse::<f64>().ok().filter(|x| x.is_finite())
        });
        match (date, value) {
            (Some(d), Some(v)) => out.push((
                d,
                v,
                cols.get(2)
                    .map(|n| n.trim_matches('"').to_string())
                    .unwrap_or_default(),
            )),
            _ if i == 0 && out.is_empty() => continue, // header
            _ => {
                return Err(format!(
                    "line {}: expected date and number, got {line:?}",
                    i + 1
                ));
            }
        }
    }
    if out.is_empty() {
        return Err("no rows found".into());
    }
    Ok(out)
}

/// Share of `done` among tasks that were due to be done (done + missed + still open).
pub fn completion_rate(done: u32, missed: u32, open: u32) -> Option<f64> {
    let total = done + missed + open;
    (total > 0).then(|| f64::from(done) / f64::from(total))
}

/// Monday of the ISO week of `d` (used by the year view's labels).
pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(i64::from(d.weekday().num_days_from_monday()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn e(date: &str, at: &str, value: f64) -> Entry {
        Entry {
            date: d(date),
            at: at.into(),
            value,
        }
    }

    #[test]
    fn several_entries_a_day() {
        let es = [
            e("2026-10-05", "2026-10-05T20:00:00Z", 72.0),
            e("2026-10-05", "2026-10-05T07:00:00Z", 73.0),
            e("2026-10-06", "2026-10-06T07:00:00Z", 72.5),
        ];
        assert_eq!(
            daily(&es, Aggregate::Latest)[&d("2026-10-05")],
            72.0,
            "the latest by time, not by order"
        );
        assert_eq!(daily(&es, Aggregate::Average)[&d("2026-10-05")], 72.5);
        assert_eq!(daily(&es, Aggregate::Sum)[&d("2026-10-05")], 145.0);
        assert_eq!(daily(&es, Aggregate::Max)[&d("2026-10-05")], 73.0);
        assert_eq!(daily(&es, Aggregate::Latest).len(), 2);
    }

    #[test]
    fn trends() {
        let mut m = BTreeMap::new();
        m.insert(d("2026-10-06"), 4.0);
        m.insert(d("2026-10-04"), 2.0);
        let w = trend(&m, d("2026-10-06"), Range::Week);
        assert_eq!(w.len(), 7);
        assert_eq!(w[0].start, d("2026-09-30"));
        assert_eq!(
            w[6],
            Point {
                start: d("2026-10-06"),
                value: Some(4.0)
            }
        );
        assert_eq!(w[5].value, None);
        assert_eq!(trend(&m, d("2026-10-06"), Range::Month).len(), 30);
        let y = trend(&m, d("2026-10-06"), Range::Year);
        assert_eq!(y.len(), 52);
        // The last week bucket (30 Sep – 6 Oct) averages both days.
        assert_eq!(y[51].value, Some(3.0));
        assert_eq!(y[51].start, d("2026-09-30"));
    }

    #[test]
    fn csv_imports() {
        let rows =
            parse_csv("date,weight,note\n2026-01-02,80.5,after holidays\n2026-01-09, 79.8\n")
                .unwrap();
        assert_eq!(
            rows,
            [
                (d("2026-01-02"), 80.5, "after holidays".into()),
                (d("2026-01-09"), 79.8, String::new())
            ]
        );
        let rows = parse_csv("\u{feff}Kuupäev;Kaal\n02.01.2026;80,5\n").unwrap();
        assert_eq!(rows, [(d("2026-01-02"), 80.5, String::new())]);
        assert_eq!(
            parse_csv("2026-01-02T07:30:00Z\t5").unwrap()[0].0,
            d("2026-01-02")
        );
        assert!(
            parse_csv("2026-01-02,80\nnot a row")
                .unwrap_err()
                .contains("line 2")
        );
        assert!(parse_csv("date,value\n").is_err());
    }

    #[test]
    fn rates() {
        assert_eq!(completion_rate(3, 1, 0), Some(0.75));
        assert_eq!(completion_rate(0, 0, 0), None);
        assert_eq!(week_start(d("2026-10-07")), d("2026-10-05"));
    }
}
