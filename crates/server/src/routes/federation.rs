//! Sharing projects with people on other Streamline servers (Phase 8, D-69, D-100).
//! Home side: invite links, the people who joined, and the endpoints their servers call
//! (join, feed, live stream, forwarded edits, leave). Replica side: join, list, sync, leave.

use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    AppState,
    auth::AuthUser,
    error::{ApiResult, AppError, bad},
    federation::{self, Feed, Forward, Forwarded, JoinRequest, JoinView, Joined, LinkError},
    util::{now, random_token, sha256_hex},
};

/// Invite links work for a week.
const INVITE_DAYS: i64 = 7;

/// The project, if it can be shared with other servers by this user: top level, shared
/// with a group they own (or they're an admin), and not itself from another server.
async fn shareable(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<(String, String)> {
    let mut conn = state.db.read.acquire().await?;
    let p = crate::routes::projects::load_visible(&mut conn, user, id).await?;
    let replica: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fed_items WHERE item_id = ?)")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
    if replica {
        return Err(bad(
            "this project is shared from another server; invite people there",
        ));
    }
    if p.parent_id.is_some() {
        return Err(bad(
            "share the top-level project; everything in it comes along",
        ));
    }
    let Some(group) = p.owner_group_id.clone() else {
        return Err(bad(
            "share the project with a group first; people from other servers join that group",
        ));
    };
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM group_members WHERE group_id = ? AND user_id = ?")
            .bind(&group)
            .bind(user.id())
            .fetch_optional(&mut *conn)
            .await?;
    if role.as_deref() != Some("owner") && !user.user.is_admin {
        return Err(AppError::Forbidden(
            "only the group's owners can share its projects with other servers".into(),
        ));
    }
    Ok((p.id, group))
}

#[derive(Serialize, ts_rs::TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Invite {
    /// Goes after `/join/` in the link (`https://this-server/join/<token>`). Shown once.
    pub token: String,
    pub expires_at: String,
}

#[utoipa::path(post, path = "/projects/{id}/invites", tag = "federation", summary = "Make a single-use link that lets someone on another Streamline server join this project", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = Invite), (status = 400, description = "Not shareable", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_invite(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Invite>> {
    let (project, _) = shareable(&state, &user, &id).await?;
    let token = random_token();
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(INVITE_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query(
        "INSERT INTO fed_invites (token_hash, project_id, created_by, created_at, expires_at) VALUES (?,?,?,?,?)",
    )
    .bind(sha256_hex(&token))
    .bind(&project)
    .bind(user.id())
    .bind(now())
    .bind(&expires_at)
    .execute(&state.db.write)
    .await?;
    Ok(Json(Invite { token, expires_at }))
}

/// Someone from another server who joined a project.
#[derive(Serialize, sqlx::FromRow, ts_rs::TS, utoipa::ToSchema)]
#[ts(export)]
pub struct RemoteMember {
    /// The link's id (to remove them).
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    /// `name@their-server`.
    pub username: String,
    pub created_at: String,
    /// When their server last checked in.
    pub last_seen_at: Option<String>,
}

#[utoipa::path(get, path = "/projects/{id}/remote-members", tag = "federation", summary = "People from other servers who joined this project", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = Vec<RemoteMember>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn remote_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<RemoteMember>>> {
    crate::routes::projects::load_visible(&mut *state.db.read.acquire().await?, &user, &id).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT l.id, l.remote_user_id AS user_id, u.display_name, u.username, l.created_at, l.last_seen_at
             FROM fed_links l JOIN users u ON u.id = l.remote_user_id
             WHERE l.project_id = ? AND l.revoked_at IS NULL ORDER BY l.created_at",
        )
        .bind(&id)
        .fetch_all(&state.db.read)
        .await?,
    ))
}

#[utoipa::path(delete, path = "/federation/links/{id}", tag = "federation", summary = "Remove someone from another server from a project (their copy goes)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 404, description = "Not found"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let project: String =
        sqlx::query_scalar("SELECT project_id FROM fed_links WHERE id = ? AND revoked_at IS NULL")
            .bind(&id)
            .fetch_optional(&state.db.read)
            .await?
            .ok_or(AppError::NotFound)?;
    shareable(&state, &user, &project).await?;
    federation::revoke(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- called by other Streamline servers -------------------------------------------

/// A collaborator's server redeems an invite (no sign-in: the invite is the credential).
#[utoipa::path(post, path = "/federation/join", tag = "federation", summary = "Server to server: redeem an invite link", responses((status = 200, description = "Link id, secret, project and group"), (status = 400, description = "Invalid or used invite", body = crate::error::Problem)), security(()))]
pub async fn join_here(
    State(state): State<AppState>,
    Json(j): Json<JoinRequest>,
) -> ApiResult<Json<Joined>> {
    Ok(Json(federation::redeem(&state, j).await?))
}

#[derive(Deserialize)]
pub struct Since {
    #[serde(default)]
    since: i64,
}

#[utoipa::path(get, path = "/federation/links/{id}/feed", tag = "federation", summary = "Server to server: the shared project's changes since a revision", params(("id" = String, Path, description = "Link id"), ("since" = Option<i64>, Query, description = "Revision from the last answer")), responses((status = 200, description = "Changed projects, routines and tasks, people, items that left"), (status = 401, description = "Wrong key"), (status = 410, description = "No longer shared")), security(("bearer" = [])))]
pub async fn feed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<Since>,
    headers: HeaderMap,
) -> Result<Json<Feed>, LinkError> {
    let (link, group) = federation::authorize(&state, &id, &headers).await?;
    Ok(Json(
        federation::feed(&state, &link, &group, q.since).await?,
    ))
}

#[utoipa::path(get, path = "/federation/links/{id}/events", tag = "federation", summary = "Server to server: a live stream that says `poke` when the shared project changes", params(("id" = String, Path, description = "Link id")), responses((status = 200, description = "text/event-stream"), (status = 401, description = "Wrong key"), (status = 410, description = "No longer shared")), security(("bearer" = [])))]
pub async fn events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, LinkError> {
    let (link, group) = federation::authorize(&state, &id, &headers).await?;
    let changes = BroadcastStream::new(state.bus.subscribe());
    // Ends when the link is no longer valid (checked each minute) or after an hour.
    let started = std::time::Instant::now();
    let check = {
        let state = state.clone();
        let link_id = link.id.clone();
        move || {
            let state = state.clone();
            let link_id = link_id.clone();
            async move {
                sqlx::query_scalar::<_, bool>(
                    "SELECT revoked_at IS NULL FROM fed_links WHERE id = ?",
                )
                .bind(&link_id)
                .fetch_optional(&state.db.read)
                .await
                .ok()
                .flatten()
                .unwrap_or(false)
            }
        }
    };
    let ticks =
        tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(Duration::from_secs(60)))
            .skip(1)
            .then(move |_| {
                let check = check.clone();
                async move { Err::<std::sync::Arc<crate::events::Change>, bool>(check().await) }
            });
    let remote = link.remote_user_id.clone();
    let stream = futures_util::stream::select(changes.map(|c| c.map_err(|_| true)), ticks)
        .take_while(move |m| {
            std::future::ready(
                started.elapsed() < Duration::from_secs(3600) && !matches!(m, Err(false)),
            )
        })
        .filter_map(move |m| {
            let out = match m {
                Ok(c) if federation::concerns(&c, &group, &remote) => {
                    Some(Ok(Event::default().event("poke").data("poke")))
                }
                // Missed some changes: poke to be safe.
                Err(true) => Some(Ok(Event::default().event("poke").data("poke"))),
                _ => None,
            };
            std::future::ready(out)
        });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[utoipa::path(post, path = "/federation/links/{id}/forward", tag = "federation", summary = "Server to server: run an edit of the shared project as the collaborator", params(("id" = String, Path, description = "Link id")), responses((status = 200, description = "The edit's own status and answer"), (status = 403, description = "Outside the shared project"), (status = 401, description = "Wrong key"), (status = 410, description = "No longer shared")), security(("bearer" = [])))]
pub async fn forward(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(f): Json<Forward>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let link = match federation::authorize(&state, &id, &headers).await {
        Ok((link, _)) => link,
        Err(e) => return e.into_response(),
    };
    match federation::run_forwarded(&state, &link, f).await {
        Ok(out) => Json::<Forwarded>(out).into_response(),
        Err(e) => e.into_response(),
    }
}

#[utoipa::path(delete, path = "/federation/links/{id}/self", tag = "federation", summary = "Server to server: the collaborator leaves the project", params(("id" = String, Path, description = "Link id")), responses((status = 204, description = "Done"), (status = 401, description = "Wrong key")), security(("bearer" = [])))]
pub async fn leave_here(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, LinkError> {
    match federation::authorize(&state, &id, &headers).await {
        Ok(_) | Err(LinkError::Gone) => {}
        Err(e) => return Err(e),
    }
    federation::revoke(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- this server's people joining projects elsewhere ------------------------------

#[utoipa::path(get, path = "/federation/joins", tag = "federation", summary = "Projects you joined on other servers", responses((status = 200, body = Vec<JoinView>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list_joins(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<JoinView>>> {
    Ok(Json(federation::joins_of(&state, user.id()).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct NewJoin {
    /// The invite link (`https://their-server/join/…`).
    invite: String,
    /// This server's address as you use it (shown to the others with your name).
    origin: String,
}

#[utoipa::path(post, path = "/federation/joins", tag = "federation", summary = "Join a project on another server with its invite link", request_body = NewJoin, responses((status = 200, body = JoinView), (status = 400, description = "The invite didn't work", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_join(
    State(state): State<AppState>,
    user: AuthUser,
    Json(n): Json<NewJoin>,
) -> ApiResult<Json<JoinView>> {
    if n.invite.len() > 2000 || n.origin.len() > 500 {
        return Err(bad("too long"));
    }
    let j = federation::join(&state, &user, &n.invite, &n.origin).await?;
    Ok(Json(JoinView::from(&j)))
}

async fn my_join(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<federation::Join> {
    federation::load_join(state, id)
        .await?
        .filter(|j| j.user_id == user.id())
        .ok_or(AppError::NotFound)
}

#[utoipa::path(post, path = "/federation/joins/{id}/sync", tag = "federation", summary = "Bring a joined project up to date now", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = JoinView), (status = 404, description = "Not found"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn sync_join(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<JoinView>> {
    let j = my_join(&state, &user, &id).await?;
    let _ = federation::pull(&state, &j.id).await;
    let j = federation::load_join(&state, &id)
        .await?
        .ok_or_else(|| AppError::Forbidden("this project is no longer shared with you".into()))?;
    Ok(Json(JoinView::from(&j)))
}

#[utoipa::path(delete, path = "/federation/joins/{id}", tag = "federation", summary = "Leave a project joined on another server (its copy here goes)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 404, description = "Not found"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_join(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let j = my_join(&state, &user, &id).await?;
    federation::leave(&state, &j).await?;
    Ok(StatusCode::NO_CONTENT)
}
