use axum::{
    Json,
    extract::{Path, State},
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
    models::{DayEntry, Project, Task, upsert_entry, upsert_project, upsert_task},
    util::{check_position, double_option, id_or_new, now},
    visibility,
};

fn check_name(name: &str) -> ApiResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 200 {
        return Err(bad("name must be 1-200 characters"));
    }
    Ok(n.to_string())
}

fn check_color(c: &Option<String>) -> ApiResult<()> {
    match c {
        Some(c) if c.len() > 32 => Err(bad("color too long")),
        _ => Ok(()),
    }
}

pub async fn load_visible(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<Project> {
    let p: Project = sqlx::query_as("SELECT * FROM projects WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    if !visibility::can_see(
        &user.user,
        p.owner_user_id.as_deref(),
        p.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(p)
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Project>>> {
    let rows = sqlx::query_as(&format!(
        "SELECT * FROM projects WHERE {} AND deleted_at IS NULL ORDER BY position",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateProject {
    id: Option<String>,
    name: String,
    color: Option<String>,
    position: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateProject>,
) -> ApiResult<Json<Project>> {
    let id = id_or_new(c.id)?;
    let name = check_name(&c.name)?;
    check_color(&c.color)?;
    if let Some(p) = &c.position {
        check_position(p)?;
    }
    let mut tx = state.db.write.begin().await?;
    // Idempotent retry: creating the same id again returns the existing project.
    if let Some(existing) = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        if existing.owner_user_id.as_deref() == Some(user.id()) {
            return Ok(Json(existing));
        }
        return Err(AppError::Conflict("id already in use".into()));
    }
    let position = match c.position {
        Some(p) => p,
        None => {
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM projects WHERE owner_user_id = ? AND deleted_at IS NULL",
            )
            .bind(user.id())
            .fetch_one(&mut *tx)
            .await?;
            key_after(last.as_deref())
        }
    };
    let ts = now();
    let p = Project {
        id,
        owner_user_id: Some(user.id().to_string()),
        owner_group_id: None,
        name,
        color: c.color,
        position,
        archived_at: None,
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_project(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::project(&p)]);
    Ok(Json(p))
}

#[derive(Deserialize)]
pub struct PatchProject {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    color: Option<Option<String>>,
    position: Option<String>,
    archived: Option<bool>,
}

pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchProject>,
) -> ApiResult<Json<Project>> {
    let mut tx = state.db.write.begin().await?;
    let mut p = load_visible(&mut tx, &user, &id).await?;
    if let Some(n) = c.name {
        p.name = check_name(&n)?;
    }
    if let Some(color) = c.color {
        check_color(&color)?;
        p.color = color;
    }
    if let Some(pos) = c.position {
        check_position(&pos)?;
        p.position = pos;
    }
    if let Some(a) = c.archived {
        p.archived_at = if a {
            Some(p.archived_at.clone().unwrap_or_else(now))
        } else {
            None
        };
    }
    p.updated_at = now();
    p.rev = next_rev(&mut tx).await?;
    upsert_project(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::project(&p)]);
    Ok(Json(p))
}

/// Soft-deletes the project together with its tasks and their day-plan entries.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut p = load_visible(&mut tx, &user, &id).await?;
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    let mut changes = vec![];
    let tasks: Vec<Task> =
        sqlx::query_as("SELECT * FROM tasks WHERE project_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for mut t in tasks {
        let entries: Vec<DayEntry> =
            sqlx::query_as("SELECT * FROM day_entries WHERE task_id = ? AND deleted_at IS NULL")
                .bind(&t.id)
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
        t.updated_at = ts.clone();
        t.rev = rev;
        upsert_task(&mut tx, &t).await?;
        changes.push(Change::task(&t));
    }
    p.deleted_at = Some(ts.clone());
    p.updated_at = ts;
    p.rev = rev;
    upsert_project(&mut tx, &p).await?;
    changes.push(Change::project(&p));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}
