use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use streamline_domain::order::key_after;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{DayEntry, TASK_STATUSES, Task, log_task_event, upsert_entry, upsert_task},
    util::{check_position, check_range, double_option, id_or_new, now, parse_date},
    visibility,
};

pub const DEFAULT_TASK_TYPE: &str = "tt_carry_on";

pub async fn load_visible(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<Task> {
    let t: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    if !visibility::can_see(
        &user.user,
        t.owner_user_id.as_deref(),
        t.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(t)
}

fn check_title(t: &str) -> ApiResult<String> {
    let t = t.trim();
    if t.is_empty() || t.chars().count() > 500 {
        return Err(bad("title must be 1-500 characters"));
    }
    Ok(t.to_string())
}

fn check_notes(n: &str) -> ApiResult<()> {
    if n.len() > 100_000 {
        Err(bad("notes too long"))
    } else {
        Ok(())
    }
}

async fn check_project(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &Option<String>,
) -> ApiResult<()> {
    if let Some(id) = id {
        crate::routes::projects::load_visible(conn, user, id)
            .await
            .map_err(|_| bad("unknown project"))?;
    }
    Ok(())
}

async fn check_task_type(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<()> {
    let ok: Option<String> = sqlx::query_scalar(
        "SELECT id FROM task_types WHERE id = ? AND deleted_at IS NULL AND (builtin = 1 OR owner_user_id = ?)",
    )
    .bind(id)
    .bind(user.id())
    .fetch_optional(conn)
    .await?;
    ok.map(|_| ()).ok_or_else(|| bad("unknown task type"))
}

fn check_attrs(
    estimate: Option<i32>,
    difficulty: Option<i32>,
    importance: Option<i32>,
    urgency: Option<i32>,
) -> ApiResult<()> {
    check_range("estimate_min", estimate, 0, 24 * 60)?;
    check_range("difficulty", difficulty, 1, 3)?;
    check_range("importance", importance, 0, 3)?;
    check_range("urgency", urgency, 0, 3)?;
    Ok(())
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    project_id: Option<String>,
    inbox: Option<bool>,
    status: Option<String>,
}

#[utoipa::path(get, path = "/tasks", tag = "tasks", summary = "List tasks", params(ListQuery), responses((status = 200, body = Vec<Task>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    let rows: Vec<Task> = sqlx::query_as(&format!(
        "SELECT * FROM tasks WHERE {} AND deleted_at IS NULL
           AND (?2 IS NULL OR project_id = ?2)
           AND (?3 = 0 OR project_id IS NULL)
           AND (?4 IS NULL OR status = ?4)
         ORDER BY position",
        visibility::OWNED_VISIBLE_SQL.replace('?', "?1")
    ))
    .bind(user.id())
    .bind(&q.project_id)
    .bind(q.inbox.unwrap_or(false))
    .bind(&q.status)
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[utoipa::path(get, path = "/tasks/{id}", tag = "tasks", summary = "Get a task", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = Task), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    let mut conn = state.db.read.acquire().await?;
    Ok(Json(load_visible(&mut conn, &user, &id).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTask {
    id: Option<String>,
    title: String,
    #[serde(default)]
    notes: String,
    project_id: Option<String>,
    position: Option<String>,
    due_date: Option<String>,
    estimate_min: Option<i32>,
    difficulty: Option<i32>,
    importance: Option<i32>,
    urgency: Option<i32>,
    task_type_id: Option<String>,
    /// Also plan the new task into this day (`YYYY-MM-DD`).
    day: Option<String>,
    /// Client-chosen id for that day entry.
    day_entry_id: Option<String>,
}

#[utoipa::path(post, path = "/tasks", tag = "tasks", summary = "Create a task (optionally planned into a day)", request_body = CreateTask, responses((status = 200, body = Task), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateTask>,
) -> ApiResult<Json<Task>> {
    let id = id_or_new(c.id)?;
    let title = check_title(&c.title)?;
    check_notes(&c.notes)?;
    check_attrs(c.estimate_min, c.difficulty, c.importance, c.urgency)?;
    if let Some(d) = &c.due_date {
        parse_date(d)?;
    }
    if let Some(d) = &c.day {
        parse_date(d)?;
    }
    if let Some(p) = &c.position {
        check_position(p)?;
    }
    let entry_id = id_or_new(c.day_entry_id)?;
    let task_type_id = c.task_type_id.unwrap_or_else(|| DEFAULT_TASK_TYPE.into());

    let mut tx = state.db.write.begin().await?;
    if let Some(existing) = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        if existing.owner_user_id.as_deref() == Some(user.id()) {
            return Ok(Json(existing));
        }
        return Err(AppError::Conflict("id already in use".into()));
    }
    check_project(&mut tx, &user, &c.project_id).await?;
    check_task_type(&mut tx, &user, &task_type_id).await?;
    let position = match c.position {
        Some(p) => p,
        None => {
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM tasks WHERE owner_user_id = ? AND project_id IS ? AND deleted_at IS NULL",
            )
            .bind(user.id())
            .bind(&c.project_id)
            .fetch_one(&mut *tx)
            .await?;
            key_after(last.as_deref())
        }
    };
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let t = Task {
        id,
        owner_user_id: Some(user.id().into()),
        owner_group_id: None,
        assignee_user_id: None,
        project_id: c.project_id,
        title,
        notes: c.notes,
        status: "open".into(),
        position,
        due_date: c.due_date,
        estimate_min: c.estimate_min,
        difficulty: c.difficulty,
        importance: c.importance,
        urgency: c.urgency,
        actual_min: 0,
        task_type_id,
        carry_count: 0,
        started_at: None,
        completed_at: None,
        completed_by: None,
        ext_source: None,
        ext_id: None,
        ext_url: None,
        series_id: None,
        occurrence_key: None,
        occurrence_date: None,
        window_end: None,
        created_at: ts.clone(),
        updated_at: ts.clone(),
        deleted_at: None,
        rev,
    };
    upsert_task(&mut tx, &t).await?;
    let mut changes = vec![Change::task(&t)];
    if let Some(day) = c.day {
        let last: Option<String> =
            sqlx::query_scalar("SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL")
                .bind(user.id())
                .bind(&day)
                .fetch_one(&mut *tx)
                .await?;
        let e = DayEntry {
            id: entry_id,
            user_id: user.id().into(),
            date: day,
            task_id: t.id.clone(),
            position: key_after(last.as_deref()),
            start_time: None,
            duration_min: None,
            created_at: ts.clone(),
            updated_at: ts,
            deleted_at: None,
            rev,
        };
        upsert_entry(&mut tx, &e).await?;
        changes.push(Change::entry(&e));
    }
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(Json(t))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchTask {
    title: Option<String>,
    notes: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    project_id: Option<Option<String>>,
    status: Option<String>,
    position: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    estimate_min: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    difficulty: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    importance: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    urgency: Option<Option<i32>>,
    task_type_id: Option<String>,
    /// Mark an open task as in progress (`true`) or not started (`false`).
    in_progress: Option<bool>,
}

#[utoipa::path(patch, path = "/tasks/{id}", tag = "tasks", summary = "Update a task; status done completes it", params(("id" = String, Path, description = "ULID")), request_body = PatchTask, responses((status = 200, body = Task), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchTask>,
) -> ApiResult<Json<Task>> {
    let mut tx = state.db.write.begin().await?;
    let mut t = load_visible(&mut tx, &user, &id).await?;
    if let Some(v) = c.title {
        t.title = check_title(&v)?;
    }
    if let Some(v) = c.notes {
        check_notes(&v)?;
        t.notes = v;
    }
    if let Some(v) = c.project_id {
        check_project(&mut tx, &user, &v).await?;
        t.project_id = v;
    }
    if let Some(v) = c.position {
        check_position(&v)?;
        t.position = v;
    }
    if let Some(v) = c.due_date {
        if let Some(d) = &v {
            parse_date(d)?;
        }
        t.due_date = v;
    }
    if let Some(v) = c.estimate_min {
        t.estimate_min = v;
    }
    if let Some(v) = c.difficulty {
        t.difficulty = v;
    }
    if let Some(v) = c.importance {
        t.importance = v;
    }
    if let Some(v) = c.urgency {
        t.urgency = v;
    }
    check_attrs(t.estimate_min, t.difficulty, t.importance, t.urgency)?;
    if let Some(v) = c.task_type_id {
        check_task_type(&mut tx, &user, &v).await?;
        t.task_type_id = v;
    }
    if let Some(status) = c.status
        && status != t.status
    {
        if !TASK_STATUSES.contains(&status.as_str()) {
            return Err(bad("unknown status"));
        }
        let kind = match status.as_str() {
            "done" => {
                t.completed_at = Some(now());
                t.completed_by = Some(user.id().into());
                "completed".to_string()
            }
            "open" => {
                t.completed_at = None;
                t.completed_by = None;
                t.started_at = None;
                "reopened".to_string()
            }
            other => {
                t.completed_at = None;
                t.completed_by = None;
                other.to_string()
            }
        };
        t.status = status;
        log_task_event(&mut tx, &t.id, Some(user.id()), &kind, None).await?;
    }
    if let Some(p) = c.in_progress {
        t.started_at = if p {
            Some(t.started_at.clone().unwrap_or_else(now))
        } else {
            None
        };
    }
    t.updated_at = now();
    t.rev = next_rev(&mut tx).await?;
    upsert_task(&mut tx, &t).await?;
    tx.commit().await?;
    state.bus.publish([Change::task(&t)]);
    Ok(Json(t))
}

#[utoipa::path(delete, path = "/tasks/{id}", tag = "tasks", summary = "Delete a task", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut t = load_visible(&mut tx, &user, &id).await?;
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    let mut changes = vec![];
    let entries: Vec<DayEntry> =
        sqlx::query_as("SELECT * FROM day_entries WHERE task_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for mut e in entries {
        e.deleted_at = Some(ts.clone());
        e.updated_at = ts.clone();
        e.rev = rev;
        upsert_entry(&mut tx, &e).await?;
        changes.push(Change::entry(&e));
    }
    t.deleted_at = Some(ts.clone());
    t.updated_at = ts;
    t.rev = rev;
    upsert_task(&mut tx, &t).await?;
    changes.push(Change::task(&t));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}
