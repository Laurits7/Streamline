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
        user,
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

/// Refusal for planning a task whose project is still an idea (D-72).
pub const IDEA_UNPLANNABLE: &str =
    "this task's project is still an idea; activate the project to plan its tasks";

/// A linked event must be one of the user's own (events are personal).
async fn check_event(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &Option<String>,
) -> ApiResult<()> {
    if let Some(id) = id {
        let ok: Option<String> = sqlx::query_scalar(
            "SELECT id FROM events WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(user.id())
        .fetch_optional(conn)
        .await?;
        ok.ok_or_else(|| bad("unknown calendar event"))?;
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
        visibility::OWNED_VISIBLE_SQL
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
    /// Where it has to be done; defaults to the project's default place.
    place_id: Option<String>,
    /// The calendar event (instance id) the task is for.
    event_id: Option<String>,
    /// Also plan the new task into this day (`YYYY-MM-DD`).
    day: Option<String>,
    /// Client-chosen id for that day entry.
    day_entry_id: Option<String>,
    /// Share a task without a project with a group (tasks in a project follow the project).
    owner_group_id: Option<String>,
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
    let owner = match &c.project_id {
        Some(p) => crate::ownership::project_owner(&mut tx, p)
            .await?
            .unwrap_or_else(|| crate::ownership::Owner::me(&user)),
        None => crate::ownership::chosen(&user, c.owner_group_id.as_deref())?,
    };
    check_task_type(&mut tx, &user, &task_type_id).await?;
    crate::routes::places::check_place(&mut tx, &user, &c.place_id).await?;
    check_event(&mut tx, &user, &c.event_id).await?;
    let place_id = match (&c.place_id, &c.project_id) {
        (Some(p), _) => Some(p.clone()),
        (None, Some(project)) => {
            sqlx::query_scalar("SELECT default_place_id FROM projects WHERE id = ?")
                .bind(project)
                .fetch_one(&mut *tx)
                .await?
        }
        (None, None) => None,
    };
    let position = match c.position {
        Some(p) => p,
        None => {
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM tasks WHERE project_id IS ?1 AND (?1 IS NOT NULL OR owner_user_id = ?2) AND deleted_at IS NULL",
            )
            .bind(&c.project_id)
            .bind(user.id())
            .fetch_one(&mut *tx)
            .await?;
            key_after(last.as_deref())
        }
    };
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let t = Task {
        id,
        owner_user_id: owner.user.clone(),
        owner_group_id: owner.group.clone(),
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
        place_id,
        event_id: c.event_id,
        also_project_ids: sqlx::types::Json(vec![]),
        depends_on: sqlx::types::Json(vec![]),
        blocked: false,
        wait_min: None,
        ready_at: None,
        workflow_instance_id: None,
        workflow_step: None,
        workflow_steps: None,
        series_id: None,
        occurrence_key: None,
        occurrence_date: None,
        window_end: None,
        waiting_since: None,
        check_back_at: None,
        waiting_note: String::new(),
        waiting_by: None,
        created_at: ts.clone(),
        updated_at: ts.clone(),
        deleted_at: None,
        rev,
    };
    if c.day.is_some() && crate::routes::projects::is_idea(&mut tx, t.project_id.as_deref()).await?
    {
        return Err(bad(IDEA_UNPLANNABLE));
    }
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
    /// Other projects to also list the task in (replaces the current list).
    also_project_ids: Option<Vec<String>>,
    /// Where it has to be done; `null` = anywhere.
    #[serde(default, deserialize_with = "double_option")]
    place_id: Option<Option<String>>,
    /// The calendar event (instance id) the task is for; `null` = none.
    #[serde(default, deserialize_with = "double_option")]
    event_id: Option<Option<String>>,
    /// Tasks that must be finished first (replaces the current list). Cycles are refused.
    depends_on: Option<Vec<String>>,
    /// Minutes to wait after the last prerequisite is done before this becomes ready.
    #[serde(default, deserialize_with = "double_option")]
    wait_min: Option<Option<i32>>,
    /// For a task without a project: share it with a group (`null` = just you).
    #[serde(default, deserialize_with = "double_option")]
    owner_group_id: Option<Option<String>>,
    /// Waiting for results (D-70): `true` starts waiting (or changes the check-back time
    /// and note while waiting), `false` ends it; the task stays in progress.
    waiting: Option<bool>,
    /// When to check back (RFC 3339); `null` = no time, wait until changed by hand.
    #[serde(default, deserialize_with = "double_option")]
    check_back_at: Option<Option<String>>,
    /// What's being waited for.
    waiting_note: Option<String>,
}

/// A check-back time as the server stores timestamps (UTC, milliseconds).
fn check_back_time(v: &str) -> ApiResult<String> {
    chrono::DateTime::parse_from_rfc3339(v)
        .map(|d| {
            d.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .map_err(|_| bad("check_back_at must be an RFC 3339 time"))
}

fn clear_waiting(t: &mut Task) {
    t.waiting_since = None;
    t.check_back_at = None;
    t.waiting_note = String::new();
    t.waiting_by = None;
}

/// Validate prerequisites: visible, not the task itself, no cycles, at most 20.
async fn check_deps(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    task_id: &str,
    ids: Vec<String>,
) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for id in ids {
        if out.contains(&id) {
            continue;
        }
        if id == task_id {
            return Err(bad("a task can't wait for itself"));
        }
        load_visible(conn, user, &id)
            .await
            .map_err(|_| bad("unknown prerequisite"))?;
        out.push(id);
    }
    if out.len() > 20 {
        return Err(bad("at most 20 prerequisites"));
    }
    let rows: Vec<(String, sqlx::types::Json<Vec<String>>)> = sqlx::query_as(&format!(
        "SELECT id, depends_on FROM tasks WHERE {} AND deleted_at IS NULL AND depends_on <> '[]'",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&mut *conn)
    .await?;
    let graph = rows.into_iter().map(|(id, d)| (id, d.0)).collect();
    if streamline_domain::deps::creates_cycle(task_id, &out, &graph) {
        return Err(bad("that would make tasks wait for each other in a circle"));
    }
    Ok(out)
}

/// Validate "also in" projects: visible, unique, not the main project, at most 10.
async fn check_also(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    main: &Option<String>,
    ids: Vec<String>,
) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for id in ids {
        if Some(&id) == main.as_ref() || out.contains(&id) {
            continue;
        }
        crate::routes::projects::load_visible(conn, user, &id)
            .await
            .map_err(|_| bad("unknown project"))?;
        out.push(id);
    }
    if out.len() > 10 {
        return Err(bad("a task can be in at most 10 extra projects"));
    }
    Ok(out)
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
    let old_status = t.status.clone();
    if let Some(v) = c.title {
        t.title = check_title(&v)?;
    }
    if let Some(v) = c.notes {
        check_notes(&v)?;
        t.notes = v;
    }
    let old_audience =
        visibility::audience(t.owner_user_id.as_deref(), t.owner_group_id.as_deref());
    let old_project = t.project_id.clone();
    if let Some(v) = c.project_id {
        check_project(&mut tx, &user, &v).await?;
        t.project_id = v;
        // A task in a project belongs to whoever owns the project.
        if let Some(p) = &t.project_id
            && let Some(o) = crate::ownership::project_owner(&mut tx, p).await?
        {
            t.owner_user_id = o.user;
            t.owner_group_id = o.group;
        }
        // Becoming the main project replaces an "also in" link to it.
        let main = t.project_id.clone();
        t.also_project_ids.0.retain(|p| Some(p) != main.as_ref());
    }
    if let Some(g) = c.owner_group_id {
        if t.project_id.is_some() {
            return Err(bad(
                "this task is shared through its project; move it out of the project to share it separately",
            ));
        }
        let o = crate::ownership::chosen(&user, g.as_deref())?;
        t.owner_user_id = o.user;
        t.owner_group_id = o.group;
    }
    if let Some(v) = c.wait_min {
        check_range("wait_min", v, 0, 24 * 60)?;
        t.wait_min = v;
    }
    let deps_changed = c.depends_on.is_some();
    if let Some(v) = c.depends_on {
        t.depends_on = sqlx::types::Json(check_deps(&mut tx, &user, &t.id, v).await?);
    }
    if deps_changed {
        crate::deps::refresh(&mut tx, &mut t).await?;
    }
    if let Some(v) = c.place_id {
        crate::routes::places::check_place(&mut tx, &user, &v).await?;
        t.place_id = v;
    }
    if let Some(v) = c.event_id {
        check_event(&mut tx, &user, &v).await?;
        t.event_id = v;
    }
    if let Some(v) = c.also_project_ids {
        t.also_project_ids = sqlx::types::Json(check_also(&mut tx, &user, &t.project_id, v).await?);
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
            clear_waiting(&mut t);
            None
        };
    }
    if t.status != "open" {
        // Finished (or dropped): nothing left to wait for.
        clear_waiting(&mut t);
    }
    match c.waiting {
        Some(true) => {
            if t.status != "open" {
                return Err(bad("only an open task can wait for results"));
            }
            let check_back = match c.check_back_at {
                Some(Some(v)) => Some(check_back_time(&v)?),
                Some(None) => None,
                // Keep a time already set while still waiting; a due one is used up.
                None => t
                    .check_back_at
                    .clone()
                    .filter(|_| t.waiting_since.is_some()),
            };
            let note = c.waiting_note.unwrap_or_else(|| t.waiting_note.clone());
            if note.chars().count() > 500 {
                return Err(bad("waiting note must be at most 500 characters"));
            }
            let started = t.waiting_since.is_none();
            t.waiting_since = Some(t.waiting_since.clone().unwrap_or_else(now));
            t.started_at = Some(t.started_at.clone().unwrap_or_else(now));
            t.waiting_by = Some(user.id().into());
            t.check_back_at = check_back;
            t.waiting_note = note.trim().to_string();
            if started {
                log_task_event(
                    &mut tx,
                    &t.id,
                    Some(user.id()),
                    "waiting",
                    Some(serde_json::json!({"check_back_at": t.check_back_at})),
                )
                .await?;
            }
        }
        Some(false) => clear_waiting(&mut t),
        None => {
            if c.check_back_at.is_some() || c.waiting_note.is_some() {
                return Err(bad("set waiting to change the check-back time or note"));
            }
        }
    }
    t.updated_at = now();
    t.rev = next_rev(&mut tx).await?;
    upsert_task(&mut tx, &t).await?;
    let mut changes = vec![Change::task(&t)];
    // People who could see it before but not any more (e.g. unshared) drop it.
    changes.extend(crate::ownership::gone_for(
        &changes[0],
        &old_audience,
        &t.updated_at,
    ));
    if t.status != old_status {
        // Finishing (or reopening) a prerequisite unblocks (or re-blocks) what waits for it.
        crate::deps::refresh_dependents(&mut tx, &t.id, &mut changes).await?;
    }
    // Moved into an idea: off the plan from today on (D-72).
    if t.project_id != old_project
        && crate::routes::projects::is_idea(&mut tx, t.project_id.as_deref()).await?
    {
        let today = crate::rollover::today_for(&user.user)
            .format("%Y-%m-%d")
            .to_string();
        crate::routes::projects::unplan(
            &mut tx,
            &[t.id.clone()],
            &today,
            t.rev,
            &t.updated_at,
            &mut changes,
        )
        .await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);
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
    // A deleted prerequisite no longer blocks (D-6).
    crate::deps::refresh_dependents(&mut tx, &t.id, &mut changes).await?;
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}
