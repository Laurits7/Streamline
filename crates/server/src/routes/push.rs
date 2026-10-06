//! Web Push subscriptions (Phase 6.2): the server's public key, registering a device,
//! and a test notification.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    error::{ApiResult, bad},
    notify::{self, Notification},
    util::{new_id, now},
};

#[derive(Serialize, utoipa::ToSchema)]
pub struct PushKey {
    /// VAPID public key (base64url), the browser's `applicationServerKey`.
    public_key: String,
}

#[utoipa::path(get, path = "/push/key", tag = "account", summary = "The server's Web Push public key", responses((status = 200, body = PushKey), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn key(State(state): State<AppState>, _user: AuthUser) -> Json<PushKey> {
    Json(PushKey {
        public_key: state.vapid.public_key.clone(),
    })
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Keys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Subscribe {
    endpoint: String,
    keys: Keys,
}

#[utoipa::path(post, path = "/push/subscriptions", tag = "account", summary = "Register this device for push notifications", request_body = Subscribe, responses((status = 204, description = "Done"), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn subscribe(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Json(c): Json<Subscribe>,
) -> ApiResult<StatusCode> {
    let url = reqwest::Url::parse(&c.endpoint).map_err(|_| bad("bad endpoint"))?;
    let local = url.scheme() == "http" && url.host_str() == Some("127.0.0.1"); // tests
    if (url.scheme() != "https" && !local)
        || c.endpoint.len() > 1000
        || c.keys.p256dh.len() > 200
        || c.keys.auth.len() > 100
    {
        return Err(bad("bad subscription"));
    }
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>();
    // One row per endpoint; a device that changes hands moves to the new user.
    sqlx::query(
        "INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh, auth, user_agent, created_at) VALUES (?,?,?,?,?,?,?)
         ON CONFLICT(endpoint) DO UPDATE SET user_id = excluded.user_id, p256dh = excluded.p256dh, auth = excluded.auth,
           user_agent = excluded.user_agent, failures = 0",
    )
    .bind(new_id())
    .bind(user.id())
    .bind(&c.endpoint)
    .bind(&c.keys.p256dh)
    .bind(&c.keys.auth)
    .bind(ua)
    .bind(now())
    .execute(&state.db.write)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Unsubscribe {
    endpoint: String,
}

#[utoipa::path(post, path = "/push/subscriptions/remove", tag = "account", summary = "Stop push notifications to a device", request_body = Unsubscribe, responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn unsubscribe(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<Unsubscribe>,
) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ? AND user_id = ?")
        .bind(&c.endpoint)
        .bind(user.id())
        .execute(&state.db.write)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PushStatus {
    /// Devices registered for push.
    devices: i64,
    ntfy: bool,
}

#[utoipa::path(post, path = "/push/test", tag = "account", summary = "Send a test notification to all your devices", responses((status = 200, body = PushStatus), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn test(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<PushStatus>> {
    let devices: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM push_subscriptions WHERE user_id = ?")
            .bind(user.id())
            .fetch_one(&state.db.read)
            .await?;
    notify::send(
        &state,
        user.id(),
        &Notification {
            kind: "test".into(),
            title: "Streamline".into(),
            body: "Notifications work on this device.".into(),
            url: "/settings".into(),
        },
    );
    Ok(Json(PushStatus {
        devices,
        ntfy: user.user.ntfy_url.is_some(),
    }))
}
