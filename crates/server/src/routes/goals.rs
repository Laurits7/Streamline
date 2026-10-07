//! Long-term goals (SPEC §6.10): goals with milestones and linked work, derived progress
//! with a manual override (D-21), and periodic reviews. Personal, or shared with a group.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use streamline_domain::{
    goals::{Counts, progress},
    order::key_after,
};

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{Goal, Me, Milestone, upsert_goal},
    ownership::{self, gone_for},
    util::{double_option, id_or_new, new_id, now, parse_date},
    visibility,
};

const STATUSES: [&str; 4] = ["active", "paused", "achieved", "dropped"];

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

/// Linked projects and tasks must be ones the user can see.
async fn check_links(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    g: &Goal,
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
    if g.milestones.0.len() > 50 || g.project_ids.0.len() > 50 || g.task_ids.0.len() > 500 {
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
    for (table, ids) in [("projects", &g.project_ids.0), ("tasks", &g.task_ids.0)] {
        for id in ids {
            let ok: bool = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?2 AND deleted_at IS NULL AND {})",
                visibility::OWNED_VISIBLE_SQL
            ))
            .bind(user.id())
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
            if !ok {
                return Err(bad(format!(
                    "unknown {} in links",
                    &table[..table.len() - 1]
                )));
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
    /// Share with a group you belong to.
    owner_group_id: Option<String>,
    #[serde(default)]
    milestones: Vec<Milestone>,
    #[serde(default)]
    project_ids: Vec<String>,
    #[serde(default)]
    task_ids: Vec<String>,
}

#[utoipa::path(post, path = "/goals", tag = "goals", summary = "Create a goal", request_body = CreateGoal, responses((status = 200, body = Goal), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateGoal>,
) -> ApiResult<Json<Goal>> {
    let owner = ownership::chosen(&user, c.owner_group_id.as_deref())?;
    let id = id_or_new(c.id)?;
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
        status: "active".into(),
        progress_override: None,
        milestones: sqlx::types::Json(c.milestones),
        project_ids: sqlx::types::Json(c.project_ids),
        task_ids: sqlx::types::Json(c.task_ids),
        position: key_after(last.as_deref()),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    check_links(&mut tx, &user, &g).await?;
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
    /// `active`, `paused`, `achieved` or `dropped`.
    status: Option<String>,
    /// 0–1, or null to go back to derived progress.
    #[serde(default, deserialize_with = "double_option")]
    progress_override: Option<Option<f64>>,
    milestones: Option<Vec<Milestone>>,
    project_ids: Option<Vec<String>>,
    task_ids: Option<Vec<String>>,
    position: Option<String>,
    /// Share with a group (or `null` to make it yours again).
    #[serde(default, deserialize_with = "double_option")]
    owner_group_id: Option<Option<String>>,
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
    let before = visibility::audience(g.owner_user_id.as_deref(), g.owner_group_id.as_deref());
    if let Some(v) = c.title {
        g.title = v.trim().into();
    }
    if let Some(v) = c.description {
        g.description = v;
    }
    if let Some(v) = c.target_date {
        g.target_date = v;
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
    if let Some(v) = c.position {
        crate::util::check_position(&v)?;
        g.position = v;
    }
    if let Some(group) = c.owner_group_id {
        let o = ownership::chosen(&user, group.as_deref())?;
        g.owner_user_id = o.user;
        g.owner_group_id = o.group;
    }
    check_links(&mut tx, &user, &g).await?;
    g.updated_at = now();
    g.rev = next_rev(&mut tx).await?;
    upsert_goal(&mut tx, &g).await?;
    tx.commit().await?;
    let change = Change::goal(&g);
    let gone = gone_for(&change, &before, &g.updated_at);
    state.bus.publish(std::iter::once(change).chain(gone));
    Ok(Json(g))
}

#[utoipa::path(delete, path = "/goals/{id}", tag = "goals", summary = "Delete a goal (its linked work stays)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut g = load(&mut tx, &user, &id).await?;
    g.deleted_at = Some(now());
    g.updated_at = now();
    g.rev = next_rev(&mut tx).await?;
    upsert_goal(&mut tx, &g).await?;
    tx.commit().await?;
    state.bus.publish([Change::goal(&g)]);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct GoalProgress {
    pub goal_id: String,
    /// What's shown: the override, else the derived value (0–1); `null` = nothing to measure yet.
    pub progress: Option<f64>,
    /// Derived from milestones and linked work, even when overridden.
    pub derived: Option<f64>,
    pub milestones_done: u32,
    pub milestones: u32,
    pub tasks_done: u32,
    pub tasks: u32,
}

/// Progress as `viewer` sees it: linked tasks and projects they can't see don't count
/// (a shared goal mustn't reveal a member's private work).
pub async fn progress_of(
    conn: &mut sqlx::SqliteConnection,
    g: &Goal,
    viewer: &str,
) -> sqlx::Result<GoalProgress> {
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
    .fetch_one(conn)
    .await?;
    counts.tasks_done = done.max(0) as u32;
    counts.tasks = total.max(0) as u32;
    Ok(GoalProgress {
        goal_id: g.id.clone(),
        progress: progress(counts, g.progress_override),
        derived: progress(counts, None),
        milestones_done: counts.milestones_done,
        milestones: counts.milestones,
        tasks_done: counts.tasks_done,
        tasks: counts.tasks,
    })
}

#[utoipa::path(get, path = "/goals/progress", tag = "goals", summary = "Progress of every goal you can see", responses((status = 200, body = Vec<GoalProgress>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn all_progress(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<GoalProgress>>> {
    let mut conn = state.db.read.acquire().await?;
    let goals: Vec<Goal> = sqlx::query_as(&format!(
        "SELECT * FROM goals WHERE deleted_at IS NULL AND {}",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&mut *conn)
    .await?;
    let mut out = vec![];
    for g in &goals {
        out.push(progress_of(&mut conn, g, user.id()).await?);
    }
    Ok(Json(out))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReviewNote {
    goal_id: String,
    #[serde(default)]
    note: String,
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
    for n in &c.notes {
        if n.note.len() > 20_000 {
            return Err(bad("note too long"));
        }
        let g = load(&mut tx, &user, &n.goal_id).await?;
        let p = progress_of(&mut tx, &g, user.id()).await?;
        sqlx::query("INSERT INTO goal_reviews (id, goal_id, user_id, date, progress, note, created_at) VALUES (?,?,?,?,?,?,?)")
            .bind(new_id())
            .bind(&g.id)
            .bind(user.id())
            .bind(&today)
            .bind(p.progress)
            .bind(n.note.trim())
            .bind(now())
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
