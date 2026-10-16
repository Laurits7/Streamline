//! The morning digest (10.6): one short message with the day ahead: the plan, fixed times,
//! what's due, and waiting items to check on. Short enough for a phone notification.

/// What the day holds, gathered by the server.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Day {
    /// Open tasks planned for the day without a time, in plan order.
    pub planned: Vec<String>,
    /// Things at a time: (HH:MM, title), timed tasks and calendar events, any order.
    pub fixed: Vec<(String, String)>,
    /// All-day calendar events.
    pub all_day: Vec<String>,
    /// Open tasks due today or earlier that aren't planned: (title, overdue).
    pub due: Vec<(String, bool)>,
    /// Waiting tasks whose check-back time has come by the end of the day.
    pub check_back: Vec<String>,
}

/// Items listed per line before "+N more".
const PER_LINE: usize = 3;

fn list(items: &[String]) -> String {
    let shown: Vec<&str> = items.iter().take(PER_LINE).map(String::as_str).collect();
    let more = items.len().saturating_sub(PER_LINE);
    if more > 0 {
        format!("{}, +{more} more", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

/// Title and body of the digest.
pub fn compose(day: &Day) -> (String, String) {
    let mut fixed = day.fixed.clone();
    fixed.sort();
    let things = day.planned.len() + fixed.len() + day.due.len();
    let title = match things {
        0 => "Good morning: nothing planned yet".to_string(),
        1 => "Good morning: 1 thing today".to_string(),
        n => format!("Good morning: {n} things today"),
    };
    let mut lines = Vec::new();
    if !day.all_day.is_empty() {
        lines.push(format!("Today: {}", list(&day.all_day)));
    }
    if !fixed.is_empty() {
        let at: Vec<String> = fixed
            .iter()
            .map(|(t, what)| format!("{t} {what}"))
            .collect();
        lines.push(format!("At set times: {}", list(&at)));
    }
    if !day.planned.is_empty() {
        lines.push(format!("Planned: {}", list(&day.planned)));
    }
    if !day.due.is_empty() {
        let due: Vec<String> = day
            .due
            .iter()
            .map(|(t, overdue)| {
                if *overdue {
                    format!("{t} (overdue)")
                } else {
                    t.clone()
                }
            })
            .collect();
        lines.push(format!("Due: {}", list(&due)));
    }
    if !day.check_back.is_empty() {
        lines.push(format!("Check back on: {}", list(&day.check_back)));
    }
    if things == 0 {
        lines.push("Open Streamline to plan your day.".into());
    }
    (title, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_full_day() {
        let day = Day {
            planned: s(&[
                "Call plumber",
                "Buy milk",
                "Laundry",
                "Water plants",
                "Tidy desk",
            ]),
            fixed: vec![
                ("14:00".into(), "Team call".into()),
                ("09:30".into(), "Dentist".into()),
            ],
            all_day: s(&["Anna's birthday"]),
            due: vec![
                ("Pay electricity".into(), true),
                ("Return book".into(), false),
            ],
            check_back: s(&["Parcel from shop"]),
        };
        let (title, body) = compose(&day);
        assert_eq!(title, "Good morning: 9 things today");
        assert_eq!(
            body,
            "Today: Anna's birthday\nAt set times: 09:30 Dentist, 14:00 Team call\nPlanned: Call plumber, Buy milk, Laundry, +2 more\nDue: Pay electricity (overdue), Return book\nCheck back on: Parcel from shop"
        );
    }

    #[test]
    fn an_empty_day_suggests_planning() {
        let (title, body) = compose(&Day::default());
        assert_eq!(title, "Good morning: nothing planned yet");
        assert_eq!(body, "Open Streamline to plan your day.");
        let one = Day {
            planned: s(&["Laundry"]),
            ..Default::default()
        };
        assert_eq!(
            compose(&one),
            (
                "Good morning: 1 thing today".into(),
                "Planned: Laundry".into()
            )
        );
    }
}
