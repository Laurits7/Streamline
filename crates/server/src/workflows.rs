//! Starting workflow runs (SPEC §6.3b): one chain of tasks per selected variant, with
//! prerequisites and waits wired up. Used by "Start" and by routines (3b.6).

use sqlx::SqliteConnection;
use streamline_domain::{order::key_after, workflow};

use crate::{
    db::next_rev,
    events::Change,
    models::{DayEntry, Task, WorkflowTemplate, upsert_entry, upsert_task, upsert_workflow},
    util::{new_id, now},
};

/// Who a run goes to (D-116).
#[derive(Default)]
pub enum Assignee {
    /// The chore's default assignee, or whose turn it is (which moves the turns on).
    #[default]
    FromChore,
    /// This person (`None` = anyone in the group).
    Person(Option<String>),
}

/// Where the created tasks go and how they belong to a routine, if any.
#[derive(Default)]
pub struct RunOptions {
    /// Who owns the steps when they have no project (a routine's owner; default: the starter).
    pub owner: Option<crate::ownership::Owner>,
    pub assignee: Assignee,
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
/// none were chosen) a chain of tasks where each step waits for the previous one. All of
/// them go to one person, if any (D-116). `starter` is who pressed Start (`None` for a
/// routine).
pub async fn start(
    conn: &mut SqliteConnection,
    starter: Option<&str>,
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
    let mut project_id = opts.project_id.clone().or(tpl.project_id.clone());
    // The template's project was checked when it was saved; the starter may have lost
    // access since (left the group). Then the run goes to their inbox instead.
    if let (Some(p), Some(u)) = (&project_id, starter) {
        let visible: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?2 AND deleted_at IS NULL AND {})",
            crate::visibility::OWNED_VISIBLE_SQL
        ))
        .bind(u)
        .bind(p)
        .fetch_one(&mut *conn)
        .await?;
        if !visible {
            project_id = None;
        }
    }
    // Steps belong to whoever owns their project (a group's laundry is the group's).
    let owner = match &project_id {
        Some(p) => crate::ownership::project_owner(conn, p).await?,
        None => None,
    }
    .or(opts.owner.clone())
    .unwrap_or(crate::ownership::Owner {
        user: starter.map(str::to_string),
        group: None,
    });
    let mut changes = vec![];
    let assignee = match (&owner.group, &opts.assignee) {
        (None, _) => None,
        (Some(g), a) => {
            let members = crate::ownership::members(conn, g).await?;
            let turn = streamline_domain::assign::assignee_for(
                tpl.assignee_user_id.as_deref(),
                &tpl.rotation_user_ids.0,
                &members,
                tpl.rotation_turn.max(0) as u64,
            );
            let chosen = match a {
                Assignee::FromChore => turn.clone(),
                Assignee::Person(p) => p.clone().filter(|p| members.contains(p)),
            };
            // Whoever's turn it was took it: the turns move on (not for a routine's own
            // assignee: the routine has its own turns).
            let own = matches!(a, Assignee::FromChore) || opts.occurrence.is_none();
            let rotating = tpl.rotation_user_ids.0.iter().any(|u| members.contains(u));
            if own && rotating && chosen.is_some() && chosen == turn {
                let mut next = tpl.clone();
                next.rotation_turn += 1;
                next.updated_at = now();
                next.rev = next_rev(conn).await?;
                upsert_workflow(conn, &next).await?;
                changes.push(Change::workflow(&next));
            }
            chosen
        }
    };
    let mut tasks = vec![];
    for (run_index, (variant, skip)) in runs.iter().enumerate() {
        let instance = new_id();
        let mut prev: Option<String> = None;
        for c in workflow::chain(&steps, skip) {
            let src = tpl.steps.0.iter().find(|s| s.id == c.step.id).unwrap();
            let rev = next_rev(conn).await?;
            let ts = now();
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM tasks WHERE project_id IS ?1 AND (?1 IS NOT NULL OR (owner_user_id IS ?2 AND owner_group_id IS ?3)) AND deleted_at IS NULL",
            )
            .bind(&project_id)
            .bind(&owner.user)
            .bind(&owner.group)
            .fetch_one(&mut *conn)
            .await?;
            let first = c.number == 1;
            let t = Task {
                id: new_id(),
                owner_user_id: owner.user.clone(),
                owner_group_id: owner.group.clone(),
                assignee_user_id: assignee.clone(),
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
                event_id: None,
                window_end: None,
                waiting_since: None,
                check_back_at: None,
                waiting_note: String::new(),
                waiting_by: None,
                checklist: sqlx::types::Json(vec![]),
                ongoing: false,
                start_date: None,
                worked_days: sqlx::types::Json(vec![]),
                created_at: ts.clone(),
                updated_at: ts.clone(),
                deleted_at: None,
                rev,
            };
            upsert_task(conn, &t).await?;
            changes.push(Change::task(&t));
            // The first step goes on the day of the person doing it: the assignee, else
            // whoever started it, else (a shared routine) everyone in the group.
            let people = match (first, &opts.day) {
                (false, _) | (_, None) => vec![],
                _ => match (&assignee, starter) {
                    (Some(_), _) | (None, None) => {
                        crate::routines::planners(
                            conn,
                            owner.user.as_deref(),
                            owner.group.as_deref(),
                            assignee.as_deref(),
                        )
                        .await?
                    }
                    (None, Some(u)) => vec![u.to_string()],
                },
            };
            for user_id in people {
                let day = opts.day.clone().unwrap_or_default();
                let last: Option<String> = sqlx::query_scalar(
                    "SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
                )
                .bind(&user_id)
                .bind(&day)
                .fetch_one(&mut *conn)
                .await?;
                let e = DayEntry {
                    id: new_id(),
                    user_id,
                    date: day,
                    task_id: t.id.clone(),
                    position: key_after(last.as_deref()),
                    start_time: opts.start_time.clone(),
                    duration_min: None,
                    created_at: ts.clone(),
                    updated_at: ts.clone(),
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
