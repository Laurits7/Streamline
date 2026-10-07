//! In-process change bus feeding the SSE endpoint. Every change carries the global
//! `rev`, the entity kind, the full entity, and the users allowed to see it.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use crate::models::{DayRecord, Goal, MetricDefinition, MetricEntry};
use crate::{
    models::{
        Calendar, CalendarAccountView, CalendarEvent, DayEntry, DayPlan, DayTemplate, EventProject,
        FocusSession, FocusTimer, Group, OccasionTemplate, Person, Place, Project, Series, Task,
        TaskTemplate, TimeBlock, WorkflowTemplate,
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
    /// A group changed: its members (and `extra`, e.g. someone just removed) hear about it.
    pub fn group(g: &Group, extra: &[String]) -> Self {
        let mut audience: Vec<String> = g.members.iter().map(|m| m.user_id.clone()).collect();
        audience.extend(extra.iter().cloned());
        Self {
            rev: g.rev,
            kind: "group",
            data: serde_json::to_value(g).unwrap(),
            audience,
        }
    }
    /// The user's group memberships changed: their clients resync (what they can see changed).
    pub fn membership(user_id: &str) -> Self {
        Self {
            rev: 0,
            kind: "membership",
            data: serde_json::Value::Null,
            audience: vec![user_id.to_string()],
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
    pub fn task_template(t: &TaskTemplate) -> Self {
        Self {
            rev: t.rev,
            kind: "task_template",
            data: serde_json::to_value(t).unwrap(),
            audience: visibility::audience(t.owner_user_id.as_deref(), t.owner_group_id.as_deref()),
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
    pub fn calendar(c: &Calendar) -> Self {
        Self {
            rev: c.rev,
            kind: "calendar",
            data: serde_json::to_value(c).unwrap(),
            audience: vec![c.user_id.clone()],
        }
    }
    pub fn event(e: &CalendarEvent) -> Self {
        Self {
            rev: e.rev,
            kind: "event",
            data: serde_json::to_value(e).unwrap(),
            audience: vec![e.user_id.clone()],
        }
    }
    /// The calendar account's settings or sync status changed (not part of the change feed).
    pub fn calendar_account(user_id: &str, a: Option<&CalendarAccountView>) -> Self {
        Self {
            rev: 0,
            kind: "calendar_account",
            data: serde_json::to_value(a).unwrap(),
            audience: vec![user_id.to_string()],
        }
    }
    pub fn day_template(t: &DayTemplate) -> Self {
        Self {
            rev: t.rev,
            kind: "day_template",
            data: serde_json::to_value(t).unwrap(),
            audience: vec![t.owner_user_id.clone()],
        }
    }
    pub fn time_block(b: &TimeBlock) -> Self {
        Self {
            rev: b.rev,
            kind: "time_block",
            data: serde_json::to_value(b).unwrap(),
            audience: vec![b.user_id.clone()],
        }
    }
    pub fn day_record(r: &DayRecord) -> Self {
        Self {
            rev: r.rev,
            kind: "day_record",
            data: serde_json::to_value(r).unwrap(),
            audience: vec![r.user_id.clone()],
        }
    }
    pub fn metric(m: &MetricDefinition) -> Self {
        Self {
            rev: m.rev,
            kind: "metric",
            data: serde_json::to_value(m).unwrap(),
            audience: vec![m.owner_user_id.clone()],
        }
    }
    pub fn metric_entry(e: &MetricEntry) -> Self {
        Self {
            rev: e.rev,
            kind: "metric_entry",
            data: serde_json::to_value(e).unwrap(),
            audience: vec![e.user_id.clone()],
        }
    }
    pub fn goal(g: &Goal) -> Self {
        Self {
            rev: g.rev,
            kind: "goal",
            data: serde_json::to_value(g).unwrap(),
            audience: visibility::audience(g.owner_user_id.as_deref(), g.owner_group_id.as_deref()),
        }
    }
    pub fn event_project(e: &EventProject) -> Self {
        Self {
            rev: e.rev,
            kind: "event_project",
            data: serde_json::to_value(e).unwrap(),
            audience: vec![e.user_id.clone()],
        }
    }
    pub fn person(p: &Person) -> Self {
        Self {
            rev: p.rev,
            kind: "person",
            data: serde_json::to_value(p).unwrap(),
            audience: vec![p.owner_user_id.clone()],
        }
    }
    pub fn occasion_template(t: &OccasionTemplate) -> Self {
        Self {
            rev: t.rev,
            kind: "occasion_template",
            data: serde_json::to_value(t).unwrap(),
            audience: vec![t.owner_user_id.clone()],
        }
    }
    /// The instance's nameday calendar was (re)loaded: clients refetch it.
    pub fn namedays(user_ids: Vec<String>) -> Self {
        Self {
            rev: 0,
            kind: "namedays",
            data: serde_json::Value::Null,
            audience: user_ids,
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
