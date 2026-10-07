//! Notifications to users: in-app (over the live event stream) and, since Phase 6, Web
//! Push to the user's devices and ntfy (SPEC §6.2d, D-5, D-62). Users can turn kinds off.

use serde::Serialize;
use ts_rs::TS;

use crate::{AppState, events::Change};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Notification {
    /// e.g. `plan_evening`, `plan_morning`, `ready`, `check_back`, `focus`, `conflict`, `metric`.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// In-app path to open, e.g. `/plan/2026-10-07`.
    pub url: String,
}

/// The setting a notification kind belongs to (what users switch on and off).
pub fn group_of(kind: &str) -> &str {
    match kind {
        "plan_evening" | "plan_morning" => "planning",
        // A waited-for task needing a look is "ready" again (D-70).
        "check_back" => "ready",
        k => k,
    }
}

pub const GROUPS: [&str; 5] = ["planning", "ready", "focus", "conflict", "metric"];

pub fn send(state: &AppState, user_id: &str, n: &Notification) {
    let (state, user_id, n) = (state.clone(), user_id.to_string(), n.clone());
    tokio::spawn(async move {
        let off: Option<String> = sqlx::query_scalar("SELECT notify_off FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_optional(&state.db.read)
            .await
            .ok()
            .flatten();
        let off: Vec<String> = off
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if n.kind != "test" && off.iter().any(|k| k == group_of(&n.kind)) {
            return;
        }
        state.bus.publish([Change::notification(
            &user_id,
            serde_json::to_value(&n).unwrap_or_default(),
        )]);
        if let Err(e) = crate::push::deliver(&state, &user_id, &n).await {
            tracing::warn!("push delivery failed: {e:#}");
        }
    });
}
