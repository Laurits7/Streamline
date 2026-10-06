//! Notifications to users. Phase 2a delivers them in-app (over the live event stream);
//! Web Push and ntfy (SPEC §6.2d, D-5) become additional channels here in Phase 6, so
//! callers don't change.

use serde::Serialize;
use ts_rs::TS;

use crate::{AppState, events::Change};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Notification {
    /// e.g. `plan_evening`, `plan_morning`.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// In-app path to open, e.g. `/plan/2026-10-07`.
    pub url: String,
}

pub fn send(state: &AppState, user_id: &str, n: &Notification) {
    state.bus.publish([Change::notification(
        user_id,
        serde_json::to_value(n).unwrap_or_default(),
    )]);
}
