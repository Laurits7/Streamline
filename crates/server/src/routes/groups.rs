//! Groups (SPEC §6.4): create them, manage members (group owners and admins), leave.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::SqliteConnection;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{Group, GroupMember, UserSummary},
    util::{id_or_new, now},
};

#[derive(sqlx::FromRow)]
struct GroupRow {
    id: String,
    name: String,
    created_by: Option<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    rev: i64,
}

pub async fn load(conn: &mut SqliteConnection, id: &str) -> sqlx::Result<Option<Group>> {
    let Some(r) = sqlx::query_as::<_, GroupRow>("SELECT * FROM groups WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let members = sqlx::query_as(
        "SELECT m.user_id, u.display_name, u.username, m.role FROM group_members m JOIN users u ON u.id = m.user_id
         WHERE m.group_id = ? AND u.deleted_at IS NULL ORDER BY m.role DESC, u.display_name",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Some(Group {
        id: r.id,
        name: r.name,
        created_by: r.created_by,
        members,
        created_at: r.created_at,
        updated_at: r.updated_at,
        deleted_at: r.deleted_at,
        rev: r.rev,
    }))
}

/// Groups the user belongs to (admins: all groups).
pub async fn visible_groups(
    conn: &mut SqliteConnection,
    user: &AuthUser,
) -> sqlx::Result<Vec<Group>> {
    let ids: Vec<String> = if user.user.is_admin {
        sqlx::query_scalar("SELECT id FROM groups WHERE deleted_at IS NULL ORDER BY name")
            .fetch_all(&mut *conn)
            .await?
    } else {
        sqlx::query_scalar(
            "SELECT g.id FROM groups g JOIN group_members m ON m.group_id = g.id WHERE m.user_id = ? AND g.deleted_at IS NULL ORDER BY g.name",
        )
        .bind(user.id())
        .fetch_all(&mut *conn)
        .await?
    };
    let mut out = vec![];
    for id in ids {
        out.extend(load(conn, &id).await?);
    }
    Ok(out)
}

/// Group owners and admins manage a group; members can only see it (and leave).
async fn load_managed(conn: &mut SqliteConnection, user: &AuthUser, id: &str) -> ApiResult<Group> {
    let g = load(conn, id)
        .await?
        .filter(|g| g.deleted_at.is_none())
        .ok_or(AppError::NotFound)?;
    let member = g.members.iter().find(|m| m.user_id == user.id());
    if member.is_none() && !user.user.is_admin {
        return Err(AppError::NotFound);
    }
    if !user.user.is_admin && member.is_none_or(|m| m.role != "owner") {
        return Err(AppError::Forbidden(
            "only the group's owners can change it".into(),
        ));
    }
    Ok(g)
}

async fn bump(conn: &mut SqliteConnection, id: &str) -> sqlx::Result<()> {
    let rev = next_rev(conn).await?;
    sqlx::query("UPDATE groups SET updated_at = ?, rev = ? WHERE id = ?")
        .bind(now())
        .bind(rev)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[utoipa::path(get, path = "/groups", tag = "groups", summary = "Your groups (admins: all)", responses((status = 200, body = Vec<Group>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Group>>> {
    Ok(Json(
        visible_groups(&mut *state.db.read.acquire().await?, &user).await?,
    ))
}

#[utoipa::path(get, path = "/users/directory", tag = "groups", summary = "Everyone on this install (to add to groups)", responses((status = 200, body = Vec<UserSummary>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn directory(
    State(state): State<AppState>,
    _user: AuthUser,
) -> ApiResult<Json<Vec<UserSummary>>> {
    let rows = sqlx::query_as("SELECT id, display_name, username FROM users WHERE deleted_at IS NULL ORDER BY display_name")
        .fetch_all(&state.db.read)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateGroup {
    id: Option<String>,
    name: String,
}

fn check_name(n: &str) -> ApiResult<String> {
    let n = n.trim();
    if n.is_empty() || n.chars().count() > 100 {
        return Err(bad("name must be 1-100 characters"));
    }
    Ok(n.to_string())
}

#[utoipa::path(post, path = "/groups", tag = "groups", summary = "Create a group (you become its owner)", request_body = CreateGroup, responses((status = 200, body = Group), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateGroup>,
) -> ApiResult<Json<Group>> {
    let name = check_name(&c.name)?;
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if load(&mut tx, &id).await?.is_some() {
        return Err(AppError::Conflict("id already in use".into()));
    }
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    sqlx::query("INSERT INTO groups (id, name, created_by, created_at, updated_at, rev) VALUES (?,?,?,?,?,?)")
        .bind(&id)
        .bind(&name)
        .bind(user.id())
        .bind(&ts)
        .bind(&ts)
        .bind(rev)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id, role, created_at) VALUES (?, ?, 'owner', ?)",
    )
    .bind(&id)
    .bind(user.id())
    .bind(&ts)
    .execute(&mut *tx)
    .await?;
    let g = load(&mut tx, &id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    state
        .bus
        .publish([Change::group(&g, &[]), Change::membership(user.id())]);
    Ok(Json(g))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchGroup {
    name: Option<String>,
}

#[utoipa::path(patch, path = "/groups/{id}", tag = "groups", summary = "Rename a group", params(("id" = String, Path, description = "ULID")), request_body = PatchGroup, responses((status = 200, body = Group), (status = 403, description = "Not an owner", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchGroup>,
) -> ApiResult<Json<Group>> {
    let mut tx = state.db.write.begin().await?;
    load_managed(&mut tx, &user, &id).await?;
    if let Some(n) = c.name {
        sqlx::query("UPDATE groups SET name = ? WHERE id = ?")
            .bind(check_name(&n)?)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    bump(&mut tx, &id).await?;
    let g = load(&mut tx, &id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    state.bus.publish([Change::group(&g, &[])]);
    Ok(Json(g))
}

/// Delete a group. Refused while it still owns projects, tasks or routines.
#[utoipa::path(delete, path = "/groups/{id}", tag = "groups", summary = "Delete an empty group", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 409, description = "The group still owns things", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let g = load_managed(&mut tx, &user, &id).await?;
    let owned: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM projects WHERE owner_group_id = ?1 AND deleted_at IS NULL)
              + (SELECT COUNT(*) FROM tasks WHERE owner_group_id = ?1 AND deleted_at IS NULL AND status = 'open')
              + (SELECT COUNT(*) FROM series WHERE owner_group_id = ?1 AND deleted_at IS NULL)",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    if owned > 0 {
        return Err(AppError::Conflict(
            "the group still has shared projects, tasks or routines; move or delete them first"
                .into(),
        ));
    }
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    sqlx::query("UPDATE groups SET deleted_at = ?, updated_at = ?, rev = ? WHERE id = ?")
        .bind(&ts)
        .bind(&ts)
        .bind(rev)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM group_members WHERE group_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let mut deleted = load(&mut tx, &id).await?.ok_or(AppError::NotFound)?;
    deleted.members = vec![];
    tx.commit().await?;
    let former: Vec<String> = g.members.iter().map(|m| m.user_id.clone()).collect();
    let mut changes = vec![Change::group(&deleted, &former)];
    changes.extend(former.iter().map(|u| Change::membership(u)));
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddMember {
    user_id: String,
    /// `member` (default) or `owner`.
    role: Option<String>,
}

#[utoipa::path(post, path = "/groups/{id}/members", tag = "groups", summary = "Add someone to a group (or change their role)", params(("id" = String, Path, description = "ULID")), request_body = AddMember, responses((status = 200, body = Group), (status = 403, description = "Not an owner", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn add_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<AddMember>,
) -> ApiResult<Json<Group>> {
    let role = c.role.unwrap_or_else(|| "member".into());
    if role != "member" && role != "owner" {
        return Err(bad("role must be member or owner"));
    }
    let mut tx = state.db.write.begin().await?;
    let g = load_managed(&mut tx, &user, &id).await?;
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE id = ? AND deleted_at IS NULL")
            .bind(&c.user_id)
            .fetch_optional(&mut *tx)
            .await?;
    exists.ok_or_else(|| bad("unknown user"))?;
    if role == "member"
        && g.members
            .iter()
            .filter(|m| m.role == "owner")
            .all(|m| m.user_id == c.user_id)
        && g.members.iter().any(|m| m.user_id == c.user_id)
    {
        return Err(bad("a group needs at least one owner"));
    }
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id, role, created_at) VALUES (?,?,?,?)
         ON CONFLICT(group_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(&id)
    .bind(&c.user_id)
    .bind(&role)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    bump(&mut tx, &id).await?;
    let g = load(&mut tx, &id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    state
        .bus
        .publish([Change::group(&g, &[]), Change::membership(&c.user_id)]);
    Ok(Json(g))
}

/// Remove someone from a group (owners/admins), or leave it yourself.
#[utoipa::path(delete, path = "/groups/{id}/members/{user_id}", tag = "groups", summary = "Remove a member, or leave a group", params(("id" = String, Path, description = "ULID"), ("user_id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 403, description = "Not an owner", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, member)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let g = if member == user.id() {
        load(&mut tx, &id)
            .await?
            .filter(|g| g.deleted_at.is_none() && g.members.iter().any(|m| m.user_id == member))
            .ok_or(AppError::NotFound)?
    } else {
        load_managed(&mut tx, &user, &id).await?
    };
    let owners: Vec<&GroupMember> = g.members.iter().filter(|m| m.role == "owner").collect();
    if owners.len() == 1 && owners[0].user_id == member && g.members.len() > 1 {
        return Err(bad("make someone else an owner first"));
    }
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(&id)
        .bind(&member)
        .execute(&mut *tx)
        .await?;
    bump(&mut tx, &id).await?;
    let g = load(&mut tx, &id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    state.bus.publish([
        Change::group(&g, std::slice::from_ref(&member)),
        Change::membership(&member),
    ]);
    Ok(StatusCode::NO_CONTENT)
}
