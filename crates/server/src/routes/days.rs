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
    models::{DayEntry, DayPlan, Task, User, log_task_event, upsert_day_plan, upsert_entry},
    rollover,
    util::{check_hhmm, check_position, check_range, double_option, id_or_new, now, parse_date},
    visibility,
};

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DayView {
    pub date: String,
    pub today: String,
    /// Planned entries for the date, in order.
    pub entries: Vec<DayEntry>,
    /// Tasks referenced by `entries`, plus open tasks due on or before the date.
    pub tasks: Vec<Task>,
    /// Planning state; `null` = unplanned.
    pub plan: Option<DayPlan>,
    /// Minutes of the user's available window not taken by scheduled (timed) tasks.
    pub free_min: u32,
    /// Estimated minutes of open planned tasks without a time.
    pub planned_min: u32,
}

/// Minutes a planned task takes: its slot's duration, else the estimate, else `default`.
fn entry_minutes(e: &DayEntry, t: Option<&Task>, default: i32) -> u32 {
    e.duration_min
        .or(t.and_then(|t| t.estimate_min))
        .unwrap_or(default)
        .max(0) as u32
}

/// Free time and planned time for a day (SPEC §6.2d, step "Pick"). For today, pass the
/// current local time as `now` so only the rest of the day counts as free.
pub fn day_load(
    user: &User,
    entries: &[DayEntry],
    tasks: &[Task],
    now: Option<chrono::NaiveTime>,
) -> (u32, u32) {
    use streamline_domain::time::parse_hhmm;
    let task = |id: &str| tasks.iter().find(|t| t.id == id);
    let busy: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            Some((
                parse_hhmm(e.start_time.as_deref()?)?,
                entry_minutes(e, task(&e.task_id), 30),
            ))
        })
        .collect();
    let start = parse_hhmm(&user.day_window_start).unwrap_or_default();
    let end = parse_hhmm(&user.day_window_end).unwrap_or_default();
    let window = match now {
        Some(now) => streamline_domain::planning::remaining_window(start, end, now),
        None => Some((start, end)),
    };
    let free = window.map_or(0, |(s, e)| {
        streamline_domain::planning::free_minutes(s, e, &busy)
    });
    let planned = entries
        .iter()
        .filter(|e| e.start_time.is_none())
        .filter_map(|e| {
            task(&e.task_id)
                .filter(|t| t.status == "open")
                .map(|t| entry_minutes(e, Some(t), 0))
        })
        .sum();
    (free, planned)
}

async fn load_plan(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    date: &str,
) -> sqlx::Result<Option<DayPlan>> {
    sqlx::query_as("SELECT * FROM day_plans WHERE user_id = ? AND date = ? AND deleted_at IS NULL")
        .bind(user_id)
        .bind(date)
        .fetch_optional(conn)
        .await
}

/// Aggregate for one day, so API clients can render a day in one request.
#[utoipa::path(get, path = "/days/{date}", tag = "days", summary = "A day's plan, its tasks and what's due", params(("date" = String, Path, description = "YYYY-MM-DD")), responses((status = 200, body = DayView), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
    let plan = load_plan(&mut *state.db.read.acquire().await?, user.id(), &date).await?;
    let today = rollover::today_for(&user.user)
        .format("%Y-%m-%d")
        .to_string();
    let now = (date == today).then(|| {
        let tz = streamline_domain::time::parse_tz(&user.user.timezone).unwrap_or(chrono_tz::UTC);
        chrono::Utc::now().with_timezone(&tz).time()
    });
    let (free_min, planned_min) = day_load(&user.user, &entries, &tasks, now);
    Ok(Json(DayView {
        date,
        today: rollover::today_for(&user.user)
            .format("%Y-%m-%d")
            .to_string(),
        entries,
        tasks,
        plan,
        free_min,
        planned_min,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PutPlan {
    /// Client-chosen ULID, used if the day has no plan record yet.
    id: Option<String>,
    /// `draft` (wizard in progress) or `planned` (confirmed).
    status: String,
    /// Wizard step to resume at.
    #[serde(default)]
    step: i32,
}

/// Start, update or confirm planning of a day.
#[utoipa::path(put, path = "/days/{date}/plan", tag = "days", summary = "Set a day's planning state (draft with wizard step, or planned)", params(("date" = String, Path, description = "YYYY-MM-DD")), request_body = PutPlan, responses((status = 200, body = DayPlan), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn put_plan(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
    Json(c): Json<PutPlan>,
) -> ApiResult<Json<DayPlan>> {
    parse_date(&date)?;
    if c.status != "draft" && c.status != "planned" {
        return Err(crate::error::bad("status must be draft or planned"));
    }
    check_range("step", Some(c.step), 0, 10)?;
    let mut tx = state.db.write.begin().await?;
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let p = match load_plan(&mut tx, user.id(), &date).await? {
        Some(mut p) => {
            // Going back into the wizard doesn't un-plan a planned day.
            if c.status == "planned" || p.status != "planned" {
                p.status = c.status.clone();
            }
            p.step = c.step;
            p
        }
        None => DayPlan {
            id: id_or_new(c.id.clone())?,
            user_id: user.id().into(),
            date: date.clone(),
            status: c.status.clone(),
            step: c.step,
            planned_at: None,
            created_at: ts.clone(),
            updated_at: ts.clone(),
            deleted_at: None,
            rev,
        },
    };
    let mut p = p;
    if p.status == "planned" && p.planned_at.is_none() {
        p.planned_at = Some(ts.clone());
    }
    p.updated_at = ts;
    p.rev = rev;
    upsert_day_plan(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::day_plan(&p)]);
    Ok(Json(p))
}

/// Mark a day as unplanned again.
#[utoipa::path(delete, path = "/days/{date}/plan", tag = "days", summary = "Mark a day as unplanned", params(("date" = String, Path, description = "YYYY-MM-DD")), responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_plan(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
) -> ApiResult<StatusCode> {
    parse_date(&date)?;
    let mut tx = state.db.write.begin().await?;
    let mut p = load_plan(&mut tx, user.id(), &date)
        .await?
        .ok_or(AppError::NotFound)?;
    let ts = now();
    p.deleted_at = Some(ts.clone());
    p.updated_at = ts;
    p.rev = next_rev(&mut tx).await?;
    upsert_day_plan(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::day_plan(&p)]);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddEntry {
    id: Option<String>,
    task_id: String,
    position: Option<String>,
    start_time: Option<String>,
    duration_min: Option<i32>,
}

/// Plan a task into a day. A task has at most one active entry, so planning it on
/// another day moves the existing entry.
#[utoipa::path(post, path = "/days/{date}/entries", tag = "days", summary = "Plan a task into a day (moves it if planned elsewhere)", params(("date" = String, Path, description = "YYYY-MM-DD")), request_body = AddEntry, responses((status = 200, body = DayEntry), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchEntry {
    date: Option<String>,
    position: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    start_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    duration_min: Option<Option<i32>>,
}

#[utoipa::path(patch, path = "/day-entries/{id}", tag = "days", summary = "Move, reorder or (un)schedule a planned task", params(("id" = String, Path, description = "ULID")), request_body = PatchEntry, responses((status = 200, body = DayEntry), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
#[utoipa::path(delete, path = "/day-entries/{id}", tag = "days", summary = "Remove a task from the day plan", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
