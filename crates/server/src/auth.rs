//! Password hashing, sessions (cookie) and API tokens (bearer), and the
//! `AuthUser` extractor used by every protected route.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, Method, header, request::Parts},
};

use crate::{
    AppState,
    config::CookieSecure,
    error::{AppError, bad},
    models::User,
    util::{new_id, now, random_token, sha256_hex},
};

pub const SESSION_COOKIE: &str = "sl_session";
pub const TOKEN_PREFIX: &str = "slt_";
const MAX_FAILURES: u32 = 10;
const FAILURE_WINDOW: Duration = Duration::from_secs(15 * 60);

pub fn hash_password(pw: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hash failed: {e}"))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}

/// Hashing is deliberately slow; keep it off the async worker threads.
pub async fn hash_password_async(pw: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&pw)).await?
}

pub async fn verify_password_async(pw: String, hash: Option<String>) -> bool {
    // Verify against a dummy hash for unknown users so timing doesn't reveal usernames.
    static DUMMY: OnceLock<String> = OnceLock::new();
    tokio::task::spawn_blocking(move || {
        let hash = hash.unwrap_or_else(|| {
            DUMMY
                .get_or_init(|| hash_password("dummy-password").unwrap())
                .clone()
        });
        verify_password(&pw, &hash)
    })
    .await
    .unwrap_or(false)
}

pub fn validate_new_password(pw: &str) -> Result<(), AppError> {
    if pw.chars().count() < 8 {
        return Err(bad("password must be at least 8 characters"));
    }
    Ok(())
}

pub fn validate_username(u: &str) -> Result<(), AppError> {
    let ok = !u.is_empty()
        && u.len() <= 64
        && u.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(bad(
            "username may contain letters, digits, '.', '_' and '-'",
        ))
    }
}

pub async fn create_user(
    state: &AppState,
    username: &str,
    password: &str,
    display_name: Option<&str>,
    is_admin: bool,
) -> Result<User, AppError> {
    validate_username(username)?;
    validate_new_password(password)?;
    let hash = hash_password_async(password.to_string()).await?;
    let mut tx = state.db.write.begin().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("username already taken".into()));
    }
    let rev = crate::db::next_rev(&mut tx).await?;
    let id = new_id();
    let ts = now();
    sqlx::query(
        "INSERT INTO users (id, username, display_name, password_hash, is_admin, created_at, updated_at, rev)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(username)
    .bind(display_name.filter(|d| !d.trim().is_empty()).unwrap_or(username))
    .bind(&hash)
    .bind(is_admin)
    .bind(&ts)
    .bind(&ts)
    .bind(rev)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    load_user(state, &id).await?.ok_or(AppError::NotFound)
}

pub async fn load_user(state: &AppState, id: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as("SELECT * FROM users WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(&state.db.read)
        .await
}

pub async fn user_count(state: &AppState) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL")
        .fetch_one(&state.db.read)
        .await
}

/// Create the first admin from `INITIAL_ADMIN_USER`/`INITIAL_ADMIN_PASSWORD` if the
/// database has no users yet. Without them, the web UI offers a first-run setup.
pub async fn bootstrap_admin(state: &AppState) -> anyhow::Result<()> {
    if user_count(state).await? > 0 {
        return Ok(());
    }
    match (
        &state.config.initial_admin_user,
        &state.config.initial_admin_password,
    ) {
        (Some(u), Some(p)) => {
            create_user(state, u, p, None, true)
                .await
                .map_err(|e| anyhow::anyhow!("creating initial admin: {e:?}"))?;
            tracing::info!("created initial admin user {u:?}");
        }
        _ => tracing::info!("no users yet: open the web UI to create the admin account"),
    }
    Ok(())
}

// ---- sessions ---------------------------------------------------------------

pub async fn create_session(
    state: &AppState,
    user_id: &str,
    user_agent: Option<&str>,
) -> Result<String, AppError> {
    let token = random_token();
    let expires = chrono::Utc::now() + chrono::Duration::days(state.config.session_days);
    sqlx::query("INSERT INTO sessions (token_hash, user_id, created_at, expires_at, user_agent) VALUES (?,?,?,?,?)")
        .bind(sha256_hex(&token))
        .bind(user_id)
        .bind(now())
        .bind(expires.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .bind(user_agent.map(|s| s.chars().take(200).collect::<String>()))
        .execute(&state.db.write)
        .await?;
    Ok(token)
}

pub fn session_cookie(state: &AppState, headers: &HeaderMap, token: &str) -> String {
    let max_age = state.config.session_days * 86400;
    let secure = if is_secure(state, headers) {
        "; Secure"
    } else {
        ""
    };
    format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

pub fn clear_cookie(state: &AppState, headers: &HeaderMap) -> String {
    let secure = if is_secure(state, headers) {
        "; Secure"
    } else {
        ""
    };
    format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}")
}

/// Whether the client reached us over HTTPS. We never terminate TLS ourselves, so this
/// is only known through a trusted reverse proxy, or forced by `COOKIE_SECURE`.
pub fn is_secure(state: &AppState, headers: &HeaderMap) -> bool {
    match state.config.cookie_secure {
        CookieSecure::Always => true,
        CookieSecure::Never => false,
        CookieSecure::Auto => {
            state.config.trust_proxy
                && headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| {
                        v.split(',')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .eq_ignore_ascii_case("https")
                    })
        }
    }
}

pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

// ---- login throttling ---------------------------------------------------------

pub async fn check_throttle(state: &AppState, keys: &[String]) -> Result<(), AppError> {
    let mut map = state.login_failures.lock().await;
    map.retain(|_, (_, start)| start.elapsed() < FAILURE_WINDOW);
    if keys
        .iter()
        .any(|k| map.get(k).is_some_and(|(n, _)| *n >= MAX_FAILURES))
    {
        return Err(AppError::TooManyRequests);
    }
    Ok(())
}

pub async fn record_failure(state: &AppState, keys: &[String]) {
    let mut map = state.login_failures.lock().await;
    for k in keys {
        map.entry(k.clone()).or_insert((0, Instant::now())).0 += 1;
    }
}

pub async fn clear_failures(state: &AppState, keys: &[String]) {
    let mut map = state.login_failures.lock().await;
    for k in keys {
        map.remove(k);
    }
}

// ---- extractor ------------------------------------------------------------------

/// The authenticated user, from a bearer API token or the session cookie.
pub struct AuthUser {
    pub user: User,
    pub session_hash: Option<String>,
    /// Groups the user belongs to (for visibility checks).
    pub groups: Vec<String>,
}

async fn groups_of(state: &AppState, user_id: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT group_id FROM group_members WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(&state.db.read)
        .await
}

impl AuthUser {
    pub fn id(&self) -> &str {
        &self.user.id
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        if let Some(token) = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        {
            let hash = sha256_hex(token.trim());
            let user_id: Option<String> = sqlx::query_scalar(
                "SELECT user_id FROM api_tokens WHERE token_hash = ? AND revoked_at IS NULL",
            )
            .bind(&hash)
            .fetch_optional(&state.db.read)
            .await?;
            let user_id = user_id.ok_or(AppError::Unauthorized)?;
            let user = load_user(state, &user_id)
                .await?
                .ok_or(AppError::Unauthorized)?;
            let _ = sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE token_hash = ?")
                .bind(now())
                .bind(&hash)
                .execute(&state.db.write)
                .await;
            let groups = groups_of(state, &user.id).await?;
            return Ok(AuthUser {
                user,
                session_hash: None,
                groups,
            });
        }

        let token = cookie_value(headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        let hash = sha256_hex(token);
        let user_id: Option<String> = sqlx::query_scalar(
            "SELECT user_id FROM sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(&hash)
        .bind(now())
        .fetch_optional(&state.db.read)
        .await?;
        let user = load_user(state, &user_id.ok_or(AppError::Unauthorized)?)
            .await?
            .ok_or(AppError::Unauthorized)?;

        // CSRF defence for cookie auth: state-changing requests must come from our own origin.
        if !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
            check_origin(state, headers)?;
        }
        let groups = groups_of(state, &user.id).await?;
        Ok(AuthUser {
            user,
            session_hash: Some(hash),
            groups,
        })
    }
}

/// Reject cross-origin browser requests. Browsers always send `Origin` on
/// cross-origin and non-GET fetches; if it is missing, the request is not from a
/// browser page on another site.
pub fn check_origin(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };
    let origin_host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
    let header_host = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or("").trim())
    };
    let mut allowed = vec![];
    if let Some(h) = header_host(header::HOST.as_str()) {
        allowed.push(h);
    }
    if state.config.trust_proxy
        && let Some(h) = header_host("x-forwarded-host")
    {
        allowed.push(h);
    }
    if allowed.iter().any(|h| h.eq_ignore_ascii_case(origin_host)) {
        Ok(())
    } else {
        Err(AppError::Forbidden("cross-origin request rejected".into()))
    }
}
