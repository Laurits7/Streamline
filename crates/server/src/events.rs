//! In-process change bus feeding the SSE endpoint. Every change carries the global
//! `rev`, the entity kind, the full entity, and the users allowed to see it.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use crate::{
    models::{DayEntry, Project, Task},
    visibility,
};

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub rev: i64,
    pub kind: &'static str,
    pub data: serde_json::Value,
    #[serde(skip)]
    pub audience: Vec<String>,
}

impl Change {
    pub fn task(t: &Task) -> Self {
        Self {
            rev: t.rev,
            kind: "task",
            data: serde_json::to_value(t).unwrap(),
            audience: visibility::audience(t.owner_user_id.as_deref(), t.owner_group_id.as_deref()),
        }
    }
    pub fn project(p: &Project) -> Self {
        Self {
            rev: p.rev,
            kind: "project",
            data: serde_json::to_value(p).unwrap(),
            audience: visibility::audience(p.owner_user_id.as_deref(), p.owner_group_id.as_deref()),
        }
    }
    pub fn entry(e: &DayEntry) -> Self {
        Self {
            rev: e.rev,
            kind: "day_entry",
            data: serde_json::to_value(e).unwrap(),
            audience: vec![e.user_id.clone()],
        }
    }
    /// Tell a user's other sessions that their profile/settings changed.
    pub fn me(user_id: &str, data: serde_json::Value) -> Self {
        Self {
            rev: 0,
            kind: "me",
            data,
            audience: vec![user_id.to_string()],
        }
    }
}

pub struct Bus {
    tx: broadcast::Sender<Arc<Change>>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(1024).0,
        }
    }

    pub fn publish(&self, changes: impl IntoIterator<Item = Change>) {
        for c in changes {
            // Err only means nobody is listening.
            let _ = self.tx.send(Arc::new(c));
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Change>> {
        self.tx.subscribe()
    }
}
