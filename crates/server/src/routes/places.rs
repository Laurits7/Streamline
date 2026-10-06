//! Places (SPEC §6.16, D-45). The current place is chosen per device (manually or by
//! GPS) and only used to filter lists, so the server just stores the places.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use streamline_domain::order::key_after;

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{
        Place, Project, Series, Task, upsert_place, upsert_project, upsert_series, upsert_task,
    },
    util::{double_option, id_or_new, now},
    visibility,
};

pub async fn load_visible(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<Place> {
    let p: Place = sqlx::query_as("SELECT * FROM places WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    if !visibility::can_see(
        &user.user,
        p.owner_user_id.as_deref(),
        p.owner_group_id.as_deref(),
    ) {
        return Err(AppError::NotFound);
    }
    Ok(p)
}

/// Validate an optional place reference.
pub async fn check_place(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &Option<String>,
) -> ApiResult<()> {
    if let Some(id) = id {
        load_visible(conn, user, id)
            .await
            .map_err(|_| bad("unknown place"))?;
    }
    Ok(())
}

fn check_fields(name: &str, lat: Option<f64>, lon: Option<f64>, radius: i32) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(bad("name must be 1-100 characters"));
    }
    if lat.is_some() != lon.is_some() {
        return Err(bad("give both latitude and longitude, or neither"));
    }
    if lat.is_some_and(|v| !(-90.0..=90.0).contains(&v))
        || lon.is_some_and(|v| !(-180.0..=180.0).contains(&v))
    {
        return Err(bad("coordinates out of range"));
    }
    crate::util::check_range("radius_m", Some(radius), 20, 50_000)?;
    Ok(name.to_string())
}

#[utoipa::path(get, path = "/places", tag = "places", summary = "List places", responses((status = 200, body = Vec<Place>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Place>>> {
    let rows = sqlx::query_as(&format!(
        "SELECT * FROM places WHERE {} AND deleted_at IS NULL ORDER BY position",
        visibility::OWNED_VISIBLE_SQL
    ))
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreatePlace {
    id: Option<String>,
    name: String,
    lat: Option<f64>,
    lon: Option<f64>,
    radius_m: Option<i32>,
}

#[utoipa::path(post, path = "/places", tag = "places", summary = "Create a place", request_body = CreatePlace, responses((status = 200, body = Place), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreatePlace>,
) -> ApiResult<Json<Place>> {
    let radius = c.radius_m.unwrap_or(200);
    let name = check_fields(&c.name, c.lat, c.lon, radius)?;
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if let Some(existing) = sqlx::query_as::<_, Place>("SELECT * FROM places WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        if existing.owner_user_id.as_deref() == Some(user.id()) {
            return Ok(Json(existing));
        }
        return Err(AppError::Conflict("id already in use".into()));
    }
    let last: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM places WHERE owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(user.id())
    .fetch_one(&mut *tx)
    .await?;
    let ts = now();
    let p = Place {
        id,
        owner_user_id: Some(user.id().into()),
        owner_group_id: None,
        name,
        lat: c.lat,
        lon: c.lon,
        radius_m: radius,
        position: key_after(last.as_deref()),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_place(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::place(&p)]);
    Ok(Json(p))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchPlace {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    lat: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    lon: Option<Option<f64>>,
    radius_m: Option<i32>,
    position: Option<String>,
}

#[utoipa::path(patch, path = "/places/{id}", tag = "places", summary = "Rename a place or set its location", params(("id" = String, Path, description = "ULID")), request_body = PatchPlace, responses((status = 200, body = Place), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchPlace>,
) -> ApiResult<Json<Place>> {
    let mut tx = state.db.write.begin().await?;
    let mut p = load_visible(&mut tx, &user, &id).await?;
    if let Some(v) = c.name {
        p.name = v;
    }
    if let Some(v) = c.lat {
        p.lat = v;
    }
    if let Some(v) = c.lon {
        p.lon = v;
    }
    if let Some(v) = c.radius_m {
        p.radius_m = v;
    }
    if let Some(v) = c.position {
        crate::util::check_position(&v)?;
        p.position = v;
    }
    p.name = check_fields(&p.name, p.lat, p.lon, p.radius_m)?;
    p.updated_at = now();
    p.rev = next_rev(&mut tx).await?;
    upsert_place(&mut tx, &p).await?;
    tx.commit().await?;
    state.bus.publish([Change::place(&p)]);
    Ok(Json(p))
}

/// Delete a place; tasks, projects and routines that used it become "anywhere".
#[utoipa::path(delete, path = "/places/{id}", tag = "places", summary = "Delete a place (its tasks become 'anywhere')", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut p = load_visible(&mut tx, &user, &id).await?;
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    let mut changes = vec![];
    let tasks: Vec<Task> =
        sqlx::query_as("SELECT * FROM tasks WHERE place_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for mut t in tasks {
        t.place_id = None;
        t.updated_at = ts.clone();
        t.rev = rev;
        upsert_task(&mut tx, &t).await?;
        changes.push(Change::task(&t));
    }
    let projects: Vec<Project> =
        sqlx::query_as("SELECT * FROM projects WHERE default_place_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for mut x in projects {
        x.default_place_id = None;
        x.updated_at = ts.clone();
        x.rev = rev;
        upsert_project(&mut tx, &x).await?;
        changes.push(Change::project(&x));
    }
    let series: Vec<Series> =
        sqlx::query_as("SELECT * FROM series WHERE place_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    for mut x in series {
        x.place_id = None;
        x.updated_at = ts.clone();
        x.rev = rev;
        upsert_series(&mut tx, &x).await?;
        changes.push(Change::series(&x));
    }
    p.deleted_at = Some(ts.clone());
    p.updated_at = ts;
    p.rev = rev;
    upsert_place(&mut tx, &p).await?;
    changes.push(Change::place(&p));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}
