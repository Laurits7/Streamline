//! Occasions (SPEC §6.17): the shared nameday calendar (everyone sees it; admins load it),
//! each user's people of interest, and what each kind of occasion creates.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use streamline_domain::occasions::parse_list;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{OccasionStep, OccasionTemplate, Person, upsert_occasion_template, upsert_person},
    occasions::{self, birthday_parts},
    routes::account::require_admin,
    util::{double_option, id_or_new, now},
};

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct NamedaySource {
    pub label: String,
    pub loaded_at: String,
    pub count: i64,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct NamedayDay {
    pub month: u32,
    pub day: u32,
    pub names: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct NamedayCalendar {
    /// `null` until a calendar has been loaded.
    pub source: Option<NamedaySource>,
    pub days: Vec<NamedayDay>,
}

#[utoipa::path(get, path = "/namedays", tag = "occasions", summary = "The whole nameday calendar", responses((status = 200, body = NamedayCalendar), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn calendar(
    State(state): State<AppState>,
    _user: AuthUser,
) -> ApiResult<Json<NamedayCalendar>> {
    let source = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT label, loaded_at, count FROM nameday_source WHERE id = 1",
    )
    .fetch_optional(&state.db.read)
    .await?
    .map(|(label, loaded_at, count)| NamedaySource {
        label,
        loaded_at,
        count,
    });
    let rows: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT month, day, name FROM namedays ORDER BY month, day, name")
            .fetch_all(&state.db.read)
            .await?;
    let mut days: Vec<NamedayDay> = vec![];
    for (m, d, name) in rows {
        match days.last_mut() {
            Some(last) if last.month == m as u32 && last.day == d as u32 => last.names.push(name),
            _ => days.push(NamedayDay {
                month: m as u32,
                day: d as u32,
                names: vec![name],
            }),
        }
    }
    Ok(Json(NamedayCalendar { source, days }))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    q: String,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct NameMatch {
    pub name: String,
    /// `MM-DD` of each nameday of this name.
    pub dates: Vec<String>,
}

#[utoipa::path(get, path = "/namedays/search", tag = "occasions", summary = "Find names in the nameday calendar (accents optional)", params(SearchQuery), responses((status = 200, body = Vec<NameMatch>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn search(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Vec<NameMatch>>> {
    let found = occasions::search(&mut *state.db.read.acquire().await?, &q.q, 20).await?;
    Ok(Json(
        found
            .into_iter()
            .map(|(name, dates)| NameMatch {
                name,
                dates: dates
                    .iter()
                    .map(|(m, d)| format!("{m:02}-{d:02}"))
                    .collect(),
            })
            .collect(),
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoadNamedays {
    /// A list to upload instead of downloading: one day per line, `MM-DD Name, Name`
    /// (or `DD.MM`). Leave out to download the official Estonian list.
    text: Option<String>,
    /// What to call an uploaded list (e.g. "Finnish").
    label: Option<String>,
}

#[utoipa::path(post, path = "/namedays", tag = "occasions", summary = "Load the nameday calendar: download the official list or upload one (admins)", request_body = LoadNamedays, responses((status = 200, body = NamedaySource), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 403, description = "Admins only", body = crate::error::Problem)))]
pub async fn load(
    State(state): State<AppState>,
    user: AuthUser,
    Json(b): Json<LoadNamedays>,
) -> ApiResult<Json<NamedaySource>> {
    require_admin(&user)?;
    match b.text {
        Some(text) => {
            let list = parse_list(&text).map_err(bad)?;
            let label = b.label.unwrap_or_else(|| "Uploaded list".into());
            occasions::load_namedays(&state, &list, label.trim()).await?;
        }
        None => {
            let url = state
                .config
                .namedays_url
                .clone()
                .unwrap_or_else(|| occasions::STAT_EE_URL.into());
            occasions::fetch_official(&state, &url)
                .await
                .map_err(|e| bad(format!("{e:#}")))?;
        }
    }
    let (label, loaded_at, count) =
        sqlx::query_as("SELECT label, loaded_at, count FROM nameday_source WHERE id = 1")
            .fetch_one(&state.db.read)
            .await?;
    Ok(Json(NamedaySource {
        label,
        loaded_at,
        count,
    }))
}

// ----- People -----

fn check_person(name: &str, nameday: &Option<String>, birthday: &Option<String>) -> ApiResult<()> {
    if name.trim().is_empty() || name.chars().count() > 100 {
        return Err(bad("name must be 1-100 characters"));
    }
    if nameday
        .as_ref()
        .is_some_and(|n| n.trim().is_empty() || n.len() > 60)
    {
        return Err(bad("nameday name must be 1-60 characters"));
    }
    if let Some(b) = birthday
        && birthday_parts(b).is_none()
    {
        return Err(bad(
            "birthday must be YYYY-MM-DD, or --MM-DD without a year",
        ));
    }
    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreatePerson {
    id: Option<String>,
    name: String,
    nameday_name: Option<String>,
    birthday: Option<String>,
}

#[utoipa::path(post, path = "/people", tag = "occasions", summary = "Add someone whose nameday or birthday matters to you", request_body = CreatePerson, responses((status = 200, body = Person), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_person(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreatePerson>,
) -> ApiResult<Json<Person>> {
    check_person(&c.name, &c.nameday_name, &c.birthday)?;
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if let Some(p) = sqlx::query_as::<_, Person>("SELECT * FROM people WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        return if p.owner_user_id == user.id() {
            Ok(Json(p))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    let ts = now();
    let p = Person {
        id,
        owner_user_id: user.id().into(),
        name: c.name.trim().into(),
        nameday_name: c.nameday_name.map(|n| n.trim().to_string()),
        birthday: c.birthday,
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_person(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::person(&p)]);
    occasions::materialize_user(&state, &user.user).await?;
    Ok(Json(p))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchPerson {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    nameday_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    birthday: Option<Option<String>>,
}

async fn load_person(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<Person> {
    sqlx::query_as("SELECT * FROM people WHERE id = ? AND owner_user_id = ? AND deleted_at IS NULL")
        .bind(id)
        .bind(user.id())
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

#[utoipa::path(patch, path = "/people/{id}", tag = "occasions", summary = "Change a person's name, nameday or birthday", params(("id" = String, Path, description = "ULID")), request_body = PatchPerson, responses((status = 200, body = Person), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_person(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchPerson>,
) -> ApiResult<Json<Person>> {
    let mut tx = state.db.write.begin().await?;
    let mut p = load_person(&mut tx, &user, &id).await?;
    if let Some(v) = c.name {
        p.name = v.trim().into();
    }
    if let Some(v) = c.nameday_name {
        p.nameday_name = v.map(|n| n.trim().to_string());
    }
    if let Some(v) = c.birthday {
        p.birthday = v;
    }
    check_person(&p.name, &p.nameday_name, &p.birthday)?;
    p.updated_at = now();
    p.rev = next_rev(&mut tx).await?;
    upsert_person(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::person(&p)]);
    occasions::materialize_user(&state, &user.user).await?;
    Ok(Json(p))
}

/// Remove a person. Their open occasion tasks that aren't due yet go too.
#[utoipa::path(delete, path = "/people/{id}", tag = "occasions", summary = "Remove a person (and their upcoming occasion tasks)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_person(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut p = load_person(&mut tx, &user, &id).await?;
    let ts = now();
    let mut changes = vec![];
    let today = crate::rollover::today_for(&user.user)
        .format("%Y-%m-%d")
        .to_string();
    let tasks: Vec<crate::models::Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE ext_source = 'occasion' AND ext_id LIKE ?1 || ':%' AND owner_user_id = ?2
           AND status = 'open' AND deleted_at IS NULL AND due_date >= ?3",
    )
    .bind(&p.id)
    .bind(user.id())
    .bind(&today)
    .fetch_all(&mut *tx)
    .await?;
    for mut t in tasks {
        t.deleted_at = Some(ts.clone());
        t.updated_at = ts.clone();
        t.rev = next_rev(&mut tx).await?;
        crate::models::upsert_task(&mut tx, &t).await?;
        changes.push(Change::task(&t));
    }
    p.deleted_at = Some(ts.clone());
    p.updated_at = ts;
    p.rev = next_rev(&mut tx).await?;
    upsert_person(&mut tx, &p).await?;
    changes.push(Change::person(&p));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}

// ----- Templates -----

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchTemplate {
    enabled: Option<bool>,
    steps: Option<Vec<OccasionStep>>,
}

#[utoipa::path(put, path = "/occasion-templates/{kind}", tag = "occasions", summary = "Change what a kind of occasion (nameday, birthday) creates", params(("kind" = String, Path, description = "nameday or birthday")), request_body = PatchTemplate, responses((status = 200, body = OccasionTemplate), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn put_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(kind): Path<String>,
    Json(c): Json<PatchTemplate>,
) -> ApiResult<Json<OccasionTemplate>> {
    if !occasions::KINDS.contains(&kind.as_str()) {
        return Err(AppError::NotFound);
    }
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    let mut t = occasions::templates(&mut tx, user.id(), &mut changes)
        .await?
        .into_iter()
        .find(|t| t.kind == kind)
        .ok_or(AppError::NotFound)?;
    if let Some(on) = c.enabled {
        t.enabled = on;
    }
    if let Some(steps) = c.steps {
        if steps.len() > 6 {
            return Err(bad("at most 6 steps"));
        }
        for (i, s) in steps.iter().enumerate() {
            if s.title.trim().is_empty() || s.title.chars().count() > 200 {
                return Err(bad("each step needs a title (up to 200 characters)"));
            }
            if !(-60..=30).contains(&s.offset_days) {
                return Err(bad("steps can be 60 days before to 30 days after"));
            }
            if i == 0 && s.after_previous {
                return Err(bad("the first step can't wait for a previous one"));
            }
            let ok: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM task_types WHERE id = ?1 AND deleted_at IS NULL AND (builtin = 1 OR owner_user_id = ?2))",
            )
            .bind(&s.task_type_id)
            .bind(user.id())
            .fetch_one(&mut *tx)
            .await?;
            if !ok {
                return Err(bad("unknown task type"));
            }
        }
        t.steps = sqlx::types::Json(
            steps
                .into_iter()
                .map(|s| OccasionStep {
                    title: s.title.trim().into(),
                    ..s
                })
                .collect(),
        );
    }
    t.updated_at = now();
    t.rev = next_rev(&mut tx).await?;
    upsert_occasion_template(&mut tx, &t).await?;
    tx.commit().await?;
    changes.push(Change::occasion_template(&t));
    state.bus.publish(changes);
    occasions::materialize_user(&state, &user.user).await?;
    Ok(Json(t))
}
