//! Long-term goals (SPEC §6.10): goals with milestones and linked work, derived progress
//! with a manual override (D-21), and periodic reviews. Personal, or shared with a group.
//! Habit goals (D-105) are kept day by day: each person answers yes / partly / no per day.
//! Goals form a tree with routines attached, and rules sit in it too (D-112).

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use streamline_domain::{
    goals::{Counts, Kind, Node, progress, roll_up, routine_rate},
    order::key_after,
    tree,
};

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{Goal, HabitCheck, Me, Milestone, Series, upsert_goal, upsert_habit_check},
    ownership::{self, Owner, gone_for},
    util::{double_option, id_or_new, new_id, now, parse_date},
    visibility,
};

const STATUSES: [&str; 4] = ["active", "paused", "achieved", "dropped"];

fn check_kind(k: &str) -> ApiResult<()> {
    match Kind::parse(k) {
        Some(_) => Ok(()),
        None => Err(bad("kind must be outcome, habit or rule")),
    }
}

/// Every live goal the user can see.
async fn visible_goals(conn: &mut sqlx::SqliteConnection, user: &str) -> sqlx::Result<Vec<Goal>> {
    sqlx::query_as(&format!(
        "SELECT * FROM goals WHERE deleted_at IS NULL AND {} ORDER BY position",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user)
    .fetch_all(conn)
    .await
}

/// Parent links of all goals the user can see, for tree checks.
async fn parent_map(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
) -> ApiResult<HashMap<String, Option<String>>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id, parent_id FROM goals WHERE {} AND deleted_at IS NULL",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Validate a new parent for goal `id` (`None` when creating): it must be visible, not a
/// rule, and not the goal itself or one of its sub-goals.
async fn check_parent(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: Option<&str>,
    parent_id: &str,
) -> ApiResult<Goal> {
    let parent = load(conn, user, parent_id)
        .await
        .map_err(|_| bad("unknown parent goal"))?;
    if parent.kind == "rule" {
        return Err(bad("rules can't have sub-goals"));
    }
    if let Some(id) = id
        && tree::would_cycle(id, parent_id, &parent_map(conn, user).await?)
    {
        return Err(bad(
            "a goal can't be moved under itself or one of its sub-goals",
        ));
    }
    Ok(parent)
}

fn owner_of(g: &Goal) -> Owner {
    Owner {
        user: g.owner_user_id.clone(),
        group: g.owner_group_id.clone(),
    }
}

/// Give a goal's sub-goals (all levels) its new owner: a tree is shared as a whole.
async fn reown_children(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    root: &str,
    owner: &Owner,
    rev: i64,
    ts: &str,
    changes: &mut Vec<Change>,
) -> ApiResult<()> {
    for id in tree::subtree(root, &parent_map(conn, user).await?)
        .into_iter()
        .skip(1)
    {
        let mut c = load(conn, user, &id).await?;
        let old = owner_of(&c).audience();
        c.owner_user_id = owner.user.clone();
        c.owner_group_id = owner.group.clone();
        c.updated_at = ts.into();
        c.rev = rev;
        upsert_goal(conn, &c).await?;
        let change = Change::goal(&c);
        changes.extend(gone_for(&change, &old, ts));
        changes.push(change);
    }
    Ok(())
}

/// A routine changed its schedule and continues as a new version: goals it was attached
/// to follow it.
pub async fn follow_routine(
    conn: &mut sqlx::SqliteConnection,
    old: &str,
    new: &str,
    rev: i64,
    ts: &str,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let goals: Vec<Goal> = sqlx::query_as(
        "SELECT * FROM goals WHERE deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(series_ids) WHERE value = ?1)",
    )
    .bind(old)
    .fetch_all(&mut *conn)
    .await?;
    for mut g in goals {
        for s in g.series_ids.0.iter_mut().filter(|s| *s == old) {
            *s = new.into();
        }
        g.updated_at = ts.into();
        g.rev = rev;
        upsert_goal(conn, &g).await?;
        changes.push(Change::goal(&g));
    }
    Ok(())
}

async fn load(conn: &mut sqlx::SqliteConnection, user: &AuthUser, id: &str) -> ApiResult<Goal> {
    let g: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    if !visibility::can_see(
        user,
        g.owner_user_id.as_deref(),
        g.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(g)
}

/// Linked projects, tasks and routines must be ones the user can see. Only newly linked
/// ones are checked against `before`: a linked task deleted since (or a group member's
/// private one) doesn't block other edits.
async fn check_links(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    g: &Goal,
    before: Option<&Goal>,
) -> ApiResult<()> {
    if g.title.trim().is_empty() || g.title.chars().count() > 200 {
        return Err(bad("title must be 1-200 characters"));
    }
    if !STATUSES.contains(&g.status.as_str()) {
        return Err(bad("status must be active, paused, achieved or dropped"));
    }
    if g.progress_override
        .is_some_and(|p| !(0.0..=1.0).contains(&p))
    {
        return Err(bad("progress must be between 0 and 1"));
    }
    if let Some(d) = &g.target_date {
        parse_date(d)?;
    }
    if let Some(d) = &g.start_date {
        parse_date(d)?;
    }
    if g.milestones.0.len() > 50
        || g.project_ids.0.len() > 50
        || g.task_ids.0.len() > 500
        || g.series_ids.0.len() > 50
    {
        return Err(bad("too many milestones or links"));
    }
    for m in &g.milestones.0 {
        if m.title.trim().is_empty() || m.title.chars().count() > 200 {
            return Err(bad("each milestone needs a title"));
        }
        if let Some(d) = &m.due_date {
            parse_date(d)?;
        }
    }
    let links = |g: Option<&Goal>| {
        g.map(|g| {
            [
                g.project_ids.0.clone(),
                g.task_ids.0.clone(),
                g.series_ids.0.clone(),
            ]
        })
        .unwrap_or_default()
    };
    let (now_linked, was_linked) = (links(Some(g)), links(before));
    for (i, (table, what)) in [
        ("projects", "project"),
        ("tasks", "task"),
        ("series", "routine"),
    ]
    .into_iter()
    .enumerate()
    {
        for id in now_linked[i]
            .iter()
            .filter(|id| !was_linked[i].contains(id))
        {
            let ok: bool = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?2 AND deleted_at IS NULL AND {})",
                visibility::OWNED_VISIBLE_SQL
            ))
            .bind(user.id())
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
            if !ok {
                return Err(bad(format!("unknown {what} in links")));
            }
        }
    }
    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateGoal {
    id: Option<String>,
    title: String,
    #[serde(default)]
    description: String,
    target_date: Option<String>,
    /// `outcome` (default), `habit` (D-105) or `rule` (D-112).
    kind: Option<String>,
    /// Habits and rules: when the period starts (default: today).
    start_date: Option<String>,
    /// Share with a group you belong to (a sub-goal has its parent's owner).
    owner_group_id: Option<String>,
    /// The goal it's part of.
    parent_id: Option<String>,
    #[serde(default)]
    milestones: Vec<Milestone>,
    #[serde(default)]
    project_ids: Vec<String>,
    #[serde(default)]
    task_ids: Vec<String>,
    /// Attached routines.
    #[serde(default)]
    series_ids: Vec<String>,
}

#[utoipa::path(post, path = "/goals", tag = "goals", summary = "Create a goal", request_body = CreateGoal, responses((status = 200, body = Goal), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateGoal>,
) -> ApiResult<Json<Goal>> {
    let id = id_or_new(c.id)?;
    let kind = c.kind.unwrap_or_else(|| "outcome".into());
    check_kind(&kind)?;
    for d in [&c.start_date, &c.target_date].into_iter().flatten() {
        parse_date(d)?;
    }
    let start_date = match (&kind[..], c.start_date) {
        ("habit" | "rule", None) => Some(
            crate::rollover::today_for(&user.user)
                .format("%Y-%m-%d")
                .to_string(),
        ),
        (_, d) => d,
    };
    let mut tx = state.db.write.begin().await?;
    if let Some(g) = sqlx::query_as::<_, Goal>("SELECT * FROM goals WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        return if visibility::can_see(
            &user,
            g.owner_user_id.as_deref(),
            g.owner_group_id.as_deref(),
        ) {
            Ok(Json(g))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    // A sub-goal has its parent's owner (the whole tree is shared together).
    let owner = match &c.parent_id {
        Some(pid) => {
            let parent = check_parent(&mut tx, &user, None, pid).await?;
            if c.owner_group_id.is_some() && c.owner_group_id != parent.owner_group_id {
                return Err(bad(
                    "sub-goals are shared together with their top-level goal",
                ));
            }
            owner_of(&parent)
        }
        None => ownership::chosen(&state, &user, c.owner_group_id.as_deref()).await?,
    };
    let last: Option<String> = sqlx::query_scalar(&format!(
        "SELECT MAX(position) FROM goals WHERE deleted_at IS NULL AND {}",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_one(&mut *tx)
    .await?;
    let ts = now();
    let g = Goal {
        id,
        owner_user_id: owner.user,
        owner_group_id: owner.group,
        title: c.title.trim().into(),
        description: c.description,
        target_date: c.target_date,
        kind,
        start_date,
        parent_id: c.parent_id,
        status: "active".into(),
        progress_override: None,
        milestones: sqlx::types::Json(c.milestones),
        project_ids: sqlx::types::Json(c.project_ids),
        task_ids: sqlx::types::Json(c.task_ids),
        series_ids: sqlx::types::Json(c.series_ids),
        position: key_after(last.as_deref()),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    check_links(&mut tx, &user, &g, None).await?;
    upsert_goal(&mut tx, &g).await?;
    tx.commit().await?;
    state.bus.publish([Change::goal(&g)]);
    Ok(Json(g))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchGoal {
    title: Option<String>,
    description: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    target_date: Option<Option<String>>,
    /// `outcome`, `habit` or `rule`.
    kind: Option<String>,
    /// Habits and rules: when the period starts.
    #[serde(default, deserialize_with = "double_option")]
    start_date: Option<Option<String>>,
    /// `active`, `paused`, `achieved` or `dropped`.
    status: Option<String>,
    /// 0–1, or null to go back to derived progress.
    #[serde(default, deserialize_with = "double_option")]
    progress_override: Option<Option<f64>>,
    milestones: Option<Vec<Milestone>>,
    project_ids: Option<Vec<String>>,
    task_ids: Option<Vec<String>>,
    /// Attached routines.
    series_ids: Option<Vec<String>>,
    position: Option<String>,
    /// Share a top-level goal with a group (or `null` to make it yours again).
    #[serde(default, deserialize_with = "double_option")]
    owner_group_id: Option<Option<String>>,
    /// Move it under another goal (it takes that goal's owner), or `null` for top level.
    #[serde(default, deserialize_with = "double_option")]
    parent_id: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/goals/{id}", tag = "goals", summary = "Change a goal, its milestones or links", params(("id" = String, Path, description = "ULID")), request_body = PatchGoal, responses((status = 200, body = Goal), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchGoal>,
) -> ApiResult<Json<Goal>> {
    let mut tx = state.db.write.begin().await?;
    let mut g = load(&mut tx, &user, &id).await?;
    let old = g.clone();
    let before = visibility::audience(g.owner_user_id.as_deref(), g.owner_group_id.as_deref());
    if let Some(v) = c.title {
        g.title = v.trim().into();
    }
    if let Some(v) = c.description {
        g.description = v;
    }
    if let Some(v) = c.target_date {
        if let Some(d) = &v {
            parse_date(d)?;
        }
        g.target_date = v;
    }
    if let Some(v) = c.kind {
        check_kind(&v)?;
        if v == "rule" && g.kind != "rule" {
            let has_children: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM goals WHERE parent_id = ? AND deleted_at IS NULL)",
            )
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
            if has_children {
                return Err(bad("a goal with sub-goals can't become a rule"));
            }
        }
        g.kind = v;
    }
    if let Some(v) = c.start_date {
        if let Some(d) = &v {
            parse_date(d)?;
        }
        g.start_date = v;
    }
    if let Some(v) = c.status {
        g.status = v;
    }
    if let Some(v) = c.progress_override {
        g.progress_override = v;
    }
    if let Some(v) = c.milestones {
        g.milestones = sqlx::types::Json(v);
    }
    if let Some(v) = c.project_ids {
        g.project_ids = sqlx::types::Json(v);
    }
    if let Some(v) = c.task_ids {
        g.task_ids = sqlx::types::Json(v);
    }
    if let Some(v) = c.series_ids {
        g.series_ids = sqlx::types::Json(v);
    }
    if let Some(v) = c.position {
        crate::util::check_position(&v)?;
        g.position = v;
    }
    // New owner: chosen for a top-level goal, or its new parent's; sub-goals follow.
    let mut new_owner = None;
    if let Some(parent_id) = c.parent_id
        && parent_id != g.parent_id
    {
        if let Some(pid) = &parent_id {
            new_owner = Some(owner_of(
                &check_parent(&mut tx, &user, Some(&id), pid).await?,
            ));
        }
        g.parent_id = parent_id;
    }
    if let Some(group) = c.owner_group_id {
        if g.parent_id.is_some() {
            if group != g.owner_group_id {
                return Err(bad(
                    "sub-goals are shared together with their top-level goal",
                ));
            }
        } else {
            new_owner = Some(ownership::chosen(&state, &user, group.as_deref()).await?);
        }
    }
    check_links(&mut tx, &user, &g, Some(&old)).await?;
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let mut changes = vec![];
    if let Some(o) = new_owner.filter(|o| *o != owner_of(&g)) {
        g.owner_user_id = o.user.clone();
        g.owner_group_id = o.group.clone();
        reown_children(&mut tx, &user, &id, &o, rev, &ts, &mut changes).await?;
    }
    g.updated_at = ts;
    g.rev = rev;
    upsert_goal(&mut tx, &g).await?;
    tx.commit().await?;
    let change = Change::goal(&g);
    changes.extend(gone_for(&change, &before, &g.updated_at));
    changes.push(change);
    state.bus.publish(changes);
    Ok(Json(g))
}

/// Its sub-goals move up to its parent (or to the top level).
#[utoipa::path(delete, path = "/goals/{id}", tag = "goals", summary = "Delete a goal (its linked work stays, its sub-goals move up)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut g = load(&mut tx, &user, &id).await?;
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let mut changes = vec![];
    let children: Vec<Goal> =
        sqlx::query_as("SELECT * FROM goals WHERE parent_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for mut c in children {
        c.parent_id = g.parent_id.clone();
        c.updated_at = ts.clone();
        c.rev = rev;
        upsert_goal(&mut tx, &c).await?;
        changes.push(Change::goal(&c));
    }
    g.deleted_at = Some(ts.clone());
    g.updated_at = ts;
    g.rev = rev;
    upsert_goal(&mut tx, &g).await?;
    tx.commit().await?;
    changes.push(Change::goal(&g));
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct GoalProgress {
    pub goal_id: String,
    /// What's shown: the override, else the derived value (0–1); `null` = nothing to
    /// measure yet (always for rules).
    pub progress: Option<f64>,
    /// The average of its parts (own milestones and linked work, each attached routine,
    /// each sub-goal; D-112), even when overridden. Habits: their score.
    pub derived: Option<f64>,
    pub milestones_done: u32,
    pub milestones: u32,
    pub tasks_done: u32,
    pub tasks: u32,
    /// Habits: the viewer's own answers over the last four weeks (D-105).
    pub habit: Option<HabitSummary>,
    /// Attached routines over the last four weeks (ones the viewer can see).
    pub routines: Vec<RoutineRate>,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct RoutineRate {
    pub series_id: String,
    pub done: u32,
    pub missed: u32,
    /// done ÷ (done + missed); `null` = nothing decided yet.
    pub rate: Option<f64>,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct HabitSummary {
    pub yes: u32,
    pub partly: u32,
    pub no: u32,
    /// Days of the window without an answer.
    pub unanswered: u32,
    /// Days in a row answered yes (up to today, or yesterday while today is open).
    pub streak: u32,
}

/// The viewer's today, in their timezone.
async fn today_of(
    conn: &mut sqlx::SqliteConnection,
    viewer: &str,
) -> sqlx::Result<chrono::NaiveDate> {
    let tz: Option<String> = sqlx::query_scalar("SELECT timezone FROM users WHERE id = ?")
        .bind(viewer)
        .fetch_optional(&mut *conn)
        .await?;
    let tz =
        streamline_domain::time::parse_tz(tz.as_deref().unwrap_or("UTC")).unwrap_or(chrono_tz::UTC);
    Ok(chrono::Utc::now().with_timezone(&tz).date_naive())
}

/// Progress of `goals` (all the viewer's visible goals, so trees are whole) as `viewer`
/// sees it, rolled up the goal tree (D-112).
pub async fn progress_all(
    conn: &mut sqlx::SqliteConnection,
    goals: &[Goal],
    viewer: &str,
) -> sqlx::Result<Vec<GoalProgress>> {
    let today = today_of(conn, viewer).await?;
    let mut out = vec![];
    let mut nodes = HashMap::new();
    for g in goals {
        let (p, node) = own_progress(conn, g, viewer, today).await?;
        out.push(p);
        nodes.insert(g.id.clone(), node);
    }
    let rolled = roll_up(&nodes);
    for p in &mut out {
        let r = rolled.get(&p.goal_id).copied().unwrap_or_default();
        p.derived = r.derived;
        p.progress = r.progress;
    }
    Ok(out)
}

/// One goal's own parts, as `viewer` sees them: linked tasks, projects and routines they
/// can't see don't count (a shared goal mustn't reveal a member's private work).
async fn own_progress(
    conn: &mut sqlx::SqliteConnection,
    g: &Goal,
    viewer: &str,
    today: chrono::NaiveDate,
) -> sqlx::Result<(GoalProgress, Node)> {
    let ms = &g.milestones.0;
    let mut counts = Counts {
        milestones_done: ms.iter().filter(|m| m.done).count() as u32,
        milestones: ms.len() as u32,
        ..Default::default()
    };
    // Linked tasks plus every task in linked projects and their subprojects.
    let (done, total): (i64, i64) = sqlx::query_as(
        "WITH RECURSIVE tree(id) AS (
             SELECT value FROM json_each(?1)
             UNION SELECT p.id FROM projects p JOIN tree ON p.parent_id = tree.id WHERE p.deleted_at IS NULL)
         SELECT COALESCE(SUM(status = 'done'), 0), COUNT(*) FROM tasks
         WHERE deleted_at IS NULL AND status NOT IN ('skipped', 'wont_do')
           AND (id IN (SELECT value FROM json_each(?2)) OR project_id IN (SELECT id FROM tree))
           AND (owner_user_id = ?3 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?3))",
    )
    .bind(serde_json::to_string(&g.project_ids.0).unwrap_or_default())
    .bind(serde_json::to_string(&g.task_ids.0).unwrap_or_default())
    .bind(viewer)
    .fetch_one(&mut *conn)
    .await?;
    counts.tasks_done = done.max(0) as u32;
    counts.tasks = total.max(0) as u32;
    // Habits: the share of good days (partly = half) over the last four weeks.
    let (habit, habit_score) = if g.kind == "habit" {
        let s = habit_summary(conn, g, viewer, today).await?;
        (Some(s.0), s.1)
    } else {
        (None, None)
    };
    // Attached routines: done ÷ (done + missed) over the same four weeks.
    let series: Vec<Series> = sqlx::query_as(
        "SELECT * FROM series WHERE id IN (SELECT value FROM json_each(?1)) AND deleted_at IS NULL
           AND (owner_user_id = ?2 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?2))",
    )
    .bind(serde_json::to_string(&g.series_ids.0).unwrap_or_default())
    .bind(viewer)
    .fetch_all(&mut *conn)
    .await?;
    let mut routines = vec![];
    for s in &series {
        let (done, missed) =
            crate::routines::recent_outcomes(conn, s, today, streamline_domain::habits::SCORE_DAYS)
                .await?;
        routines.push(RoutineRate {
            series_id: s.id.clone(),
            done,
            missed,
            rate: routine_rate(done, missed),
        });
    }
    let node = Node {
        parent: g.parent_id.clone(),
        kind: Kind::parse(&g.kind).unwrap_or_default(),
        status: g.status.clone(),
        own: progress(counts, None),
        routines: routines.iter().map(|r| r.rate).collect(),
        habit: habit_score,
        manual: g.progress_override,
    };
    let p = GoalProgress {
        goal_id: g.id.clone(),
        progress: None,
        derived: None,
        habit,
        routines,
        milestones_done: counts.milestones_done,
        milestones: counts.milestones,
        tasks_done: counts.tasks_done,
        tasks: counts.tasks,
    };
    Ok((p, node))
}

/// A habit's last four weeks for one person, and its score (`None`: nothing answered).
async fn habit_summary(
    conn: &mut sqlx::SqliteConnection,
    g: &Goal,
    viewer: &str,
    today: chrono::NaiveDate,
) -> sqlx::Result<(HabitSummary, Option<f64>)> {
    use streamline_domain::habits::{Answer, SCORE_DAYS, stats, streak, window};
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT date, result FROM habit_checks WHERE goal_id = ? AND user_id = ? AND deleted_at IS NULL ORDER BY date DESC LIMIT 400",
    )
    .bind(&g.id)
    .bind(viewer)
    .fetch_all(&mut *conn)
    .await?;
    let answers: Vec<(chrono::NaiveDate, Answer)> = rows
        .iter()
        .filter_map(|(d, r)| {
            Some((
                chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?,
                Answer::parse(r)?,
            ))
        })
        .collect();
    let date = |d: &Option<String>| {
        d.as_deref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    };
    let st = window(date(&g.start_date), date(&g.target_date), today, SCORE_DAYS)
        .map(|(from, to)| stats(&answers, from, to))
        .unwrap_or_default();
    Ok((
        HabitSummary {
            yes: st.yes,
            partly: st.partly,
            no: st.no,
            unanswered: st.unanswered,
            streak: streak(&answers, today),
        },
        st.score,
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PutCheck {
    /// `yes`, `partly` or `no`.
    result: String,
    #[serde(default)]
    note: String,
}

/// Answer a habit for a day (yours; each member of a shared habit answers for themselves).
#[utoipa::path(put, path = "/goals/{id}/checks/{date}", tag = "goals", summary = "Say how a habit went on a day (yes, partly, no)", params(("id" = String, Path, description = "ULID"), ("date" = String, Path, description = "YYYY-MM-DD")), request_body = PutCheck, responses((status = 200, body = HabitCheck), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn put_check(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, date)): Path<(String, String)>,
    Json(c): Json<PutCheck>,
) -> ApiResult<Json<HabitCheck>> {
    parse_date(&date)?;
    if streamline_domain::habits::Answer::parse(&c.result).is_none() {
        return Err(bad("result must be yes, partly or no"));
    }
    let note = c.note.trim();
    if note.chars().count() > 1000 {
        return Err(bad("note too long"));
    }
    let mut tx = state.db.write.begin().await?;
    let g = load(&mut tx, &user, &id).await?;
    if g.kind != "habit" {
        return Err(bad("only habits are answered per day"));
    }
    let existing: Option<HabitCheck> =
        sqlx::query_as("SELECT * FROM habit_checks WHERE goal_id = ? AND user_id = ? AND date = ?")
            .bind(&g.id)
            .bind(user.id())
            .bind(&date)
            .fetch_optional(&mut *tx)
            .await?;
    let ts = now();
    let mut check = existing.unwrap_or_else(|| HabitCheck {
        id: new_id(),
        goal_id: g.id.clone(),
        user_id: user.id().into(),
        date: date.clone(),
        result: String::new(),
        note: String::new(),
        created_at: ts.clone(),
        updated_at: ts.clone(),
        deleted_at: None,
        rev: 0,
    });
    check.result = c.result;
    check.note = note.into();
    check.deleted_at = None;
    check.updated_at = ts;
    check.rev = next_rev(&mut tx).await?;
    upsert_habit_check(&mut tx, &check).await?;
    tx.commit().await?;
    state.bus.publish([Change::habit_check(&check)]);
    Ok(Json(check))
}

#[utoipa::path(delete, path = "/goals/{id}/checks/{date}", tag = "goals", summary = "Clear a day's answer for a habit", params(("id" = String, Path, description = "ULID"), ("date" = String, Path, description = "YYYY-MM-DD")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_check(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, date)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    load(&mut tx, &user, &id).await?;
    let existing: Option<HabitCheck> = sqlx::query_as(
        "SELECT * FROM habit_checks WHERE goal_id = ? AND user_id = ? AND date = ? AND deleted_at IS NULL",
    )
    .bind(&id)
    .bind(user.id())
    .bind(&date)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(mut c) = existing {
        c.deleted_at = Some(now());
        c.updated_at = now();
        c.rev = next_rev(&mut tx).await?;
        upsert_habit_check(&mut tx, &c).await?;
        tx.commit().await?;
        state.bus.publish([Change::habit_check(&c)]);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/goals/progress", tag = "goals", summary = "Progress of every goal you can see", responses((status = 200, body = Vec<GoalProgress>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn all_progress(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<GoalProgress>>> {
    let mut conn = state.db.read.acquire().await?;
    let goals = visible_goals(&mut conn, user.id()).await?;
    Ok(Json(progress_all(&mut conn, &goals, user.id()).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReviewNote {
    goal_id: String,
    #[serde(default)]
    note: String,
    /// Habits: how the period went, `yes`, `partly` or `no` (D-105).
    verdict: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubmitReview {
    notes: Vec<ReviewNote>,
}

#[derive(Serialize, sqlx::FromRow, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct GoalReview {
    pub id: String,
    pub goal_id: String,
    pub user_id: String,
    pub date: String,
    pub progress: Option<f64>,
    pub note: String,
    pub created_at: String,
    /// Habits: `yes`, `partly` or `no` for the period.
    pub verdict: Option<String>,
}

/// Finish the periodic review: a note (and a progress snapshot) per goal.
#[utoipa::path(post, path = "/goals/review", tag = "goals", summary = "Save a goals review (notes per goal); marks the review as done", request_body = SubmitReview, responses((status = 200, body = Me), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn review(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<SubmitReview>,
) -> ApiResult<Json<Me>> {
    let today = crate::rollover::today_for(&user.user)
        .format("%Y-%m-%d")
        .to_string();
    let mut tx = state.db.write.begin().await?;
    let goals = visible_goals(&mut tx, user.id()).await?;
    let all = progress_all(&mut tx, &goals, user.id()).await?;
    for n in &c.notes {
        if n.note.len() > 20_000 {
            return Err(bad("note too long"));
        }
        let g = load(&mut tx, &user, &n.goal_id).await?;
        let progress = all
            .iter()
            .find(|p| p.goal_id == g.id)
            .and_then(|p| p.progress);
        if let Some(v) = &n.verdict
            && streamline_domain::habits::Answer::parse(v).is_none()
        {
            return Err(bad("verdict must be yes, partly or no"));
        }
        sqlx::query("INSERT INTO goal_reviews (id, goal_id, user_id, date, progress, note, created_at, verdict) VALUES (?,?,?,?,?,?,?,?)")
            .bind(new_id())
            .bind(&g.id)
            .bind(user.id())
            .bind(&today)
            .bind(progress)
            .bind(n.note.trim())
            .bind(now())
            .bind(&n.verdict)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE users SET last_review_date = ? WHERE id = ?")
        .bind(&today)
        .bind(user.id())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut u = user.user.clone();
    u.last_review_date = Some(today);
    let me = Me::from(&u);
    state
        .bus
        .publish([Change::me(&u.id, serde_json::to_value(&me)?)]);
    Ok(Json(me))
}

#[utoipa::path(get, path = "/goals/{id}/reviews", tag = "goals", summary = "Past review notes of a goal", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = Vec<GoalReview>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn reviews(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<GoalReview>>> {
    let mut conn = state.db.read.acquire().await?;
    load(&mut conn, &user, &id).await?;
    let rows = sqlx::query_as("SELECT * FROM goal_reviews WHERE goal_id = ? ORDER BY date DESC, created_at DESC LIMIT 100")
        .bind(&id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(Json(rows))
}
