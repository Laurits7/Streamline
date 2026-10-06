//! First-run setup, login/logout, profile, API tokens and admin user management.

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;

use crate::{
    AppState,
    auth::{self, AuthUser},
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{ApiToken, Me, User},
    util::{new_id, now, random_token, sha256_hex},
};

#[derive(Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
    display_name: Option<String>,
}

pub async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(
        json!({ "needs_setup": auth::user_count(&state).await? == 0 }),
    ))
}

/// Create the first (admin) account. Only possible while there are no users.
pub async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> ApiResult<Response> {
    auth::check_origin(&state, &headers)?;
    if auth::user_count(&state).await? > 0 {
        return Err(AppError::Forbidden("setup already completed".into()));
    }
    let user = auth::create_user(
        &state,
        c.username.trim(),
        &c.password,
        c.display_name.as_deref(),
        true,
    )
    .await?;
    login_response(&state, &headers, &user).await
}

async fn login_response(state: &AppState, headers: &HeaderMap, user: &User) -> ApiResult<Response> {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let token = auth::create_session(state, &user.id, ua).await?;
    let cookie = auth::session_cookie(state, headers, &token);
    Ok(([(header::SET_COOKIE, cookie)], Json(Me::from(user))).into_response())
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> ApiResult<Response> {
    auth::check_origin(&state, &headers)?;
    let username = c.username.trim().to_lowercase();
    let keys = vec![format!("u:{username}"), format!("ip:{}", addr.ip())];
    auth::check_throttle(&state, &keys).await?;
    let user: Option<User> =
        sqlx::query_as("SELECT * FROM users WHERE username = ? AND deleted_at IS NULL")
            .bind(&username)
            .fetch_optional(&state.db.read)
            .await?;
    let ok =
        auth::verify_password_async(c.password, user.as_ref().map(|u| u.password_hash.clone()))
            .await;
    match user {
        Some(user) if ok => {
            auth::clear_failures(&state, &keys).await;
            login_response(&state, &headers, &user).await
        }
        _ => {
            auth::record_failure(&state, &keys).await;
            Err(AppError::Unauthorized)
        }
    }
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthUser,
) -> ApiResult<Response> {
    if let Some(hash) = &user.session_hash {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash)
            .execute(&state.db.write)
            .await?;
    }
    Ok((
        [(header::SET_COOKIE, auth::clear_cookie(&state, &headers))],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}

pub async fn me(user: AuthUser) -> Json<Me> {
    Json(Me::from(&user.user))
}

#[derive(Deserialize)]
pub struct PatchMe {
    display_name: Option<String>,
    timezone: Option<String>,
    day_end: Option<String>,
}

pub async fn patch_me(
    State(state): State<AppState>,
    user: AuthUser,
    Json(p): Json<PatchMe>,
) -> ApiResult<Json<Me>> {
    let mut u = user.user;
    if let Some(d) = p.display_name {
        let d = d.trim();
        if d.is_empty() || d.len() > 100 {
            return Err(bad("display name must be 1-100 characters"));
        }
        u.display_name = d.to_string();
    }
    if let Some(tz) = p.timezone {
        streamline_domain::time::parse_tz(&tz).ok_or_else(|| bad("unknown timezone"))?;
        u.timezone = tz;
    }
    if let Some(de) = p.day_end {
        crate::util::check_hhmm(&de)?;
        u.day_end = de;
    }
    let mut tx = state.db.write.begin().await?;
    let rev = crate::db::next_rev(&mut tx).await?;
    sqlx::query("UPDATE users SET display_name = ?, timezone = ?, day_end = ?, updated_at = ?, rev = ? WHERE id = ?")
        .bind(&u.display_name)
        .bind(&u.timezone)
        .bind(&u.day_end)
        .bind(now())
        .bind(rev)
        .bind(&u.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let me = Me::from(&u);
    state
        .bus
        .publish([Change::me(&u.id, serde_json::to_value(&me)?)]);
    Ok(Json(me))
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current_password: String,
    new_password: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Json(p): Json<ChangePassword>,
) -> ApiResult<StatusCode> {
    if !auth::verify_password_async(p.current_password, Some(user.user.password_hash.clone())).await
    {
        return Err(AppError::Forbidden("current password is wrong".into()));
    }
    auth::validate_new_password(&p.new_password)?;
    let hash = auth::hash_password_async(p.new_password).await?;
    // Sign out all other sessions.
    let mut tx = state.db.write.begin().await?;
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(hash)
        .bind(now())
        .bind(user.id())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash <> ?")
        .bind(user.id())
        .bind(user.session_hash.as_deref().unwrap_or(""))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- API tokens ---------------------------------------------------------------

pub async fn list_tokens(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ApiToken>>> {
    let rows = sqlx::query_as(
        "SELECT id, name, created_at, last_used_at FROM api_tokens WHERE user_id = ? AND revoked_at IS NULL ORDER BY created_at",
    )
    .bind(user.id())
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateToken {
    name: String,
}

/// Returns the plaintext token exactly once.
pub async fn create_token(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateToken>,
) -> ApiResult<Json<serde_json::Value>> {
    let name = c.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(bad("name must be 1-100 characters"));
    }
    let token = format!("{}{}", auth::TOKEN_PREFIX, random_token());
    let id = new_id();
    let ts = now();
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, created_at) VALUES (?,?,?,?,?)",
    )
    .bind(&id)
    .bind(user.id())
    .bind(name)
    .bind(sha256_hex(&token))
    .bind(&ts)
    .execute(&state.db.write)
    .await?;
    Ok(Json(
        json!({ "token": token, "info": ApiToken { id, name: name.into(), created_at: ts, last_used_at: None } }),
    ))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let r = sqlx::query(
        "UPDATE api_tokens SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(now())
    .bind(&id)
    .bind(user.id())
    .execute(&state.db.write)
    .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---- admin: users ---------------------------------------------------------------

fn require_admin(user: &AuthUser) -> ApiResult<()> {
    if user.user.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden("admin only".into()))
    }
}

pub async fn list_users(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Me>>> {
    require_admin(&user)?;
    let users: Vec<User> =
        sqlx::query_as("SELECT * FROM users WHERE deleted_at IS NULL ORDER BY username")
            .fetch_all(&state.db.read)
            .await?;
    Ok(Json(users.iter().map(Me::from).collect()))
}

#[derive(Deserialize)]
pub struct CreateUser {
    username: String,
    password: String,
    display_name: Option<String>,
    #[serde(default)]
    is_admin: bool,
}

pub async fn create_user(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateUser>,
) -> ApiResult<Json<Me>> {
    require_admin(&user)?;
    let u = auth::create_user(
        &state,
        c.username.trim(),
        &c.password,
        c.display_name.as_deref(),
        c.is_admin,
    )
    .await?;
    Ok(Json(Me::from(&u)))
}

#[derive(Deserialize)]
pub struct PatchUser {
    display_name: Option<String>,
    is_admin: Option<bool>,
    password: Option<String>,
}

pub async fn patch_user(
    State(state): State<AppState>,
    admin: AuthUser,
    Path(id): Path<String>,
    Json(p): Json<PatchUser>,
) -> ApiResult<Json<Me>> {
    require_admin(&admin)?;
    let mut u = auth::load_user(&state, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(d) = p.display_name {
        u.display_name = d.trim().to_string();
    }
    if let Some(a) = p.is_admin {
        if !a && u.id == admin.user.id {
            return Err(bad("you cannot remove your own admin rights"));
        }
        u.is_admin = a;
    }
    let mut tx = state.db.write.begin().await?;
    if let Some(pw) = p.password {
        auth::validate_new_password(&pw)?;
        let hash = auth::hash_password_async(pw).await?;
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(&u.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(&u.id)
            .execute(&mut *tx)
            .await?;
    }
    let rev = crate::db::next_rev(&mut tx).await?;
    sqlx::query(
        "UPDATE users SET display_name = ?, is_admin = ?, updated_at = ?, rev = ? WHERE id = ?",
    )
    .bind(&u.display_name)
    .bind(u.is_admin)
    .bind(now())
    .bind(rev)
    .bind(&u.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(Me::from(&u)))
}

pub async fn delete_user(
    State(state): State<AppState>,
    admin: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_admin(&admin)?;
    if id == admin.user.id {
        return Err(bad("you cannot delete yourself"));
    }
    let mut tx = state.db.write.begin().await?;
    let rev = crate::db::next_rev(&mut tx).await?;
    let ts = now();
    let r = sqlx::query("UPDATE users SET deleted_at = ?, updated_at = ?, rev = ? WHERE id = ? AND deleted_at IS NULL")
        .bind(&ts)
        .bind(&ts)
        .bind(rev)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE api_tokens SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL")
        .bind(&ts)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
