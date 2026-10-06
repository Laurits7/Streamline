//! Routines (SPEC §6.3). Editing a single occurrence is just editing its task; these
//! endpoints change the routine itself "from a date on" (default: today).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Duration;
use serde::Deserialize;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{DayEntry, Series, Task, upsert_entry, upsert_series, upsert_task},
    rollover::today_for,
    routines::{self, SeriesStats, date},
    util::{check_hhmm, check_range, double_option, id_or_new, now, parse_date},
    visibility,
};

async fn load_visible(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<Series> {
    let s: Series = sqlx::query_as("SELECT * FROM series WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    if !visibility::can_see(
        &user.user,
        s.owner_user_id.as_deref(),
        s.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(s)
}

/// Default task type per kind of routine (D-1).
fn default_type(mode: &str) -> &'static str {
    match mode {
        "anchored" => "tt_expires",
        "flexible" => "tt_window",
        _ => "tt_carry_on",
    }
}

/// Validate a routine's fields (after applying a create/patch).
async fn check(conn: &mut sqlx::SqliteConnection, user: &AuthUser, s: &Series) -> ApiResult<()> {
    let title = s.title.trim();
    if title.is_empty() || title.chars().count() > 500 {
        return Err(bad("title must be 1-500 characters"));
    }
    match s.mode.as_str() {
        "repeat" | "anchored" => {
            let rule = s
                .rrule
                .as_deref()
                .ok_or_else(|| bad("a repeat rule (rrule) is required"))?;
            streamline_domain::recurrence::validate(rule)
                .map_err(|e| bad(format!("invalid repeat rule: {e}")))?;
            if s.mode == "anchored" {
                check_hhmm(
                    s.start_time
                        .as_deref()
                        .ok_or_else(|| bad("a fixed-time routine needs start_time"))?,
                )?;
            }
        }
        "flexible" => {
            check_range(
                "times_per_window",
                Some(
                    s.times_per_window
                        .ok_or_else(|| bad("times_per_window is required"))?,
                ),
                1,
                31,
            )?;
            if !matches!(s.window.as_deref(), Some("week" | "month")) {
                return Err(bad("window must be week or month"));
            }
        }
        _ => return Err(bad("mode must be repeat, anchored or flexible")),
    }
    let start = parse_date(&s.dtstart)?;
    if let Some(u) = &s.until
        && parse_date(u)? < start
    {
        return Err(bad("the end date is before the start"));
    }
    check_range("duration_min", s.duration_min, 1, 24 * 60)?;
    check_range("estimate_min", s.estimate_min, 0, 24 * 60)?;
    check_range("difficulty", s.difficulty, 1, 3)?;
    check_range("importance", s.importance, 0, 3)?;
    check_range("urgency", s.urgency, 0, 3)?;
    if let Some(p) = &s.project_id {
        crate::routes::projects::load_visible(conn, user, p)
            .await
            .map_err(|_| bad("unknown project"))?;
    }
    let ok: Option<String> = sqlx::query_scalar(
        "SELECT id FROM task_types WHERE id = ? AND deleted_at IS NULL AND (builtin = 1 OR owner_user_id = ?)",
    )
    .bind(&s.task_type_id)
    .bind(user.id())
    .fetch_optional(&mut *conn)
    .await?;
    ok.map(|_| ()).ok_or_else(|| bad("unknown task type"))?;
    crate::routes::places::check_place(conn, user, &s.place_id).await
}

#[utoipa::path(get, path = "/series", tag = "routines", summary = "List routines", responses((status = 200, body = Vec<Series>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Series>>> {
    let rows = sqlx::query_as(&format!(
        "SELECT * FROM series WHERE {} AND deleted_at IS NULL ORDER BY title",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[utoipa::path(get, path = "/series/stats", tag = "routines", summary = "Streaks, next dates and window progress of all routines", responses((status = 200, body = Vec<SeriesStats>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn all_stats(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<SeriesStats>>> {
    let series: Vec<Series> = sqlx::query_as(&format!(
        "SELECT * FROM series WHERE {} AND deleted_at IS NULL",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    let mut out = vec![];
    for s in &series {
        out.push(routines::stats(&state, &user.user, s).await?);
    }
    Ok(Json(out))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSeries {
    id: Option<String>,
    title: String,
    #[serde(default)]
    notes: String,
    /// `repeat`, `anchored` or `flexible`.
    mode: String,
    rrule: Option<String>,
    /// Defaults to today.
    dtstart: Option<String>,
    until: Option<String>,
    start_time: Option<String>,
    duration_min: Option<i32>,
    times_per_window: Option<i32>,
    window: Option<String>,
    project_id: Option<String>,
    /// Defaults by mode: repeat = Carry on, anchored = Expires, flexible = Within its window.
    task_type_id: Option<String>,
    estimate_min: Option<i32>,
    difficulty: Option<i32>,
    importance: Option<i32>,
    urgency: Option<i32>,
    place_id: Option<String>,
}

#[utoipa::path(post, path = "/series", tag = "routines", summary = "Create a routine (its occurrences appear up to tomorrow)", request_body = CreateSeries, responses((status = 200, body = Series), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateSeries>,
) -> ApiResult<Json<Series>> {
    let today = today_for(&user.user).format("%Y-%m-%d").to_string();
    let ts = now();
    let mut s = Series {
        id: id_or_new(c.id)?,
        owner_user_id: Some(user.id().into()),
        owner_group_id: None,
        project_id: c.project_id,
        title: c.title.trim().to_string(),
        notes: c.notes,
        task_type_id: c
            .task_type_id
            .unwrap_or_else(|| default_type(&c.mode).into()),
        mode: c.mode,
        rrule: c
            .rrule
            .map(|r| r.trim().trim_start_matches("RRULE:").to_string()),
        dtstart: c.dtstart.unwrap_or(today),
        until: c.until,
        start_time: c.start_time,
        duration_min: c.duration_min,
        times_per_window: c.times_per_window,
        window: c.window,
        estimate_min: c.estimate_min,
        difficulty: c.difficulty,
        importance: c.importance,
        urgency: c.urgency,
        place_id: c.place_id,
        materialized_through: None,
        split_from: None,
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: 0,
    };
    let mut tx = state.db.write.begin().await?;
    if let Some(existing) = sqlx::query_as::<_, Series>("SELECT * FROM series WHERE id = ?")
        .bind(&s.id)
        .fetch_optional(&mut *tx)
        .await?
    {
        if existing.owner_user_id.as_deref() == Some(user.id()) {
            return Ok(Json(existing));
        }
        return Err(AppError::Conflict("id already in use".into()));
    }
    check(&mut tx, &user, &s).await?;
    s.rev = next_rev(&mut tx).await?;
    upsert_series(&mut tx, &s).await?;
    tx.commit().await?;
    state.bus.publish([Change::series(&s)]);
    routines::materialize_user(&state, &user.user).await?;
    Ok(Json(s))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchSeries {
    /// Apply to occurrences from this date on (`YYYY-MM-DD`, default today). Earlier
    /// occurrences and their history are never changed.
    from: Option<String>,
    title: Option<String>,
    notes: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    project_id: Option<Option<String>>,
    task_type_id: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    estimate_min: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    difficulty: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    importance: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    urgency: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    place_id: Option<Option<String>>,
    // Schedule: changing any of these splits the routine at `from`.
    mode: Option<String>,
    rrule: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    start_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    duration_min: Option<Option<i32>>,
    times_per_window: Option<i32>,
    window: Option<String>,
    /// End date (inclusive); `null` = no end.
    #[serde(default, deserialize_with = "double_option")]
    until: Option<Option<String>>,
}

/// Soft-delete a routine's open occurrences from `from` on (for flexible routines also
/// the ones whose window is still running), with their day-plan entries.
async fn drop_open_occurrences(
    conn: &mut sqlx::SqliteConnection,
    series_id: &str,
    from: &str,
    rev: i64,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let tasks: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE series_id = ? AND deleted_at IS NULL AND status = 'open'
           AND (occurrence_date >= ?2 OR (window_end IS NOT NULL AND window_end >= ?2))",
    )
    .bind(series_id)
    .bind(from)
    .fetch_all(&mut *conn)
    .await?;
    let ts = now();
    for mut t in tasks {
        let entries: Vec<DayEntry> =
            sqlx::query_as("SELECT * FROM day_entries WHERE task_id = ? AND deleted_at IS NULL")
                .bind(&t.id)
                .fetch_all(&mut *conn)
                .await?;
        for mut e in entries {
            e.deleted_at = Some(ts.clone());
            e.updated_at = ts.clone();
            e.rev = rev;
            upsert_entry(conn, &e).await?;
            changes.push(Change::entry(&e));
        }
        t.deleted_at = Some(ts.clone());
        t.updated_at = ts.clone();
        t.rev = rev;
        upsert_task(conn, &t).await?;
        changes.push(Change::task(&t));
        crate::deps::refresh_dependents(conn, &t.id, changes).await?;
    }
    Ok(())
}

#[utoipa::path(patch, path = "/series/{id}", tag = "routines", summary = "Change a routine from a date on (\"all future\")", params(("id" = String, Path, description = "ULID")), request_body = PatchSeries, responses((status = 200, description = "The routine as it applies from `from` on (a new id if the schedule changed)", body = Series), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchSeries>,
) -> ApiResult<Json<Series>> {
    let today = today_for(&user.user);
    let from = match &c.from {
        Some(f) => parse_date(f)?.max(today),
        None => today,
    };
    let from_s = from.format("%Y-%m-%d").to_string();
    let mut tx = state.db.write.begin().await?;
    let old = load_visible(&mut tx, &user, &id).await?;
    let mut next = old.clone();
    if let Some(v) = c.title {
        next.title = v.trim().to_string();
    }
    if let Some(v) = c.notes {
        next.notes = v;
    }
    if let Some(v) = c.project_id {
        next.project_id = v;
    }
    if let Some(v) = c.task_type_id {
        next.task_type_id = v;
    }
    if let Some(v) = c.estimate_min {
        next.estimate_min = v;
    }
    if let Some(v) = c.difficulty {
        next.difficulty = v;
    }
    if let Some(v) = c.importance {
        next.importance = v;
    }
    if let Some(v) = c.urgency {
        next.urgency = v;
    }
    if let Some(v) = c.place_id {
        next.place_id = v;
    }
    let mut schedule_changed = false;
    if let Some(v) = c.mode.filter(|v| *v != old.mode) {
        next.mode = v;
        schedule_changed = true;
    }
    if let Some(v) = c
        .rrule
        .map(|r| r.trim().trim_start_matches("RRULE:").to_string())
        .filter(|v| Some(v) != old.rrule.as_ref())
    {
        next.rrule = Some(v);
        schedule_changed = true;
    }
    if let Some(v) = c.start_time.filter(|v| *v != old.start_time) {
        next.start_time = v;
        schedule_changed = true;
    }
    if let Some(v) = c.duration_min.filter(|v| *v != old.duration_min) {
        next.duration_min = v;
        schedule_changed = true;
    }
    if let Some(v) = c
        .times_per_window
        .filter(|v| Some(*v) != old.times_per_window)
    {
        next.times_per_window = Some(v);
        schedule_changed = true;
    }
    if let Some(v) = c.window.filter(|v| Some(v) != old.window.as_ref()) {
        next.window = Some(v);
        schedule_changed = true;
    }
    if let Some(v) = c.until {
        next.until = v;
    }
    check(&mut tx, &user, &next).await?;

    let rev = next_rev(&mut tx).await?;
    let ts = now();
    let mut changes = vec![];
    let result = if schedule_changed {
        // Split: the old routine ends the day before `from` (keeping its history), and a
        // new one carries on from `from`. Occurrence keys can't be reused, so the open
        // future occurrences of the old routine go away and the new one creates its own.
        drop_open_occurrences(&mut tx, &old.id, &from_s, rev, &mut changes).await?;
        let history: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks WHERE series_id = ? AND deleted_at IS NULL",
        )
        .bind(&old.id)
        .fetch_one(&mut *tx)
        .await?;
        let mut ended = old.clone();
        if history == 0 {
            ended.deleted_at = Some(ts.clone());
        } else {
            ended.until = Some((from - Duration::days(1)).format("%Y-%m-%d").to_string());
        }
        ended.updated_at = ts.clone();
        ended.rev = rev;
        upsert_series(&mut tx, &ended).await?;
        changes.push(Change::series(&ended));
        // Don't redo what's already settled: start after the last occurrence that is done,
        // missed or skipped. "N per week/month" routines instead start right away and top
        // up the current window with what's still missing (see routines::materialize_series),
        // so only windows *after* the current one count as settled.
        let current_window_start = old
            .window
            .as_deref()
            .and_then(|w| routines::current_window(w, from, user.user.week_start as u32))
            .map(|(s, _)| s.format("%Y-%m-%d").to_string());
        let last_closed: Option<String> = if next.mode == "flexible" && old.mode == "flexible" {
            sqlx::query_scalar(
                "SELECT MAX(window_end) FROM tasks
                 WHERE series_id = ? AND deleted_at IS NULL AND status <> 'open' AND occurrence_date > ?",
            )
            .bind(&old.id)
            .bind(current_window_start.unwrap_or_default())
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT MAX(COALESCE(window_end, occurrence_date)) FROM tasks
                 WHERE series_id = ? AND deleted_at IS NULL AND status <> 'open'",
            )
            .bind(&old.id)
            .fetch_one(&mut *tx)
            .await?
        };
        let mut start = date(&old.dtstart).map_or(from, |d| d.max(from));
        if let Some(d) = last_closed.as_deref().and_then(date) {
            start = start.max(d + Duration::days(1));
        }
        next.id = crate::util::new_id();
        next.split_from = Some(old.id.clone());
        next.dtstart = start.format("%Y-%m-%d").to_string();
        next.materialized_through = None;
        next.created_at = ts.clone();
        next
    } else {
        // Same schedule: update in place, and bring open occurrences from `from` on in line.
        if let Some(u) = next.until.as_deref().and_then(date) {
            let after = (u + Duration::days(1)).format("%Y-%m-%d").to_string();
            drop_open_occurrences(&mut tx, &old.id, &after, rev, &mut changes).await?;
        }
        let tasks: Vec<Task> = sqlx::query_as(
            "SELECT * FROM tasks WHERE series_id = ? AND deleted_at IS NULL AND status = 'open' AND occurrence_date >= ?",
        )
        .bind(&old.id)
        .bind(&from_s)
        .fetch_all(&mut *tx)
        .await?;
        for mut t in tasks {
            t.title = next.title.clone();
            t.notes = next.notes.clone();
            t.project_id = next.project_id.clone();
            t.task_type_id = next.task_type_id.clone();
            t.estimate_min = next.estimate_min;
            t.difficulty = next.difficulty;
            t.importance = next.importance;
            t.urgency = next.urgency;
            t.place_id = next.place_id.clone();
            t.updated_at = ts.clone();
            t.rev = rev;
            upsert_task(&mut tx, &t).await?;
            changes.push(Change::task(&t));
        }
        next
    };
    let mut result = result;
    result.updated_at = ts;
    result.rev = rev;
    upsert_series(&mut tx, &result).await?;
    changes.push(Change::series(&result));
    tx.commit().await?;
    state.bus.publish(changes);
    routines::materialize_user(&state, &user.user).await?;
    Ok(Json(result))
}

/// End a routine: it stops, open occurrences from today on disappear, and the history
/// (done, missed, skipped occurrences) stays.
#[utoipa::path(delete, path = "/series/{id}", tag = "routines", summary = "End a routine (history is kept)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let today = today_for(&user.user).format("%Y-%m-%d").to_string();
    let mut tx = state.db.write.begin().await?;
    let mut s = load_visible(&mut tx, &user, &id).await?;
    let rev = next_rev(&mut tx).await?;
    let mut changes = vec![];
    drop_open_occurrences(&mut tx, &id, &today, rev, &mut changes).await?;
    let ts = now();
    s.deleted_at = Some(ts.clone());
    s.updated_at = ts;
    s.rev = rev;
    upsert_series(&mut tx, &s).await?;
    changes.push(Change::series(&s));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}
