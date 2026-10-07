//! Default tasks (D-71): ready-made tasks with a checklist, personal or shared with a
//! group. The client creates tasks from them with the usual `POST /tasks`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{ChecklistItem, TaskTemplate, upsert_task_template, with_checklist},
    routes::tasks::{check_attrs, check_notes, check_task_type, check_title},
    util::{double_option, id_or_new, now},
    visibility,
};

async fn load_visible(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<TaskTemplate> {
    let t: TaskTemplate =
        sqlx::query_as("SELECT * FROM task_templates WHERE id = ? AND deleted_at IS NULL")
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

/// Validate everything a template refers to; the checklist is stored unticked.
async fn check(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    t: &mut TaskTemplate,
) -> ApiResult<()> {
    t.title = check_title(&t.title)?;
    check_notes(&t.notes)?;
    check_attrs(t.estimate_min, t.difficulty, t.importance, t.urgency)?;
    let items = crate::models::checklist(t.checklist.0.clone()).map_err(bad)?;
    t.checklist = sqlx::types::Json(with_checklist(&items, streamline_domain::checklist::fresh));
    if let Some(tt) = &t.task_type_id {
        check_task_type(conn, user, tt).await?;
    }
    if let Some(p) = &t.project_id {
        crate::routes::projects::load_visible(conn, user, p)
            .await
            .map_err(|_| bad("unknown project"))?;
    }
    crate::routes::places::check_place(conn, user, &t.place_id).await?;
    Ok(())
}

#[utoipa::path(get, path = "/task-templates", tag = "tasks", summary = "List default tasks", responses((status = 200, body = Vec<TaskTemplate>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<TaskTemplate>>> {
    let rows = sqlx::query_as(&format!(
        "SELECT * FROM task_templates WHERE {} AND deleted_at IS NULL ORDER BY title",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTemplate {
    id: Option<String>,
    title: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    checklist: Vec<ChecklistItem>,
    estimate_min: Option<i32>,
    difficulty: Option<i32>,
    importance: Option<i32>,
    urgency: Option<i32>,
    task_type_id: Option<String>,
    project_id: Option<String>,
    place_id: Option<String>,
    /// Share it with a group (`null` = just you).
    owner_group_id: Option<String>,
}

#[utoipa::path(post, path = "/task-templates", tag = "tasks", summary = "Create a default task", request_body = CreateTemplate, responses((status = 200, body = TaskTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateTemplate>,
) -> ApiResult<Json<TaskTemplate>> {
    let id = id_or_new(c.id)?;
    let owner = crate::ownership::chosen(&user, c.owner_group_id.as_deref())?;
    let mut tx = state.db.write.begin().await?;
    if let Some(existing) =
        sqlx::query_as::<_, TaskTemplate>("SELECT * FROM task_templates WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if visibility::can_see(
            &user,
            existing.owner_user_id.as_deref(),
            existing.owner_group_id.as_deref(),
        ) {
            return Ok(Json(existing));
        }
        return Err(AppError::Conflict("id already in use".into()));
    }
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM task_templates WHERE {} AND deleted_at IS NULL",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_one(&mut *tx)
    .await?;
    if count >= 500 {
        return Err(bad("at most 500 default tasks"));
    }
    let ts = now();
    let mut t = TaskTemplate {
        id,
        owner_user_id: owner.user,
        owner_group_id: owner.group,
        title: c.title,
        notes: c.notes,
        checklist: sqlx::types::Json(c.checklist),
        estimate_min: c.estimate_min,
        difficulty: c.difficulty,
        importance: c.importance,
        urgency: c.urgency,
        task_type_id: c.task_type_id,
        project_id: c.project_id,
        place_id: c.place_id,
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: 0,
    };
    check(&mut tx, &user, &mut t).await?;
    t.rev = next_rev(&mut tx).await?;
    upsert_task_template(&mut tx, &t).await?;
    tx.commit().await?;
    state.bus.publish([Change::task_template(&t)]);
    Ok(Json(t))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchTemplate {
    title: Option<String>,
    notes: Option<String>,
    checklist: Option<Vec<ChecklistItem>>,
    #[serde(default, deserialize_with = "double_option")]
    estimate_min: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    difficulty: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    importance: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    urgency: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    task_type_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    place_id: Option<Option<String>>,
    /// Share with a group (`null` = just you).
    #[serde(default, deserialize_with = "double_option")]
    owner_group_id: Option<Option<String>>,
}

/// Edit a default task. Tasks already made from it don't change.
#[utoipa::path(patch, path = "/task-templates/{id}", tag = "tasks", summary = "Edit a default task (existing tasks don't change)", params(("id" = String, Path, description = "ULID")), request_body = PatchTemplate, responses((status = 200, body = TaskTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchTemplate>,
) -> ApiResult<Json<TaskTemplate>> {
    let mut tx = state.db.write.begin().await?;
    let mut t = load_visible(&mut tx, &user, &id).await?;
    let old_audience =
        visibility::audience(t.owner_user_id.as_deref(), t.owner_group_id.as_deref());
    if let Some(v) = c.title {
        t.title = v;
    }
    if let Some(v) = c.notes {
        t.notes = v;
    }
    if let Some(v) = c.checklist {
        t.checklist = sqlx::types::Json(v);
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
    if let Some(v) = c.task_type_id {
        t.task_type_id = v;
    }
    if let Some(v) = c.project_id {
        t.project_id = v;
    }
    if let Some(v) = c.place_id {
        t.place_id = v;
    }
    if let Some(g) = c.owner_group_id {
        let o = crate::ownership::chosen(&user, g.as_deref())?;
        t.owner_user_id = o.user;
        t.owner_group_id = o.group;
    }
    check(&mut tx, &user, &mut t).await?;
    t.updated_at = now();
    t.rev = next_rev(&mut tx).await?;
    upsert_task_template(&mut tx, &t).await?;
    tx.commit().await?;
    let change = Change::task_template(&t);
    let gone = crate::ownership::gone_for(&change, &old_audience, &t.updated_at);
    state.bus.publish(std::iter::once(change).chain(gone));
    Ok(Json(t))
}

#[utoipa::path(delete, path = "/task-templates/{id}", tag = "tasks", summary = "Delete a default task", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut t = load_visible(&mut tx, &user, &id).await?;
    let ts = now();
    t.deleted_at = Some(ts.clone());
    t.updated_at = ts;
    t.rev = next_rev(&mut tx).await?;
    upsert_task_template(&mut tx, &t).await?;
    tx.commit().await?;
    state.bus.publish([Change::task_template(&t)]);
    Ok(StatusCode::NO_CONTENT)
}
