//! First-run setup, login/logout, profile, API tokens and admin user management.

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::{
    AppState,
    auth::{self, AuthUser},
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{ApiToken, Me, User},
    util::{new_id, now, random_token, sha256_hex},
};

#[derive(Serialize, utoipa::ToSchema)]
pub struct SetupStatus {
    pub needs_setup: bool,
}

/// A new API token. `token` is only ever shown here.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CreatedToken {
    pub token: String,
    pub info: ApiToken,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Credentials {
    username: String,
    password: String,
    display_name: Option<String>,
}

#[utoipa::path(get, path = "/setup", tag = "account", summary = "Whether first-run setup is needed", responses((status = 200, body = SetupStatus), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<SetupStatus>> {
    Ok(Json(SetupStatus {
        needs_setup: auth::user_count(&state).await? == 0,
    }))
}

/// Create the first (admin) account. Only possible while there are no users.
#[utoipa::path(post, path = "/setup", tag = "account", summary = "Create the first admin account and sign in", request_body = Credentials, responses((status = 200, body = Me), (status = 403, description = "Setup already done", body = crate::error::Problem), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[utoipa::path(post, path = "/auth/login", tag = "account", summary = "Sign in (sets the session cookie)", request_body = Credentials, responses((status = 200, body = Me), (status = 429, description = "Too many failed attempts", body = crate::error::Problem), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[utoipa::path(post, path = "/auth/logout", tag = "account", summary = "Sign out this session", responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[utoipa::path(get, path = "/me", tag = "account", summary = "The signed-in user", responses((status = 200, body = Me), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn me(user: AuthUser) -> Json<Me> {
    Json(Me::from(&user.user))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchMe {
    display_name: Option<String>,
    timezone: Option<String>,
    day_end: Option<String>,
    locale: Option<String>,
    week_start: Option<i32>,
    plan_mode: Option<String>,
    plan_time_evening: Option<String>,
    plan_time_morning: Option<String>,
    day_window_start: Option<String>,
    day_window_end: Option<String>,
    focus_work_min: Option<i32>,
    focus_short_break_min: Option<i32>,
    focus_long_break_min: Option<i32>,
    focus_long_every: Option<i32>,
}

#[utoipa::path(patch, path = "/me", tag = "account", summary = "Update profile and preferences", request_body = PatchMe, responses((status = 200, body = Me), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
    if let Some(l) = p.locale {
        let ok = l.is_empty()
            || (l.len() <= 35
                && l.split('-').all(|part| {
                    !part.is_empty()
                        && part.len() <= 8
                        && part.chars().all(|c| c.is_ascii_alphanumeric())
                }));
        if !ok {
            return Err(bad(
                "locale must be a language tag like en-GB, or empty for the browser default",
            ));
        }
        u.locale = l;
    }
    if let Some(w) = p.week_start {
        if !(1..=7).contains(&w) {
            return Err(bad("week_start must be 1 (Monday) to 7 (Sunday)"));
        }
        u.week_start = w;
    }
    if let Some(m) = p.plan_mode {
        streamline_domain::planning::PlanMode::parse(&m)
            .ok_or_else(|| bad("plan_mode must be evening, morning or both"))?;
        u.plan_mode = m;
    }
    for (value, field) in [
        (p.plan_time_evening, &mut u.plan_time_evening),
        (p.plan_time_morning, &mut u.plan_time_morning),
        (p.day_window_start, &mut u.day_window_start),
        (p.day_window_end, &mut u.day_window_end),
    ] {
        if let Some(v) = value {
            crate::util::check_hhmm(&v)?;
            *field = v;
        }
    }
    for (value, field, name, hi) in [
        (
            p.focus_work_min,
            &mut u.focus_work_min,
            "focus_work_min",
            180,
        ),
        (
            p.focus_short_break_min,
            &mut u.focus_short_break_min,
            "focus_short_break_min",
            60,
        ),
        (
            p.focus_long_break_min,
            &mut u.focus_long_break_min,
            "focus_long_break_min",
            120,
        ),
        (
            p.focus_long_every,
            &mut u.focus_long_every,
            "focus_long_every",
            12,
        ),
    ] {
        if let Some(v) = value {
            crate::util::check_range(name, Some(v), 1, hi)?;
            *field = v;
        }
    }
    if u.day_window_start == u.day_window_end {
        return Err(bad("the available part of the day can't be empty"));
    }
    let mut tx = state.db.write.begin().await?;
    let rev = crate::db::next_rev(&mut tx).await?;
    sqlx::query(
        "UPDATE users SET display_name = ?, timezone = ?, day_end = ?, locale = ?, week_start = ?, plan_mode = ?, plan_time_evening = ?, plan_time_morning = ?, day_window_start = ?, day_window_end = ?, focus_work_min = ?, focus_short_break_min = ?, focus_long_break_min = ?, focus_long_every = ?, updated_at = ?, rev = ? WHERE id = ?",
    )
        .bind(&u.display_name)
        .bind(&u.timezone)
        .bind(&u.day_end)
        .bind(&u.locale)
        .bind(u.week_start)
    .bind(&u.plan_mode)
    .bind(&u.plan_time_evening)
    .bind(&u.plan_time_morning)
    .bind(&u.day_window_start)
    .bind(&u.day_window_end)
    .bind(u.focus_work_min)
    .bind(u.focus_short_break_min)
    .bind(u.focus_long_break_min)
    .bind(u.focus_long_every)
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

/// Merge UI preferences: top-level keys in the body replace stored ones; `null` removes a key.
#[utoipa::path(patch, path = "/me/prefs", tag = "account", summary = "Merge UI preferences (null removes a key)", request_body = Object, responses((status = 200, body = Me), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_prefs(
    State(state): State<AppState>,
    user: AuthUser,
    Json(patch): Json<serde_json::Map<String, serde_json::Value>>,
) -> ApiResult<Json<Me>> {
    let mut tx = state.db.write.begin().await?;
    // Re-read inside the transaction so concurrent merges don't lose keys.
    let stored: String = sqlx::query_scalar("SELECT prefs FROM users WHERE id = ?")
        .bind(user.id())
        .fetch_one(&mut *tx)
        .await?;
    let mut prefs: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&stored).unwrap_or_default();
    for (k, v) in patch {
        if k.len() > 100 {
            return Err(bad("preference keys are at most 100 characters"));
        }
        if v.is_null() {
            prefs.remove(&k);
        } else {
            prefs.insert(k, v);
        }
    }
    let json = serde_json::Value::Object(prefs).to_string();
    if json.len() > 64 * 1024 {
        return Err(bad("preferences are limited to 64 KB"));
    }
    let rev = crate::db::next_rev(&mut tx).await?;
    sqlx::query("UPDATE users SET prefs = ?, updated_at = ?, rev = ? WHERE id = ?")
        .bind(&json)
        .bind(now())
        .bind(rev)
        .bind(user.id())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut u = user.user;
    u.prefs = json;
    let me = Me::from(&u);
    state
        .bus
        .publish([Change::me(&u.id, serde_json::to_value(&me)?)]);
    Ok(Json(me))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ChangePassword {
    current_password: String,
    new_password: String,
}

#[utoipa::path(post, path = "/me/password", tag = "account", summary = "Change password (signs out other sessions)", request_body = ChangePassword, responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[utoipa::path(get, path = "/tokens", tag = "account", summary = "List API tokens", responses((status = 200, body = Vec<ApiToken>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateToken {
    name: String,
}

/// Returns the plaintext token exactly once.
#[utoipa::path(post, path = "/tokens", tag = "account", summary = "Create an API token (plaintext returned once)", request_body = CreateToken, responses((status = 200, body = CreatedToken), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_token(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateToken>,
) -> ApiResult<Json<CreatedToken>> {
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
    Ok(Json(CreatedToken {
        token,
        info: ApiToken {
            id,
            name: name.into(),
            created_at: ts,
            last_used_at: None,
        },
    }))
}

#[utoipa::path(delete, path = "/tokens/{id}", tag = "account", summary = "Revoke an API token", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[utoipa::path(get, path = "/users", tag = "users", summary = "List users (admin)", responses((status = 200, body = Vec<Me>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list_users(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Me>>> {
    require_admin(&user)?;
    let users: Vec<User> =
        sqlx::query_as("SELECT * FROM users WHERE deleted_at IS NULL ORDER BY username")
            .fetch_all(&state.db.read)
            .await?;
    Ok(Json(users.iter().map(Me::from).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateUser {
    username: String,
    password: String,
    display_name: Option<String>,
    #[serde(default)]
    is_admin: bool,
}

#[utoipa::path(post, path = "/users", tag = "users", summary = "Create a user (admin)", request_body = CreateUser, responses((status = 200, body = Me), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchUser {
    display_name: Option<String>,
    is_admin: Option<bool>,
    password: Option<String>,
}

#[utoipa::path(patch, path = "/users/{id}", tag = "users", summary = "Update a user (admin)", params(("id" = String, Path, description = "ULID")), request_body = PatchUser, responses((status = 200, body = Me), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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

#[utoipa::path(delete, path = "/users/{id}", tag = "users", summary = "Delete a user (admin)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
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
