//! The morning digest (10.6): at the user's chosen time, one message through the usual
//! channels (in-app, push, ntfy) with the day ahead. Off unless a time is set; sent once per
//! logical day, and only within two hours of that time (a server that was down at the time
//! skips the day rather than sending yesterday's morning at noon).

use axum::{Json, extract::State};
use chrono::{Duration, NaiveDate, NaiveTime, Timelike};
use serde::Serialize;
use streamline_domain::digest::{Day, compose};
use streamline_domain::time::{parse_hhmm, parse_tz, resolve_local};
use ts_rs::TS;

use crate::{AppState, auth::AuthUser, error::ApiResult, models::User, notify::Notification};

/// How long after the chosen time the digest may still go out.
const WINDOW_MIN: u32 = 120;

/// The day ahead for `user` on `day`.
pub async fn gather(state: &AppState, user: &User, day: NaiveDate) -> anyhow::Result<Day> {
    let date = day.format("%Y-%m-%d").to_string();
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let from = resolve_local(tz, day.and_time(NaiveTime::MIN));
    let to = resolve_local(tz, (day + Duration::days(1)).and_time(NaiveTime::MIN));
    let ts =
        |t: chrono::DateTime<chrono::Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let db = &state.db.read;

    // The day plan: timed tasks are fixed, the rest planned in order.
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT t.title, e.start_time FROM day_entries e JOIN tasks t ON t.id = e.task_id
          WHERE e.user_id = ?1 AND e.date = ?2 AND e.deleted_at IS NULL AND t.deleted_at IS NULL AND t.status = 'open'
          ORDER BY e.start_time IS NOT NULL, e.position",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_all(db)
    .await?;
    let mut out = Day::default();
    for (title, at) in rows {
        match at {
            Some(at) => out.fixed.push((at, title)),
            None => out.planned.push(title),
        }
    }

    // Calendar events: timed ones starting today, and all-day ones.
    let events: Vec<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT e.title, e.all_day, e.start_at FROM events e JOIN calendars c ON c.id = e.calendar_id
          WHERE e.user_id = ?1 AND e.deleted_at IS NULL AND c.enabled = 1 AND (
            e.all_day = 0 AND e.start_at >= ?2 AND e.start_at < ?3
            OR e.all_day = 1 AND e.start_date <= ?4 AND e.end_date > ?4)",
    )
    .bind(&user.id)
    .bind(ts(from))
    .bind(ts(to))
    .bind(&date)
    .fetch_all(db)
    .await?;
    for (title, all_day, start) in events {
        if all_day {
            out.all_day.push(title);
        } else if let Some(s) = start
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        {
            let local = s.with_timezone(&tz);
            out.fixed
                .push((format!("{:02}:{:02}", local.hour(), local.minute()), title));
        }
    }

    // Due today or earlier and not on today's plan.
    let visible = crate::visibility::visible_sql("t.");
    let due: Vec<(String, String)> = sqlx::query_as(&format!(
        "SELECT t.title, t.due_date FROM tasks t
          WHERE {visible} AND t.deleted_at IS NULL AND t.status = 'open' AND t.due_date IS NOT NULL AND t.due_date <= ?2
            AND NOT EXISTS (SELECT 1 FROM day_entries e WHERE e.task_id = t.id AND e.date = ?2 AND e.deleted_at IS NULL)
          ORDER BY t.due_date LIMIT 20"
    ))
    .bind(&user.id)
    .bind(&date)
    .fetch_all(db)
    .await?;
    out.due = due
        .into_iter()
        .map(|(title, d)| (title, d < date))
        .collect();

    // Waiting for something that should be checked on today.
    out.check_back = sqlx::query_scalar(&format!(
        "SELECT t.title FROM tasks t
          WHERE {visible} AND t.deleted_at IS NULL AND t.status = 'open' AND t.waiting_since IS NOT NULL
            AND t.check_back_at IS NOT NULL AND t.check_back_at < ?2
          ORDER BY t.check_back_at LIMIT 20"
    ))
    .bind(&user.id)
    .bind(ts(to))
    .fetch_all(db)
    .await?;
    Ok(out)
}

/// Send the digests that are due (the one-minute job tick).
pub async fn run_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> =
        sqlx::query_as("SELECT * FROM users WHERE deleted_at IS NULL AND digest_time IS NOT NULL")
            .fetch_all(&state.db.read)
            .await?;
    for user in users {
        let day = crate::rollover::today_for(&user);
        let date = day.format("%Y-%m-%d").to_string();
        if user.last_digest_date.as_deref() == Some(date.as_str()) {
            continue;
        }
        let Some(at) = user.digest_time.as_deref().and_then(parse_hhmm) else {
            continue;
        };
        let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
        let now = chrono::Utc::now().with_timezone(&tz).time();
        let day_end = parse_hhmm(&user.day_end).unwrap_or(NaiveTime::MIN);
        // Minutes since the logical day began, so a digest after midnight works too.
        let rel = |t: NaiveTime| {
            (t.hour() * 60 + t.minute() + 1440 - (day_end.hour() * 60 + day_end.minute())) % 1440
        };
        let (now_rel, at_rel) = (rel(now), rel(at));
        if now_rel < at_rel || now_rel - at_rel > WINDOW_MIN {
            continue;
        }
        // Once per day, even if sending fails.
        sqlx::query("UPDATE users SET last_digest_date = ? WHERE id = ?")
            .bind(&date)
            .bind(&user.id)
            .execute(&state.db.write)
            .await?;
        let (title, body) = compose(&gather(state, &user, day).await?);
        crate::notify::send(
            state,
            &user.id,
            &Notification {
                kind: "digest".into(),
                title,
                body,
                url: "/".into(),
            },
        );
    }
    Ok(())
}

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DigestPreview {
    pub title: String,
    pub body: String,
}

/// What today's digest says (for Settings).
#[utoipa::path(get, path = "/me/digest", tag = "account", summary = "Preview today's morning digest", responses((status = 200, body = DigestPreview), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<DigestPreview>> {
    let day = crate::rollover::today_for(&user.user);
    let (title, body) = compose(&gather(&state, &user.user, day).await?);
    Ok(Json(DigestPreview { title, body }))
}
