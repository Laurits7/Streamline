//! Background jobs on a single interval tick. All jobs are idempotent.

use std::time::Duration;

use crate::AppState;

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            // Occurrences first, so a missed routine day is recorded by the rollover.
            if let Err(e) = crate::routines::materialize_all(&state).await {
                tracing::warn!("routines job failed: {e:#}");
            }
            if let Err(e) = crate::rollover::run_all(&state).await {
                tracing::warn!("rollover job failed: {e:#}");
            }
            if let Err(e) = crate::reminders::run_all(&state).await {
                tracing::warn!("reminder job failed: {e:#}");
            }
            if let Err(e) = crate::deps::release_waiting(&state).await {
                tracing::warn!("wait-time job failed: {e:#}");
            }
            if let Err(e) = crate::routes::focus::advance_all(&state).await {
                tracing::warn!("focus job failed: {e:#}");
            }
            // Drop expired sessions.
            let _ = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
                .bind(crate::util::now())
                .execute(&state.db.write)
                .await;
        }
    });
}
