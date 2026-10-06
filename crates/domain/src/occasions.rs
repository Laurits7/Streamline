//! Occasions: namedays and birthdays (SPEC §6.17, D-57). Parsing a nameday calendar, name
//! search, and when an occasion's steps (buy a present 2 days before → greet on the day)
//! fall due.

use chrono::{Datelike, Duration, NaiveDate};

/// One name on one day of the year.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nameday {
    pub month: u32,
    pub day: u32,
    pub name: String,
}

fn valid(month: u32, day: u32) -> bool {
    NaiveDate::from_ymd_opt(2024, month, day).is_some() // a leap year: 29 Feb is valid
}

/// Folds a name for searching: lowercase, Estonian/Nordic letters without accents
/// (`Tõnu` matches "tonu"), so people can type without special keys.
pub fn fold(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'õ' | 'ö' | 'ó' | 'ò' | 'ô' => 'o',
            'ä' | 'á' | 'à' | 'â' | 'å' => 'a',
            'ü' | 'ú' | 'ù' | 'û' => 'u',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'š' => 's',
            'ž' => 'z',
            c => c,
        })
        .collect()
}

fn split_names(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split([',', ';'])
        .map(|n| n.trim().trim_end_matches('.').trim())
        .filter(|n| {
            !n.is_empty()
                && n.chars()
                    .all(|c| c.is_alphabetic() || c == '-' || c == ' ' || c == '\'')
        })
        .map(String::from)
}

/// A nameday list in plain text, one day per line: `MM-DD Name, Name` or `DD.MM Name, Name`.
/// Blank lines and `#` comments are skipped. Errors name the first bad line.
pub fn parse_list(text: &str) -> Result<Vec<Nameday>, String> {
    let mut out = vec![];
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (date, names) = line
            .split_once(|c: char| c.is_whitespace() || c == ':' || c == ';')
            .ok_or_else(|| format!("line {}: expected a date and names", i + 1))?;
        let (month, day) =
            parse_date(date).ok_or_else(|| format!("line {}: bad date {date:?}", i + 1))?;
        out.extend(
            split_names(names.trim_start_matches([':', ';'])).map(|name| Nameday {
                month,
                day,
                name,
            }),
        );
    }
    if out.is_empty() {
        return Err("no namedays found".into());
    }
    Ok(out)
}

/// `MM-DD` or `DD.MM` → (month, day).
fn parse_date(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().trim_end_matches('.');
    let (month, day) = if let Some((m, d)) = s.split_once('-') {
        (m.parse().ok()?, d.parse().ok()?)
    } else {
        let (d, m) = s.split_once('.')?;
        (m.parse().ok()?, d.parse().ok()?)
    };
    valid(month, day).then_some((month, day))
}

/// The nameday page of Statistics Estonia (stat.ee/nimed/NIMEPAEVAD): a table per month
/// with rows `<td>DD.MM</td><td><a>Name</a>, <a>Name</a></td>`.
pub fn parse_stat_ee(html: &str) -> Result<Vec<Nameday>, String> {
    let mut out = vec![];
    let mut rest = html;
    while let Some(p) = rest.find("</td>") {
        let (before, after) = rest.split_at(p);
        rest = &after[5..];
        let cell = before.rsplit('>').next().unwrap_or("").trim();
        let Some((month, day)) = (cell.len() == 5).then(|| parse_date(cell)).flatten() else {
            continue;
        };
        let Some(start) = rest.trim_start().strip_prefix("<td") else {
            continue;
        };
        let body = &start[start.find('>').map_or(0, |i| i + 1)..];
        let body = &body[..body.find("</td>").unwrap_or(body.len())];
        out.extend(split_names(&strip_tags(body)).map(|name| Nameday { month, day, name }));
    }
    if out.len() < 300 {
        return Err(format!(
            "found only {} namedays; the page layout may have changed",
            out.len()
        ));
    }
    Ok(out)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// The date of a yearly occasion in `year`. 29 February falls on the 28th in other years.
pub fn in_year(month: u32, day: u32, year: i32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day).or_else(|| {
        (month == 2 && day == 29)
            .then(|| NaiveDate::from_ymd_opt(year, 2, 28))
            .flatten()
    })
}

/// One step of an occasion: e.g. "Buy a present for {name}" 2 days before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Days from the occasion: −2 = two days before.
    pub offset_days: i32,
}

/// Occasions whose tasks should exist now: those not yet past whose earliest step is at
/// most `lead_days` away. Returns each occasion's date and its steps' dates.
pub fn due_occasions(
    dates: &[(u32, u32)],
    steps: &[Step],
    today: NaiveDate,
    lead_days: i64,
) -> Vec<(NaiveDate, Vec<NaiveDate>)> {
    if steps.is_empty() {
        return vec![];
    }
    let earliest = steps.iter().map(|s| s.offset_days).min().unwrap_or(0);
    let mut out = vec![];
    for &(m, d) in dates {
        for year in [today.year(), today.year() + 1] {
            let Some(date) = in_year(m, d, year) else {
                continue;
            };
            let first = date + Duration::days(i64::from(earliest));
            if date >= today && today >= first - Duration::days(lead_days) {
                let step_dates = steps
                    .iter()
                    .map(|s| date + Duration::days(i64::from(s.offset_days)))
                    .collect();
                out.push((date, step_dates));
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn plain_lists() {
        let l =
            parse_list("# Estonian\n01-01 Algo, Alo\n15.08 Hanna; Jaana\n\n29.02: Ulmi").unwrap();
        assert_eq!(l.len(), 5);
        assert_eq!(
            l[2],
            Nameday {
                month: 8,
                day: 15,
                name: "Hanna".into()
            }
        );
        assert_eq!(
            l[4],
            Nameday {
                month: 2,
                day: 29,
                name: "Ulmi".into()
            }
        );
        assert!(parse_list("31.02 X").unwrap_err().contains("line 1"));
        assert!(parse_list("# nothing").is_err());
    }

    #[test]
    fn stat_ee_page() {
        // The page's structure (shortened): headings, dates and linked names.
        let mut html = String::from(
            r#"<html><h1>Nimepäevad</h1><p>Täna on nimepäev: <a>Bruno</a>, <a>Edmund</a>.</p>
            <h2><a>Jaanuar</a></h2><table><tr><td>01.01</td><td><a href="/nimed/ALGO">Algo</a>, <a>Alo</a>, <a>Uuno</a></td></tr>
            <tr><td>17.01</td><td><a>Tõnu</a>, <a>Tõnis</a></td></tr></table><h2>Veebruar</h2>
            <table><tr><td>29.02</td><td><a>Ulmi</a>, <a>Une</a></td></tr>
            <tr><td>28.08</td><td><a>August</a>, <a>Gustav</a></td></tr>"#,
        );
        for i in 0..300 {
            html.push_str(&format!(
                "<tr><td>{:02}.03</td><td><a>Name</a></td></tr>",
                i % 28 + 1
            ));
        }
        html.push_str("<tr><td>31.12</td><td><a>Silver</a>, <a>Silvo</a></td></tr></table><p>Nimepäevade allikas: Piret Mäeniit, Eesti Nimed.</p>");
        let l = parse_stat_ee(&html).unwrap();
        assert_eq!(
            &l[..2],
            &[
                Nameday {
                    month: 1,
                    day: 1,
                    name: "Algo".into()
                },
                Nameday {
                    month: 1,
                    day: 1,
                    name: "Alo".into()
                }
            ]
        );
        assert!(l.contains(&Nameday {
            month: 1,
            day: 17,
            name: "Tõnu".into()
        }));
        assert!(l.contains(&Nameday {
            month: 2,
            day: 29,
            name: "Une".into()
        }));
        assert!(l.contains(&Nameday {
            month: 8,
            day: 28,
            name: "August".into()
        }));
        assert_eq!(
            l.last().unwrap(),
            &Nameday {
                month: 12,
                day: 31,
                name: "Silvo".into()
            }
        );
        assert!(
            !l.iter()
                .any(|n| ["Bruno", "Nimepäevade", "Piret", "Veebruar"].contains(&n.name.as_str()))
        );
        assert!(
            parse_stat_ee("<td>01.01</td><td>Algo</td>").is_err(),
            "too few means the layout changed"
        );
    }

    #[test]
    fn folding() {
        assert_eq!(fold(" Tõnu "), "tonu");
        assert_eq!(fold("Ülle"), "ulle");
        assert_eq!(fold("Šarlota"), "sarlota");
    }

    #[test]
    fn leap_days() {
        assert_eq!(in_year(2, 29, 2027), Some(d("2027-02-28")));
        assert_eq!(in_year(2, 29, 2028), Some(d("2028-02-29")));
        assert_eq!(in_year(4, 31, 2028), None);
    }

    #[test]
    fn which_occasions_are_due() {
        let steps = [Step { offset_days: -2 }, Step { offset_days: 0 }];
        // 15 Aug: the present step (13 Aug) comes into view 7 days earlier, on 6 Aug.
        assert!(due_occasions(&[(8, 15)], &steps, d("2026-08-05"), 7).is_empty());
        assert_eq!(
            due_occasions(&[(8, 15)], &steps, d("2026-08-06"), 7),
            [(d("2026-08-15"), vec![d("2026-08-13"), d("2026-08-15")])]
        );
        assert_eq!(
            due_occasions(&[(8, 15)], &steps, d("2026-08-15"), 7).len(),
            1,
            "still today"
        );
        assert!(
            due_occasions(&[(8, 15)], &steps, d("2026-08-16"), 7).is_empty(),
            "past"
        );
        // Across the year boundary: 1 Jan next year, present on 30 Dec.
        assert_eq!(
            due_occasions(&[(1, 1)], &steps, d("2026-12-26"), 7),
            [(d("2027-01-01"), vec![d("2026-12-30"), d("2027-01-01")])]
        );
        // Across a month boundary, and a name with two days.
        let both = due_occasions(&[(3, 1), (3, 3)], &steps, d("2026-02-25"), 7);
        assert_eq!(
            both.iter().map(|o| o.0).collect::<Vec<_>>(),
            [d("2026-03-01"), d("2026-03-03")]
        );
        assert_eq!(both[0].1[0], d("2026-02-27"));
        // Leap-day birthday in a common year.
        assert_eq!(
            due_occasions(&[(2, 29)], &steps, d("2027-02-27"), 0)[0].0,
            d("2027-02-28")
        );
    }
}

#[cfg(test)]
mod live {
    /// `STAT_EE_HTML=/path/to/saved/page cargo test -p streamline-domain stat_ee_saved -- --ignored`
    #[test]
    #[ignore]
    fn stat_ee_saved() {
        let html = std::fs::read_to_string(std::env::var("STAT_EE_HTML").unwrap()).unwrap();
        let l = super::parse_stat_ee(&html).unwrap();
        let days: std::collections::BTreeSet<_> = l.iter().map(|n| (n.month, n.day)).collect();
        let missing: Vec<_> = (1..=12)
            .flat_map(|m| (1..=31).map(move |d| (m, d)))
            .filter(|&(m, d)| super::valid(m, d) && !days.contains(&(m, d)))
            .collect();
        println!("missing {missing:?}");
        println!(
            "{} names on {} days; 31.12: {:?}",
            l.len(),
            days.len(),
            l.iter()
                .filter(|n| n.month == 12 && n.day == 31)
                .map(|n| &n.name)
                .collect::<Vec<_>>()
        );
        assert_eq!(days.len(), 366);
    }
}
