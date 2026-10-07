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
    pub unit_system: String,
    pub review_cadence: String,
    pub last_review_date: Option<String>,
    pub notify_off: String,
    pub ntfy_url: Option<String>,
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
    /// `metric` (kg) or `imperial` (lb) for displaying weight; values are stored metric.
    #[ts(type = "'metric' | 'imperial'")]
    pub unit_system: String,
    /// How often the goals review is offered.
    #[ts(type = "'off' | 'weekly' | 'monthly'")]
    pub review_cadence: String,
    pub last_review_date: Option<String>,
    /// Notification kinds turned off: `planning`, `ready`, `focus`, `conflict`, `metric`.
    pub notify_off: Vec<String>,
    /// ntfy topic URL notifications are also sent to.
    pub ntfy_url: Option<String>,
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
            unit_system: u.unit_system.clone(),
            review_cadence: u.review_cadence.clone(),
            last_review_date: u.last_review_date.clone(),
            notify_off: serde_json::from_str(&u.notify_off).unwrap_or_default(),
            ntfy_url: u.ntfy_url.clone(),
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
    /// What the project is about (plain text).
    pub description: String,
    /// `active`, or `idea`: not started yet, so its tasks ask for no attention (D-72).
    pub status: String,
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
    /// The calendar event (instance) the task is for; only its owner can resolve it.
    pub event_id: Option<String>,
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

/// A group of users (e.g. "Family") with its members (SPEC §6.4).
#[derive(Debug, Clone, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub created_by: Option<String>,
    pub members: Vec<GroupMember>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct GroupMember {
    pub user_id: String,
    pub display_name: String,
    pub username: String,
    /// `owner` (can manage the group) or `member`.
    #[ts(type = "'owner' | 'member'")]
    pub role: String,
}

/// Someone you can add to a group (every user of this install).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct UserSummary {
    pub id: String,
    pub display_name: String,
    pub username: String,
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
    #[ts(as = "Vec<WorkflowStep>")]
    #[schema(value_type = Vec<WorkflowStep>)]
    pub steps: sqlx::types::Json<Vec<WorkflowStep>>,
    #[ts(as = "Vec<WorkflowVariant>")]
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
        "INSERT INTO projects (id, owner_user_id, owner_group_id, parent_id, name, color, position, archived_at, default_place_id, description, status, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id,
           parent_id=excluded.parent_id, name=excluded.name, color=excluded.color, position=excluded.position, archived_at=excluded.archived_at,
           default_place_id=excluded.default_place_id, description=excluded.description, status=excluded.status,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&p.id).bind(&p.owner_user_id).bind(&p.owner_group_id).bind(&p.parent_id).bind(&p.name).bind(&p.color)
    .bind(&p.position).bind(&p.archived_at).bind(&p.default_place_id).bind(&p.description).bind(&p.status).bind(&p.created_at).bind(&p.updated_at).bind(&p.deleted_at).bind(p.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn upsert_task(conn: &mut SqliteConnection, t: &Task) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tasks (id, owner_user_id, owner_group_id, assignee_user_id, project_id, title, notes, status, position,
           due_date, estimate_min, difficulty, importance, urgency, actual_min, task_type_id, carry_count, started_at, completed_at,
           completed_by, ext_source, ext_id, ext_url, place_id, event_id, also_project_ids, depends_on, blocked, wait_min, ready_at,
           workflow_instance_id, workflow_step, workflow_steps, series_id, occurrence_key, occurrence_date, window_end,
           created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id,
           assignee_user_id=excluded.assignee_user_id, project_id=excluded.project_id, title=excluded.title,
           notes=excluded.notes, status=excluded.status, position=excluded.position, due_date=excluded.due_date,
           estimate_min=excluded.estimate_min, difficulty=excluded.difficulty, importance=excluded.importance,
           urgency=excluded.urgency, actual_min=excluded.actual_min, task_type_id=excluded.task_type_id,
           carry_count=excluded.carry_count, started_at=excluded.started_at, completed_at=excluded.completed_at, completed_by=excluded.completed_by,
           ext_source=excluded.ext_source, ext_id=excluded.ext_id, ext_url=excluded.ext_url,
           also_project_ids=excluded.also_project_ids, place_id=excluded.place_id, event_id=excluded.event_id,
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
    .bind(&t.ext_url).bind(&t.place_id).bind(&t.event_id).bind(&t.also_project_ids).bind(&t.depends_on).bind(t.blocked)
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
           duration_min=excluded.duration_min, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev
         WHERE day_entries.user_id = excluded.user_id",
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
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev
         WHERE day_plans.user_id = excluded.user_id",
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

/// A connected calendar account (SPEC §6.5). The password is stored encrypted and never
/// leaves the server.
#[derive(Debug, Clone, FromRow)]
pub struct CalendarAccount {
    pub id: String,
    pub user_id: String,
    pub kind: String,
    pub url: String,
    pub username: String,
    pub secret: Option<Vec<u8>>,
    pub status: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub rev: i64,
}

/// What the API shows of a calendar account.
#[derive(Debug, Clone, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct CalendarAccountView {
    pub id: String,
    pub kind: String,
    pub url: String,
    pub username: String,
    pub has_password: bool,
    /// `new` (not synced yet), `ok` or `error`.
    pub status: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
}

impl From<&CalendarAccount> for CalendarAccountView {
    fn from(a: &CalendarAccount) -> Self {
        Self {
            id: a.id.clone(),
            kind: a.kind.clone(),
            url: a.url.clone(),
            username: a.username.clone(),
            has_password: a.secret.is_some(),
            status: a.status.clone(),
            last_sync_at: a.last_sync_at.clone(),
            last_error: a.last_error.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Calendar {
    pub id: String,
    pub account_id: String,
    pub user_id: String,
    pub href: String,
    pub name: String,
    pub color: Option<String>,
    pub user_color: Option<String>,
    pub enabled: bool,
    /// All-day events block the whole day.
    pub all_day_busy: bool,
    #[serde(skip)]
    #[ts(skip)]
    pub ctag: Option<String>,
    #[serde(skip)]
    #[ts(skip)]
    pub expanded_for: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_calendar(conn: &mut SqliteConnection, c: &Calendar) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO calendars (id, account_id, user_id, href, name, color, user_color, enabled, ctag, expanded_for, created_at, updated_at, deleted_at, rev, all_day_busy)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, color=excluded.color, user_color=excluded.user_color,
           enabled=excluded.enabled, all_day_busy=excluded.all_day_busy, ctag=excluded.ctag, expanded_for=excluded.expanded_for,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&c.id).bind(&c.account_id).bind(&c.user_id).bind(&c.href).bind(&c.name).bind(&c.color).bind(&c.user_color)
    .bind(c.enabled).bind(&c.ctag).bind(&c.expanded_for).bind(&c.created_at).bind(&c.updated_at).bind(&c.deleted_at).bind(c.rev).bind(c.all_day_busy)
    .execute(conn)
    .await
    .map(|_| ())
}

/// One occurrence of a calendar event, as expanded from the cached iCalendar data.
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema, PartialEq)]
#[ts(export)]
pub struct CalendarEvent {
    pub id: String,
    pub user_id: String,
    pub calendar_id: String,
    pub uid: String,
    pub instance_key: String,
    pub title: String,
    pub location: Option<String>,
    pub all_day: bool,
    /// Timed events: RFC 3339 UTC.
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    /// All-day events: `YYYY-MM-DD`, end exclusive.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// Blocks time (not marked free/transparent).
    pub busy: bool,
    pub recurring: bool,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_event(conn: &mut SqliteConnection, e: &CalendarEvent) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO events (id, user_id, calendar_id, uid, instance_key, title, location, all_day, start_at, end_at, start_date, end_date, busy, recurring, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET title=excluded.title, location=excluded.location, all_day=excluded.all_day,
           start_at=excluded.start_at, end_at=excluded.end_at, start_date=excluded.start_date, end_date=excluded.end_date,
           busy=excluded.busy, recurring=excluded.recurring, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&e.id).bind(&e.user_id).bind(&e.calendar_id).bind(&e.uid).bind(&e.instance_key).bind(&e.title).bind(&e.location)
    .bind(e.all_day).bind(&e.start_at).bind(&e.end_at).bind(&e.start_date).bind(&e.end_date).bind(e.busy).bind(e.recurring)
    .bind(&e.updated_at).bind(&e.deleted_at).bind(e.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Someone whose nameday and/or birthday matters to the user (SPEC §6.17).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Person {
    pub id: String,
    pub owner_user_id: String,
    /// How tasks address them, e.g. "Mari (sister)".
    pub name: String,
    /// The name as it appears in the nameday calendar.
    pub nameday_name: Option<String>,
    /// `YYYY-MM-DD`, or `--MM-DD` when the year isn't known.
    pub birthday: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_person(conn: &mut SqliteConnection, p: &Person) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO people (id, owner_user_id, name, nameday_name, birthday, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, nameday_name=excluded.nameday_name, birthday=excluded.birthday,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&p.id).bind(&p.owner_user_id).bind(&p.name).bind(&p.nameday_name).bind(&p.birthday)
    .bind(&p.created_at).bind(&p.updated_at).bind(&p.deleted_at).bind(p.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A calendar event (every instance of a recurring one) assigned to a project (D-68).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct EventProject {
    pub id: String,
    pub user_id: String,
    pub calendar_id: String,
    /// The event's iCalendar UID (shared by all instances of a recurring event).
    pub uid: String,
    pub project_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_event_project(
    conn: &mut SqliteConnection,
    e: &EventProject,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO event_projects (id, user_id, calendar_id, uid, project_id, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id, updated_at=excluded.updated_at,
           deleted_at=excluded.deleted_at, rev=excluded.rev
         WHERE event_projects.user_id = excluded.user_id",
    )
    .bind(&e.id).bind(&e.user_id).bind(&e.calendar_id).bind(&e.uid).bind(&e.project_id)
    .bind(&e.created_at).bind(&e.updated_at).bind(&e.deleted_at).bind(e.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// One task an occasion creates.
#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, utoipa::ToSchema, PartialEq)]
#[ts(export)]
pub struct OccasionStep {
    /// `{name}` is replaced by the person's name.
    pub title: String,
    /// Days from the occasion: −2 = two days before.
    pub offset_days: i32,
    pub task_type_id: String,
    /// Waits until the previous step is done (e.g. greet after buying the present).
    #[serde(default)]
    pub after_previous: bool,
}

/// What a kind of occasion creates for a user.
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct OccasionTemplate {
    pub id: String,
    pub owner_user_id: String,
    /// `nameday` or `birthday`.
    pub kind: String,
    pub enabled: bool,
    #[ts(as = "Vec<OccasionStep>")]
    #[schema(value_type = Vec<OccasionStep>)]
    pub steps: sqlx::types::Json<Vec<OccasionStep>>,
    pub updated_at: String,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_occasion_template(
    conn: &mut SqliteConnection,
    t: &OccasionTemplate,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO occasion_templates (id, owner_user_id, kind, enabled, steps, updated_at, rev) VALUES (?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled, steps=excluded.steps, updated_at=excluded.updated_at, rev=excluded.rev",
    )
    .bind(&t.id).bind(&t.owner_user_id).bind(&t.kind).bind(t.enabled).bind(&t.steps).bind(&t.updated_at).bind(t.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A block of a day template.
#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, utoipa::ToSchema, PartialEq)]
#[ts(export)]
pub struct TemplateBlock {
    /// The block's theme, e.g. "Deep work: hard tasks".
    pub title: String,
    /// `HH:MM`
    pub start: String,
    pub end: String,
    /// `hard`, `medium` or `easy`: what kind of task the planner puts here.
    pub energy: Option<String>,
}

/// A reusable layout of time blocks (SPEC §6.8).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DayTemplate {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    /// ISO weekdays it applies to automatically: 1 = Monday … 7 = Sunday.
    #[ts(as = "Vec<i32>")]
    #[schema(value_type = Vec<i32>)]
    pub weekdays: sqlx::types::Json<Vec<i32>>,
    #[ts(as = "Vec<TemplateBlock>")]
    #[schema(value_type = Vec<TemplateBlock>)]
    pub blocks: sqlx::types::Json<Vec<TemplateBlock>>,
    pub position: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_day_template(conn: &mut SqliteConnection, t: &DayTemplate) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO day_templates (id, owner_user_id, name, weekdays, blocks, position, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, weekdays=excluded.weekdays, blocks=excluded.blocks, position=excluded.position,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&t.id).bind(&t.owner_user_id).bind(&t.name).bind(&t.weekdays).bind(&t.blocks).bind(&t.position)
    .bind(&t.created_at).bind(&t.updated_at).bind(&t.deleted_at).bind(t.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A time block on one day.
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct TimeBlock {
    pub id: String,
    pub user_id: String,
    pub date: String,
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub energy: Option<String>,
    pub template_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_time_block(conn: &mut SqliteConnection, b: &TimeBlock) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO time_blocks (id, user_id, date, title, start_time, end_time, energy, template_id, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET title=excluded.title, start_time=excluded.start_time, end_time=excluded.end_time, energy=excluded.energy,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&b.id).bind(&b.user_id).bind(&b.date).bind(&b.title).bind(&b.start_time).bind(&b.end_time).bind(&b.energy)
    .bind(&b.template_id).bind(&b.created_at).bind(&b.updated_at).bind(&b.deleted_at).bind(b.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A day's reflection (SPEC §6.2b). Personal.
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DayRecord {
    pub id: String,
    pub user_id: String,
    pub date: String,
    /// Free text.
    pub journal: String,
    pub went_well: String,
    pub went_badly: String,
    pub tomorrow: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_day_record(conn: &mut SqliteConnection, r: &DayRecord) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO day_records (id, user_id, date, journal, went_well, went_badly, tomorrow, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET journal=excluded.journal, went_well=excluded.went_well, went_badly=excluded.went_badly,
           tomorrow=excluded.tomorrow, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&r.id).bind(&r.user_id).bind(&r.date).bind(&r.journal).bind(&r.went_well).bind(&r.went_badly).bind(&r.tomorrow)
    .bind(&r.created_at).bind(&r.updated_at).bind(&r.deleted_at).bind(r.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Something a user tracks: mood, weight, or their own (sleep, water, steps…).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct MetricDefinition {
    pub id: String,
    pub owner_user_id: String,
    /// `mood` or `weight` for the built-ins.
    pub key: Option<String>,
    pub name: String,
    #[ts(type = "'number' | 'scale' | 'yes_no'")]
    pub kind: String,
    /// Weight is stored in kg.
    pub unit: String,
    pub scale_min: Option<i32>,
    pub scale_max: Option<i32>,
    /// How several entries of a day combine.
    #[ts(type = "'latest' | 'average' | 'sum' | 'max'")]
    pub aggregate: String,
    /// `HH:MM`: a reminder if nothing is logged by then.
    pub reminder_time: Option<String>,
    #[serde(skip)]
    #[ts(skip)]
    pub last_reminded: Option<String>,
    pub archived: bool,
    pub position: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_metric(conn: &mut SqliteConnection, m: &MetricDefinition) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO metric_definitions (id, owner_user_id, key, name, kind, unit, scale_min, scale_max, aggregate, reminder_time, last_reminded,
           archived, position, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, unit=excluded.unit, scale_min=excluded.scale_min, scale_max=excluded.scale_max,
           aggregate=excluded.aggregate, reminder_time=excluded.reminder_time, last_reminded=excluded.last_reminded,
           archived=excluded.archived, position=excluded.position, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&m.id).bind(&m.owner_user_id).bind(&m.key).bind(&m.name).bind(&m.kind).bind(&m.unit).bind(m.scale_min).bind(m.scale_max)
    .bind(&m.aggregate).bind(&m.reminder_time).bind(&m.last_reminded).bind(m.archived).bind(&m.position)
    .bind(&m.created_at).bind(&m.updated_at).bind(&m.deleted_at).bind(m.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct MetricEntry {
    pub id: String,
    pub user_id: String,
    pub metric_id: String,
    /// The logical day it counts for.
    pub date: String,
    /// When it was logged.
    pub at: String,
    pub value: f64,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_metric_entry(conn: &mut SqliteConnection, e: &MetricEntry) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO metric_entries (id, user_id, metric_id, date, at, value, note, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET date=excluded.date, at=excluded.at, value=excluded.value, note=excluded.note,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&e.id).bind(&e.user_id).bind(&e.metric_id).bind(&e.date).bind(&e.at).bind(e.value).bind(&e.note)
    .bind(&e.created_at).bind(&e.updated_at).bind(&e.deleted_at).bind(e.rev)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A milestone of a goal.
#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, utoipa::ToSchema, PartialEq)]
#[ts(export)]
pub struct Milestone {
    pub id: String,
    pub title: String,
    pub due_date: Option<String>,
    pub done: bool,
}

/// A long-term goal (SPEC §6.10).
#[derive(Debug, Clone, Serialize, FromRow, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Goal {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_group_id: Option<String>,
    pub title: String,
    pub description: String,
    pub target_date: Option<String>,
    #[ts(type = "'active' | 'paused' | 'achieved' | 'dropped'")]
    pub status: String,
    /// 0–1, set by hand; `null` = derived from milestones and linked work.
    pub progress_override: Option<f64>,
    #[ts(as = "Vec<Milestone>")]
    #[schema(value_type = Vec<Milestone>)]
    pub milestones: sqlx::types::Json<Vec<Milestone>>,
    #[ts(as = "Vec<String>")]
    #[schema(value_type = Vec<String>)]
    pub project_ids: sqlx::types::Json<Vec<String>>,
    #[ts(as = "Vec<String>")]
    #[schema(value_type = Vec<String>)]
    pub task_ids: sqlx::types::Json<Vec<String>>,
    pub position: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    #[ts(type = "number")]
    pub rev: i64,
}

pub async fn upsert_goal(conn: &mut SqliteConnection, g: &Goal) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO goals (id, owner_user_id, owner_group_id, title, description, target_date, status, progress_override, milestones,
           project_ids, task_ids, position, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET owner_user_id=excluded.owner_user_id, owner_group_id=excluded.owner_group_id, title=excluded.title,
           description=excluded.description, target_date=excluded.target_date, status=excluded.status, progress_override=excluded.progress_override,
           milestones=excluded.milestones, project_ids=excluded.project_ids, task_ids=excluded.task_ids, position=excluded.position,
           updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, rev=excluded.rev",
    )
    .bind(&g.id).bind(&g.owner_user_id).bind(&g.owner_group_id).bind(&g.title).bind(&g.description).bind(&g.target_date)
    .bind(&g.status).bind(g.progress_override).bind(&g.milestones).bind(&g.project_ids).bind(&g.task_ids).bind(&g.position)
    .bind(&g.created_at).bind(&g.updated_at).bind(&g.deleted_at).bind(g.rev)
    .execute(conn)
    .await
    .map(|_| ())
}
