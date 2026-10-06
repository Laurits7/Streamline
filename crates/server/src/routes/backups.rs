//! Backups for admins (Phase 6.3).

use axum::{
    Json,
    extract::{Path, State},
    http::header,
    response::IntoResponse,
};

use crate::{
    AppState,
    auth::AuthUser,
    backup::{self, Backup},
    error::{ApiResult, AppError},
    routes::account::require_admin,
};

#[utoipa::path(get, path = "/admin/backups", tag = "users", summary = "List database backups (admins)", responses((status = 200, body = Vec<Backup>), (status = 403, description = "Admins only", body = crate::error::Problem)))]
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Backup>>> {
    require_admin(&user)?;
    Ok(Json(backup::list(&state)?))
}

#[utoipa::path(post, path = "/admin/backups", tag = "users", summary = "Back up the database now (admins)", responses((status = 200, body = Backup), (status = 403, description = "Admins only", body = crate::error::Problem)))]
pub async fn create(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Backup>> {
    require_admin(&user)?;
    Ok(Json(backup::run(&state).await?))
}

#[utoipa::path(get, path = "/admin/backups/{name}", tag = "users", summary = "Download a backup (admins)", params(("name" = String, Path, description = "streamline-YYYYMMDD-HHMMSS.db")), responses((status = 200, description = "The SQLite file", content_type = "application/vnd.sqlite3"), (status = 403, description = "Admins only", body = crate::error::Problem), (status = 404, description = "No such backup")))]
pub async fn download(
    State(state): State<AppState>,
    user: AuthUser,
    Path(name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&user)?;
    if !backup::valid_name(&name) {
        return Err(AppError::NotFound);
    }
    let bytes = tokio::fs::read(backup::dir(&state).join(&name))
        .await
        .map_err(|_| AppError::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.sqlite3".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        bytes,
    ))
}
