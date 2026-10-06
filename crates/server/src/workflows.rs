//! Starting workflow runs (SPEC §6.3b): one chain of tasks per selected variant, with
//! prerequisites and waits wired up. Used by "Start" and by routines (3b.6).

use sqlx::SqliteConnection;
use streamline_domain::{order::key_after, workflow};

use crate::{
    db::next_rev,
    events::Change,
    models::{DayEntry, Task, WorkflowTemplate, upsert_entry, upsert_task},
    util::{new_id, now},
};

/// Where the created tasks go and how they belong to a routine, if any.
#[derive(Default)]
pub struct RunOptions {
    pub project_id: Option<String>,
    /// Plan the first step of each chain into this day (at `start_time`, if given).
    pub day: Option<String>,
    pub start_time: Option<String>,
    /// For routine occurrences: (series id, occurrence key, occurrence date).
    pub occurrence: Option<(String, String, String)>,
    pub task_type_id: Option<String>,
    pub place_id: Option<String>,
    pub due_date: Option<String>,
}

/// Create the runs: for each chosen variant (or the whole template if it has none, or
/// none were chosen) a chain of tasks where each step waits for the previous one.
pub async fn start(
    conn: &mut SqliteConnection,
    owner_user_id: &str,
    tpl: &WorkflowTemplate,
    variant_ids: &[String],
    opts: &RunOptions,
) -> anyhow::Result<(Vec<Task>, Vec<Change>)> {
    let steps: Vec<workflow::Step> = tpl
        .steps
        .0
        .iter()
        .map(|s| workflow::Step {
            id: s.id.clone(),
            title: s.title.clone(),
            wait_min: s.wait_min.map(|m| m.max(0) as u32),
        })
        .collect();
    let chosen: Vec<_> = tpl
        .variants
        .0
        .iter()
        .filter(|v| variant_ids.contains(&v.id))
        .collect();
    let runs: Vec<(Option<&str>, Vec<String>)> = if chosen.is_empty() {
        vec![(None, vec![])]
    } else {
        chosen
            .iter()
            .map(|v| (Some(v.name.as_str()), v.skip.clone()))
            .collect()
    };
    let project_id = opts.project_id.clone().or(tpl.project_id.clone());
    let mut tasks = vec![];
    let mut changes = vec![];
    for (run_index, (variant, skip)) in runs.iter().enumerate() {
        let instance = new_id();
        let mut prev: Option<String> = None;
        for c in workflow::chain(&steps, skip) {
            let src = tpl.steps.0.iter().find(|s| s.id == c.step.id).unwrap();
            let rev = next_rev(conn).await?;
            let ts = now();
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM tasks WHERE owner_user_id = ? AND project_id IS ? AND deleted_at IS NULL",
            )
            .bind(owner_user_id)
            .bind(&project_id)
            .fetch_one(&mut *conn)
            .await?;
            let first = c.number == 1;
            let t = Task {
                id: new_id(),
                owner_user_id: Some(owner_user_id.into()),
                owner_group_id: None,
                assignee_user_id: None,
                project_id: project_id.clone(),
                title: match variant {
                    Some(v) => format!("{} ({v})", c.step.title),
                    None => c.step.title.clone(),
                },
                notes: if first {
                    tpl.description.clone()
                } else {
                    String::new()
                },
                status: "open".into(),
                position: key_after(last.as_deref()),
                due_date: if first { opts.due_date.clone() } else { None },
                estimate_min: src.estimate_min,
                difficulty: src.difficulty,
                importance: None,
                urgency: None,
                actual_min: 0,
                task_type_id: opts
                    .task_type_id
                    .clone()
                    .unwrap_or_else(|| "tt_carry_on".into()),
                carry_count: 0,
                started_at: None,
                completed_at: None,
                completed_by: None,
                ext_source: None,
                ext_id: None,
                ext_url: None,
                place_id: opts.place_id.clone(),
                also_project_ids: sqlx::types::Json(vec![]),
                depends_on: sqlx::types::Json(prev.iter().cloned().collect()),
                blocked: prev.is_some(),
                wait_min: c.wait_before_min.map(|m| m as i32),
                ready_at: None,
                workflow_instance_id: Some(instance.clone()),
                workflow_step: Some(c.number as i32),
                workflow_steps: Some(c.total as i32),
                series_id: opts.occurrence.as_ref().map(|o| o.0.clone()),
                occurrence_key: opts
                    .occurrence
                    .as_ref()
                    .map(|o| format!("{}/{run_index}/{}", o.1, c.number)),
                occurrence_date: opts.occurrence.as_ref().map(|o| o.2.clone()),
                window_end: None,
                created_at: ts.clone(),
                updated_at: ts.clone(),
                deleted_at: None,
                rev,
            };
            upsert_task(conn, &t).await?;
            changes.push(Change::task(&t));
            if first && let Some(day) = &opts.day {
                let last: Option<String> = sqlx::query_scalar(
                    "SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
                )
                .bind(owner_user_id)
                .bind(day)
                .fetch_one(&mut *conn)
                .await?;
                let e = DayEntry {
                    id: new_id(),
                    user_id: owner_user_id.into(),
                    date: day.clone(),
                    task_id: t.id.clone(),
                    position: key_after(last.as_deref()),
                    start_time: opts.start_time.clone(),
                    duration_min: None,
                    created_at: ts.clone(),
                    updated_at: ts,
                    deleted_at: None,
                    rev,
                };
                upsert_entry(conn, &e).await?;
                changes.push(Change::entry(&e));
            }
            prev = Some(t.id.clone());
            tasks.push(t);
        }
    }
    Ok((tasks, changes))
}
