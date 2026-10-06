//! The day plan: entries placing tasks on a date, separate from the tasks themselves.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use streamline_domain::order::key_after;
use ts_rs::TS;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError},
    events::Change,
    models::{DayEntry, Task, log_task_event, upsert_entry},
    rollover,
    util::{check_hhmm, check_position, check_range, double_option, id_or_new, now, parse_date},
    visibility,
};

#[derive(Serialize, TS)]
#[ts(export)]
pub struct DayView {
    pub date: String,
    pub today: String,
    /// Planned entries for the date, in order.
    pub entries: Vec<DayEntry>,
    /// Tasks referenced by `entries`, plus open tasks due on or before the date.
    pub tasks: Vec<Task>,
}

/// Aggregate for one day, so API clients can render a day in one request.
pub async fn get_day(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
) -> ApiResult<Json<DayView>> {
    parse_date(&date)?;
    rollover::run_for_user(&state, &user.user).await?;
    let db = &state.db.read;
    let entries: Vec<DayEntry> = sqlx::query_as(
        "SELECT * FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL ORDER BY start_time IS NULL, start_time, position",
    )
    .bind(user.id())
    .bind(&date)
    .fetch_all(db)
    .await?;
    let tasks: Vec<Task> = sqlx::query_as(&format!(
        "SELECT * FROM tasks WHERE {} AND deleted_at IS NULL AND (
            id IN (SELECT task_id FROM day_entries WHERE user_id = ?1 AND date = ?2 AND deleted_at IS NULL)
            OR (status = 'open' AND due_date IS NOT NULL AND due_date <= ?2))
         ORDER BY position",
        visibility::OWNED_VISIBLE_SQL.replace('?', "?1")
    ))
    .bind(user.id())
    .bind(&date)
    .fetch_all(db)
    .await?;
    Ok(Json(DayView {
        date,
        today: rollover::today_for(&user.user)
            .format("%Y-%m-%d")
            .to_string(),
        entries,
        tasks,
    }))
}

#[derive(Deserialize)]
pub struct AddEntry {
    id: Option<String>,
    task_id: String,
    position: Option<String>,
    start_time: Option<String>,
    duration_min: Option<i32>,
}

/// Plan a task into a day. A task has at most one active entry, so planning it on
/// another day moves the existing entry.
pub async fn add_entry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
    Json(c): Json<AddEntry>,
) -> ApiResult<Json<DayEntry>> {
    parse_date(&date)?;
    if let Some(p) = &c.position {
        check_position(p)?;
    }
    if let Some(t) = &c.start_time {
        check_hhmm(t)?;
    }
    check_range("duration_min", c.duration_min, 0, 24 * 60)?;
    let mut tx = state.db.write.begin().await?;
    let task = crate::routes::tasks::load_visible(&mut tx, &user, &c.task_id).await?;
    let position = match c.position {
        Some(p) => p,
        None => {
            let last: Option<String> =
                sqlx::query_scalar("SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL")
                    .bind(user.id())
                    .bind(&date)
                    .fetch_one(&mut *tx)
                    .await?;
            key_after(last.as_deref())
        }
    };
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let existing: Option<DayEntry> =
        sqlx::query_as("SELECT * FROM day_entries WHERE task_id = ? AND deleted_at IS NULL")
            .bind(&task.id)
            .fetch_optional(&mut *tx)
            .await?;
    let e = match existing {
        Some(mut e) => {
            if e.date != date {
                log_task_event(
                    &mut tx,
                    &task.id,
                    Some(user.id()),
                    "moved",
                    Some(serde_json::json!({"from": e.date, "to": date})),
                )
                .await?;
            }
            e.date = date;
            e.position = position;
            e.start_time = c.start_time;
            e.duration_min = c.duration_min;
            e.updated_at = ts;
            e.rev = rev;
            e
        }
        None => {
            log_task_event(
                &mut tx,
                &task.id,
                Some(user.id()),
                "planned",
                Some(serde_json::json!({"date": date})),
            )
            .await?;
            DayEntry {
                id: id_or_new(c.id)?,
                user_id: user.id().into(),
                date,
                task_id: task.id.clone(),
                position,
                start_time: c.start_time,
                duration_min: c.duration_min,
                created_at: ts.clone(),
                updated_at: ts,
                deleted_at: None,
                rev,
            }
        }
    };
    upsert_entry(&mut tx, &e).await?;
    tx.commit().await?;
    state.bus.publish([Change::entry(&e)]);
    Ok(Json(e))
}

async fn load_entry(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<DayEntry> {
    sqlx::query_as("SELECT * FROM day_entries WHERE id = ? AND user_id = ? AND deleted_at IS NULL")
        .bind(id)
        .bind(user.id())
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct PatchEntry {
    date: Option<String>,
    position: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    start_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    duration_min: Option<Option<i32>>,
}

pub async fn patch_entry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchEntry>,
) -> ApiResult<Json<DayEntry>> {
    let mut tx = state.db.write.begin().await?;
    let mut e = load_entry(&mut tx, &user, &id).await?;
    if let Some(d) = c.date
        && d != e.date
    {
        parse_date(&d)?;
        let kind = if d > e.date { "snoozed" } else { "moved" };
        log_task_event(
            &mut tx,
            &e.task_id,
            Some(user.id()),
            kind,
            Some(serde_json::json!({"from": e.date, "to": d})),
        )
        .await?;
        e.date = d;
    }
    if let Some(p) = c.position {
        check_position(&p)?;
        e.position = p;
    }
    if let Some(t) = c.start_time {
        if let Some(t) = &t {
            check_hhmm(t)?;
        }
        e.start_time = t;
    }
    if let Some(d) = c.duration_min {
        check_range("duration_min", d, 0, 24 * 60)?;
        e.duration_min = d;
    }
    e.updated_at = now();
    e.rev = next_rev(&mut tx).await?;
    upsert_entry(&mut tx, &e).await?;
    tx.commit().await?;
    state.bus.publish([Change::entry(&e)]);
    Ok(Json(e))
}

/// Remove a task from the day plan (the task itself stays in its project).
pub async fn delete_entry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut e = load_entry(&mut tx, &user, &id).await?;
    let ts = now();
    e.deleted_at = Some(ts.clone());
    e.updated_at = ts;
    e.rev = next_rev(&mut tx).await?;
    upsert_entry(&mut tx, &e).await?;
    log_task_event(
        &mut tx,
        &e.task_id,
        Some(user.id()),
        "unplanned",
        Some(serde_json::json!({"date": e.date})),
    )
    .await?;
    tx.commit().await?;
    state.bus.publish([Change::entry(&e)]);
    Ok(StatusCode::NO_CONTENT)
}
