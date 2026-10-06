//! Day rollover: once a user's day has ended, unfinished planned tasks are carried
//! over or marked missed according to their task type (SPEC §6.2c). Idempotent:
//! guarded by `users.last_rollover_date`, and only touches entries dated before today.

use chrono::{NaiveDate, Utc};
use streamline_domain::{
    order::key_after,
    rollover::{DayEndBehavior, Outcome, day_end_outcome},
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

    let mut changes = Vec::new();
    let ts = now();
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
        // Prerequisites arrive in Phase 3b; until then nothing is blocked.
        match day_end_outcome(behavior, false) {
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
