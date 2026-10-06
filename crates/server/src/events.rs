//! In-process change bus feeding the SSE endpoint. Every change carries the global
//! `rev`, the entity kind, the full entity, and the users allowed to see it.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use crate::{
    models::{
        DayEntry, DayPlan, FocusSession, FocusTimer, Place, Project, Series, Task, WorkflowTemplate,
    },
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
    pub fn day_plan(p: &DayPlan) -> Self {
        Self {
            rev: p.rev,
            kind: "day_plan",
            data: serde_json::to_value(p).unwrap(),
            audience: vec![p.user_id.clone()],
        }
    }
    pub fn workflow(w: &WorkflowTemplate) -> Self {
        Self {
            rev: w.rev,
            kind: "workflow",
            data: serde_json::to_value(w).unwrap(),
            audience: visibility::audience(w.owner_user_id.as_deref(), w.owner_group_id.as_deref()),
        }
    }
    pub fn place(p: &Place) -> Self {
        Self {
            rev: p.rev,
            kind: "place",
            data: serde_json::to_value(p).unwrap(),
            audience: visibility::audience(p.owner_user_id.as_deref(), p.owner_group_id.as_deref()),
        }
    }
    pub fn series(s: &Series) -> Self {
        Self {
            rev: s.rev,
            kind: "series",
            data: serde_json::to_value(s).unwrap(),
            audience: visibility::audience(s.owner_user_id.as_deref(), s.owner_group_id.as_deref()),
        }
    }
    pub fn focus_timer(user_id: &str, t: &FocusTimer) -> Self {
        Self {
            rev: t.rev,
            kind: "focus_timer",
            data: serde_json::to_value(t).unwrap(),
            audience: vec![user_id.to_string()],
        }
    }
    pub fn focus_session(s: &FocusSession) -> Self {
        Self {
            rev: s.rev,
            kind: "focus_session",
            data: serde_json::to_value(s).unwrap(),
            audience: vec![s.user_id.clone()],
        }
    }
    /// Something to show the user now (e.g. a planning reminder). Not part of the change feed.
    pub fn notification(user_id: &str, data: serde_json::Value) -> Self {
        Self {
            rev: 0,
            kind: "notification",
            data,
            audience: vec![user_id.to_string()],
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
