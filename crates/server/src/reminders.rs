//! Planning reminders (SPEC §6.2d): once per kind and target day, at the user's
//! planning time, unless that day is already planned (D-8: one reminder, no nagging).

use chrono::Utc;
use streamline_domain::{
    planning::{PlanMode, ReminderKind, due_reminders},
    time::{parse_hhmm, parse_tz},
};

use crate::{
    AppState,
    models::User,
    notify::{self, Notification},
    rollover::today_for,
    util::now,
};

/// Send the planning reminders that are due for one user. Returns what was sent.
pub async fn run_for_user(state: &AppState, user: &User) -> anyhow::Result<Vec<Notification>> {
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let local_now = Utc::now().with_timezone(&tz).naive_local();
    let (Some(day_end), Some(evening), Some(morning), Some(mode)) = (
        parse_hhmm(&user.day_end),
        parse_hhmm(&user.plan_time_evening),
        parse_hhmm(&user.plan_time_morning),
        PlanMode::parse(&user.plan_mode),
    ) else {
        return Ok(vec![]);
    };
    let mut sent = vec![];
    for r in due_reminders(local_now, today_for(user), day_end, mode, evening, morning) {
        let date = r.target.format("%Y-%m-%d").to_string();
        let planned: Option<String> = sqlx::query_scalar(
            "SELECT id FROM day_plans WHERE user_id = ? AND date = ? AND status = 'planned' AND deleted_at IS NULL",
        )
        .bind(&user.id)
        .bind(&date)
        .fetch_optional(&state.db.read)
        .await?;
        if planned.is_some() {
            continue;
        }
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO reminder_log (user_id, kind, date, sent_at) VALUES (?,?,?,?)",
        )
        .bind(&user.id)
        .bind(r.kind.as_str())
        .bind(&date)
        .bind(now())
        .execute(&state.db.write)
        .await?
        .rows_affected();
        if inserted == 0 {
            continue; // already reminded
        }
        let n = Notification {
            kind: r.kind.as_str().into(),
            title: match r.kind {
                ReminderKind::Evening => "Time to plan tomorrow".into(),
                ReminderKind::Morning => "Plan your day".into(),
            },
            body: "A few minutes now makes the day easier.".into(),
            url: format!("/plan/{date}"),
        };
        notify::send(state, &user.id, &n);
        sent.push(n);
    }
    Ok(sent)
}

pub async fn run_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> = sqlx::query_as("SELECT * FROM users WHERE deleted_at IS NULL")
        .fetch_all(&state.db.read)
        .await?;
    for u in users {
        if let Err(e) = run_for_user(state, &u).await {
            tracing::warn!("reminders failed for user {}: {e:#}", u.id);
        }
    }
    Ok(())
}
