//! Routines (series): creating their occurrence tasks, ending flexible windows, and
//! streak statistics (SPEC §6.3). The date logic lives in `domain::recurrence`.

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use sqlx::SqliteConnection;
use streamline_domain::{
    order::key_after,
    recurrence::{Outcome, Schedule, Window, plan_occurrences, streak, window_bounds},
};
use ts_rs::TS;

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{DayEntry, Series, Task, User, upsert_entry, upsert_task},
    rollover::today_for,
    util::{new_id, now},
};

/// How far back occurrences are created when catching up after downtime.
const CATCH_UP_DAYS: i64 = 31;

pub fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn fmt(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn schedule(s: &Series) -> Option<Schedule> {
    match s.mode.as_str() {
        "flexible" => Some(Schedule::Flexible {
            times: s.times_per_window.unwrap_or(1).clamp(1, 31) as u32,
            window: Window::parse(s.window.as_deref()?)?,
        }),
        _ => Some(Schedule::Rule {
            rule: s.rrule.clone()?,
        }),
    }
}

/// Create the occurrences of one routine within `[from, to]` that don't exist yet
/// (including ones deleted on purpose). Returns the changes to broadcast.
pub async fn materialize_series(
    conn: &mut SqliteConnection,
    s: &Series,
    week_start: u32,
    from: NaiveDate,
    to: NaiveDate,
) -> anyhow::Result<Vec<Change>> {
    let (Some(sched), Some(dtstart)) = (schedule(s), date(&s.dtstart)) else {
        return Ok(vec![]);
    };
    let until = s.until.as_deref().and_then(date);
    // A routine that replaced an earlier version mid-window only adds what's still
    // missing in that first window (slots already done under the old version count).
    let mut credit: Option<(NaiveDate, u32)> = None;
    if let (Schedule::Flexible { window, .. }, Some(_)) = (&sched, &s.split_from) {
        let (ws, we) = window_bounds(dtstart, *window, week_start);
        let mut done = 0u32;
        for id in lineage(conn, s).await?.iter().skip(1) {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tasks WHERE series_id = ? AND deleted_at IS NULL AND status = 'done'
                   AND occurrence_date >= ? AND occurrence_date <= ?",
            )
            .bind(id)
            .bind(fmt(ws))
            .bind(fmt(we))
            .fetch_one(&mut *conn)
            .await?;
            done += n as u32;
        }
        credit = Some((ws, done));
    }
    let mut changes = vec![];
    for o in plan_occurrences(&sched, dtstart, until, from, to, week_start)
        .map_err(anyhow::Error::msg)?
    {
        if let (Some((ws, done)), Some(i)) = (credit, o.index)
            && o.date == ws
            && i <= done
        {
            continue;
        }
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE series_id = ? AND occurrence_key = ?")
                .bind(&s.id)
                .bind(&o.key)
                .fetch_optional(&mut *conn)
                .await?;
        if exists.is_some() {
            continue;
        }
        let rev = next_rev(conn).await?;
        let ts = now();
        let last: Option<String> = sqlx::query_scalar(
            "SELECT MAX(position) FROM tasks WHERE owner_user_id IS ? AND project_id IS ? AND deleted_at IS NULL",
        )
        .bind(&s.owner_user_id)
        .bind(&s.project_id)
        .fetch_one(&mut *conn)
        .await?;
        let t = Task {
            id: new_id(),
            owner_user_id: s.owner_user_id.clone(),
            owner_group_id: s.owner_group_id.clone(),
            assignee_user_id: None,
            project_id: s.project_id.clone(),
            title: s.title.clone(),
            notes: s.notes.clone(),
            status: "open".into(),
            position: key_after(last.as_deref()),
            due_date: Some(fmt(o.window_end.unwrap_or(o.date))),
            estimate_min: s.estimate_min,
            difficulty: s.difficulty,
            importance: s.importance,
            urgency: s.urgency,
            actual_min: 0,
            task_type_id: s.task_type_id.clone(),
            carry_count: 0,
            started_at: None,
            completed_at: None,
            completed_by: None,
            ext_source: None,
            ext_id: None,
            ext_url: None,
            also_project_ids: sqlx::types::Json(vec![]),
            series_id: Some(s.id.clone()),
            occurrence_key: Some(o.key.clone()),
            occurrence_date: Some(fmt(o.date)),
            window_end: o.window_end.map(fmt),
            created_at: ts.clone(),
            updated_at: ts.clone(),
            deleted_at: None,
            rev,
        };
        upsert_task(conn, &t).await?;
        changes.push(Change::task(&t));
        // Fixed-time routines are planned on their day's timeline right away.
        if s.mode == "anchored"
            && let Some(user_id) = &s.owner_user_id
        {
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
            )
            .bind(user_id)
            .bind(fmt(o.date))
            .fetch_one(&mut *conn)
            .await?;
            let e = DayEntry {
                id: new_id(),
                user_id: user_id.clone(),
                date: fmt(o.date),
                task_id: t.id.clone(),
                position: key_after(last.as_deref()),
                start_time: s.start_time.clone(),
                duration_min: s.duration_min,
                created_at: ts.clone(),
                updated_at: ts,
                deleted_at: None,
                rev,
            };
            upsert_entry(conn, &e).await?;
            changes.push(Change::entry(&e));
        }
    }
    Ok(changes)
}

/// The routine and the versions it replaced, newest first (ids).
pub async fn lineage(conn: &mut SqliteConnection, s: &Series) -> sqlx::Result<Vec<String>> {
    let mut out = vec![s.id.clone()];
    let mut prev = s.split_from.clone();
    while let Some(id) = prev.filter(|id| !out.contains(id) && out.len() < 50) {
        prev = sqlx::query_scalar("SELECT split_from FROM series WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
        out.push(id);
    }
    Ok(out)
}

/// Bring a user's routines up to date: occurrences through tomorrow (so tomorrow can be
/// planned in the evening), catching up on missed days after downtime.
pub async fn materialize_user(state: &AppState, user: &User) -> anyhow::Result<()> {
    let today = today_for(user);
    let to = today + Duration::days(1);
    let series: Vec<Series> = sqlx::query_as(
        "SELECT * FROM series WHERE owner_user_id = ? AND deleted_at IS NULL AND (materialized_through IS NULL OR materialized_through < ?)",
    )
    .bind(&user.id)
    .bind(fmt(to))
    .fetch_all(&state.db.read)
    .await?;
    if series.is_empty() {
        return Ok(());
    }
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    for s in series {
        let after = s
            .materialized_through
            .as_deref()
            .and_then(date)
            .map(|d| d + Duration::days(1));
        let from = after
            .unwrap_or(today)
            .max(today - Duration::days(CATCH_UP_DAYS));
        changes.extend(materialize_series(&mut tx, &s, user.week_start as u32, from, to).await?);
        sqlx::query("UPDATE series SET materialized_through = ? WHERE id = ?")
            .bind(fmt(to))
            .bind(&s.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(())
}

/// Create occurrences for one specific (usually future) day being viewed or planned.
pub async fn materialize_day(state: &AppState, user: &User, day: NaiveDate) -> anyhow::Result<()> {
    let series: Vec<Series> =
        sqlx::query_as("SELECT * FROM series WHERE owner_user_id = ? AND deleted_at IS NULL")
            .bind(&user.id)
            .fetch_all(&state.db.read)
            .await?;
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    for s in series {
        changes.extend(materialize_series(&mut tx, &s, user.week_start as u32, day, day).await?);
    }
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(())
}

pub async fn materialize_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> = sqlx::query_as(
        "SELECT * FROM users WHERE deleted_at IS NULL AND id IN (SELECT owner_user_id FROM series WHERE deleted_at IS NULL)",
    )
    .fetch_all(&state.db.read)
    .await?;
    for u in users {
        if let Err(e) = materialize_user(state, &u).await {
            tracing::warn!("materializing routines failed for user {}: {e:#}", u.id);
        }
    }
    Ok(())
}

// ---- statistics ---------------------------------------------------------------------

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct SeriesStats {
    pub series_id: String,
    /// Consecutive completed occurrences (or fully completed windows) up to now.
    pub streak: u32,
    /// Next occurrence date from today on (`YYYY-MM-DD`), if any.
    pub next_date: Option<String>,
    /// Flexible routines: completed / total in the current window.
    pub window_done: u32,
    pub window_total: u32,
}

/// A flexible routine's occurrence: its status and the end of its window.
type Slot = (String, Option<NaiveDate>);

fn outcome(status: &str, pending: bool) -> Outcome {
    match status {
        "done" => Outcome::Done,
        "missed" => Outcome::Missed,
        "skipped" | "wont_do" => Outcome::Skipped,
        _ if pending => Outcome::Pending,
        // Still open after its day (carry-on type): undecided, doesn't break the streak.
        _ => Outcome::Pending,
    }
}

pub async fn stats(state: &AppState, user: &User, s: &Series) -> anyhow::Result<SeriesStats> {
    let today = today_for(user);
    // Earlier versions of the routine count too (streaks survive schedule changes).
    let ids = lineage(&mut *state.db.read.acquire().await?, s).await?;
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT status, occurrence_date, window_end FROM tasks
         WHERE series_id IN ({placeholders}) AND deleted_at IS NULL AND occurrence_date IS NOT NULL
         ORDER BY occurrence_date, occurrence_key"
    );
    let mut q = sqlx::query_as::<_, (String, String, Option<String>)>(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(&state.db.read).await?;
    let (mut window_done, mut window_total) = (0, 0);
    let outcomes: Vec<Outcome> = if s.mode == "flexible" {
        // One outcome per window: done when every slot is done.
        let mut windows: Vec<(String, Vec<Slot>)> = vec![];
        for (status, start, end) in rows {
            let end = end.as_deref().and_then(date);
            match windows.last_mut() {
                Some((w, v)) if *w == start => v.push((status, end)),
                _ => windows.push((start, vec![(status, end)])),
            }
        }
        windows
            .iter()
            .map(|(start, slots)| {
                // The window containing today (later windows may already exist when a
                // future day was viewed).
                let current = date(start).is_some_and(|s| s <= today)
                    && slots.iter().any(|(_, e)| e.is_some_and(|e| e >= today));
                let done = slots.iter().filter(|(st, _)| st == "done").count() as u32;
                if current {
                    window_done = done;
                    window_total = slots.len() as u32;
                }
                if done == slots.len() as u32 {
                    Outcome::Done
                } else if slots.iter().any(|(st, _)| st == "missed") {
                    Outcome::Missed
                } else if current || slots.iter().any(|(st, _)| st == "open") {
                    Outcome::Pending
                } else {
                    Outcome::Skipped
                }
            })
            .collect()
    } else {
        rows.iter()
            .map(|(status, d, _)| outcome(status, date(d).is_some_and(|d| d >= today)))
            .collect()
    };
    let next_date = match (schedule(s), date(&s.dtstart)) {
        (Some(sched), Some(dtstart)) => plan_occurrences(
            &sched,
            dtstart,
            s.until.as_deref().and_then(date),
            today,
            today + Duration::days(400),
            user.week_start as u32,
        )
        .ok()
        .and_then(|o| o.first().map(|o| fmt(o.date.max(today)))),
        _ => None,
    };
    Ok(SeriesStats {
        series_id: s.id.clone(),
        streak: streak(&outcomes),
        next_date,
        window_done,
        window_total,
    })
}

/// The window containing `today` for a flexible routine's window kind (for rolling over).
pub fn current_window(
    window: &str,
    today: NaiveDate,
    week_start: u32,
) -> Option<(NaiveDate, NaiveDate)> {
    Window::parse(window).map(|w| window_bounds(today, w, week_start))
}
