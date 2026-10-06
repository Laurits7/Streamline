//! Database rows and API DTOs. Types marked `#[ts(export)]` are exported to
//! `web/src/lib/api/types` by `cargo test` (see `.cargo/config.toml`).

use serde::Serialize;
use sqlx::{FromRow, SqliteConnection};
use ts_rs::TS;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub timezone: String,
    pub day_end: String,
    pub last_rollover_date: Option<String>,
    pub locale: String,
    pub week_start: i32,
}

/// The signed-in user as seen by themselves (and by admins in the user list).
#[derive(Debug, Clone, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
    pub timezone: String,
    pub day_end: String,
    /// BCP 47 language tag for dates and numbers; empty = the browser's language.
    pub locale: String,
    /// First day of the week, ISO weekday (1 = Monday ... 7 = Sunday).
    pub week_start: i32,
}

impl From<&User> for Me {
    fn from(u: &User) -> Self {
        Me {
            id: u.id.clone(),
            username: u.username.clone(),
            display_name: u.display_name.clone(),
            is_admin: u.is_admin,
            timezone: u.timezone.clone(),
            day_end: u.day_end.clone(),
            locale: u.locale.clone(),
            week_start: u.week_start,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct TaskType {
    pub id: String,
    pub key: String,
    pub name: String,
    #[ts(type = "'carry' | 'expire' | 'window' | 'deadline'")]
    pub day_end_behavior: String,
    pub counts_for_streak: bool,
    pub shows_overdue: bool,
    pub shows_carry_count: bool,
    pub builtin: bool,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Project {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_group_id: Option<String>,
    /// Parent project for nested projects (`null` = top level).
    pub parent_id: Option<String>,
    pub name: String,
    pub color: Option<String>,
    pub position: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Task {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_group_id: Option<String>,
    pub assignee_user_id: Option<String>,
    pub project_id: Option<String>,
    pub title: String,
    pub notes: String,
    #[ts(as = "TaskStatus")]
    #[schema(value_type = TaskStatus)]
    pub status: String,
    pub position: String,
    pub due_date: Option<String>,
    pub estimate_min: Option<i32>,
    pub difficulty: Option<i32>,
    pub importance: Option<i32>,
    pub urgency: Option<i32>,
    pub actual_min: i32,
    pub task_type_id: String,
    pub carry_count: i32,
    pub completed_at: Option<String>,
    pub completed_by: Option<String>,
    pub ext_source: Option<String>,
    pub ext_id: Option<String>,
    pub ext_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

#[derive(Debug, Clone, Copy, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Done,
    Missed,
    Skipped,
    WontDo,
}

pub const TASK_STATUSES: &[&str] = &["open", "done", "missed", "skipped", "wont_do"];

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DayEntry {
    pub id: String,
    pub user_id: String,
    pub date: String,
    pub task_id: String,
    pub position: String,
    pub start_time: Option<String>,
    pub duration_min: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

// ---- persistence helpers ---------------------------------------------------

pub async fn upsert_project(conn: &mut SqliteConnection, p: &Project) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO projects (id, owner_user_id, owner_group_id, parent_id, name, color, position, archived_at, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id,
           parent_id=excluded.parent_id, name=excluded.name, color=excluded.color, position=excluded.position, archived_at=excluded.archived_at,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&p.id).bind(&p.owner_user_id).bind(&p.owner_group_id).bind(&p.parent_id).bind(&p.name).bind(&p.color)
    .bind(&p.position).bind(&p.archived_at).bind(&p.created_at).bind(&p.updated_at).bind(&p.deleted_at).bind(p.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn upsert_task(conn: &mut SqliteConnection, t: &Task) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tasks (id, owner_user_id, owner_group_id, assignee_user_id, project_id, title, notes, status, position,
           due_date, estimate_min, difficulty, importance, urgency, actual_min, task_type_id, carry_count, completed_at,
           completed_by, ext_source, ext_id, ext_url, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id,
           assignee_user_id=excluded.assignee_user_id, project_id=excluded.project_id, title=excluded.title,
           notes=excluded.notes, status=excluded.status, position=excluded.position, due_date=excluded.due_date,
           estimate_min=excluded.estimate_min, difficulty=excluded.difficulty, importance=excluded.importance,
           urgency=excluded.urgency, actual_min=excluded.actual_min, task_type_id=excluded.task_type_id,
           carry_count=excluded.carry_count, completed_at=excluded.completed_at, completed_by=excluded.completed_by,
           ext_source=excluded.ext_source, ext_id=excluded.ext_id, ext_url=excluded.ext_url,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&t.id).bind(&t.owner_user_id).bind(&t.owner_group_id).bind(&t.assignee_user_id).bind(&t.project_id)
    .bind(&t.title).bind(&t.notes).bind(&t.status).bind(&t.position).bind(&t.due_date).bind(t.estimate_min)
    .bind(t.difficulty).bind(t.importance).bind(t.urgency).bind(t.actual_min).bind(&t.task_type_id)
    .bind(t.carry_count).bind(&t.completed_at).bind(&t.completed_by).bind(&t.ext_source).bind(&t.ext_id)
    .bind(&t.ext_url).bind(&t.created_at).bind(&t.updated_at).bind(&t.deleted_at).bind(t.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn upsert_entry(conn: &mut SqliteConnection, e: &DayEntry) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO day_entries (id, user_id, date, task_id, position, start_time, duration_min, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET date=excluded.date, position=excluded.position, start_time=excluded.start_time,
           duration_min=excluded.duration_min, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&e.id).bind(&e.user_id).bind(&e.date).bind(&e.task_id).bind(&e.position).bind(&e.start_time)
    .bind(e.duration_min).bind(&e.created_at).bind(&e.updated_at).bind(&e.deleted_at).bind(e.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn log_task_event(
    conn: &mut SqliteConnection,
    task_id: &str,
    user_id: Option<&str>,
    kind: &str,
    data: Option<serde_json::Value>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO task_events (id, task_id, user_id, kind, source, data, at) VALUES (?,?,?,?, 'app', ?, ?)")
        .bind(crate::util::new_id())
        .bind(task_id)
        .bind(user_id)
        .bind(kind)
        .bind(data.map(|d| d.to_string()))
        .bind(crate::util::now())
        .execute(conn)
        .await
        .map(|_| ())
}
