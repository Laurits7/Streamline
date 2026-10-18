//! Day templates, time blocks and the plan suggester (SPEC §6.8, Phase 5a).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use streamline_domain::{
    order::key_after,
    planner::{self, Block, Candidate, Energy, Input},
};

use crate::{
    AppState,
    auth::AuthUser,
    blocks::{self, minutes},
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{
        DayTemplate, TemplateBlock, TimeBlock, WeekTemplate, upsert_day_template,
        upsert_time_block, upsert_week_template,
    },
    util::{check_position, double_option, id_or_new, new_id, now, parse_date},
    visibility::OWNED_VISIBLE_SQL,
};

const ENERGIES: [&str; 3] = ["hard", "medium", "easy"];

fn check_block(title: &str, start: &str, end: &str, energy: &Option<String>) -> ApiResult<()> {
    if title.trim().is_empty() || title.chars().count() > 100 {
        return Err(bad("a block needs a title (up to 100 characters)"));
    }
    match (minutes(start), minutes(end)) {
        (Some(s), Some(e)) if e > s => {}
        (Some(_), Some(_)) => {
            return Err(bad("a block must end after it starts (on the same day)"));
        }
        _ => return Err(bad("times must be HH:MM")),
    }
    if energy.as_deref().is_some_and(|e| !ENERGIES.contains(&e)) {
        return Err(bad("energy must be hard, medium or easy"));
    }
    Ok(())
}

/// Items of the template's blocks: at most 20 per block, routines the user can see, task
/// titles of 1-200 characters. Items without an id get one.
async fn check_items(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    blocks: &mut [TemplateBlock],
) -> ApiResult<()> {
    let mut ids = std::collections::HashSet::new();
    for b in blocks {
        if b.items.len() > 20 {
            return Err(bad("at most 20 routines and tasks in a block"));
        }
        let mut gone = vec![];
        for it in &mut b.items {
            it.id = if it.id.is_empty() {
                new_id()
            } else {
                ulid::Ulid::from_string(&it.id)
                    .map_err(|_| bad("item ids must be ULIDs"))?
                    .to_string()
            };
            if !ids.insert(it.id.clone()) {
                return Err(bad("item ids must be unique"));
            }
            match it.kind.as_str() {
                "routine" => {
                    // Kept pointing at the routine's current version; a deleted routine
                    // leaves the template.
                    let id = crate::routines::current_version(
                        conn,
                        it.series_id.as_deref().unwrap_or_default(),
                    )
                    .await?;
                    let deleted: Option<bool> = sqlx::query_scalar(&format!(
                        "SELECT deleted_at IS NOT NULL FROM series WHERE {OWNED_VISIBLE_SQL} AND id = ?2"
                    ))
                    .bind(user.id())
                    .bind(&id)
                    .fetch_optional(&mut *conn)
                    .await?;
                    match deleted {
                        None => return Err(bad("unknown routine")),
                        Some(true) => gone.push(it.id.clone()),
                        Some(false) => {}
                    }
                    it.series_id = Some(id);
                    (it.title, it.estimate_min, it.notes) = (None, None, None);
                }
                "task" => {
                    let title = it.title.as_deref().unwrap_or_default().trim();
                    if title.is_empty() || title.chars().count() > 200 {
                        return Err(bad("a task needs a title (up to 200 characters)"));
                    }
                    it.title = Some(title.into());
                    if it.estimate_min.is_some_and(|m| !(1..=1440).contains(&m)) {
                        return Err(bad("estimate must be 1-1440 minutes"));
                    }
                    if it
                        .notes
                        .as_ref()
                        .is_some_and(|n| n.chars().count() > 10_000)
                    {
                        return Err(bad("notes are too long"));
                    }
                    it.series_id = None;
                }
                _ => return Err(bad("an item is a routine or a task")),
            }
        }
        b.items.retain(|it| !gone.contains(&it.id));
    }
    Ok(())
}

fn check_template(name: &str, weekdays: &[i32], blocks: &[TemplateBlock]) -> ApiResult<()> {
    if name.trim().is_empty() || name.chars().count() > 60 {
        return Err(bad("name must be 1-60 characters"));
    }
    if weekdays.iter().any(|d| !(1..=7).contains(d)) {
        return Err(bad("weekdays are 1 (Monday) to 7 (Sunday)"));
    }
    if blocks.len() > 20 {
        return Err(bad("at most 20 blocks"));
    }
    for b in blocks {
        check_block(&b.title, &b.start, &b.end, &b.energy)?;
    }
    Ok(())
}

// ----- Templates -----

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTemplate {
    id: Option<String>,
    name: String,
    #[serde(default)]
    weekdays: Vec<i32>,
    #[serde(default)]
    blocks: Vec<TemplateBlock>,
}

/// A weekday belongs to one template: giving it to this one takes it from the others.
async fn take_weekdays(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    keep: &str,
    weekdays: &[i32],
    changes: &mut Vec<Change>,
) -> ApiResult<()> {
    let others: Vec<DayTemplate> = sqlx::query_as(
        "SELECT * FROM day_templates WHERE owner_user_id = ? AND id <> ? AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(keep)
    .fetch_all(&mut *conn)
    .await?;
    for mut o in others {
        if o.weekdays.0.iter().any(|d| weekdays.contains(d)) {
            o.weekdays.0.retain(|d| !weekdays.contains(d));
            o.updated_at = now();
            o.rev = next_rev(conn).await?;
            upsert_day_template(conn, &o).await?;
            changes.push(Change::day_template(&o));
        }
    }
    Ok(())
}

#[utoipa::path(post, path = "/day-templates", tag = "days", summary = "Create a day template (a layout of time blocks)", request_body = CreateTemplate, responses((status = 200, body = DayTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_template(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateTemplate>,
) -> ApiResult<Json<DayTemplate>> {
    let mut c = c;
    check_template(&c.name, &c.weekdays, &c.blocks)?;
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    check_items(&mut tx, &user, &mut c.blocks).await?;
    if let Some(t) = sqlx::query_as::<_, DayTemplate>("SELECT * FROM day_templates WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        return if t.owner_user_id == user.id() {
            Ok(Json(t))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    let last: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM day_templates WHERE owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(user.id())
    .fetch_one(&mut *tx)
    .await?;
    let mut changes = vec![];
    take_weekdays(&mut tx, user.id(), &id, &c.weekdays, &mut changes).await?;
    let ts = now();
    let t = DayTemplate {
        id,
        owner_user_id: user.id().into(),
        name: c.name.trim().into(),
        weekdays: sqlx::types::Json(c.weekdays),
        blocks: sqlx::types::Json(c.blocks),
        position: key_after(last.as_deref()),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_day_template(&mut tx, &t).await?;
    if !t.weekdays.0.is_empty() {
        blocks::default_from_weekdays(&mut tx, user.id(), &mut changes).await?;
    }
    tx.commit().await?;
    changes.push(Change::day_template(&t));
    state.bus.publish(changes);
    blocks::materialize_user(&state, &user.user).await?;
    Ok(Json(t))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchTemplate {
    name: Option<String>,
    weekdays: Option<Vec<i32>>,
    blocks: Option<Vec<TemplateBlock>>,
    position: Option<String>,
}

async fn load_template(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<DayTemplate> {
    sqlx::query_as(
        "SELECT * FROM day_templates WHERE id = ? AND owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.id())
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Days set up automatically follow the change (blocks again; routines and tasks only the
/// new ones, D-113); days changed by hand keep theirs.
#[utoipa::path(patch, path = "/day-templates/{id}", tag = "days", summary = "Change a day template", params(("id" = String, Path, description = "ULID")), request_body = PatchTemplate, responses((status = 200, body = DayTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchTemplate>,
) -> ApiResult<Json<DayTemplate>> {
    let mut tx = state.db.write.begin().await?;
    let mut t = load_template(&mut tx, &user, &id).await?;
    let mut changes = vec![];
    if let Some(v) = c.name {
        t.name = v.trim().into();
    }
    let weekdays = c.weekdays.is_some();
    if let Some(v) = c.weekdays {
        take_weekdays(&mut tx, user.id(), &t.id, &v, &mut changes).await?;
        t.weekdays = sqlx::types::Json(v);
    }
    if let Some(mut v) = c.blocks {
        check_items(&mut tx, &user, &mut v).await?;
        t.blocks = sqlx::types::Json(v);
    }
    if let Some(v) = c.position {
        check_position(&v)?;
        t.position = v;
    }
    check_template(&t.name, &t.weekdays.0, &t.blocks.0)?;
    t.updated_at = now();
    t.rev = next_rev(&mut tx).await?;
    upsert_day_template(&mut tx, &t).await?;
    if weekdays {
        blocks::default_from_weekdays(&mut tx, user.id(), &mut changes).await?;
    }
    tx.commit().await?;
    changes.push(Change::day_template(&t));
    state.bus.publish(changes);
    blocks::materialize_user(&state, &user.user).await?;
    Ok(Json(t))
}

#[utoipa::path(delete, path = "/day-templates/{id}", tag = "days", summary = "Delete a day template (days keep their blocks)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut t = load_template(&mut tx, &user, &id).await?;
    t.deleted_at = Some(now());
    t.updated_at = now();
    t.rev = next_rev(&mut tx).await?;
    upsert_day_template(&mut tx, &t).await?;
    let mut changes = vec![Change::day_template(&t)];
    // Weekdays that had it get none.
    for mut w in blocks::week_templates(&mut tx, user.id()).await? {
        if w.days.0.iter().any(|d| d.as_deref() == Some(&t.id)) {
            for d in w.days.0.iter_mut() {
                if d.as_deref() == Some(&t.id) {
                    *d = None;
                }
            }
            w.updated_at = now();
            w.rev = next_rev(&mut tx).await?;
            upsert_week_template(&mut tx, &w).await?;
            changes.push(Change::week_template(&w));
        }
    }
    tx.commit().await?;
    state.bus.publish(changes);
    blocks::materialize_user(&state, &user.user).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- Week templates -----

/// Name, 7 days of the user's day templates (or none), and a valid mode with its dates;
/// one default week and one alternating week at most.
async fn check_week(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    w: &mut WeekTemplate,
) -> ApiResult<()> {
    if w.name.trim().is_empty() || w.name.chars().count() > 60 {
        return Err(bad("name must be 1-60 characters"));
    }
    w.name = w.name.trim().into();
    if w.days.0.len() != 7 {
        return Err(bad("a week has 7 days (Monday first)"));
    }
    for id in w.days.0.iter().flatten() {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM day_templates WHERE id = ? AND owner_user_id = ? AND deleted_at IS NULL)",
        )
        .bind(id)
        .bind(user.id())
        .fetch_one(&mut *conn)
        .await?;
        if !ok {
            return Err(bad("unknown day template"));
        }
    }
    for d in [&w.start_date, &w.end_date].into_iter().flatten() {
        parse_date(d)?;
    }
    match w.mode.as_str() {
        "default" => (w.start_date, w.end_date) = (None, None),
        "alternate" => {
            if w.start_date.is_none() {
                return Err(bad(
                    "an alternating week needs the date of a week it applies to",
                ));
            }
            w.end_date = None;
        }
        "range" => match (&w.start_date, &w.end_date) {
            (Some(s), Some(e)) if s <= e => {}
            (Some(_), Some(_)) => return Err(bad("the range must end on or after its start")),
            _ => return Err(bad("a date range needs a start and an end date")),
        },
        _ => return Err(bad("mode must be default, alternate or range")),
    }
    if w.mode != "range" {
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM week_templates WHERE owner_user_id = ? AND mode = ? AND id <> ? AND deleted_at IS NULL)",
        )
        .bind(user.id())
        .bind(&w.mode)
        .bind(&w.id)
        .fetch_one(&mut *conn)
        .await?;
        if taken {
            return Err(bad(if w.mode == "default" {
                "there is a default week already"
            } else {
                "there is an alternating week already"
            }));
        }
    }
    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateWeek {
    id: Option<String>,
    name: String,
    /// 7 day template ids or `null`, Monday first (default: none).
    #[serde(default)]
    days: Option<Vec<Option<String>>>,
    /// `default`, `alternate` or `range`.
    mode: String,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[utoipa::path(post, path = "/week-templates", tag = "days", summary = "Create a week template (a day template for each weekday)", request_body = CreateWeek, responses((status = 200, body = WeekTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_week(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateWeek>,
) -> ApiResult<Json<WeekTemplate>> {
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if let Some(w) = sqlx::query_as::<_, WeekTemplate>("SELECT * FROM week_templates WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        return if w.owner_user_id == user.id() {
            Ok(Json(w))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    let last: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM week_templates WHERE owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(user.id())
    .fetch_one(&mut *tx)
    .await?;
    let ts = now();
    let mut w = WeekTemplate {
        id,
        owner_user_id: user.id().into(),
        name: c.name,
        days: sqlx::types::Json(c.days.unwrap_or_else(|| vec![None; 7])),
        mode: c.mode,
        start_date: c.start_date,
        end_date: c.end_date,
        position: key_after(last.as_deref()),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: 0,
    };
    check_week(&mut tx, &user, &mut w).await?;
    w.rev = next_rev(&mut tx).await?;
    upsert_week_template(&mut tx, &w).await?;
    let mut changes = vec![Change::week_template(&w)];
    if w.mode == "default" {
        blocks::weekdays_from_default(&mut tx, user.id(), &mut changes).await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);
    blocks::materialize_user(&state, &user.user).await?;
    Ok(Json(w))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchWeek {
    name: Option<String>,
    days: Option<Vec<Option<String>>>,
    mode: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    start_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    end_date: Option<Option<String>>,
    position: Option<String>,
}

async fn load_week(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<WeekTemplate> {
    sqlx::query_as(
        "SELECT * FROM week_templates WHERE id = ? AND owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.id())
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Days set up automatically follow the change, as with day templates.
#[utoipa::path(patch, path = "/week-templates/{id}", tag = "days", summary = "Change a week template", params(("id" = String, Path, description = "ULID")), request_body = PatchWeek, responses((status = 200, body = WeekTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_week(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchWeek>,
) -> ApiResult<Json<WeekTemplate>> {
    let mut tx = state.db.write.begin().await?;
    let mut w = load_week(&mut tx, &user, &id).await?;
    let was_default = w.mode == "default";
    if let Some(v) = c.name {
        w.name = v;
    }
    if let Some(v) = c.days {
        w.days = sqlx::types::Json(v);
    }
    if let Some(v) = c.mode {
        w.mode = v;
    }
    if let Some(v) = c.start_date {
        w.start_date = v;
    }
    if let Some(v) = c.end_date {
        w.end_date = v;
    }
    if let Some(v) = c.position {
        check_position(&v)?;
        w.position = v;
    }
    check_week(&mut tx, &user, &mut w).await?;
    w.updated_at = now();
    w.rev = next_rev(&mut tx).await?;
    upsert_week_template(&mut tx, &w).await?;
    let mut changes = vec![Change::week_template(&w)];
    if was_default || w.mode == "default" {
        blocks::weekdays_from_default(&mut tx, user.id(), &mut changes).await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);
    blocks::materialize_user(&state, &user.user).await?;
    Ok(Json(w))
}

#[utoipa::path(delete, path = "/week-templates/{id}", tag = "days", summary = "Delete a week template (days already set up keep their blocks until it changes them)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_week(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut w = load_week(&mut tx, &user, &id).await?;
    w.deleted_at = Some(now());
    w.updated_at = now();
    w.rev = next_rev(&mut tx).await?;
    upsert_week_template(&mut tx, &w).await?;
    let mut changes = vec![Change::week_template(&w)];
    if w.mode == "default" {
        blocks::weekdays_from_default(&mut tx, user.id(), &mut changes).await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);
    blocks::materialize_user(&state, &user.user).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- Blocks of a day -----

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ApplyTemplate {
    /// `null` removes the day's blocks.
    template_id: Option<String>,
}

#[utoipa::path(post, path = "/days/{date}/apply-template", tag = "days", summary = "Give a day a template's blocks (or none)", params(("date" = String, Path, description = "YYYY-MM-DD")), request_body = ApplyTemplate, responses((status = 200, body = Vec<TimeBlock>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn apply_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
    Json(c): Json<ApplyTemplate>,
) -> ApiResult<Json<Vec<TimeBlock>>> {
    parse_date(&date)?;
    let mut tx = state.db.write.begin().await?;
    let tpl = match &c.template_id {
        Some(id) => Some(load_template(&mut tx, &user, id).await?),
        None => None,
    };
    let mut changes = blocks::apply(&mut tx, user.id(), &date, tpl.as_ref(), false).await?;
    if let Some(t) = &tpl {
        changes.extend(blocks::place_items(&mut tx, &user.user, parse_date(&date)?, t).await?);
    }
    blocks::mark_manual(&mut tx, user.id(), &date).await?;
    let now_blocks: Vec<TimeBlock> = sqlx::query_as("SELECT * FROM time_blocks WHERE user_id = ? AND date = ? AND deleted_at IS NULL ORDER BY start_time")
        .bind(user.id())
        .bind(&date)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(Json(now_blocks))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateBlock {
    id: Option<String>,
    title: String,
    start_time: String,
    end_time: String,
    energy: Option<String>,
}

#[utoipa::path(post, path = "/days/{date}/blocks", tag = "days", summary = "Add a time block to a day", params(("date" = String, Path, description = "YYYY-MM-DD")), request_body = CreateBlock, responses((status = 200, body = TimeBlock), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_block(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
    Json(c): Json<CreateBlock>,
) -> ApiResult<Json<TimeBlock>> {
    parse_date(&date)?;
    check_block(&c.title, &c.start_time, &c.end_time, &c.energy)?;
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if let Some(b) = sqlx::query_as::<_, TimeBlock>("SELECT * FROM time_blocks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        return if b.user_id == user.id() {
            Ok(Json(b))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    blocks::mark_manual(&mut tx, user.id(), &date).await?;
    let ts = now();
    let b = TimeBlock {
        id,
        user_id: user.id().into(),
        date,
        title: c.title.trim().into(),
        start_time: c.start_time,
        end_time: c.end_time,
        energy: c.energy,
        template_id: None,
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_time_block(&mut tx, &b).await?;
    tx.commit().await?;
    state.bus.publish([Change::time_block(&b)]);
    Ok(Json(b))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchBlock {
    title: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    energy: Option<Option<String>>,
}

async fn load_block(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<TimeBlock> {
    sqlx::query_as("SELECT * FROM time_blocks WHERE id = ? AND user_id = ? AND deleted_at IS NULL")
        .bind(id)
        .bind(user.id())
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

#[utoipa::path(patch, path = "/time-blocks/{id}", tag = "days", summary = "Move, resize or rename a time block", params(("id" = String, Path, description = "ULID")), request_body = PatchBlock, responses((status = 200, body = TimeBlock), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_block(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchBlock>,
) -> ApiResult<Json<TimeBlock>> {
    let mut tx = state.db.write.begin().await?;
    let mut b = load_block(&mut tx, &user, &id).await?;
    if let Some(v) = c.title {
        b.title = v.trim().into();
    }
    if let Some(v) = c.start_time {
        b.start_time = v;
    }
    if let Some(v) = c.end_time {
        b.end_time = v;
    }
    if let Some(v) = c.energy {
        b.energy = v;
    }
    check_block(&b.title, &b.start_time, &b.end_time, &b.energy)?;
    blocks::mark_manual(&mut tx, user.id(), &b.date).await?;
    b.updated_at = now();
    b.rev = next_rev(&mut tx).await?;
    upsert_time_block(&mut tx, &b).await?;
    tx.commit().await?;
    state.bus.publish([Change::time_block(&b)]);
    Ok(Json(b))
}

#[utoipa::path(delete, path = "/time-blocks/{id}", tag = "days", summary = "Remove a time block", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_block(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut b = load_block(&mut tx, &user, &id).await?;
    blocks::mark_manual(&mut tx, user.id(), &b.date).await?;
    b.deleted_at = Some(now());
    b.updated_at = now();
    b.rev = next_rev(&mut tx).await?;
    upsert_time_block(&mut tx, &b).await?;
    tx.commit().await?;
    state.bus.publish([Change::time_block(&b)]);
    Ok(StatusCode::NO_CONTENT)
}

// ----- Suggest -----

/// A suggested time for one task, with the reasons (codes: overdue, due_today,
/// due_tomorrow, due_soon {days}, expires_today, urgent, important, hard_in_hard_block,
/// easy_in_easy_block, earliest_free_time, estimate_assumed).
#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct Suggestion {
    pub task_id: String,
    pub block_id: Option<String>,
    /// `HH:MM`
    pub start_time: String,
    pub duration_min: u32,
    #[ts(type = "Array<{ code: string, days?: number }>")]
    #[schema(value_type = Vec<Object>)]
    pub reasons: serde_json::Value,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct Unplaced {
    pub task_id: String,
    /// `blocked` (waiting for a prerequisite) or `no_room` (with `minutes`).
    #[ts(type = "{ code: 'blocked' } | { code: 'no_room', minutes: number }")]
    #[schema(value_type = Object)]
    pub reason: serde_json::Value,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct SuggestedPlan {
    pub suggestions: Vec<Suggestion>,
    pub unplaced: Vec<Unplaced>,
}

/// Times for the day's planned tasks that don't have one yet. Nothing is changed: the
/// client applies what the user accepts.
#[utoipa::path(post, path = "/days/{date}/suggest", tag = "days", summary = "Suggest times for the day's unscheduled planned tasks (nothing is saved)", params(("date" = String, Path, description = "YYYY-MM-DD")), responses((status = 200, body = SuggestedPlan), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn suggest(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
) -> ApiResult<Json<SuggestedPlan>> {
    let day = parse_date(&date)?;
    let mut conn = state.db.read.acquire().await?;
    let u = &user.user;
    let items = blocks::day_items(&mut conn, u, day).await?.items;
    use streamline_domain::conflicts::Kind;
    let busy: Vec<(u32, u32)> = items
        .iter()
        .filter(|i| i.kind != Kind::Block)
        .map(|i| (i.start, i.end))
        .collect();
    let tbs: Vec<TimeBlock> = sqlx::query_as("SELECT * FROM time_blocks WHERE user_id = ? AND date = ? AND deleted_at IS NULL ORDER BY start_time")
        .bind(user.id())
        .bind(&date)
        .fetch_all(&mut *conn)
        .await?;
    let energy = |e: &Option<String>| match e.as_deref() {
        Some("hard") => Some(Energy::Hard),
        Some("medium") => Some(Energy::Medium),
        Some("easy") => Some(Energy::Easy),
        _ => None,
    };
    let block_list: Vec<Block> = tbs
        .iter()
        .filter_map(|b| {
            Some(Block {
                id: b.id.clone(),
                start: minutes(&b.start_time)?,
                end: minutes(&b.end_time)?,
                energy: energy(&b.energy),
            })
        })
        .collect();
    let tasks = blocks::unscheduled(&mut conn, user.id(), &date).await?;
    let behaviors: std::collections::HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, day_end_behavior FROM task_types")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();
    let candidates = tasks
        .iter()
        .map(|t| Candidate {
            id: t.id.clone(),
            estimate: t.estimate_min.filter(|&m| m > 0).map(|m| m as u32),
            difficulty: t.difficulty.map(|d| d.clamp(1, 3) as u8),
            importance: t.importance.unwrap_or(0).clamp(0, 3) as u8,
            urgency: t.urgency.unwrap_or(0).clamp(0, 3) as u8,
            due_in_days: t
                .due_date
                .as_deref()
                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .map(|d| (d - day).num_days()),
            expires_today: behaviors
                .get(&t.task_type_id)
                .is_some_and(|b| b == "expire"),
            blocked: t.blocked,
        })
        .collect();
    let window = (
        minutes(&u.day_window_start).unwrap_or(8 * 60),
        minutes(&u.day_window_end).unwrap_or(22 * 60),
    );
    let window = if window.1 > window.0 {
        window
    } else {
        (window.0, 1440)
    };
    let today = crate::rollover::today_for(u);
    let from = if day == today {
        let tz = streamline_domain::time::parse_tz(&u.timezone).unwrap_or(chrono_tz::UTC);
        let t = chrono::Utc::now().with_timezone(&tz).time();
        use chrono::Timelike;
        // Round up to the next quarter hour.
        (t.hour() * 60 + t.minute()).div_ceil(15) * 15
    } else if day < today {
        return Err(bad("can't plan a past day"));
    } else {
        0
    };
    let plan = planner::suggest(&Input {
        blocks: block_list,
        busy,
        tasks: candidates,
        window,
        from,
    });
    Ok(Json(SuggestedPlan {
        suggestions: plan
            .placements
            .into_iter()
            .map(|p| Suggestion {
                task_id: p.task_id,
                block_id: p.block_id,
                start_time: blocks::hhmm(p.start),
                duration_min: p.minutes,
                reasons: serde_json::to_value(&p.reasons).unwrap_or_default(),
            })
            .collect(),
        unplaced: plan
            .leftovers
            .into_iter()
            .map(|(task_id, reason)| Unplaced {
                task_id,
                reason: serde_json::to_value(&reason).unwrap_or_default(),
            })
            .collect(),
    }))
}
