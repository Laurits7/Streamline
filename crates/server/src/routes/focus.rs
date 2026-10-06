//! The focus (Pomodoro) timer (SPEC §6.9). One timer per user, kept on the server so
//! every device shows the same countdown; finished intervals become focus sessions and
//! work time is added to the task's actual time.

use axum::{Json, extract::State};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use streamline_domain::focus::{Action, Phase, Session, SessionKind, Timer};
use ts_rs::TS;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, bad},
    events::Change,
    models::{FocusSession, FocusTimer, Task, User, upsert_task},
    notify::{self, Notification},
    util::{new_id, now},
};

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct FocusState {
    pub timer: FocusTimer,
    /// The server's clock (Unix ms), so clients can correct for clock differences.
    #[ts(type = "number")]
    pub server_now: i64,
}

#[derive(sqlx::FromRow)]
struct TimerRow {
    task_id: Option<String>,
    phase: String,
    running_since: Option<String>,
    elapsed_ms: i64,
    length_min: i32,
    cycle_done: i32,
    rev: i64,
}

fn ms(ts: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn to_api(t: &Timer, rev: i64) -> FocusTimer {
    FocusTimer {
        task_id: t.task_id.clone(),
        phase: t.phase.as_str().into(),
        running_since_ms: t.running_since,
        elapsed_ms: t.elapsed_ms,
        length_min: t.length_min as i32,
        cycle_done: t.cycle_done as i32,
        rev,
    }
}

async fn load(conn: &mut SqliteConnection, user_id: &str) -> sqlx::Result<(Timer, i64)> {
    let row: Option<TimerRow> = sqlx::query_as("SELECT * FROM focus_timers WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(conn)
        .await?;
    Ok(match row {
        None => (Timer::idle(), 0),
        Some(r) => (
            Timer {
                phase: Phase::parse(&r.phase).unwrap_or(Phase::Idle),
                task_id: r.task_id,
                running_since: r.running_since.as_deref().and_then(ms),
                elapsed_ms: r.elapsed_ms,
                length_min: r.length_min.max(0) as u32,
                cycle_done: r.cycle_done.max(0) as u32,
            },
            r.rev,
        ),
    })
}

pub async fn current(state: &AppState, user_id: &str) -> sqlx::Result<FocusTimer> {
    let (t, rev) = load(&mut *state.db.read.acquire().await?, user_id).await?;
    Ok(to_api(&t, rev))
}

/// Apply a change to a user's timer inside one transaction: save it, log finished
/// sessions, add work minutes to tasks, and broadcast everything.
async fn transition(
    state: &AppState,
    user: &User,
    f: impl FnOnce(Timer, i64) -> (Timer, Vec<Session>),
) -> anyhow::Result<(FocusTimer, Timer, Timer)> {
    let mut tx = state.db.write.begin().await?;
    let (before, old_rev) = load(&mut tx, &user.id).await?;
    let now_ms = Utc::now().timestamp_millis();
    let (after, sessions) = f(before.clone(), now_ms);
    if after == before && sessions.is_empty() {
        tx.rollback().await?;
        return Ok((to_api(&after, old_rev), before, after));
    }
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    sqlx::query(
        "INSERT INTO focus_timers (user_id, task_id, phase, running_since, elapsed_ms, length_min, cycle_done, updated_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?)
         ON CONFLICT(user_id) DO UPDATE SET task_id=excluded.task_id, phase=excluded.phase, running_since=excluded.running_since,
           elapsed_ms=excluded.elapsed_ms, length_min=excluded.length_min, cycle_done=excluded.cycle_done,
           updated_at=excluded.updated_at, rev=excluded.rev",
    )
    .bind(&user.id)
    .bind(&after.task_id)
    .bind(after.phase.as_str())
    .bind(after.running_since.map(iso))
    .bind(after.elapsed_ms)
    .bind(after.length_min as i32)
    .bind(after.cycle_done as i32)
    .bind(&ts)
    .bind(rev)
    .execute(&mut *tx)
    .await?;

    let mut changes = vec![];
    for s in &sessions {
        let fs = FocusSession {
            id: new_id(),
            user_id: user.id.clone(),
            task_id: s.task_id.clone(),
            kind: if s.kind == SessionKind::Work {
                "work"
            } else {
                "break"
            }
            .into(),
            started_at: iso(s.started_ms),
            ended_at: iso(s.ended_ms),
            minutes: s.minutes as i32,
            completed: s.completed,
            created_at: ts.clone(),
            updated_at: ts.clone(),
            deleted_at: None,
            rev,
        };
        sqlx::query(
            "INSERT INTO focus_sessions (id, user_id, task_id, kind, started_at, ended_at, minutes, completed, created_at, updated_at, rev)
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&fs.id)
        .bind(&fs.user_id)
        .bind(&fs.task_id)
        .bind(&fs.kind)
        .bind(&fs.started_at)
        .bind(&fs.ended_at)
        .bind(fs.minutes)
        .bind(fs.completed)
        .bind(&fs.created_at)
        .bind(&fs.updated_at)
        .bind(fs.rev)
        .execute(&mut *tx)
        .await?;
        changes.push(Change::focus_session(&fs));
        // Work time counts towards the task's actual time (SPEC §6.6).
        if s.kind == SessionKind::Work
            && s.minutes > 0
            && let Some(task_id) = &s.task_id
            && let Some(mut t) =
                sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL")
                    .bind(task_id)
                    .fetch_optional(&mut *tx)
                    .await?
        {
            t.actual_min += s.minutes as i32;
            t.updated_at = ts.clone();
            t.rev = rev;
            upsert_task(&mut tx, &t).await?;
            changes.push(Change::task(&t));
        }
    }
    // Starting work on an open task marks it in progress.
    if after.phase == Phase::Work
        && after.running_since.is_some()
        && let Some(task_id) = &after.task_id
        && let Some(mut t) = sqlx::query_as::<_, Task>(
            "SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL AND status = 'open' AND started_at IS NULL",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
    {
        t.started_at = Some(ts.clone());
        t.updated_at = ts.clone();
        t.rev = rev;
        upsert_task(&mut tx, &t).await?;
        changes.push(Change::task(&t));
    }
    tx.commit().await?;
    let api = to_api(&after, rev);
    changes.push(Change::focus_timer(&user.id, &api));
    state.bus.publish(changes);
    Ok((api, before, after))
}

#[utoipa::path(get, path = "/focus", tag = "focus", summary = "The focus timer", responses((status = 200, body = FocusState), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn get_focus(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<FocusState>> {
    // Complete anything already due before answering.
    let settings = user.user.focus_settings();
    let (timer, _, _) = transition(&state, &user.user, |t, now| t.advance(now, &settings)).await?;
    Ok(Json(FocusState {
        timer,
        server_now: Utc::now().timestamp_millis(),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FocusAction {
    /// `start` (with optional `task_id`), `pause`, `resume`, `skip`, `stop`, or `sync`
    /// (just complete whatever is due, e.g. when a client's countdown reaches zero).
    action: String,
    task_id: Option<String>,
}

#[utoipa::path(post, path = "/focus", tag = "focus", summary = "Control the focus timer", request_body = FocusAction, responses((status = 200, body = FocusState), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn post_focus(
    State(state): State<AppState>,
    user: AuthUser,
    Json(a): Json<FocusAction>,
) -> ApiResult<Json<FocusState>> {
    let action = match a.action.as_str() {
        "start" => {
            if let Some(id) = &a.task_id {
                crate::routes::tasks::load_visible(&mut *state.db.read.acquire().await?, &user, id)
                    .await
                    .map_err(|_| bad("unknown task"))?;
            }
            Some(Action::Start { task_id: a.task_id })
        }
        "pause" => Some(Action::Pause),
        "resume" => Some(Action::Resume),
        "skip" => Some(Action::Skip),
        "stop" => Some(Action::Stop),
        "sync" => None,
        _ => return Err(bad("unknown action")),
    };
    let settings = user.user.focus_settings();
    let (timer, _, _) = transition(&state, &user.user, |t, now| match action {
        Some(action) => t.apply(action, now, &settings),
        None => t.advance(now, &settings),
    })
    .await?;
    Ok(Json(FocusState {
        timer,
        server_now: Utc::now().timestamp_millis(),
    }))
}

/// Background job: complete intervals that ran out while nobody was looking, and
/// tell the user (in-app) that a break or the next work interval is due.
pub async fn advance_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> = sqlx::query_as(
        "SELECT u.* FROM users u JOIN focus_timers f ON f.user_id = u.id
         WHERE u.deleted_at IS NULL AND f.running_since IS NOT NULL",
    )
    .fetch_all(&state.db.read)
    .await?;
    for u in users {
        let settings = u.focus_settings();
        let (_, before, after) = transition(state, &u, |t, now| t.advance(now, &settings)).await?;
        if after.phase != before.phase {
            let (title, body) = if after.phase.is_break() {
                (
                    "Time for a break",
                    format!("{} minutes. Step away for a moment.", after.length_min),
                )
            } else {
                (
                    "Break's over",
                    "Ready for the next focus interval?".to_string(),
                )
            };
            notify::send(
                state,
                &u.id,
                &Notification {
                    kind: "focus".into(),
                    title: title.into(),
                    body,
                    url: "/focus".into(),
                },
            );
        }
    }
    Ok(())
}
