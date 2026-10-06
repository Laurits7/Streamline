//! A day's activity log (owner request): what happened during a logical day, built from
//! what is already recorded (task events, focus sessions, planning). Meant for looking
//! back and reflecting (SPEC §6.2b).

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::SecondsFormat;
use serde::Serialize;
use streamline_domain::time::{day_bounds, parse_hhmm, parse_tz};
use ts_rs::TS;

use crate::{AppState, auth::AuthUser, error::ApiResult, util::parse_date};

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct LogItem {
    /// When it happened (UTC). Missed items use the end of the day.
    pub at: String,
    /// `completed`, `skipped`, `wont_do`, `missed`, `started`, `created`, `focus`, `planned`.
    pub kind: String,
    pub task_id: Option<String>,
    pub title: String,
    /// Main project path of the task, if any.
    pub project: Option<String>,
    /// Focus minutes.
    pub minutes: Option<i32>,
    /// The entry as a sentence, e.g. `Completed “Fix tap” (House)`.
    pub text: String,
}

#[derive(sqlx::FromRow)]
struct Row {
    at: String,
    kind: String,
    task_id: Option<String>,
    title: String,
    project: Option<String>,
    minutes: Option<i32>,
    extra: Option<String>,
}

fn sentence(r: &Row) -> String {
    let what = match &r.project {
        Some(p) => format!("“{}” ({p})", r.title),
        None => format!("“{}”", r.title),
    };
    match r.kind.as_str() {
        "completed" => format!("Completed {what}"),
        "skipped" => format!("Skipped {what}"),
        "wont_do" => format!("Decided not to do {what}"),
        "missed" => format!("Missed {what}"),
        "started" => format!("Started {what}"),
        "created" => format!("Added {what}"),
        "focus" => match r.extra.as_deref() {
            Some("1") => format!("Focused {} min on {what}", r.minutes.unwrap_or(0)),
            _ => format!(
                "Focused {} min on {what} (stopped early)",
                r.minutes.unwrap_or(0)
            ),
        },
        "planned" => format!("Planned {}", r.title),
        _ => what,
    }
}

#[utoipa::path(get, path = "/days/{date}/log", tag = "days", summary = "What happened during a day, in time order", params(("date" = String, Path, description = "YYYY-MM-DD")), responses((status = 200, body = Vec<LogItem>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn get_log(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
) -> ApiResult<Json<Vec<LogItem>>> {
    let day = parse_date(&date)?;
    let tz = parse_tz(&user.user.timezone).unwrap_or(chrono_tz::UTC);
    let day_end = parse_hhmm(&user.user.day_end).unwrap_or_default();
    let (start, end) = day_bounds(day, tz, day_end);
    let (start, end) = (
        start.to_rfc3339_opts(SecondsFormat::Millis, true),
        end.to_rfc3339_opts(SecondsFormat::Millis, true),
    );
    // Main-project name of a task (subproject paths are shown by the client from ids).
    const PROJECT: &str = "(SELECT name FROM projects p WHERE p.id = t.project_id)";
    let sql = format!(
        "SELECT e.at AS at, e.kind AS kind, t.id AS task_id, t.title AS title, {PROJECT} AS project, NULL AS minutes, NULL AS extra
           FROM task_events e JOIN tasks t ON t.id = e.task_id
          WHERE (t.owner_user_id = ?1 OR e.user_id = ?1) AND t.deleted_at IS NULL AND e.at >= ?2 AND e.at < ?3
            AND e.kind IN ('completed', 'skipped', 'wont_do')
            AND NOT EXISTS (SELECT 1 FROM task_events r WHERE r.task_id = e.task_id AND r.kind = 'reopened' AND r.at > e.at)
         UNION ALL
         SELECT ?3, 'missed', t.id, t.title, {PROJECT}, NULL, NULL
           FROM task_events e JOIN tasks t ON t.id = e.task_id
          WHERE t.owner_user_id = ?1 AND t.deleted_at IS NULL AND e.kind = 'missed'
            AND (json_extract(e.data, '$.date') = ?4 OR json_extract(e.data, '$.window_end') = ?4)
         UNION ALL
         SELECT t.started_at, 'started', t.id, t.title, {PROJECT}, NULL, NULL
           FROM tasks t
          WHERE t.owner_user_id = ?1 AND t.deleted_at IS NULL AND t.started_at >= ?2 AND t.started_at < ?3
         UNION ALL
         SELECT t.created_at, 'created', t.id, t.title, {PROJECT}, NULL, NULL
           FROM tasks t
          WHERE t.owner_user_id = ?1 AND t.deleted_at IS NULL AND t.series_id IS NULL
            AND t.created_at >= ?2 AND t.created_at < ?3
         UNION ALL
         SELECT f.started_at, 'focus', t.id, COALESCE(t.title, 'no task'), {PROJECT}, f.minutes, CAST(f.completed AS TEXT)
           FROM focus_sessions f LEFT JOIN tasks t ON t.id = f.task_id
          WHERE f.user_id = ?1 AND f.deleted_at IS NULL AND f.kind = 'work' AND f.started_at >= ?2 AND f.started_at < ?3
         UNION ALL
         SELECT d.planned_at, 'planned', NULL, CASE WHEN d.date > ?4 THEN 'the next day (' || d.date || ')' ELSE 'the day' END, NULL, NULL, NULL
           FROM day_plans d
          WHERE d.user_id = ?1 AND d.deleted_at IS NULL AND d.status = 'planned' AND d.planned_at >= ?2 AND d.planned_at < ?3
         ORDER BY at"
    );
    let rows: Vec<Row> = sqlx::query_as(&sql)
        .bind(user.id())
        .bind(&start)
        .bind(&end)
        .bind(&date)
        .fetch_all(&state.db.read)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| LogItem {
                text: sentence(&r),
                at: r.at,
                kind: r.kind,
                task_id: r.task_id,
                title: r.title,
                project: r.project,
                minutes: r.minutes,
            })
            .collect(),
    ))
}
