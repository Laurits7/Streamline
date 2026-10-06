//! Day rollover: once a user's day has ended, unfinished planned tasks are carried
//! over or marked missed according to their task type (SPEC §6.2c). Idempotent:
//! guarded by `users.last_rollover_date`, and only touches entries dated before today.

use chrono::{NaiveDate, Utc};
use streamline_domain::{
    order::key_after,
    rollover::{DayEndBehavior, Outcome, WindowOutcome, day_end_outcome, window_outcome},
    time::{logical_date, parse_hhmm, parse_tz},
};

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{DayEntry, Task, User, log_task_event, upsert_entry, upsert_task},
    util::now,
};

/// The user's current logical date.
pub fn today_for(user: &User) -> NaiveDate {
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let day_end = parse_hhmm(&user.day_end).unwrap_or(chrono::NaiveTime::MIN);
    logical_date(Utc::now(), tz, day_end)
}

pub async fn run_for_user(state: &AppState, user: &User) -> anyhow::Result<()> {
    let today = today_for(user);
    let today_s = today.format("%Y-%m-%d").to_string();
    if user.last_rollover_date.as_deref() == Some(today_s.as_str()) {
        return Ok(());
    }

    let mut tx = state.db.write.begin().await?;
    let mut changes = Vec::new();
    let ts = now();

    // Routine occurrences of an "expires" type whose day passed undone: missed, whether
    // or not they were planned (planned ones are also caught by the entry loop below).
    // An occurrence moved to another day counts by its new (due) day.
    let expired: Vec<Task> = sqlx::query_as(
        "SELECT t.* FROM tasks t JOIN task_types tt ON tt.id = t.task_type_id
         WHERE (t.owner_user_id = ?1 OR t.owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))
           AND t.deleted_at IS NULL AND t.status = 'open'
           AND t.series_id IS NOT NULL AND tt.day_end_behavior = 'expire' AND t.blocked = 0
           AND COALESCE(t.due_date, t.occurrence_date) < ?2",
    )
    .bind(&user.id)
    .bind(&today_s)
    .fetch_all(&mut *tx)
    .await?;
    for mut t in expired {
        t.status = "missed".into();
        t.updated_at = ts.clone();
        t.rev = next_rev(&mut tx).await?;
        upsert_task(&mut tx, &t).await?;
        log_task_event(
            &mut tx,
            &t.id,
            None,
            "missed",
            Some(serde_json::json!({"date": t.occurrence_date})),
        )
        .await?;
        changes.push(Change::task(&t));
    }

    // "N times per week/month" tasks whose window ended: missed, or rolled into the
    // current window when the task type says so.
    let ended: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT t.id, tt.window_overflow, s.window FROM tasks t
         JOIN task_types tt ON tt.id = t.task_type_id
         LEFT JOIN series s ON s.id = t.series_id
         WHERE (t.owner_user_id = ?1 OR t.owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))
           AND t.deleted_at IS NULL AND t.status = 'open'
           AND t.window_end IS NOT NULL AND t.window_end < ?2 AND t.blocked = 0",
    )
    .bind(&user.id)
    .bind(&today_s)
    .fetch_all(&mut *tx)
    .await?;
    for (id, overflow, window) in ended {
        let mut t: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
        let Some(end) = t.window_end.as_deref().and_then(crate::routines::date) else {
            continue;
        };
        let current = window
            .as_deref()
            .and_then(|w| crate::routines::current_window(w, today, user.week_start as u32));
        match (window_outcome(end, today, overflow == "roll"), current) {
            (WindowOutcome::Roll, Some((start, end))) => {
                t.occurrence_date = Some(start.format("%Y-%m-%d").to_string());
                t.window_end = Some(end.format("%Y-%m-%d").to_string());
                t.due_date = t.window_end.clone();
                t.carry_count += 1;
                log_task_event(
                    &mut tx,
                    &t.id,
                    None,
                    "carried",
                    Some(serde_json::json!({"window_end": t.window_end})),
                )
                .await?;
            }
            (WindowOutcome::Keep, _) => continue,
            _ => {
                t.status = "missed".into();
                log_task_event(
                    &mut tx,
                    &t.id,
                    None,
                    "missed",
                    Some(serde_json::json!({"window_end": end.to_string()})),
                )
                .await?;
            }
        }
        t.updated_at = ts.clone();
        t.rev = next_rev(&mut tx).await?;
        upsert_task(&mut tx, &t).await?;
        changes.push(Change::task(&t));
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT e.id, tt.day_end_behavior FROM day_entries e
         JOIN tasks t ON t.id = e.task_id
         JOIN task_types tt ON tt.id = t.task_type_id
         WHERE e.user_id = ? AND e.deleted_at IS NULL AND e.date < ?
           AND t.status = 'open' AND t.deleted_at IS NULL
         ORDER BY e.date, e.position",
    )
    .bind(&user.id)
    .bind(&today_s)
    .fetch_all(&mut *tx)
    .await?;

    let mut last_pos: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
    )
    .bind(&user.id)
    .bind(&today_s)
    .fetch_one(&mut *tx)
    .await?;

    for (entry_id, behavior) in rows {
        let mut entry: DayEntry = sqlx::query_as("SELECT * FROM day_entries WHERE id = ?")
            .bind(&entry_id)
            .fetch_one(&mut *tx)
            .await?;
        let mut task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(&entry.task_id)
            .fetch_one(&mut *tx)
            .await?;
        let behavior = DayEndBehavior::parse(&behavior).unwrap_or(DayEndBehavior::Carry);
        // Blocked tasks (waiting for a prerequisite) never miss (SPEC §6.2c).
        match day_end_outcome(behavior, task.blocked) {
            Outcome::CarryTo => {
                let from = entry.date.clone();
                let days = NaiveDate::parse_from_str(&from, "%Y-%m-%d")
                    .map(|d| (today - d).num_days())
                    .unwrap_or(1);
                let rev = next_rev(&mut tx).await?;
                let pos = key_after(last_pos.as_deref());
                last_pos = Some(pos.clone());
                entry.date = today_s.clone();
                entry.position = pos;
                entry.start_time = None;
                entry.updated_at = ts.clone();
                entry.rev = rev;
                upsert_entry(&mut tx, &entry).await?;
                task.carry_count += days as i32;
                task.updated_at = ts.clone();
                task.rev = rev;
                upsert_task(&mut tx, &task).await?;
                log_task_event(
                    &mut tx,
                    &task.id,
                    None,
                    "carried",
                    Some(serde_json::json!({"from": from, "to": today_s})),
                )
                .await?;
                changes.push(Change::entry(&entry));
                changes.push(Change::task(&task));
            }
            Outcome::Miss => {
                let rev = next_rev(&mut tx).await?;
                task.status = "missed".into();
                task.updated_at = ts.clone();
                task.rev = rev;
                upsert_task(&mut tx, &task).await?;
                log_task_event(
                    &mut tx,
                    &task.id,
                    None,
                    "missed",
                    Some(serde_json::json!({"date": entry.date})),
                )
                .await?;
                changes.push(Change::task(&task));
            }
            Outcome::Keep => {}
        }
    }
    // Missed prerequisites no longer block (D-6).
    let missed: Vec<String> = changes
        .iter()
        .filter(|c| c.kind == "task" && c.data["status"] == "missed")
        .filter_map(|c| c.data["id"].as_str().map(String::from))
        .collect();
    for id in missed {
        crate::deps::refresh_dependents(&mut tx, &id, &mut changes).await?;
    }
    sqlx::query("UPDATE users SET last_rollover_date = ? WHERE id = ?")
        .bind(&today_s)
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(())
}

pub async fn run_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> = sqlx::query_as("SELECT * FROM users WHERE deleted_at IS NULL")
        .fetch_all(&state.db.read)
        .await?;
    for u in users {
        if let Err(e) = run_for_user(state, &u).await {
            tracing::warn!("rollover failed for user {}: {e:#}", u.id);
        }
    }
    Ok(())
}
