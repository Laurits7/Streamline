//! Workflow templates (SPEC §6.3b): create/edit them, and start runs.

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
    models::{
        Series, Task, WorkflowStep, WorkflowTemplate, WorkflowVariant, upsert_series,
        upsert_workflow,
    },
    util::{check_range, double_option, id_or_new, now, parse_date},
    visibility,
    workflows::{self, RunOptions},
};

pub async fn load_visible(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<WorkflowTemplate> {
    let w: WorkflowTemplate =
        sqlx::query_as("SELECT * FROM workflow_templates WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(conn)
            .await?
            .ok_or(AppError::NotFound)?;
    if !visibility::can_see(
        &user.user,
        w.owner_user_id.as_deref(),
        w.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(w)
}

fn check(name: &str, steps: &[WorkflowStep], variants: &[WorkflowVariant]) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(bad("name must be 1-200 characters"));
    }
    if steps.is_empty() || steps.len() > 30 {
        return Err(bad("a workflow needs 1-30 steps"));
    }
    let mut ids = std::collections::HashSet::new();
    for s in steps {
        if s.id.is_empty() || s.id.len() > 40 || !ids.insert(s.id.as_str()) {
            return Err(bad("step ids must be unique and 1-40 characters"));
        }
        if s.title.trim().is_empty() || s.title.chars().count() > 200 {
            return Err(bad("step titles must be 1-200 characters"));
        }
        check_range("estimate_min", s.estimate_min, 0, 24 * 60)?;
        check_range("wait_min", s.wait_min, 0, 24 * 60)?;
        check_range("difficulty", s.difficulty, 1, 3)?;
    }
    if variants.len() > 20 {
        return Err(bad("at most 20 variants"));
    }
    let mut vids = std::collections::HashSet::new();
    for v in variants {
        if v.id.is_empty()
            || !vids.insert(v.id.as_str())
            || v.name.trim().is_empty()
            || v.name.chars().count() > 100
        {
            return Err(bad(
                "variants need a unique id and a name of 1-100 characters",
            ));
        }
        if v.skip.iter().any(|s| !ids.contains(s.as_str())) {
            return Err(bad("a variant skips a step that doesn't exist"));
        }
        if steps.iter().all(|s| v.skip.contains(&s.id)) {
            return Err(bad(format!("variant “{}” skips every step", v.name)));
        }
    }
    Ok(name.to_string())
}

#[utoipa::path(get, path = "/workflows", tag = "workflows", summary = "List workflow templates", responses((status = 200, body = Vec<WorkflowTemplate>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<WorkflowTemplate>>> {
    let rows = sqlx::query_as(&format!(
        "SELECT * FROM workflow_templates WHERE {} AND deleted_at IS NULL ORDER BY name",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateWorkflow {
    id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    project_id: Option<String>,
    steps: Vec<WorkflowStep>,
    #[serde(default)]
    variants: Vec<WorkflowVariant>,
}

#[utoipa::path(post, path = "/workflows", tag = "workflows", summary = "Create a workflow template", request_body = CreateWorkflow, responses((status = 200, body = WorkflowTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateWorkflow>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let name = check(&c.name, &c.steps, &c.variants)?;
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if let Some(existing) =
        sqlx::query_as::<_, WorkflowTemplate>("SELECT * FROM workflow_templates WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if existing.owner_user_id.as_deref() == Some(user.id()) {
            return Ok(Json(existing));
        }
        return Err(AppError::Conflict("id already in use".into()));
    }
    if let Some(p) = &c.project_id {
        crate::routes::projects::load_visible(&mut tx, &user, p)
            .await
            .map_err(|_| bad("unknown project"))?;
    }
    let ts = now();
    let w = WorkflowTemplate {
        id,
        owner_user_id: Some(user.id().into()),
        owner_group_id: None,
        name,
        description: c.description,
        project_id: c.project_id,
        steps: sqlx::types::Json(c.steps),
        variants: sqlx::types::Json(c.variants),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_workflow(&mut tx, &w).await?;
    tx.commit().await?;
    state.bus.publish([Change::workflow(&w)]);
    Ok(Json(w))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchWorkflow {
    name: Option<String>,
    description: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    project_id: Option<Option<String>>,
    steps: Option<Vec<WorkflowStep>>,
    variants: Option<Vec<WorkflowVariant>>,
}

/// Edit a template. Runs already started keep their tasks.
#[utoipa::path(patch, path = "/workflows/{id}", tag = "workflows", summary = "Edit a workflow template (running chains are not changed)", params(("id" = String, Path, description = "ULID")), request_body = PatchWorkflow, responses((status = 200, body = WorkflowTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchWorkflow>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let mut tx = state.db.write.begin().await?;
    let mut w = load_visible(&mut tx, &user, &id).await?;
    if let Some(v) = c.name {
        w.name = v;
    }
    if let Some(v) = c.description {
        w.description = v;
    }
    if let Some(v) = c.project_id {
        if let Some(p) = &v {
            crate::routes::projects::load_visible(&mut tx, &user, p)
                .await
                .map_err(|_| bad("unknown project"))?;
        }
        w.project_id = v;
    }
    if let Some(v) = c.steps {
        w.steps = sqlx::types::Json(v);
    }
    if let Some(v) = c.variants {
        w.variants = sqlx::types::Json(v);
    }
    w.name = check(&w.name, &w.steps.0, &w.variants.0)?;
    w.updated_at = now();
    w.rev = next_rev(&mut tx).await?;
    upsert_workflow(&mut tx, &w).await?;
    tx.commit().await?;
    state.bus.publish([Change::workflow(&w)]);
    Ok(Json(w))
}

/// Delete a template; running chains stay, routines that used it go back to single tasks.
#[utoipa::path(delete, path = "/workflows/{id}", tag = "workflows", summary = "Delete a workflow template", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut w = load_visible(&mut tx, &user, &id).await?;
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    let mut changes = vec![];
    let series: Vec<Series> = sqlx::query_as(
        "SELECT * FROM series WHERE workflow_template_id = ? AND deleted_at IS NULL",
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;
    for mut s in series {
        s.workflow_template_id = None;
        s.workflow_variant_ids = sqlx::types::Json(vec![]);
        s.updated_at = ts.clone();
        s.rev = rev;
        upsert_series(&mut tx, &s).await?;
        changes.push(Change::series(&s));
    }
    w.deleted_at = Some(ts.clone());
    w.updated_at = ts;
    w.rev = rev;
    upsert_workflow(&mut tx, &w).await?;
    changes.push(Change::workflow(&w));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct StartWorkflow {
    /// Variants to run, one chain each (e.g. two loads: whites and towels). Empty = the whole template once.
    #[serde(default)]
    variant_ids: Vec<String>,
    /// Put the steps in this project (default: the template's).
    project_id: Option<String>,
    /// Plan the first step of each chain into this day (`YYYY-MM-DD`).
    day: Option<String>,
}

#[utoipa::path(post, path = "/workflows/{id}/start", tag = "workflows", summary = "Start a workflow: one chain of tasks per chosen variant", params(("id" = String, Path, description = "ULID")), request_body = StartWorkflow, responses((status = 200, description = "The created step tasks", body = Vec<Task>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn start(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<StartWorkflow>,
) -> ApiResult<Json<Vec<Task>>> {
    if let Some(d) = &c.day {
        parse_date(d)?;
    }
    let mut tx = state.db.write.begin().await?;
    let w = load_visible(&mut tx, &user, &id).await?;
    if c.variant_ids
        .iter()
        .any(|v| !w.variants.0.iter().any(|x| x.id == *v))
    {
        return Err(bad("unknown variant"));
    }
    if let Some(p) = &c.project_id {
        crate::routes::projects::load_visible(&mut tx, &user, p)
            .await
            .map_err(|_| bad("unknown project"))?;
    }
    let opts = RunOptions {
        project_id: c.project_id,
        day: c.day,
        ..Default::default()
    };
    let (tasks, changes) = workflows::start(&mut tx, user.id(), &w, &c.variant_ids, &opts).await?;
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(Json(tasks))
}
