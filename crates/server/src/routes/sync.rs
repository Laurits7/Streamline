//! Change feed (`/sync?since=rev`) and the realtime SSE stream (`/events`).
//! Clients do a full sync once, then apply SSE changes, and after any reconnect
//! call `/sync?since=<last rev>` to catch up.

use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use ts_rs::TS;

use crate::models::{DayRecord, Goal, MetricDefinition, MetricEntry};
use crate::{
    AppState,
    auth::AuthUser,
    db::current_rev,
    error::ApiResult,
    models::{
        Calendar, CalendarAccountView, CalendarEvent, DayEntry, DayPlan, DayTemplate, FocusSession,
        FocusTimer, Group, Me, OccasionTemplate, Person, Place, Project, Series, Task, TaskType,
        TimeBlock, WorkflowTemplate,
    },
    rollover,
};

/// How far back a full sync reaches for finished tasks and past day entries.
const HISTORY_DAYS: i64 = 30;

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SyncQuery {
    since: Option<i64>,
}

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct SyncResponse {
    #[ts(type = "number")]
    pub rev: i64,
    pub full: bool,
    pub me: Me,
    pub today: String,
    pub task_types: Vec<TaskType>,
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub day_entries: Vec<DayEntry>,
    pub day_plans: Vec<DayPlan>,
    pub focus_timer: FocusTimer,
    pub focus_sessions: Vec<FocusSession>,
    pub series: Vec<Series>,
    pub places: Vec<Place>,
    pub workflows: Vec<WorkflowTemplate>,
    /// Your connected calendar account, if any (always current).
    pub calendar_account: Option<CalendarAccountView>,
    pub calendars: Vec<Calendar>,
    /// Calendar event instances (a full sync covers the last 30 days onwards).
    pub events: Vec<CalendarEvent>,
    pub day_templates: Vec<DayTemplate>,
    /// Time blocks (a full sync covers the last 30 days onwards).
    pub time_blocks: Vec<TimeBlock>,
    /// Reflections (a full sync covers the last 30 days onwards).
    pub day_records: Vec<DayRecord>,
    pub metrics: Vec<MetricDefinition>,
    /// Logged values (a full sync covers the last 400 days).
    pub metric_entries: Vec<MetricEntry>,
    /// Your goals and your groups' goals.
    pub goals: Vec<Goal>,
    /// People whose namedays and birthdays matter to you.
    pub people: Vec<Person>,
    /// What each kind of occasion creates (always complete).
    pub occasion_templates: Vec<OccasionTemplate>,
    /// Your groups with their members (always complete).
    pub groups: Vec<Group>,
    /// The server's clock (Unix ms), so clients can correct for clock differences.
    #[ts(type = "number")]
    pub server_now: i64,
}

#[utoipa::path(get, path = "/sync", tag = "sync", summary = "Everything visible to you, or only what changed after `since`", params(SyncQuery), responses((status = 200, body = SyncResponse), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn sync(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<SyncQuery>,
) -> ApiResult<Json<SyncResponse>> {
    crate::routines::materialize_user(&state, &user.user).await?;
    crate::occasions::materialize_user(&state, &user.user).await?;
    crate::blocks::materialize_user(&state, &user.user).await?;
    rollover::run_for_user(&state, &user.user).await?;
    let since = q.since.unwrap_or(0).max(0);
    let full = since == 0;
    let db = &state.db.read;
    // Read the counter first: anything committed afterwards is re-sent next time (harmless).
    let rev = current_rev(db).await?;
    let today = rollover::today_for(&user.user);
    let cutoff_date = (today - chrono::Duration::days(HISTORY_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let cutoff_ts = (chrono::Utc::now() - chrono::Duration::days(HISTORY_DAYS)).to_rfc3339();

    let task_types = sqlx::query_as(
        "SELECT * FROM task_types WHERE (builtin = 1 OR owner_user_id = ?1) AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let projects = sqlx::query_as(
        "SELECT * FROM projects WHERE (owner_user_id = ?1 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1)) AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let tasks = sqlx::query_as(
        "SELECT * FROM tasks WHERE (owner_user_id = ?1 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1)) AND (
            ?2 = 0 AND deleted_at IS NULL AND (status = 'open' OR updated_at >= ?3)
            OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_ts)
    .fetch_all(db)
    .await?;
    let day_entries = sqlx::query_as(
        "SELECT * FROM day_entries WHERE user_id = ?1 AND (
            ?2 = 0 AND deleted_at IS NULL AND date >= ?3
            OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_date)
    .fetch_all(db)
    .await?;

    let day_plans = sqlx::query_as(
        "SELECT * FROM day_plans WHERE user_id = ?1 AND (
            ?2 = 0 AND deleted_at IS NULL AND date >= ?3
            OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_date)
    .fetch_all(db)
    .await?;

    let focus_sessions = sqlx::query_as(
        "SELECT * FROM focus_sessions WHERE user_id = ?1 AND (
            ?2 = 0 AND deleted_at IS NULL AND started_at >= ?3
            OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_ts)
    .fetch_all(db)
    .await?;
    let focus_timer = crate::routes::focus::current(&state, user.id()).await?;
    let groups =
        crate::routes::groups::visible_groups(&mut *state.db.read.acquire().await?, &user).await?;
    let workflows = sqlx::query_as(
        "SELECT * FROM workflow_templates WHERE owner_user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let places = sqlx::query_as(
        "SELECT * FROM places WHERE owner_user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let series = sqlx::query_as(
        "SELECT * FROM series WHERE (owner_user_id = ?1 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1)) AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;

    let calendar_account =
        crate::calsync::account_for(&mut *state.db.read.acquire().await?, user.id())
            .await?
            .map(|a| CalendarAccountView::from(&a));
    let calendars = sqlx::query_as(
        "SELECT * FROM calendars WHERE user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let events = sqlx::query_as(
        "SELECT * FROM events WHERE user_id = ?1 AND (
            ?2 = 0 AND deleted_at IS NULL AND (all_day = 0 AND end_at >= ?3 OR all_day = 1 AND end_date >= ?4)
            OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_ts)
    .bind(&cutoff_date)
    .fetch_all(db)
    .await?;

    let day_templates = sqlx::query_as(
        "SELECT * FROM day_templates WHERE owner_user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2) ORDER BY position",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let time_blocks = sqlx::query_as(
        "SELECT * FROM time_blocks WHERE user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL AND date >= ?3 OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_date)
    .fetch_all(db)
    .await?;
    {
        let mut tx = state.db.write.begin().await?;
        let mut changes = vec![];
        crate::tracking::ensure_builtins(&mut tx, user.id(), &mut changes).await?;
        tx.commit().await?;
    }
    let day_records = sqlx::query_as(
        "SELECT * FROM day_records WHERE user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL AND date >= ?3 OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&cutoff_date)
    .fetch_all(db)
    .await?;
    let metrics = sqlx::query_as(
        "SELECT * FROM metric_definitions WHERE owner_user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2) ORDER BY position",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let year_ago = (today - chrono::Duration::days(400))
        .format("%Y-%m-%d")
        .to_string();
    let metric_entries = sqlx::query_as(
        "SELECT * FROM metric_entries WHERE user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL AND date >= ?3 OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .bind(&year_ago)
    .fetch_all(db)
    .await?;
    let goals = sqlx::query_as(
        "SELECT * FROM goals WHERE (owner_user_id = ?1 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1)) AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2) ORDER BY position",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let people = sqlx::query_as(
        "SELECT * FROM people WHERE owner_user_id = ?1 AND (?2 = 0 AND deleted_at IS NULL OR ?2 > 0 AND rev > ?2)",
    )
    .bind(user.id())
    .bind(since)
    .fetch_all(db)
    .await?;
    let occasion_templates = {
        let mut tx = state.db.write.begin().await?;
        let mut changes = vec![];
        let t = crate::occasions::templates(&mut tx, user.id(), &mut changes).await?;
        tx.commit().await?;
        t
    };

    Ok(Json(SyncResponse {
        rev,
        full,
        me: Me::from(&user.user),
        today: today.format("%Y-%m-%d").to_string(),
        task_types,
        projects,
        tasks,
        day_entries,
        day_plans,
        focus_timer,
        focus_sessions,
        series,
        places,
        workflows,
        calendar_account,
        calendars,
        events,
        day_templates,
        time_blocks,
        day_records,
        metrics,
        metric_entries,
        goals,
        people,
        occasion_templates,
        groups,
        server_now: chrono::Utc::now().timestamp_millis(),
    }))
}

#[utoipa::path(get, path = "/today", tag = "sync", summary = "Your current logical date", responses((status = 200, body = Today), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn today(user: AuthUser) -> Json<Today> {
    Json(Today {
        date: rollover::today_for(&user.user)
            .format("%Y-%m-%d")
            .to_string(),
    })
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct Today {
    /// `YYYY-MM-DD`, taking timezone and day end into account.
    pub date: String,
}

#[utoipa::path(get, path = "/task-types", tag = "sync", summary = "Task types (end-of-day behaviour)", responses((status = 200, body = Vec<TaskType>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn task_types(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<TaskType>>> {
    let rows = sqlx::query_as(
        "SELECT * FROM task_types WHERE (builtin = 1 OR owner_user_id = ?) AND deleted_at IS NULL ORDER BY builtin DESC, name",
    )
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

/// Server-sent events: `change` events carry `{rev, kind, data}`; a `resync` event
/// means the client fell behind and should call `/sync?since=<last rev>`.
#[utoipa::path(get, path = "/events", tag = "sync", summary = "Server-sent events: `change` ({rev, kind, data}), `resync`, `hello`", responses((status = 200, body = String, content_type = "text/event-stream"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn events(
    State(state): State<AppState>,
    user: AuthUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Who this subscriber is for audience matching: the user, and each of their groups.
    // (When memberships change, clients get a `membership` event and reconnect.)
    let mut keys: std::collections::HashSet<String> = user
        .groups
        .iter()
        .map(|g| crate::visibility::group_key(g))
        .collect();
    keys.insert(user.user.id.clone());
    let stream = BroadcastStream::new(state.bus.subscribe()).filter_map(move |msg| {
        let out = match msg {
            Ok(c) if c.audience.iter().any(|a| keys.contains(a)) => Some(
                Event::default()
                    .event("change")
                    .id(c.rev.to_string())
                    .data(serde_json::to_string(&*c).unwrap_or_default()),
            ),
            Ok(_) => None,
            Err(BroadcastStreamRecvError::Lagged(_)) => {
                Some(Event::default().event("resync").data("1"))
            }
        };
        std::future::ready(out.map(Ok))
    });
    let hello = futures_util::stream::once(std::future::ready(Ok(Event::default()
        .event("hello")
        .data("1"))));
    Sse::new(hello.chain(stream)).keep_alive(KeepAlive::new().interval(Duration::from_secs(25)))
}
