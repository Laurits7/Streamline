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
    pub plan_mode: String,
    pub plan_time_evening: String,
    pub plan_time_morning: String,
    pub day_window_start: String,
    pub day_window_end: String,
    pub prefs: String,
    pub focus_work_min: i32,
    pub focus_short_break_min: i32,
    pub focus_long_break_min: i32,
    pub focus_long_every: i32,
}

impl User {
    pub fn focus_settings(&self) -> streamline_domain::focus::Settings {
        let clamp = |v: i32, hi: i32| v.clamp(1, hi) as u32;
        streamline_domain::focus::Settings {
            work_min: clamp(self.focus_work_min, 180),
            short_break_min: clamp(self.focus_short_break_min, 60),
            long_break_min: clamp(self.focus_long_break_min, 120),
            long_every: clamp(self.focus_long_every, 12),
        }
    }
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
    /// When the user plans: `evening` (plan tomorrow), `morning` (plan today) or `both`.
    #[ts(type = "'evening' | 'morning' | 'both'")]
    pub plan_mode: String,
    pub plan_time_evening: String,
    pub plan_time_morning: String,
    /// The part of the day counted as available time (`HH:MM`).
    pub day_window_start: String,
    pub day_window_end: String,
    /// Free-form UI preferences (e.g. each view's filters), shared across devices.
    #[ts(type = "Record<string, unknown>")]
    #[schema(value_type = Object)]
    pub prefs: serde_json::Value,
    /// Pomodoro lengths in minutes, and how many work intervals until a long break.
    pub focus_work_min: i32,
    pub focus_short_break_min: i32,
    pub focus_long_break_min: i32,
    pub focus_long_every: i32,
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
            plan_mode: u.plan_mode.clone(),
            plan_time_evening: u.plan_time_evening.clone(),
            plan_time_morning: u.plan_time_morning.clone(),
            day_window_start: u.day_window_start.clone(),
            day_window_end: u.day_window_end.clone(),
            prefs: serde_json::from_str(&u.prefs).unwrap_or_else(|_| serde_json::json!({})),
            focus_work_min: u.focus_work_min,
            focus_short_break_min: u.focus_short_break_min,
            focus_long_break_min: u.focus_long_break_min,
            focus_long_every: u.focus_long_every,
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
    /// Place given to new tasks created in this project.
    pub default_place_id: Option<String>,
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
    /// Set when work on the task began ("in progress" while the task is open).
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub completed_by: Option<String>,
    pub ext_source: Option<String>,
    pub ext_id: Option<String>,
    pub ext_url: Option<String>,
    /// Where the task has to be done; `null` = anywhere.
    pub place_id: Option<String>,
    /// Other projects the task is also listed in (besides its main `project_id`).
    #[ts(type = "Array<string>")]
    #[schema(value_type = Vec<String>)]
    pub also_project_ids: sqlx::types::Json<Vec<String>>,
    /// Tasks that must be finished first (SPEC §6.3b).
    #[ts(type = "Array<string>")]
    #[schema(value_type = Vec<String>)]
    pub depends_on: sqlx::types::Json<Vec<String>>,
    /// A prerequisite is still open: not in the ready stack, can't be planned.
    pub blocked: bool,
    /// Minutes to wait after the last prerequisite is done (e.g. a machine running).
    pub wait_min: Option<i32>,
    /// While waiting: when the task becomes ready.
    pub ready_at: Option<String>,
    /// Steps of a running workflow: the run, this step's number and the step count.
    pub workflow_instance_id: Option<String>,
    pub workflow_step: Option<i32>,
    pub workflow_steps: Option<i32>,
    /// Set for occurrences of a routine.
    pub series_id: Option<String>,
    /// Identifies the occurrence within its routine (a date, or window start + "#n").
    pub occurrence_key: Option<String>,
    /// The day the occurrence is for (or the first day of its window).
    pub occurrence_date: Option<String>,
    /// For "N times per week/month" routines: the last day of the window.
    pub window_end: Option<String>,
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

/// One step of a workflow template.
#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct WorkflowStep {
    /// Stable id within the template (variants refer to it).
    pub id: String,
    pub title: String,
    pub estimate_min: Option<i32>,
    /// Wait this long after this step before the next one is ready (e.g. the machine runs).
    pub wait_min: Option<i32>,
    pub difficulty: Option<i32>,
}

/// A variant of a workflow: the template's steps minus `skip`.
#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct WorkflowVariant {
    pub id: String,
    pub name: String,
    pub skip: Vec<String>,
}

/// A reusable multi-step chore (SPEC §6.3b), e.g. Laundry with variants per load.
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct WorkflowTemplate {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_group_id: Option<String>,
    pub name: String,
    pub description: String,
    /// Project the steps go into (optional).
    pub project_id: Option<String>,
    #[ts(type = "Array<WorkflowStep>")]
    #[schema(value_type = Vec<WorkflowStep>)]
    pub steps: sqlx::types::Json<Vec<WorkflowStep>>,
    #[ts(type = "Array<WorkflowVariant>")]
    #[schema(value_type = Vec<WorkflowVariant>)]
    pub variants: sqlx::types::Json<Vec<WorkflowVariant>>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_workflow(
    conn: &mut SqliteConnection,
    w: &WorkflowTemplate,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO workflow_templates (id, owner_user_id, owner_group_id, name, description, project_id, steps, variants, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, project_id=excluded.project_id,
           steps=excluded.steps, variants=excluded.variants, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&w.id).bind(&w.owner_user_id).bind(&w.owner_group_id).bind(&w.name).bind(&w.description).bind(&w.project_id)
    .bind(&w.steps).bind(&w.variants).bind(&w.created_at).bind(&w.updated_at).bind(&w.deleted_at).bind(w.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Somewhere tasks are done (SPEC §6.16). Coordinates are optional (for GPS detection).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Place {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_group_id: Option<String>,
    pub name: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// How close (metres) counts as being there.
    pub radius_m: i32,
    pub position: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_place(conn: &mut SqliteConnection, p: &Place) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO places (id, owner_user_id, owner_group_id, name, lat, lon, radius_m, position, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, lat=excluded.lat, lon=excluded.lon, radius_m=excluded.radius_m,
           position=excluded.position, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&p.id).bind(&p.owner_user_id).bind(&p.owner_group_id).bind(&p.name).bind(p.lat).bind(p.lon)
    .bind(p.radius_m).bind(&p.position).bind(&p.created_at).bind(&p.updated_at).bind(&p.deleted_at).bind(p.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A routine: a template that spawns one task per occurrence (SPEC §6.3).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Series {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_group_id: Option<String>,
    pub project_id: Option<String>,
    pub title: String,
    pub notes: String,
    /// `repeat` (on the rule's dates), `anchored` (at a fixed time) or `flexible` (N per week/month).
    #[ts(type = "'repeat' | 'anchored' | 'flexible'")]
    pub mode: String,
    /// iCalendar RRULE without `RRULE:`, e.g. `FREQ=WEEKLY;BYDAY=MO,WE` (repeat/anchored).
    pub rrule: Option<String>,
    /// First day (`YYYY-MM-DD`).
    pub dtstart: String,
    /// Last day, inclusive; `null` = no end.
    pub until: Option<String>,
    /// `HH:MM` for anchored routines.
    pub start_time: Option<String>,
    pub duration_min: Option<i32>,
    /// Flexible routines: how many times per window.
    pub times_per_window: Option<i32>,
    #[ts(type = "'week' | 'month' | null")]
    pub window: Option<String>,
    pub task_type_id: String,
    pub estimate_min: Option<i32>,
    pub difficulty: Option<i32>,
    pub importance: Option<i32>,
    pub urgency: Option<i32>,
    /// Place given to each occurrence.
    pub place_id: Option<String>,
    /// Each occurrence starts this workflow instead of a single task (3b.6).
    pub workflow_template_id: Option<String>,
    #[ts(type = "Array<string>")]
    #[schema(value_type = Vec<String>)]
    pub workflow_variant_ids: sqlx::types::Json<Vec<String>>,
    #[serde(skip)]
    #[ts(skip)]
    pub materialized_through: Option<String>,
    /// The routine this one replaced when its schedule changed (progress and streaks
    /// continue across versions).
    pub split_from: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

/// The user's focus timer as clients see it. Remaining time at `now` (ms) is
/// `length_min * 60000 - elapsed_ms - (running_since_ms ? now - running_since_ms : 0)`.
#[derive(Debug, Clone, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct FocusTimer {
    pub task_id: Option<String>,
    #[ts(type = "'idle' | 'work' | 'short_break' | 'long_break'")]
    pub phase: String,
    /// Unix ms when the clock last started; `null` = paused or waiting to start.
    #[ts(type = "number | null")]
    pub running_since_ms: Option<i64>,
    #[ts(type = "number")]
    pub elapsed_ms: i64,
    pub length_min: i32,
    /// Completed work intervals since the last long break.
    pub cycle_done: i32,
    #[ts(type = "number")]
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct FocusSession {
    pub id: String,
    pub user_id: String,
    pub task_id: Option<String>,
    #[ts(type = "'work' | 'break'")]
    pub kind: String,
    pub started_at: String,
    pub ended_at: String,
    pub minutes: i32,
    /// Ran its full length (vs skipped or stopped early).
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

/// Planning state of one day. No record (or a deleted one) means the day is unplanned.
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DayPlan {
    pub id: String,
    pub user_id: String,
    pub date: String,
    /// `draft` while the planning wizard is in progress, `planned` once confirmed.
    #[ts(type = "'draft' | 'planned'")]
    pub status: String,
    /// The wizard step to resume at.
    pub step: i32,
    pub planned_at: Option<String>,
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
        "INSERT INTO projects (id, owner_user_id, owner_group_id, parent_id, name, color, position, archived_at, default_place_id, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id,
           parent_id=excluded.parent_id, name=excluded.name, color=excluded.color, position=excluded.position, archived_at=excluded.archived_at,
           default_place_id=excluded.default_place_id,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&p.id).bind(&p.owner_user_id).bind(&p.owner_group_id).bind(&p.parent_id).bind(&p.name).bind(&p.color)
    .bind(&p.position).bind(&p.archived_at).bind(&p.default_place_id).bind(&p.created_at).bind(&p.updated_at).bind(&p.deleted_at).bind(p.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn upsert_task(conn: &mut SqliteConnection, t: &Task) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tasks (id, owner_user_id, owner_group_id, assignee_user_id, project_id, title, notes, status, position,
           due_date, estimate_min, difficulty, importance, urgency, actual_min, task_type_id, carry_count, started_at, completed_at,
           completed_by, ext_source, ext_id, ext_url, place_id, also_project_ids, depends_on, blocked, wait_min, ready_at,
           workflow_instance_id, workflow_step, workflow_steps, series_id, occurrence_key, occurrence_date, window_end,
           created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id,
           assignee_user_id=excluded.assignee_user_id, project_id=excluded.project_id, title=excluded.title,
           notes=excluded.notes, status=excluded.status, position=excluded.position, due_date=excluded.due_date,
           estimate_min=excluded.estimate_min, difficulty=excluded.difficulty, importance=excluded.importance,
           urgency=excluded.urgency, actual_min=excluded.actual_min, task_type_id=excluded.task_type_id,
           carry_count=excluded.carry_count, started_at=excluded.started_at, completed_at=excluded.completed_at, completed_by=excluded.completed_by,
           ext_source=excluded.ext_source, ext_id=excluded.ext_id, ext_url=excluded.ext_url,
           also_project_ids=excluded.also_project_ids, place_id=excluded.place_id,
           depends_on=excluded.depends_on, blocked=excluded.blocked, wait_min=excluded.wait_min, ready_at=excluded.ready_at,
           workflow_instance_id=excluded.workflow_instance_id, workflow_step=excluded.workflow_step,
           workflow_steps=excluded.workflow_steps,
           occurrence_date=excluded.occurrence_date, window_end=excluded.window_end,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&t.id).bind(&t.owner_user_id).bind(&t.owner_group_id).bind(&t.assignee_user_id).bind(&t.project_id)
    .bind(&t.title).bind(&t.notes).bind(&t.status).bind(&t.position).bind(&t.due_date).bind(t.estimate_min)
    .bind(t.difficulty).bind(t.importance).bind(t.urgency).bind(t.actual_min).bind(&t.task_type_id)
    .bind(t.carry_count).bind(&t.started_at).bind(&t.completed_at).bind(&t.completed_by).bind(&t.ext_source).bind(&t.ext_id)
    .bind(&t.ext_url).bind(&t.place_id).bind(&t.also_project_ids).bind(&t.depends_on).bind(t.blocked)
    .bind(t.wait_min).bind(&t.ready_at).bind(&t.workflow_instance_id).bind(t.workflow_step).bind(t.workflow_steps).bind(&t.series_id).bind(&t.occurrence_key).bind(&t.occurrence_date).bind(&t.window_end)
    .bind(&t.created_at).bind(&t.updated_at).bind(&t.deleted_at).bind(t.rev)
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

pub async fn upsert_series(conn: &mut SqliteConnection, s: &Series) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO series (id, owner_user_id, owner_group_id, project_id, title, notes, mode, rrule, dtstart, until,
           start_time, duration_min, times_per_window, window, task_type_id, estimate_min, difficulty, importance, urgency,
           materialized_through, split_from, place_id, workflow_template_id, workflow_variant_ids, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id, title=excluded.title, notes=excluded.notes,
           mode=excluded.mode, rrule=excluded.rrule, dtstart=excluded.dtstart, until=excluded.until,
           start_time=excluded.start_time, duration_min=excluded.duration_min, times_per_window=excluded.times_per_window,
           window=excluded.window, task_type_id=excluded.task_type_id, estimate_min=excluded.estimate_min,
           difficulty=excluded.difficulty, importance=excluded.importance, urgency=excluded.urgency,
           place_id=excluded.place_id, workflow_template_id=excluded.workflow_template_id,
           workflow_variant_ids=excluded.workflow_variant_ids,
           materialized_through=excluded.materialized_through, updated_at=excluded.updated_at,
           deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&s.id).bind(&s.owner_user_id).bind(&s.owner_group_id).bind(&s.project_id).bind(&s.title).bind(&s.notes)
    .bind(&s.mode).bind(&s.rrule).bind(&s.dtstart).bind(&s.until).bind(&s.start_time).bind(s.duration_min)
    .bind(s.times_per_window).bind(&s.window).bind(&s.task_type_id).bind(s.estimate_min).bind(s.difficulty)
    .bind(s.importance).bind(s.urgency).bind(&s.materialized_through).bind(&s.split_from).bind(&s.place_id).bind(&s.workflow_template_id).bind(&s.workflow_variant_ids).bind(&s.created_at).bind(&s.updated_at)
    .bind(&s.deleted_at).bind(s.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn upsert_day_plan(conn: &mut SqliteConnection, p: &DayPlan) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO day_plans (id, user_id, date, status, step, planned_at, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET status=excluded.status, step=excluded.step, planned_at=excluded.planned_at,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&p.id).bind(&p.user_id).bind(&p.date).bind(&p.status).bind(p.step).bind(&p.planned_at)
    .bind(&p.created_at).bind(&p.updated_at).bind(&p.deleted_at).bind(p.rev)
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
