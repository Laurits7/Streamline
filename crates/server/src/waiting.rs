//! Waiting for results (D-70): when a waiting task's check-back time passes, it stops
//! waiting (back to in progress, "check back now") and whoever set it waiting is told.

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{Task, log_task_event, upsert_task},
    notify::{self, Notification},
    util::now,
};

/// Job: end the waits whose check-back time has come. Idempotent: a task is picked up
/// once, since `waiting_since` is cleared (`check_back_at` stays as the marker).
pub async fn check_back(state: &AppState) -> anyhow::Result<()> {
    let ts = now();
    let due: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE waiting_since IS NOT NULL AND check_back_at IS NOT NULL
           AND check_back_at <= ? AND status = 'open' AND deleted_at IS NULL",
    )
    .bind(&ts)
    .fetch_all(&state.db.read)
    .await?;
    if due.is_empty() {
        return Ok(());
    }
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    let mut notices = vec![];
    for mut t in due {
        t.waiting_since = None;
        t.updated_at = ts.clone();
        t.rev = next_rev(&mut tx).await?;
        upsert_task(&mut tx, &t).await?;
        log_task_event(&mut tx, &t.id, None, "check_back", None).await?;
        changes.push(Change::task(&t));
        if let Some(who) = t
            .waiting_by
            .clone()
            .or_else(|| t.assignee_user_id.clone())
            .or_else(|| t.owner_user_id.clone())
        {
            notices.push((who, t));
        }
    }
    tx.commit().await?;
    state.bus.publish(changes);
    for (who, t) in notices {
        notify::send(
            state,
            &who,
            &Notification {
                kind: "check_back".into(),
                title: format!("Check back on “{}”", t.title),
                body: t.waiting_note.clone(),
                url: "/".into(),
            },
        );
    }
    Ok(())
}
