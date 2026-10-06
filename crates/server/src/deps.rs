//! Keeping `tasks.blocked` / `ready_at` in step with prerequisites (SPEC §6.3b). The
//! graph rules live in `domain::deps`.

use chrono::{Duration, SecondsFormat, Utc};
use sqlx::SqliteConnection;
use streamline_domain::deps::{is_blocked, prereq_state};

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{Task, log_task_event, upsert_task},
    notify::{self, Notification},
    util::now,
};

/// Recompute whether `t` is blocked. Returns true if it changed (the caller saves it).
/// When the last prerequisite is resolved and the task has a wait time, it becomes ready
/// only after that wait.
pub async fn refresh(conn: &mut SqliteConnection, t: &mut Task) -> sqlx::Result<bool> {
    let mut states = vec![];
    for id in t.depends_on.0.iter() {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM tasks WHERE id = ? AND deleted_at IS NULL")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        states.push(prereq_state(status.as_deref()));
    }
    let blocked = is_blocked(&states);
    if blocked == t.blocked {
        return Ok(false);
    }
    t.blocked = blocked;
    t.ready_at = match (blocked, t.wait_min) {
        (false, Some(m)) if m > 0 => Some(
            (Utc::now() + Duration::minutes(i64::from(m)))
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        ),
        _ => None,
    };
    Ok(true)
}

/// After `task_id` changed status or was deleted: update the tasks that wait for it.
pub async fn refresh_dependents(
    conn: &mut SqliteConnection,
    task_id: &str,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM tasks WHERE deleted_at IS NULL AND status = 'open'
           AND EXISTS (SELECT 1 FROM json_each(depends_on) WHERE value = ?)",
    )
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?;
    for id in ids {
        let mut t: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(&id)
            .fetch_one(&mut *conn)
            .await?;
        if refresh(conn, &mut t).await? {
            t.updated_at = now();
            t.rev = next_rev(conn).await?;
            upsert_task(conn, &t).await?;
            let kind = if t.blocked { "blocked" } else { "unblocked" };
            log_task_event(
                conn,
                &t.id,
                None,
                kind,
                Some(serde_json::json!({"prerequisite": task_id})),
            )
            .await?;
            changes.push(Change::task(&t));
        }
    }
    Ok(())
}

/// Job: tasks whose wait time is over become ready; tell their owner.
pub async fn release_waiting(state: &AppState) -> anyhow::Result<()> {
    let due: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE ready_at IS NOT NULL AND ready_at <= ? AND deleted_at IS NULL",
    )
    .bind(now())
    .fetch_all(&state.db.read)
    .await?;
    if due.is_empty() {
        return Ok(());
    }
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    let mut ready = vec![];
    for mut t in due {
        t.ready_at = None;
        t.updated_at = now();
        t.rev = next_rev(&mut tx).await?;
        upsert_task(&mut tx, &t).await?;
        changes.push(Change::task(&t));
        if t.status == "open" {
            ready.push(t);
        }
    }
    tx.commit().await?;
    state.bus.publish(changes);
    for t in ready {
        if let Some(owner) = &t.owner_user_id {
            notify::send(
                state,
                owner,
                &Notification {
                    kind: "ready".into(),
                    title: format!("“{}” is ready", t.title),
                    body: String::new(),
                    url: "/".into(),
                },
            );
        }
    }
    Ok(())
}
