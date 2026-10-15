//! Sharing projects between Streamline servers (Phase 8, D-69, D-100).
//!
//! A top-level group project has a **home server**, which stays authoritative. Someone on
//! another server joins with a single-use invite link: their server (the **replica**)
//! redeems it and gets a per-link secret. The home server adds them to the project's group
//! as a login-less person with the same id as on their own server. The replica then keeps a
//! copy of the project (subprojects, tasks, routines) by pulling a project-scoped feed,
//! woken by a live stream, and mirrors the group so its own people see the copy. Edits its
//! people make to the copy are forwarded to the home server, which runs them as that
//! person with the usual checks, limited to the project; the result comes back through the
//! feed. While the home server can't be reached the copy is read-only. Only the replica
//! connects (outward); personal data (day plans, journal, tracking, calendars) never moves.

use std::{collections::HashSet, sync::LazyLock, time::Duration};

use anyhow::{Context, bail};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqliteConnection;
use tower::ServiceExt;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::AppError,
    events::Change,
    models::{Project, Series, Task, upsert_project, upsert_series, upsert_task},
    util::{new_id, now, sha256_hex},
};

/// Rows per feed page.
const PAGE: i64 = 1000;
/// Feed answers can be large on the first pull.
const MAX_FEED: usize = 32 << 20;
/// How often a replica pulls even without a poke.
const PULL_EVERY: Duration = Duration::from_secs(5 * 60);
/// Fields that name things which stay on each server (places, calendar events,
/// workflow templates, day plans) or would change who owns the project; dropped from
/// forwarded edits.
const LOCAL_FIELDS: [&str; 8] = [
    "day",
    "day_entry_id",
    "place_id",
    "default_place_id",
    "event_id",
    "workflow_template_id",
    "workflow_variant_ids",
    "owner_group_id",
];

/// The project and everything under it, deleted ones included (they travel as deletions).
/// Binds the root project as `?1`.
const SCOPE_CTE: &str = "WITH RECURSIVE scope(id) AS (SELECT ?1 UNION SELECT p.id FROM projects p JOIN scope s ON p.parent_id = s.id)";

/// SQL condition (tasks aliased `t`): not a replica of a project shared from elsewhere.
/// Jobs that only the home server may run on shared items use it.
pub const NOT_REPLICA_SQL: &str = "t.id NOT IN (SELECT item_id FROM fed_items)";

pub fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| match u.port() {
                Some(p) => format!("{h}:{p}"),
                None => h.to_string(),
            })
        })
        .unwrap_or_else(|| url.to_string())
}

// =====================================================================================
// Home server
// =====================================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Link {
    pub id: String,
    pub project_id: String,
    pub remote_user_id: String,
    pub secret_hash: String,
    pub created_by: Option<String>,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// Why a replica's request was refused at the link level (the replica tells them apart).
pub enum LinkError {
    /// Wrong or unknown credentials.
    Unauthorized,
    /// The link was revoked, or its person left the group: the replica should go.
    Gone,
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for LinkError {
    fn from(e: E) -> Self {
        LinkError::Internal(e.into())
    }
}

impl IntoResponse for LinkError {
    fn into_response(self) -> Response {
        match self {
            LinkError::Unauthorized => AppError::Unauthorized.into_response(),
            LinkError::Gone => (
                StatusCode::GONE,
                axum::Json(
                    json!({"type": "about:blank", "title": "Gone", "status": 410,
                    "detail": "this project is no longer shared with you"}),
                ),
            )
                .into_response(),
            LinkError::Internal(e) => AppError::Internal(e).into_response(),
        }
    }
}

/// The link a replica's request is for, checked against its secret and still valid:
/// not revoked, the project still a group project, its person still in that group.
pub async fn authorize(
    state: &AppState,
    link_id: &str,
    headers: &axum::http::HeaderMap,
) -> Result<(Link, String), LinkError> {
    let secret = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(LinkError::Unauthorized)?;
    let link: Link = sqlx::query_as("SELECT * FROM fed_links WHERE id = ?")
        .bind(link_id)
        .fetch_optional(&state.db.read)
        .await?
        .ok_or(LinkError::Unauthorized)?;
    if sha256_hex(secret.trim()) != link.secret_hash {
        return Err(LinkError::Unauthorized);
    }
    if link.revoked_at.is_some() {
        return Err(LinkError::Gone);
    }
    let group: Option<String> = sqlx::query_scalar(
        "SELECT p.owner_group_id FROM projects p JOIN group_members m ON m.group_id = p.owner_group_id AND m.user_id = ?2
         WHERE p.id = ?1 AND p.deleted_at IS NULL AND p.parent_id IS NULL",
    )
    .bind(&link.project_id)
    .bind(&link.remote_user_id)
    .fetch_optional(&state.db.read)
    .await?
    .flatten();
    let Some(group) = group else {
        revoke(state, &link.id).await?;
        return Err(LinkError::Gone);
    };
    let _ = sqlx::query("UPDATE fed_links SET last_seen_at = ? WHERE id = ?")
        .bind(now())
        .bind(&link.id)
        .execute(&state.db.write)
        .await;
    Ok((link, group))
}

/// End a link: its person leaves the project's group unless another link of theirs still
/// needs it. Their replica learns on its next request (410).
pub async fn revoke(state: &AppState, link_id: &str) -> anyhow::Result<()> {
    let mut tx = state.db.write.begin().await?;
    let Some(link) =
        sqlx::query_as::<_, Link>("SELECT * FROM fed_links WHERE id = ? AND revoked_at IS NULL")
            .bind(link_id)
            .fetch_optional(&mut *tx)
            .await?
    else {
        return Ok(());
    };
    sqlx::query("UPDATE fed_links SET revoked_at = ? WHERE id = ?")
        .bind(now())
        .bind(&link.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM fed_sent WHERE link_id = ?")
        .bind(&link.id)
        .execute(&mut *tx)
        .await?;
    let group: Option<String> =
        sqlx::query_scalar("SELECT owner_group_id FROM projects WHERE id = ?")
            .bind(&link.project_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    let mut changes = vec![];
    if let Some(g) = group {
        let still: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fed_links l JOIN projects p ON p.id = l.project_id
             WHERE l.remote_user_id = ?1 AND l.revoked_at IS NULL AND p.owner_group_id = ?2)",
        )
        .bind(&link.remote_user_id)
        .bind(&g)
        .fetch_one(&mut *tx)
        .await?;
        if !still {
            sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
                .bind(&g)
                .bind(&link.remote_user_id)
                .execute(&mut *tx)
                .await?;
            bump_group(&mut tx, &g).await?;
            if let Some(gr) = crate::routes::groups::load(&mut tx, &g).await? {
                changes.push(Change::group(&gr, &[]));
            }
        }
    }
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(())
}

async fn bump_group(conn: &mut SqliteConnection, id: &str) -> sqlx::Result<()> {
    let rev = next_rev(conn).await?;
    sqlx::query("UPDATE groups SET updated_at = ?, rev = ? WHERE id = ?")
        .bind(now())
        .bind(rev)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Who the replica's person is, as their server says.
#[derive(Debug, Serialize, Deserialize)]
pub struct Visitor {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub token: String,
    pub user: Visitor,
    /// The joining server's address (as its person uses it).
    pub origin: String,
    /// Someone who already joined a project here proves it's them again: one of their
    /// current links and its secret.
    #[serde(default)]
    pub proof: Option<Proof>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Proof {
    pub link_id: String,
    pub secret: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Joined {
    pub link_id: String,
    pub secret: String,
    pub root_project_id: String,
    pub group_id: String,
    /// The project's name, for the joiner's confirmation.
    pub name: String,
}

/// Redeem an invite (home side): the visitor becomes a member of the project's group.
pub async fn redeem(state: &AppState, j: JoinRequest) -> Result<Joined, AppError> {
    let bad = |m: &str| AppError::BadRequest(m.into());
    ulid::Ulid::from_string(&j.user.id).map_err(|_| bad("bad user id"))?;
    let username = j.user.username.trim();
    let display = j.user.display_name.trim();
    if username.is_empty() || username.len() > 100 || display.len() > 200 {
        return Err(bad("bad user"));
    }
    let origin = Url::parse(j.origin.trim()).map_err(|_| bad("bad origin"))?;
    if !matches!(origin.scheme(), "http" | "https") || origin.host_str().is_none() {
        return Err(bad("bad origin"));
    }
    let origin = origin.origin().ascii_serialization();
    let hash = sha256_hex(j.token.trim());
    // Unknown links are turned away without touching the writer.
    let known: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fed_invites WHERE token_hash = ? AND used_at IS NULL)",
    )
    .bind(&hash)
    .fetch_one(&state.db.read)
    .await?;
    if !known {
        return Err(bad(
            "this invite link isn't valid (already used or withdrawn?)",
        ));
    }

    let mut tx = state.db.write.begin().await?;
    let invite: Option<(String, String, String)> = sqlx::query_as(
        "SELECT project_id, expires_at, created_by FROM fed_invites WHERE token_hash = ? AND used_at IS NULL",
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((project_id, expires_at, created_by)) = invite else {
        return Err(bad(
            "this invite link isn't valid (already used or withdrawn?)",
        ));
    };
    if expires_at < now() {
        return Err(bad("this invite link has expired; ask for a new one"));
    }
    let project: Project =
        sqlx::query_as("SELECT * FROM projects WHERE id = ? AND deleted_at IS NULL")
            .bind(&project_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| bad("the project no longer exists"))?;
    let group = project
        .owner_group_id
        .clone()
        .filter(|_| project.parent_id.is_none())
        .ok_or_else(|| bad("the project is no longer shared with a group"))?;
    // The same person on this server, or a clash of ids: refuse.
    let existing: Option<Option<String>> =
        sqlx::query_scalar("SELECT home FROM users WHERE id = ?")
            .bind(&j.user.id)
            .fetch_optional(&mut *tx)
            .await?;
    match existing {
        Some(None) => {
            return Err(bad(
                "you're already on this server; the project can be shared with you here",
            ));
        }
        // Someone from elsewhere seen before: only their own server may use the id.
        Some(Some(home)) if home != origin => {
            return Err(bad(
                "this person's id belongs to someone from another server",
            ));
        }
        Some(Some(_)) => {
            // Someone with current links here must show one of them (else any server could
            // claim their id and address).
            let links: Vec<(String, String)> = sqlx::query_as(
                "SELECT id, secret_hash FROM fed_links WHERE remote_user_id = ? AND revoked_at IS NULL",
            )
            .bind(&j.user.id)
            .fetch_all(&mut *tx)
            .await?;
            let proven = j.proof.as_ref().is_some_and(|p| {
                links
                    .iter()
                    .any(|(id, h)| *id == p.link_id && *h == sha256_hex(p.secret.trim()))
            });
            if !links.is_empty() && !proven {
                return Err(bad("this person's id is already in use here"));
            }
            sqlx::query("UPDATE users SET display_name = ?, updated_at = ? WHERE id = ?")
                .bind(display)
                .bind(now())
                .bind(&j.user.id)
                .execute(&mut *tx)
                .await?;
        }
        None => {
            let handle =
                unique_username(&mut tx, &format!("{username}@{}", host_of(&origin))).await?;
            let rev = next_rev(&mut tx).await?;
            let ts = now();
            sqlx::query(
                "INSERT INTO users (id, username, display_name, password_hash, is_admin, created_at, updated_at, rev, home)
                 VALUES (?, ?, ?, '', 0, ?, ?, ?, ?)",
            )
            .bind(&j.user.id)
            .bind(&handle)
            .bind(if display.is_empty() { username } else { display })
            .bind(&ts)
            .bind(&ts)
            .bind(rev)
            .bind(&origin)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("UPDATE fed_invites SET used_at = ? WHERE token_hash = ?")
        .bind(now())
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    // Joining again replaces the earlier link.
    let old: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM fed_links WHERE project_id = ? AND remote_user_id = ? AND revoked_at IS NULL",
    )
    .bind(&project_id)
    .bind(&j.user.id)
    .fetch_all(&mut *tx)
    .await?;
    for id in &old {
        sqlx::query("UPDATE fed_links SET revoked_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM fed_sent WHERE link_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id, role, created_at) VALUES (?, ?, 'member', ?)
         ON CONFLICT DO NOTHING",
    )
    .bind(&group)
    .bind(&j.user.id)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    bump_group(&mut tx, &group).await?;
    let secret = format!("sls_{}", crate::util::random_token());
    let link_id = new_id();
    sqlx::query(
        "INSERT INTO fed_links (id, project_id, remote_user_id, secret_hash, created_by, created_at) VALUES (?,?,?,?,?,?)",
    )
    .bind(&link_id)
    .bind(&project_id)
    .bind(&j.user.id)
    .bind(sha256_hex(&secret))
    .bind(&created_by)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    let g = crate::routes::groups::load(&mut tx, &group).await?;
    tx.commit().await?;
    state
        .bus
        .publish(g.iter().map(|g| Change::group(g, &[])).collect::<Vec<_>>());
    Ok(Joined {
        link_id,
        secret,
        root_project_id: project_id,
        group_id: group,
        name: project.name,
    })
}

async fn unique_username(conn: &mut SqliteConnection, want: &str) -> sqlx::Result<String> {
    let mut name = want.to_string();
    for n in 2.. {
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)")
                .bind(&name)
                .fetch_one(&mut *conn)
                .await?;
        if !taken {
            break;
        }
        name = format!("{want}~{n}");
    }
    Ok(name)
}

/// Someone in the feed: a member of the project's group, or named by one of its items.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Person {
    pub id: String,
    pub username: String,
    pub display_name: String,
    /// Their server, if not the home server.
    pub home: Option<String>,
    /// `owner` or `member` for group members; `None` for others.
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Feed {
    /// Pass back as `since` next time.
    pub rev: i64,
    /// More changes are waiting: pull again right away.
    pub more: bool,
    pub root_project_id: String,
    pub group_id: String,
    pub group_name: String,
    pub people: Vec<Person>,
    pub projects: Vec<Project>,
    pub series: Vec<Series>,
    pub tasks: Vec<Task>,
    /// Items that left the project (moved elsewhere): `[kind, id]`.
    pub gone: Vec<(String, String)>,
}

/// The changes to a link's project since `since` (home side).
pub async fn feed(state: &AppState, link: &Link, group: &str, since: i64) -> anyhow::Result<Feed> {
    let mut rx = state.db.read.begin().await?;
    let root = &link.project_id;
    let cutoff: Option<i64> = sqlx::query_scalar(&format!(
        "{SCOPE_CTE} SELECT rev FROM (
            SELECT rev FROM projects WHERE id IN scope AND rev > ?2
            UNION ALL SELECT rev FROM tasks WHERE project_id IN scope AND rev > ?2
            UNION ALL SELECT rev FROM series WHERE project_id IN scope AND rev > ?2)
         ORDER BY rev LIMIT 1 OFFSET ?3"
    ))
    .bind(root)
    .bind(since)
    .bind(PAGE - 1)
    .fetch_optional(&mut *rx)
    .await?;
    let (upto, more) = match cutoff {
        Some(c) => (c, true),
        None => (
            sqlx::query_scalar("SELECT rev FROM meta_rev WHERE id = 1")
                .fetch_one(&mut *rx)
                .await?,
            false,
        ),
    };
    let rows = |table: &str, col: &str| {
        format!(
            "{SCOPE_CTE} SELECT * FROM {table} WHERE {col} IN scope AND rev > ?2 AND rev <= ?3 ORDER BY rev"
        )
    };
    let projects: Vec<Project> = sqlx::query_as(&rows("projects", "id"))
        .bind(root)
        .bind(since)
        .bind(upto)
        .fetch_all(&mut *rx)
        .await?;
    let series: Vec<Series> = sqlx::query_as(&rows("series", "project_id"))
        .bind(root)
        .bind(since)
        .bind(upto)
        .fetch_all(&mut *rx)
        .await?;
    let tasks: Vec<Task> = sqlx::query_as(&rows("tasks", "project_id"))
        .bind(root)
        .bind(since)
        .bind(upto)
        .fetch_all(&mut *rx)
        .await?;
    let gone: Vec<(String, String)> = sqlx::query_as(&format!(
        "{SCOPE_CTE} SELECT kind, item_id FROM fed_sent WHERE link_id = ?2 AND NOT (
            (kind = 'project' AND item_id IN scope)
            OR (kind = 'task' AND item_id IN (SELECT id FROM tasks WHERE project_id IN scope))
            OR (kind = 'series' AND item_id IN (SELECT id FROM series WHERE project_id IN scope)))"
    ))
    .bind(root)
    .bind(&link.id)
    .fetch_all(&mut *rx)
    .await?;
    let mut people: Vec<Person> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name, u.home, m.role FROM group_members m JOIN users u ON u.id = m.user_id
         WHERE m.group_id = ? AND u.deleted_at IS NULL",
    )
    .bind(group)
    .fetch_all(&mut *rx)
    .await?;
    let group_name: String = sqlx::query_scalar("SELECT name FROM groups WHERE id = ?")
        .bind(group)
        .fetch_one(&mut *rx)
        .await?;
    // Everyone else the items name (e.g. someone who completed a task and left since).
    let known: HashSet<String> = people.iter().map(|p| p.id.clone()).collect();
    let mut named: HashSet<String> = HashSet::new();
    for t in &tasks {
        named.extend(
            [
                &t.owner_user_id,
                &t.assignee_user_id,
                &t.completed_by,
                &t.waiting_by,
            ]
            .into_iter()
            .flatten()
            .cloned(),
        );
    }
    named.extend(projects.iter().filter_map(|p| p.owner_user_id.clone()));
    named.extend(series.iter().filter_map(|s| s.owner_user_id.clone()));
    for id in named.difference(&known) {
        if let Some(p) = sqlx::query_as::<_, Person>(
            "SELECT id, username, display_name, home, NULL AS role FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *rx)
        .await?
        {
            people.push(p);
        }
    }
    drop(rx);

    let mut tx = state.db.write.begin().await?;
    for (kind, id) in &gone {
        sqlx::query("DELETE FROM fed_sent WHERE link_id = ? AND kind = ? AND item_id = ?")
            .bind(&link.id)
            .bind(kind)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let sent = projects
        .iter()
        .map(|p| ("project", &p.id))
        .chain(series.iter().map(|s| ("series", &s.id)))
        .chain(tasks.iter().map(|t| ("task", &t.id)));
    for (kind, id) in sent {
        sqlx::query("INSERT OR IGNORE INTO fed_sent (link_id, kind, item_id) VALUES (?, ?, ?)")
            .bind(&link.id)
            .bind(kind)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Feed {
        rev: upto,
        more,
        root_project_id: root.clone(),
        group_id: group.into(),
        group_name,
        people,
        projects,
        series,
        tasks,
        gone,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Forward {
    pub method: String,
    /// The API path, e.g. `/tasks/01J…` (no `/api/v1`, no query).
    pub path: String,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Forwarded {
    pub status: u16,
    pub body: Value,
}

async fn in_scope(
    conn: &mut SqliteConnection,
    root: &str,
    kind: &str,
    id: &str,
) -> sqlx::Result<bool> {
    let sql = match kind {
        "project" => format!(
            "{SCOPE_CTE} SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?2 AND id IN scope AND deleted_at IS NULL)"
        ),
        "task" => format!(
            "{SCOPE_CTE} SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?2 AND project_id IN scope AND deleted_at IS NULL)"
        ),
        _ => format!(
            "{SCOPE_CTE} SELECT EXISTS(SELECT 1 FROM series WHERE id = ?2 AND project_id IN scope AND deleted_at IS NULL)"
        ),
    };
    sqlx::query_scalar(&sql)
        .bind(root)
        .bind(id)
        .fetch_one(conn)
        .await
}

/// Which item a write is about: `(kind, id or None for a create)`.
fn route_of(method: &Method, path: &str) -> Option<(&'static str, Option<String>)> {
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    let kind = match segs.first().copied()? {
        "tasks" => "task",
        "projects" => "project",
        "series" => "series",
        _ => return None,
    };
    match (method.as_str(), segs.len()) {
        ("POST", 1) => Some((kind, None)),
        ("PATCH" | "DELETE", 2) if ulid::Ulid::from_string(segs[1]).is_ok() => {
            Some((kind, Some(segs[1].to_string())))
        }
        _ => None,
    }
}

/// The project a create or move puts the item into (`Some(None)`: out of any project).
fn target_project(kind: &str, body: &Value) -> Option<Option<String>> {
    let field = if kind == "project" {
        "parent_id"
    } else {
        "project_id"
    };
    body.get(field).map(|v| v.as_str().map(String::from))
}

/// Run an edit a replica forwarded, as its person, if it stays inside the project.
pub async fn run_forwarded(
    state: &AppState,
    link: &Link,
    f: Forward,
) -> Result<Forwarded, AppError> {
    let refuse = |m: &str| AppError::Forbidden(m.into());
    let method: Method = f.method.parse().map_err(|_| refuse("not allowed"))?;
    let (kind, id) = route_of(&method, &f.path).ok_or_else(|| refuse("not allowed"))?;
    let mut body = f.body;
    if let Some(o) = body.as_object_mut() {
        for k in LOCAL_FIELDS {
            o.remove(k);
        }
    }
    let root = link.project_id.as_str();
    let mut conn = state.db.read.acquire().await?;
    let outside = "that's outside the shared project";
    if let Some(id) = &id {
        if !in_scope(&mut conn, root, kind, id).await? {
            return Err(AppError::NotFound);
        }
        if kind == "project"
            && id == root
            && (method == Method::DELETE || body.get("parent_id").is_some())
        {
            return Err(refuse(
                "the shared project itself can only be deleted or moved on its own server",
            ));
        }
    }
    // A create naming an id that already exists would answer with that item (an offline
    // replay): only when it's part of the shared project.
    if id.is_none()
        && let Some(given) = body.get("id").and_then(Value::as_str)
    {
        let table = match kind {
            "task" => "tasks",
            "project" => "projects",
            _ => "series",
        };
        let exists: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?)"
        ))
        .bind(given)
        .fetch_one(&mut *conn)
        .await?;
        if exists && !in_scope(&mut conn, root, kind, given).await? {
            return Err(refuse(outside));
        }
    }
    match target_project(kind, &body) {
        Some(Some(p)) if in_scope(&mut conn, root, "project", &p).await? => {}
        Some(_) => return Err(refuse(outside)),
        None if id.is_none() => return Err(refuse(outside)),
        None => {}
    }
    for (field, k) in [("also_project_ids", "project"), ("depends_on", "task")] {
        if let Some(list) = body.get(field).and_then(Value::as_array) {
            for v in list {
                if !in_scope(&mut conn, root, k, v.as_str().unwrap_or_default()).await? {
                    return Err(refuse(outside));
                }
            }
        }
    }
    drop(conn);

    let mut req = Request::builder()
        .method(method.clone())
        .uri(&f.path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(if method == Method::DELETE {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .map_err(anyhow::Error::from)?;
    req.extensions_mut()
        .insert(crate::auth::ActingUser(link.remote_user_id.clone()));
    let api = state.api.get().context("the API isn't ready")?.clone();
    let res = api.oneshot(req).await.map_err(anyhow::Error::from)?;
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Forwarded {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    })
}

// =====================================================================================
// Replica server
// =====================================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Join {
    pub id: String,
    pub user_id: String,
    pub home_url: String,
    pub root_project_id: String,
    pub group_id: String,
    pub secret: Vec<u8>,
    pub since_rev: i64,
    pub status: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
}

/// What the API shows of a project joined from another server.
#[derive(Debug, Clone, Serialize, ts_rs::TS, utoipa::ToSchema)]
#[ts(export)]
pub struct JoinView {
    pub id: String,
    /// The home server's address.
    pub home_url: String,
    pub root_project_id: String,
    /// `new`, `ok` or `error` (the home server can't be reached: read-only).
    pub status: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
}

impl From<&Join> for JoinView {
    fn from(j: &Join) -> Self {
        Self {
            id: j.id.clone(),
            home_url: j.home_url.clone(),
            root_project_id: j.root_project_id.clone(),
            status: j.status.clone(),
            last_sync_at: j.last_sync_at.clone(),
            last_error: j.last_error.clone(),
        }
    }
}

impl Change {
    pub fn join(user_id: &str, j: &JoinView) -> Self {
        Self {
            rev: 0,
            kind: "remote_project",
            data: serde_json::to_value(j).unwrap(),
            audience: vec![user_id.to_string()],
        }
    }
}

fn client(timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
    crate::net::client(timeout_secs, false)
}

/// Where an invite link points: the home server's address and the token.
pub fn parse_invite(link: &str) -> anyhow::Result<(String, String)> {
    let link = link.trim();
    let (home, token) = link
        .rsplit_once("/join/")
        .context("that doesn't look like an invite link (…/join/…)")?;
    let token = token.trim_end_matches('/');
    if token.is_empty() || token.contains(['/', '?', '#']) {
        bail!("that doesn't look like an invite link");
    }
    let url = Url::parse(home).context("that doesn't look like an invite link")?;
    crate::net::check_url(&url)?;
    Ok((home.trim_end_matches('/').to_string(), token.to_string()))
}

/// Redeem an invite on its home server and start following the project (replica side).
pub async fn join(
    state: &AppState,
    user: &AuthUser,
    invite: &str,
    origin: &str,
) -> Result<Join, AppError> {
    let bad = |m: String| AppError::BadRequest(m);
    let (home, token) = parse_invite(invite).map_err(|e| bad(format!("{e:#}")))?;
    // Already joined something on that server: prove it's the same person.
    let earlier: Option<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT id, secret FROM fed_joins WHERE user_id = ? AND home_url = ? ORDER BY created_at LIMIT 1",
    )
    .bind(user.id())
    .bind(&home)
    .fetch_optional(&state.db.read)
    .await?;
    let proof = earlier.and_then(|(link_id, sealed)| {
        state
            .secrets
            .decrypt(&sealed)
            .ok()
            .map(|secret| Proof { link_id, secret })
    });
    let body = json!({
        "proof": proof,
        "token": token,
        "user": Visitor {
            id: user.id().into(),
            username: user.user.username.clone(),
            display_name: user.user.display_name.clone(),
        },
        "origin": origin,
    });
    let res = client(30)
        .map_err(AppError::Internal)?
        .post(format!("{home}/api/v1/federation/join"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| bad(format!("can't reach {}: {e}", host_of(&home))))?;
    let status = res.status();
    let text = crate::net::text_limited(res, 1 << 20)
        .await
        .map_err(|e| bad(format!("{e:#}")))?;
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["detail"].as_str().map(String::from))
            .unwrap_or_else(|| format!("{} answered {status}", host_of(&home)));
        return Err(bad(detail));
    }
    let joined: Joined = serde_json::from_str(&text).map_err(|_| {
        bad(format!(
            "{} isn't a Streamline server that can share projects",
            host_of(&home)
        ))
    })?;
    for id in [&joined.link_id, &joined.root_project_id, &joined.group_id] {
        ulid::Ulid::from_string(id).map_err(|_| bad("unexpected answer".into()))?;
    }
    // Joining a project of this very server (or one already here) makes no sense.
    let local: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1 AND deleted_at IS NULL AND id NOT IN (SELECT item_id FROM fed_items))
           OR EXISTS(SELECT 1 FROM groups WHERE id = ?2 AND home IS NULL)",
    )
    .bind(&joined.root_project_id)
    .bind(&joined.group_id)
    .fetch_one(&state.db.read)
    .await?;
    let mine: Option<String> =
        sqlx::query_scalar("SELECT id FROM fed_joins WHERE user_id = ? AND root_project_id = ?")
            .bind(user.id())
            .bind(&joined.root_project_id)
            .fetch_optional(&state.db.read)
            .await?;
    // Ids another server already uses here (a server can't take over someone else's copy
    // by naming the same project or group).
    let clash: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fed_joins WHERE id = ?3 OR ((root_project_id = ?1 OR group_id = ?2) AND home_url <> ?4))
           OR EXISTS(SELECT 1 FROM groups WHERE id = ?2 AND home IS NOT NULL AND home <> ?4)",
    )
    .bind(&joined.root_project_id)
    .bind(&joined.group_id)
    .bind(&joined.link_id)
    .bind(&home)
    .fetch_one(&state.db.read)
    .await?;
    let refused = if local || clash {
        Some("that project is on this server already")
    } else if mine.is_some() {
        Some("you've already joined that project")
    } else {
        None
    };
    if let Some(m) = refused {
        let _ = leave_home(&home, &joined.link_id, &joined.secret).await;
        return Err(bad(m.into()));
    }
    sqlx::query(
        "INSERT INTO fed_joins (id, user_id, home_url, root_project_id, group_id, secret, created_at) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&joined.link_id)
    .bind(user.id())
    .bind(&home)
    .bind(&joined.root_project_id)
    .bind(&joined.group_id)
    .bind(state.secrets.encrypt(&joined.secret))
    .bind(now())
    .execute(&state.db.write)
    .await?;
    let _ = pull(state, &joined.link_id).await;
    Ok(load_join(state, &joined.link_id)
        .await?
        .context("join vanished")?)
}

pub async fn load_join(state: &AppState, id: &str) -> sqlx::Result<Option<Join>> {
    sqlx::query_as("SELECT * FROM fed_joins WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db.read)
        .await
}

async fn leave_home(home: &str, link_id: &str, secret: &str) -> anyhow::Result<()> {
    client(15)?
        .delete(format!("{home}/api/v1/federation/links/{link_id}/self"))
        .bearer_auth(secret)
        .send()
        .await?;
    Ok(())
}

/// Leave a joined project: tell its home server (if it answers), drop the copy.
pub async fn leave(state: &AppState, j: &Join) -> anyhow::Result<()> {
    if let Ok(secret) = state.secrets.decrypt(&j.secret) {
        let _ = leave_home(&j.home_url, &j.id, &secret).await;
    }
    remove_replica(state, j).await
}

/// Pulls of one joined project run one at a time (each join its own lock, so one slow
/// home server doesn't hold up the others).
static PULL: LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
> = LazyLock::new(Default::default);
/// A pull gives up after this long (a home server that dribbles out pages).
const PULL_DEADLINE: Duration = Duration::from_secs(10 * 60);

/// Bring a joined project up to date, and record how it went.
pub async fn pull(state: &AppState, join_id: &str) -> anyhow::Result<()> {
    let lock = PULL
        .lock()
        .unwrap()
        .entry(join_id.to_string())
        .or_default()
        .clone();
    let _guard = lock.lock().await;
    let Some(j) = load_join(state, join_id).await? else {
        return Ok(());
    };
    let result = match tokio::time::timeout(PULL_DEADLINE, pull_pages(state, &j)).await {
        Ok(r) => r,
        Err(_) => Err(anyhow::anyhow!(
            "{} is too slow to answer",
            host_of(&j.home_url)
        )),
    };
    let (status, error) = match &result {
        Ok(Pulled::Done) => ("ok", None),
        Ok(Pulled::Gone) => return remove_replica(state, &j).await,
        Err(e) => ("error", Some(format!("{e:#}"))),
    };
    sqlx::query("UPDATE fed_joins SET status = ?, last_error = ?, last_sync_at = CASE WHEN ? = 'ok' THEN ? ELSE last_sync_at END WHERE id = ?")
        .bind(status)
        .bind(&error)
        .bind(status)
        .bind(now())
        .bind(&j.id)
        .execute(&state.db.write)
        .await?;
    if let Some(j2) = load_join(state, &j.id).await?
        && (j2.status != j.status || j2.last_error != j.last_error || status == "ok")
    {
        state
            .bus
            .publish([Change::join(&j2.user_id, &JoinView::from(&j2))]);
    }
    result.map(|_| ())
}

enum Pulled {
    Done,
    Gone,
}

async fn pull_pages(state: &AppState, j: &Join) -> anyhow::Result<Pulled> {
    let secret = state.secrets.decrypt(&j.secret)?;
    let http = client(120)?;
    let mut since = j.since_rev;
    for _ in 0..1000 {
        let res = http
            .get(format!(
                "{}/api/v1/federation/links/{}/feed?since={since}",
                j.home_url, j.id
            ))
            .bearer_auth(&secret)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("can't reach {}", host_of(&j.home_url)))?;
        match res.status() {
            StatusCode::GONE => return Ok(Pulled::Gone),
            StatusCode::UNAUTHORIZED => bail!(
                "{} no longer accepts this server's key",
                host_of(&j.home_url)
            ),
            s if !s.is_success() => bail!("{} answered {s}", host_of(&j.home_url)),
            _ => {}
        }
        let feed: Feed = serde_json::from_str(&crate::net::text_limited(res, MAX_FEED).await?)
            .context("unexpected answer from the home server")?;
        if feed.root_project_id != j.root_project_id || feed.group_id != j.group_id {
            bail!("unexpected answer from the home server");
        }
        // "More" must move forward, or it never ends.
        if feed.more && feed.rev <= since {
            bail!("unexpected answer from the home server");
        }
        apply(state, j, &feed).await?;
        since = feed.rev;
        if !feed.more {
            return Ok(Pulled::Done);
        }
    }
    Ok(Pulled::Done)
}

/// Write a feed page into the copy (replica side).
async fn apply(state: &AppState, j: &Join, feed: &Feed) -> anyhow::Result<()> {
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    let ts = now();
    let home_host = host_of(&j.home_url);

    // People: login-less rows for everyone who isn't from here.
    for p in &feed.people {
        if ulid::Ulid::from_string(&p.id).is_err() {
            continue;
        }
        let here: Option<Option<String>> =
            sqlx::query_scalar("SELECT home FROM users WHERE id = ?")
                .bind(&p.id)
                .fetch_optional(&mut *tx)
                .await?;
        let display: String = p.display_name.chars().take(200).collect();
        match here {
            Some(None) => {} // one of ours
            Some(Some(_)) => {
                // Only names this server introduced (not people from a third one).
                sqlx::query("UPDATE users SET display_name = ? WHERE id = ? AND home = ?")
                    .bind(&display)
                    .bind(&p.id)
                    .bind(&j.home_url)
                    .execute(&mut *tx)
                    .await?;
            }
            None => {
                let handle = match &p.home {
                    None => format!("{}@{home_host}", p.username),
                    Some(_) => p.username.clone(),
                };
                let handle =
                    unique_username(&mut tx, &handle.chars().take(200).collect::<String>()).await?;
                let rev = next_rev(&mut tx).await?;
                sqlx::query(
                    "INSERT INTO users (id, username, display_name, password_hash, is_admin, created_at, updated_at, rev, home)
                     VALUES (?, ?, ?, '', 0, ?, ?, ?, ?)",
                )
                .bind(&p.id)
                .bind(&handle)
                .bind(&display)
                .bind(&ts)
                .bind(&ts)
                .bind(rev)
                .bind(p.home.as_deref().unwrap_or(&j.home_url))
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    // The group, mirrored: same id, its members as the home server has them. (Never one
    // of this server's own groups.)
    let clash: Option<Option<String>> = sqlx::query_scalar("SELECT home FROM groups WHERE id = ?")
        .bind(&feed.group_id)
        .fetch_optional(&mut *tx)
        .await?;
    if clash.is_some_and(|home| home.as_deref() != Some(j.home_url.as_str())) {
        bail!("the shared group clashes with one of this server's, or another server's");
    }
    let before: HashSet<String> =
        sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = ?")
            .bind(&feed.group_id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let rev = next_rev(&mut tx).await?;
    sqlx::query(
        "INSERT INTO groups (id, name, created_by, created_at, updated_at, rev, home) VALUES (?, ?, NULL, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at, rev = excluded.rev, deleted_at = NULL",
    )
    .bind(&feed.group_id)
    .bind(feed.group_name.chars().take(200).collect::<String>())
    .bind(&ts)
    .bind(&ts)
    .bind(rev)
    .bind(&j.home_url)
    .execute(&mut *tx)
    .await?;
    // People of this server are in the mirror only if they joined (the home server can't
    // put anyone else here in it).
    let joined: HashSet<String> =
        sqlx::query_scalar("SELECT user_id FROM fed_joins WHERE group_id = ?")
            .bind(&feed.group_id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let local_ids: HashSet<String> = sqlx::query_scalar("SELECT id FROM users WHERE home IS NULL")
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();
    let members: Vec<&Person> = feed
        .people
        .iter()
        .filter(|p| p.role.is_some() && (!local_ids.contains(&p.id) || joined.contains(&p.id)))
        .collect();
    let after: HashSet<String> = members.iter().map(|p| p.id.clone()).collect();
    sqlx::query("DELETE FROM group_members WHERE group_id = ?")
        .bind(&feed.group_id)
        .execute(&mut *tx)
        .await?;
    for p in &members {
        sqlx::query(
            "INSERT OR IGNORE INTO group_members (group_id, user_id, role, created_at)
             SELECT ?, id, ?, ? FROM users WHERE id = ?",
        )
        .bind(&feed.group_id)
        .bind(if p.role.as_deref() == Some("owner") {
            "owner"
        } else {
            "member"
        })
        .bind(&ts)
        .bind(&p.id)
        .execute(&mut *tx)
        .await?;
    }
    for u in before
        .symmetric_difference(&after)
        .filter(|u| local_ids.contains(*u))
    {
        changes.push(Change::membership(u));
    }
    if let Some(g) = crate::routes::groups::load(&mut tx, &feed.group_id).await? {
        changes.push(Change::group(
            &g,
            &before.difference(&after).cloned().collect::<Vec<_>>(),
        ));
    }

    // Projects, parents first.
    let mut projects: Vec<&Project> = feed.projects.iter().collect();
    let ids: HashSet<&str> = projects.iter().map(|p| p.id.as_str()).collect();
    let mut placed: HashSet<String> = HashSet::new();
    let mut ordered = vec![];
    while !projects.is_empty() {
        let before_len = projects.len();
        projects.retain(|p| {
            let ready = match &p.parent_id {
                Some(parent) => !ids.contains(parent.as_str()) || placed.contains(parent),
                None => true,
            };
            if ready {
                placed.insert(p.id.clone());
                ordered.push(*p);
            }
            !ready
        });
        if projects.len() == before_len {
            ordered.append(&mut projects); // a cycle: let the database decide
        }
    }
    let (root, group) = (j.root_project_id.as_str(), feed.group_id.as_str());
    for p in ordered {
        let mut p = p.clone();
        p.default_place_id = None;
        if p.id == root {
            p.parent_id = None;
        } else {
            // Anything else hangs under a project of this copy.
            let parent = p.parent_id.clone().filter(|parent| *parent != p.id);
            if !held(&mut tx, j, parent.as_deref().unwrap_or_default()).await? {
                continue;
            }
        }
        if !ours(&p.owner_user_id, &p.owner_group_id, group)
            || !may_write(&mut tx, "projects", &p.id, j, group).await?
        {
            continue;
        }
        p.rev = next_rev(&mut tx).await?;
        upsert_project(&mut tx, &p).await?;
        hold(&mut tx, &j.id, "project", &p.id).await?;
        changes.push(Change::project(&p));
    }
    for s in &feed.series {
        let mut s = s.clone();
        s.place_id = None;
        s.workflow_template_id = None;
        s.workflow_variant_ids = sqlx::types::Json(vec![]);
        if let Some(prev) = &s.split_from
            && !row_exists(&mut tx, "series", prev).await?
        {
            s.split_from = None;
        }
        if !row_exists(&mut tx, "task_types", &s.task_type_id).await? {
            s.task_type_id = crate::routes::tasks::DEFAULT_TASK_TYPE.into();
        }
        if !ours(&s.owner_user_id, &s.owner_group_id, group)
            || !held(&mut tx, j, s.project_id.as_deref().unwrap_or_default()).await?
            || !may_write(&mut tx, "series", &s.id, j, group).await?
        {
            continue;
        }
        s.rev = next_rev(&mut tx).await?;
        upsert_series(&mut tx, &s).await?;
        hold(&mut tx, &j.id, "series", &s.id).await?;
        changes.push(Change::series(&s));
    }
    for t in &feed.tasks {
        let mut t = t.clone();
        t.place_id = None;
        t.event_id = None;
        if !row_exists(&mut tx, "task_types", &t.task_type_id).await? {
            t.task_type_id = crate::routes::tasks::DEFAULT_TASK_TYPE.into();
        }
        if let Some(s) = &t.series_id
            && !row_exists(&mut tx, "series", s).await?
        {
            t.series_id = None;
        }
        for who in [
            &mut t.assignee_user_id,
            &mut t.completed_by,
            &mut t.waiting_by,
        ] {
            if let Some(u) = who.as_ref()
                && !row_exists(&mut tx, "users", u).await?
            {
                *who = None;
            }
        }
        if !ours(&t.owner_user_id, &t.owner_group_id, group)
            || !held(&mut tx, j, t.project_id.as_deref().unwrap_or_default()).await?
            || !may_write(&mut tx, "tasks", &t.id, j, group).await?
        {
            continue;
        }
        let was_open: Option<bool> = sqlx::query_scalar(
            "SELECT status = 'open' AND deleted_at IS NULL FROM tasks WHERE id = ?",
        )
        .bind(&t.id)
        .fetch_optional(&mut *tx)
        .await?;
        t.rev = next_rev(&mut tx).await?;
        upsert_task(&mut tx, &t).await?;
        hold(&mut tx, &j.id, "task", &t.id).await?;
        if t.deleted_at.is_some() {
            changes.extend(drop_entries(&mut tx, &t.id).await?);
        }
        changes.push(Change::task(&t));
        // Local tasks waiting for this one move on when it's done.
        if was_open == Some(true) && (t.status != "open" || t.deleted_at.is_some()) {
            crate::deps::refresh_dependents(&mut tx, &t.id, &mut changes).await?;
        }
    }
    for (kind, id) in &feed.gone {
        changes.extend(release(&mut tx, &j.id, kind, id).await?);
    }
    sqlx::query("UPDATE fed_joins SET since_rev = ? WHERE id = ?")
        .bind(feed.rev)
        .bind(&j.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(())
}

async fn row_exists(conn: &mut SqliteConnection, table: &str, id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?)"
    ))
    .bind(id)
    .fetch_one(conn)
    .await
}

/// Replicated rows belong to the mirrored group, never to a person here.
fn ours(user: &Option<String>, group: &Option<String>, mirrored: &str) -> bool {
    user.is_none() && group.as_deref() == Some(mirrored)
}

/// The item is part of this shared project's copy here: held by a join of the same project
/// from the same home server (another server can't claim it by naming the same ids).
async fn held(conn: &mut SqliteConnection, j: &Join, id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fed_items f JOIN fed_joins j ON j.id = f.join_id
         WHERE f.item_id = ? AND j.root_project_id = ? AND j.home_url = ?)",
    )
    .bind(id)
    .bind(&j.root_project_id)
    .bind(&j.home_url)
    .fetch_one(conn)
    .await
}

/// A feed may only write rows that are new here, already part of this copy, or left over
/// from an earlier copy of the same project (deleted, owned by the mirrored group): never
/// anything of this server's own.
async fn may_write(
    conn: &mut SqliteConnection,
    table: &str,
    id: &str,
    j: &Join,
    group: &str,
) -> sqlx::Result<bool> {
    let existing: Option<(Option<String>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT owner_group_id, deleted_at FROM {table} WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(match existing {
        None => true,
        Some((g, deleted)) => {
            held(conn, j, id).await? || (deleted.is_some() && g.as_deref() == Some(group))
        }
    })
}

async fn hold(conn: &mut SqliteConnection, join: &str, kind: &str, id: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO fed_items (join_id, kind, item_id) VALUES (?, ?, ?)")
        .bind(join)
        .bind(kind)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Day-plan entries of a task that's gone from here.
async fn drop_entries(conn: &mut SqliteConnection, task_id: &str) -> sqlx::Result<Vec<Change>> {
    let entries: Vec<crate::models::DayEntry> =
        sqlx::query_as("SELECT * FROM day_entries WHERE task_id = ? AND deleted_at IS NULL")
            .bind(task_id)
            .fetch_all(&mut *conn)
            .await?;
    let mut out = vec![];
    for mut e in entries {
        e.deleted_at = Some(now());
        e.updated_at = now();
        e.rev = next_rev(conn).await?;
        crate::models::upsert_entry(conn, &e).await?;
        out.push(Change::entry(&e));
    }
    Ok(out)
}

/// This join no longer holds the item; the copy goes unless another join still holds it.
async fn release(
    conn: &mut SqliteConnection,
    join: &str,
    kind: &str,
    id: &str,
) -> anyhow::Result<Vec<Change>> {
    let released =
        sqlx::query("DELETE FROM fed_items WHERE join_id = ? AND kind = ? AND item_id = ?")
            .bind(join)
            .bind(kind)
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    // Only what this join held (a feed can't name this server's own things).
    if released == 0 {
        return Ok(vec![]);
    }
    let held: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fed_items WHERE kind = ? AND item_id = ?)")
            .bind(kind)
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
    if held {
        return Ok(vec![]);
    }
    let ts = now();
    let mut out = vec![];
    match kind {
        "task" => {
            if let Some(mut t) =
                sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?
            {
                t.deleted_at = Some(ts.clone());
                t.updated_at = ts;
                t.rev = next_rev(conn).await?;
                upsert_task(conn, &t).await?;
                out.extend(drop_entries(conn, id).await?);
                out.push(Change::task(&t));
            }
        }
        "series" => {
            if let Some(mut s) = sqlx::query_as::<_, Series>(
                "SELECT * FROM series WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            {
                s.deleted_at = Some(ts.clone());
                s.updated_at = ts;
                s.rev = next_rev(conn).await?;
                upsert_series(conn, &s).await?;
                out.push(Change::series(&s));
            }
        }
        _ => {
            if let Some(mut p) = sqlx::query_as::<_, Project>(
                "SELECT * FROM projects WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            {
                p.deleted_at = Some(ts.clone());
                p.updated_at = ts;
                p.rev = next_rev(conn).await?;
                upsert_project(conn, &p).await?;
                out.push(Change::project(&p));
            }
        }
    }
    Ok(out)
}

/// Drop the copy of a joined project (left, or removed by its home server).
pub async fn remove_replica(state: &AppState, j: &Join) -> anyhow::Result<()> {
    let mut tx = state.db.write.begin().await?;
    let items: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, item_id FROM fed_items WHERE join_id = ? ORDER BY CASE kind WHEN 'task' THEN 0 WHEN 'series' THEN 1 ELSE 2 END")
            .bind(&j.id)
            .fetch_all(&mut *tx)
            .await?;
    let mut changes = vec![];
    for (kind, id) in &items {
        changes.extend(release(&mut tx, &j.id, kind, id).await?);
    }
    sqlx::query("DELETE FROM fed_joins WHERE id = ?")
        .bind(&j.id)
        .execute(&mut *tx)
        .await?;
    // The mirrored group goes when no join of this server needs it any more.
    let needed: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fed_joins WHERE group_id = ?)")
            .bind(&j.group_id)
            .fetch_one(&mut *tx)
            .await?;
    let mut members: Vec<String> = vec![];
    if !needed {
        members = sqlx::query_scalar(
            "SELECT m.user_id FROM group_members m JOIN users u ON u.id = m.user_id WHERE m.group_id = ? AND u.home IS NULL",
        )
        .bind(&j.group_id)
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM group_members WHERE group_id = ?")
            .bind(&j.group_id)
            .execute(&mut *tx)
            .await?;
        let rev = next_rev(&mut tx).await?;
        sqlx::query("UPDATE groups SET deleted_at = ?, updated_at = ?, rev = ? WHERE id = ? AND home IS NOT NULL")
            .bind(now())
            .bind(now())
            .bind(rev)
            .bind(&j.group_id)
            .execute(&mut *tx)
            .await?;
    } else {
        // Someone else here still follows it; just this person leaves the mirror.
        sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
            .bind(&j.group_id)
            .bind(&j.user_id)
            .execute(&mut *tx)
            .await?;
        members.push(j.user_id.clone());
    }
    tx.commit().await?;
    for m in &members {
        changes.push(Change::membership(m));
    }
    changes.push(Change {
        rev: 0,
        kind: "remote_project",
        data: json!({"id": j.id, "deleted": true}),
        audience: vec![j.user_id.clone()],
    });
    state.bus.publish(changes);
    Ok(())
}

/// The join through which `user` reaches a replicated item, if it is one.
async fn join_for(state: &AppState, user_id: &str, item_id: &str) -> sqlx::Result<Option<Join>> {
    sqlx::query_as(
        "SELECT j.* FROM fed_joins j JOIN fed_items f ON f.join_id = j.id WHERE f.item_id = ? AND j.user_id = ? LIMIT 1",
    )
    .bind(item_id)
    .bind(user_id)
    .fetch_optional(&state.db.read)
    .await
}

async fn is_replica(state: &AppState, item_id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fed_items WHERE item_id = ?)")
        .bind(item_id)
        .fetch_one(&state.db.read)
        .await
}

/// Send an edit to the home server as `join`'s person; on success bring the copy up to
/// date and answer with what the home server answered (revisions translated to ours).
pub async fn forward(
    state: &AppState,
    j: &Join,
    method: &Method,
    path: &str,
    body: Value,
) -> Result<(StatusCode, Value), AppError> {
    let host = host_of(&j.home_url);
    let unavailable = || {
        AppError::Unavailable(format!(
            "{host}, where this project is shared from, can't be reached right now, so it's read-only until it's back"
        ))
    };
    let secret = state
        .secrets
        .decrypt(&j.secret)
        .map_err(AppError::Internal)?;
    let req = Forward {
        method: method.to_string(),
        path: path.to_string(),
        body,
    };
    let res = client(30)
        .map_err(AppError::Internal)?
        .post(format!(
            "{}/api/v1/federation/links/{}/forward",
            j.home_url, j.id
        ))
        .bearer_auth(&secret)
        .header(header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&req).map_err(anyhow::Error::from)?)
        .send()
        .await
        .map_err(|_| unavailable())?;
    match res.status() {
        StatusCode::GONE => {
            let (state, j) = (state.clone(), j.clone());
            tokio::spawn(async move {
                let _ = remove_replica(&state, &j).await;
            });
            return Err(AppError::Forbidden(format!(
                "this project is no longer shared with you by {host}"
            )));
        }
        s if s.is_success() => {}
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST => {
            let text = crate::net::text_limited(res, 1 << 20)
                .await
                .unwrap_or_default();
            let detail = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["detail"].as_str().map(String::from))
                .unwrap_or_else(|| format!("{host} refused the change"));
            return Err(AppError::Forbidden(detail));
        }
        _ => return Err(unavailable()),
    }
    let out: Forwarded = serde_json::from_str(
        &crate::net::text_limited(res, 16 << 20)
            .await
            .map_err(|_| unavailable())?,
    )
    .map_err(|_| unavailable())?;
    let status = StatusCode::from_u16(out.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut body = out.body;
    if status.is_success() {
        if let Err(e) = pull(state, &j.id).await {
            tracing::info!("pull after a forwarded edit failed: {e:#}");
        }
        // Clients compare revisions with ours, not the home server's.
        let table = match path.trim_matches('/').split('/').next() {
            Some("tasks") => "tasks",
            Some("projects") => "projects",
            _ => "series",
        };
        if let Some(id) = body.get("id").and_then(Value::as_str).map(String::from) {
            let rev: Option<i64> =
                sqlx::query_scalar(&format!("SELECT rev FROM {table} WHERE id = ?"))
                    .bind(&id)
                    .fetch_optional(&state.db.read)
                    .await?;
            if let Some(r) = rev {
                body["rev"] = json!(r);
            }
        }
    }
    Ok((status, body))
}

/// For server-side callers that edit an item for a user (e.g. ticking a printed sheet):
/// `Some(result)` if it's a replica (the edit went to its home server), `None` otherwise.
pub async fn write_as(
    state: &AppState,
    user_id: &str,
    method: Method,
    path: &str,
    body: Value,
) -> Option<Result<(StatusCode, Value), AppError>> {
    let (_, Some(id)) = route_of(&method, path)? else {
        return None;
    };
    if !is_replica(state, &id).await.unwrap_or(false) {
        return None;
    }
    Some(match join_for(state, user_id, &id).await {
        Ok(Some(j)) => forward(state, &j, &method, path, body).await,
        Ok(None) => Err(AppError::NotFound),
        Err(e) => Err(e.into()),
    })
}

/// Whether a project here is a copy of one shared from another server.
pub async fn is_replica_project(state: &AppState, id: &str) -> bool {
    is_replica(state, id).await.unwrap_or(false)
}

/// Middleware on the API: writes to replicated tasks, projects and routines (or creating
/// one inside a replicated project) go to the home server instead of being made here.
pub async fn forward_replica_writes(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let Some((kind, id)) = route_of(&method, &path) else {
        return next.run(req).await;
    };
    let any: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fed_joins)")
        .fetch_one(&state.db.read)
        .await
        .unwrap_or(false);
    if !any {
        return next.run(req).await;
    }
    let (mut parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 8 << 20).await {
        Ok(b) => b,
        Err(_) => return AppError::BadRequest("request too large".into()).into_response(),
    };
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let rebuild =
        |parts: axum::http::request::Parts| Request::from_parts(parts, Body::from(bytes.clone()));

    let entity_replica = match &id {
        Some(id) => is_replica(&state, id).await.unwrap_or(false),
        None => false,
    };
    let target = target_project(kind, &json).flatten();
    let target_replica = match &target {
        Some(p) => is_replica(&state, p).await.unwrap_or(false),
        None => false,
    };
    if !entity_replica && !target_replica {
        return next.run(rebuild(parts)).await;
    }
    if !entity_replica && id.is_some() {
        return AppError::BadRequest(
            "this can't be moved into a project shared from another server; create it there instead".into(),
        )
        .into_response();
    }
    let user = match <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
        &mut parts, &state,
    )
    .await
    {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    let via = id.as_deref().or(target.as_deref()).unwrap_or_default();
    let j = match join_for(&state, user.id(), via).await {
        Ok(Some(j)) => j,
        Ok(None) => return AppError::NotFound.into_response(),
        Err(e) => return AppError::from(e).into_response(),
    };
    match forward(&state, &j, &method, &path, json).await {
        Ok((status, body)) if body.is_null() => status.into_response(),
        Ok((status, body)) => (status, axum::Json(body)).into_response(),
        Err(e) => e.into_response(),
    }
}

// =====================================================================================
// Background: live stream + periodic pulls (replica side)
// =====================================================================================

static LISTENING: LazyLock<std::sync::Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Follow the home server's live stream for one join: pull on every poke. Ends when the
/// join is gone; reconnects after errors.
async fn listen(state: AppState, join_id: String) {
    loop {
        let Ok(Some(j)) = load_join(&state, &join_id).await else {
            break;
        };
        let attempt = async {
            let secret = state.secrets.decrypt(&j.secret)?;
            let mut res = client(330)?
                .get(format!(
                    "{}/api/v1/federation/links/{}/events",
                    j.home_url, j.id
                ))
                .bearer_auth(secret)
                .send()
                .await?;
            if !res.status().is_success() {
                bail!("{}", res.status());
            }
            let _ = pull(&state, &j.id).await;
            while let Some(chunk) = res.chunk().await? {
                if chunk.windows(4).any(|w| w == b"poke") {
                    let _ = pull(&state, &j.id).await;
                }
            }
            anyhow::Ok(())
        };
        let failed = attempt.await.is_err();
        tokio::time::sleep(Duration::from_secs(if failed { 60 } else { 1 })).await;
    }
    LISTENING.lock().unwrap().remove(&join_id);
}

/// One round: start listeners for new joins, and pull the ones not pulled lately.
pub async fn run_due(state: &AppState) -> anyhow::Result<()> {
    let joins: Vec<Join> = sqlx::query_as("SELECT * FROM fed_joins")
        .fetch_all(&state.db.read)
        .await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::from_std(PULL_EVERY)?;
    for j in joins {
        if LISTENING.lock().unwrap().insert(j.id.clone()) {
            tokio::spawn(listen(state.clone(), j.id.clone()));
        }
        let stale = j
            .last_sync_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .is_none_or(|t| t < cutoff);
        if stale && let Err(e) = pull(state, &j.id).await {
            tracing::info!("pulling shared project {} failed: {e:#}", j.root_project_id);
        }
    }
    Ok(())
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let s = state.clone();
            crate::util::guarded("federation", async move {
                if let Err(e) = run_due(&s).await {
                    tracing::warn!("federation job failed: {e:#}");
                }
            })
            .await;
        }
    });
}

/// Joined projects of a user, for the sync payload and Settings.
pub async fn joins_of(state: &AppState, user_id: &str) -> sqlx::Result<Vec<JoinView>> {
    let rows: Vec<Join> =
        sqlx::query_as("SELECT * FROM fed_joins WHERE user_id = ? ORDER BY created_at")
            .bind(user_id)
            .fetch_all(&state.db.read)
            .await?;
    Ok(rows.iter().map(JoinView::from).collect())
}

/// For the "pokes" stream: which changes concern a link's project.
pub fn concerns(c: &Change, group: &str, remote_user: &str) -> bool {
    let key = crate::visibility::group_key(group);
    c.audience.iter().any(|a| a == &key || a == remote_user)
        && matches!(c.kind, "project" | "task" | "series" | "group")
}
