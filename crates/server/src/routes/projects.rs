use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::collections::HashMap;

use serde::Deserialize;
use streamline_domain::{marks, order::key_after, tree};

use crate::ownership::Owner;
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

/// `#rrggbb` only: colours end up in CSS, and shared projects' colours reach other users.
fn check_color(c: &Option<String>) -> ApiResult<()> {
    match c {
        Some(c)
            if !(c.len() == 7
                && c.starts_with('#')
                && c[1..].bytes().all(|b| b.is_ascii_hexdigit())) =>
        {
            Err(bad("color must look like #3b82f6"))
        }
        _ => Ok(()),
    }
}

fn check_shape(s: &str) -> ApiResult<()> {
    if marks::is_shape(s) {
        Ok(())
    } else {
        Err(bad(
            "shape must be circle, square, triangle, diamond, hexagon or star",
        ))
    }
}

/// A colour and shape for a new project that sets apart from the user's others (D-74):
/// top-level projects get an unused pair, subprojects their parent's colour and a shape
/// their parent and siblings don't have.
async fn auto_mark(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    parent: Option<&Project>,
) -> ApiResult<(Option<String>, String)> {
    if let Some(parent) = parent {
        let siblings: Vec<String> = sqlx::query_scalar(
            "SELECT shape FROM projects WHERE parent_id = ? AND deleted_at IS NULL",
        )
        .bind(&parent.id)
        .fetch_all(conn)
        .await?;
        let siblings: Vec<&str> = siblings.iter().map(String::as_str).collect();
        let shape = marks::pick_sub(&parent.shape, &siblings);
        return Ok((parent.color.clone(), shape.to_string()));
    }
    let used: Vec<(Option<String>, String)> = sqlx::query_as(&format!(
        "SELECT color, shape FROM projects WHERE {} AND parent_id IS NULL AND deleted_at IS NULL",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(conn)
    .await?;
    let used: Vec<(&str, &str)> = used
        .iter()
        .filter_map(|(c, s)| Some((c.as_deref()?, s.as_str())))
        .collect();
    let (color, shape) = marks::pick_top(&used);
    Ok((Some(color.to_string()), shape.to_string()))
}

fn check_description(d: &str) -> ApiResult<String> {
    if d.chars().count() > 10_000 {
        return Err(bad("description must be at most 10,000 characters"));
    }
    Ok(d.trim_end().to_string())
}

fn check_status(s: &str) -> ApiResult<()> {
    match s {
        "active" | "idea" => Ok(()),
        _ => Err(bad("status must be active or idea")),
    }
}

/// For SQL over `tasks t`: the task isn't in an idea project. Ideas' tasks ask for no
/// attention: no reminders, never missed, can't be planned (D-72).
pub const NOT_IN_IDEA_SQL: &str =
    "(t.project_id IS NULL OR t.project_id NOT IN (SELECT id FROM projects WHERE status = 'idea'))";

pub async fn is_idea(
    conn: &mut sqlx::SqliteConnection,
    project_id: Option<&str>,
) -> sqlx::Result<bool> {
    let Some(id) = project_id else {
        return Ok(false);
    };
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(status.as_deref() == Some("idea"))
}

/// Take open tasks off the plan from `from` (a date) on, when they move into an idea.
/// Earlier days keep their entries as history.
pub async fn unplan(
    conn: &mut sqlx::SqliteConnection,
    task_ids: &[String],
    from: &str,
    rev: i64,
    ts: &str,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let entries: Vec<DayEntry> = sqlx::query_as(
        "SELECT e.* FROM day_entries e JOIN tasks t ON t.id = e.task_id
         WHERE e.task_id IN (SELECT value FROM json_each(?1)) AND e.date >= ?2
           AND e.deleted_at IS NULL AND t.status = 'open'",
    )
    .bind(serde_json::to_string(task_ids).unwrap_or_default())
    .bind(from)
    .fetch_all(&mut *conn)
    .await?;
    for mut e in entries {
        e.deleted_at = Some(ts.to_string());
        e.updated_at = ts.to_string();
        e.rev = rev;
        upsert_entry(&mut *conn, &e).await?;
        changes.push(Change::entry(&e));
    }
    Ok(())
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
        user,
        p.owner_user_id.as_deref(),
        p.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(p)
}

#[utoipa::path(get, path = "/projects", tag = "projects", summary = "List projects", responses((status = 200, body = Vec<Project>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

/// Parent links of all of the user's live projects, for tree checks.
async fn parent_map(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
) -> ApiResult<HashMap<String, Option<String>>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id, parent_id FROM projects WHERE {} AND deleted_at IS NULL",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Validate a new parent for project `id` (`None` when creating): it must be visible,
/// have the same owner, and not be the project itself or one of its descendants.
async fn check_parent(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: Option<&str>,
    parent_id: &str,
) -> ApiResult<Project> {
    let parent = load_visible(conn, user, parent_id)
        .await
        .map_err(|_| bad("unknown parent project"))?;
    if let Some(id) = id
        && tree::would_cycle(id, parent_id, &parent_map(conn, user).await?)
    {
        return Err(bad(
            "a project can't be moved inside itself or one of its subprojects",
        ));
    }
    Ok(parent)
}

async fn last_sibling_position(
    conn: &mut sqlx::SqliteConnection,
    owner: &Owner,
    parent_id: &Option<String>,
) -> ApiResult<String> {
    let last: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM projects WHERE parent_id IS ?1
           AND (?1 IS NOT NULL OR (owner_user_id IS ?2 AND owner_group_id IS ?3)) AND deleted_at IS NULL",
    )
    .bind(parent_id)
    .bind(&owner.user)
    .bind(&owner.group)
    .fetch_one(conn)
    .await?;
    Ok(key_after(last.as_deref()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateProject {
    id: Option<String>,
    name: String,
    /// Picked automatically when left out (with the shape), so projects differ (D-74).
    color: Option<String>,
    shape: Option<String>,
    position: Option<String>,
    parent_id: Option<String>,
    default_place_id: Option<String>,
    /// Share a top-level project with a group (subprojects follow their parent).
    owner_group_id: Option<String>,
    #[serde(default)]
    description: String,
    /// `active` (default) or `idea`. Subprojects of an idea are ideas too.
    status: Option<String>,
}

#[utoipa::path(post, path = "/projects", tag = "projects", summary = "Create a project (optionally inside another)", request_body = CreateProject, responses((status = 200, body = Project), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
    if let Some(sh) = &c.shape {
        check_shape(sh)?;
    }
    let description = check_description(&c.description)?;
    let mut status = c.status.unwrap_or_else(|| "active".into());
    check_status(&status)?;
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
    let mut owner = crate::ownership::chosen(&user, c.owner_group_id.as_deref())?;
    let mut parent = None;
    if let Some(parent_id) = &c.parent_id {
        let p = check_parent(&mut tx, &user, None, parent_id).await?;
        // Subprojects take their parent's owner; one in an idea is an idea too.
        owner = Owner::of_project(&p);
        if p.status == "idea" {
            status = p.status.clone();
        }
        parent = Some(p);
    }
    let (auto_color, auto_shape) = auto_mark(&mut tx, &user, parent.as_ref()).await?;
    let color = c.color.or(auto_color);
    let shape = c.shape.unwrap_or(auto_shape);
    let position = match c.position {
        Some(p) => p,
        None => last_sibling_position(&mut tx, &owner, &c.parent_id).await?,
    };
    crate::routes::places::check_place(&mut tx, &user, &c.default_place_id).await?;
    let ts = now();
    let p = Project {
        id,
        owner_user_id: owner.user.clone(),
        owner_group_id: owner.group.clone(),
        parent_id: c.parent_id,
        name,
        color,
        shape,
        position,
        archived_at: None,
        default_place_id: c.default_place_id,
        description,
        status,
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchProject {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    color: Option<Option<String>>,
    shape: Option<String>,
    position: Option<String>,
    /// Archiving (or unarchiving) applies to the whole subtree.
    archived: Option<bool>,
    /// Move under another project (`null` = top level).
    #[serde(default, deserialize_with = "double_option")]
    parent_id: Option<Option<String>>,
    /// Place given to new tasks in this project.
    #[serde(default, deserialize_with = "double_option")]
    default_place_id: Option<Option<String>>,
    /// Share a top-level project (and everything in it) with a group; `null` = just you.
    #[serde(default, deserialize_with = "double_option")]
    owner_group_id: Option<Option<String>>,
    description: Option<String>,
    /// `active` or `idea`, for the whole subtree. Becoming an idea takes its open tasks
    /// off the plan from today on (D-72).
    status: Option<String>,
}

#[utoipa::path(patch, path = "/projects/{id}", tag = "projects", summary = "Rename, describe, recolour, reorder, archive, move or activate a project", params(("id" = String, Path, description = "ULID")), request_body = PatchProject, responses((status = 200, body = Project), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
    if let Some(d) = c.description {
        p.description = check_description(&d)?;
    }
    if let Some(st) = &c.status {
        check_status(st)?;
    }
    // Moving into an idea makes the moved subtree an idea too.
    let mut status = c.status;
    if let Some(color) = c.color {
        check_color(&color)?;
        p.color = color;
    }
    if let Some(sh) = c.shape {
        check_shape(&sh)?;
        p.shape = sh;
    }
    if let Some(parent_id) = c.parent_id
        && parent_id != p.parent_id
    {
        if let Some(pid) = &parent_id {
            let parent = check_parent(&mut tx, &user, Some(&id), pid).await?;
            if parent.status == "idea" {
                status = Some(parent.status);
            }
        }
        p.parent_id = parent_id;
        if c.position.is_none() {
            p.position =
                last_sibling_position(&mut tx, &Owner::of_project(&p), &p.parent_id).await?;
        }
    }
    if let Some(pos) = c.position {
        check_position(&pos)?;
        p.position = pos;
    }
    if let Some(v) = c.default_place_id {
        crate::routes::places::check_place(&mut tx, &user, &v).await?;
        p.default_place_id = v;
    }
    // New owner: chosen for a top-level project, or inherited from a new parent.
    let mut new_owner = None;
    if let Some(g) = c.owner_group_id {
        if p.parent_id.is_some() {
            return Err(bad(
                "subprojects are shared together with their top-level project",
            ));
        }
        new_owner = Some(crate::ownership::chosen(&user, g.as_deref())?);
    } else if let Some(parent) = &p.parent_id {
        let o = crate::ownership::project_owner(&mut tx, parent).await?;
        new_owner = o.filter(|o| *o != Owner::of_project(&p));
    }
    let rev = next_rev(&mut tx).await?;
    let mut owner_changes = vec![];
    if let Some(o) = new_owner.filter(|o| *o != Owner::of_project(&p)) {
        p.owner_user_id = o.user.clone();
        p.owner_group_id = o.group.clone();
        crate::ownership::reown_subtree(&mut tx, &id, &o, rev, &now(), &mut owner_changes).await?;
    }
    let ts = now();
    let mut changes = vec![];
    if let Some(a) = c.archived {
        let archived_at = if a {
            Some(p.archived_at.clone().unwrap_or_else(|| ts.clone()))
        } else {
            None
        };
        p.archived_at = archived_at.clone();
        // Apply to every subproject as well.
        for sub_id in tree::subtree(&id, &parent_map(&mut tx, &user).await?)
            .into_iter()
            .skip(1)
        {
            let mut sub = load_visible(&mut tx, &user, &sub_id).await?;
            if sub.archived_at.is_some() != a {
                sub.archived_at = archived_at.clone();
                sub.updated_at = ts.clone();
                sub.rev = rev;
                upsert_project(&mut tx, &sub).await?;
                changes.push(Change::project(&sub));
            }
        }
    }
    if let Some(st) = status
        && st != p.status
    {
        let subtree = tree::subtree(&id, &parent_map(&mut tx, &user).await?);
        for sub_id in subtree.iter().skip(1) {
            let mut sub = load_visible(&mut tx, &user, sub_id).await?;
            if sub.status != st {
                sub.status = st.clone();
                sub.updated_at = ts.clone();
                sub.rev = rev;
                upsert_project(&mut tx, &sub).await?;
                changes.push(Change::project(&sub));
            }
        }
        if st == "idea" {
            let tasks: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM tasks WHERE project_id IN (SELECT value FROM json_each(?1))
                   AND status = 'open' AND deleted_at IS NULL",
            )
            .bind(serde_json::to_string(&subtree).unwrap_or_default())
            .fetch_all(&mut *tx)
            .await?;
            let today = crate::rollover::today_for(&user.user)
                .format("%Y-%m-%d")
                .to_string();
            unplan(&mut tx, &tasks, &today, rev, &ts, &mut changes).await?;
        } else {
            // Activated: routines resume from today (the idea's days aren't caught up on).
            let yesterday = (crate::rollover::today_for(&user.user) - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string();
            sqlx::query(
                "UPDATE series SET materialized_through = ?1
                 WHERE project_id IN (SELECT value FROM json_each(?2)) AND deleted_at IS NULL
                   AND (materialized_through IS NULL OR materialized_through > ?1)",
            )
            .bind(&yesterday)
            .bind(serde_json::to_string(&subtree).unwrap_or_default())
            .execute(&mut *tx)
            .await?;
        }
        p.status = st;
    }
    p.updated_at = ts;
    p.rev = rev;
    upsert_project(&mut tx, &p).await?;
    changes.extend(owner_changes);
    changes.push(Change::project(&p));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(Json(p))
}

/// Soft-deletes the project and all its subprojects, with their tasks and the
/// tasks' day-plan entries.
#[utoipa::path(delete, path = "/projects/{id}", tag = "projects", summary = "Delete a project with its subprojects and tasks", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    load_visible(&mut tx, &user, &id).await?;
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    let mut changes = vec![];
    for pid in tree::subtree(&id, &parent_map(&mut tx, &user).await?) {
        let mut p = load_visible(&mut tx, &user, &pid).await?;
        let tasks: Vec<Task> =
            sqlx::query_as("SELECT * FROM tasks WHERE project_id = ? AND deleted_at IS NULL")
                .bind(&pid)
                .fetch_all(&mut *tx)
                .await?;
        for mut t in tasks {
            let entries: Vec<DayEntry> = sqlx::query_as(
                "SELECT * FROM day_entries WHERE task_id = ? AND deleted_at IS NULL",
            )
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
            crate::deps::refresh_dependents(&mut tx, &t.id, &mut changes).await?;
        }
        p.deleted_at = Some(ts.clone());
        p.updated_at = ts.clone();
        p.rev = rev;
        upsert_project(&mut tx, &p).await?;
        changes.push(Change::project(&p));
        // Tasks that were only *also* listed here just lose the link.
        let linked: Vec<Task> = sqlx::query_as(
            "SELECT * FROM tasks WHERE deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(also_project_ids) WHERE value = ?)",
        )
        .bind(&pid)
        .fetch_all(&mut *tx)
        .await?;
        for mut t in linked {
            t.also_project_ids.0.retain(|x| *x != pid);
            t.updated_at = ts.clone();
            t.rev = rev;
            upsert_task(&mut tx, &t).await?;
            changes.push(Change::task(&t));
        }
    }
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}
