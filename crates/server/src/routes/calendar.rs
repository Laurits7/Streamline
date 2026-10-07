//! Calendar settings (SPEC §6.5): connect a CalDAV account, test it, sync now, and choose
//! which calendars to show. The password is write-only: it is stored encrypted and never
//! returned. Events themselves arrive through `/sync` and the live event stream.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    calsync::{self, account_for},
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{Calendar, CalendarAccount, CalendarAccountView, upsert_calendar},
    util::{double_option, new_id, now},
};

#[utoipa::path(get, path = "/calendar/account", tag = "calendar", summary = "Your calendar account (null if none)", responses((status = 200, body = Option<CalendarAccountView>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn get_account(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Option<CalendarAccountView>>> {
    let a = account_for(&mut *state.db.read.acquire().await?, user.id()).await?;
    Ok(Json(a.as_ref().map(CalendarAccountView::from)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AccountInput {
    /// The CalDAV server or calendar URL. `user:password@` in it is moved to the fields.
    url: String,
    #[serde(default)]
    username: Option<String>,
    /// Leave out to keep the stored password.
    #[serde(default)]
    password: Option<String>,
}

/// Clean URL and credentials from the input, falling back to the stored password.
fn same_origin(a: &str, b: &str) -> bool {
    matches!((reqwest::Url::parse(a), reqwest::Url::parse(b)), (Ok(a), Ok(b)) if a.origin() == b.origin())
}

fn credentials(
    state: &AppState,
    c: &AccountInput,
    stored: Option<&CalendarAccount>,
) -> ApiResult<(String, String, String)> {
    let (url, in_url) = crate::caldav::split_credentials(&c.url).map_err(|e| bad(e.to_string()))?;
    let (mut username, mut password) = in_url.unwrap_or_default();
    if let Some(u) = c.username.as_deref().filter(|u| !u.trim().is_empty()) {
        username = u.trim().to_string();
    }
    if let Some(p) = c.password.as_deref().filter(|p| !p.is_empty()) {
        password = p.to_string();
    } else if password.is_empty()
        && let Some(sealed) = stored
            // The saved password only goes back to the server it was saved for.
            .filter(|a| same_origin(&a.url, &url))
            .and_then(|a| a.secret.as_deref())
    {
        password = state
            .secrets
            .decrypt(sealed)
            .map_err(|e| bad(e.to_string()))?;
    }
    if url.len() > 2000 || username.len() > 200 || password.len() > 1000 {
        return Err(bad("too long"));
    }
    Ok((url, username, password))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TestResult {
    ok: bool,
    /// Names of the calendars found.
    calendars: Vec<String>,
    error: Option<String>,
}

#[utoipa::path(post, path = "/calendar/test", tag = "calendar", summary = "Try a connection without saving it", request_body = AccountInput, responses((status = 200, body = TestResult), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn test(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<AccountInput>,
) -> ApiResult<Json<TestResult>> {
    let stored = account_for(&mut *state.db.read.acquire().await?, user.id()).await?;
    let (url, username, password) = credentials(&state, &c, stored.as_ref())?;
    Ok(Json(
        match calsync::probe(&url, &username, &password).await {
            Ok(cals) => TestResult {
                ok: true,
                calendars: cals.into_iter().map(|c| c.name).collect(),
                error: None,
            },
            Err(e) => TestResult {
                ok: false,
                calendars: vec![],
                error: Some(format!("{e:#}")),
            },
        },
    ))
}

/// Save the account (connecting it, or changing its URL or credentials), then sync it.
#[utoipa::path(put, path = "/calendar/account", tag = "calendar", summary = "Connect or update your calendar account, then sync", request_body = AccountInput, responses((status = 200, body = CalendarAccountView), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn put_account(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<AccountInput>,
) -> ApiResult<Json<CalendarAccountView>> {
    let mut tx = state.db.write.begin().await?;
    let stored = account_for(&mut tx, user.id()).await?;
    let (url, username, password) = credentials(&state, &c, stored.as_ref())?;
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let mut changes = vec![];
    let account = match stored {
        Some(mut a) => {
            if a.url != url {
                // A different server: start over with its calendars.
                let cals: Vec<Calendar> = sqlx::query_as(
                    "SELECT * FROM calendars WHERE account_id = ? AND deleted_at IS NULL",
                )
                .bind(&a.id)
                .fetch_all(&mut *tx)
                .await?;
                for cal in &cals {
                    changes.extend(calsync::remove_calendar(&mut tx, cal).await?);
                }
            }
            a.url = url;
            a.username = username;
            a.secret = (!password.is_empty()).then(|| state.secrets.encrypt(&password));
            a.status = "new".into();
            a.last_error = None;
            a.updated_at = ts;
            a.rev = rev;
            a
        }
        None => CalendarAccount {
            id: new_id(),
            user_id: user.id().into(),
            kind: "caldav".into(),
            url,
            username,
            secret: (!password.is_empty()).then(|| state.secrets.encrypt(&password)),
            status: "new".into(),
            last_sync_at: None,
            last_error: None,
            created_at: ts.clone(),
            updated_at: ts,
            deleted_at: None,
            rev,
        },
    };
    sqlx::query(
        "INSERT INTO calendar_accounts (id, user_id, kind, url, username, secret, status, last_sync_at, last_error, created_at, updated_at, deleted_at, rev)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET url=excluded.url, username=excluded.username, secret=excluded.secret, status=excluded.status,
           last_error=excluded.last_error, updated_at=excluded.updated_at, rev=excluded.rev",
    )
    .bind(&account.id).bind(&account.user_id).bind(&account.kind).bind(&account.url).bind(&account.username)
    .bind(&account.secret).bind(&account.status).bind(&account.last_sync_at).bind(&account.last_error)
    .bind(&account.created_at).bind(&account.updated_at).bind(&account.deleted_at).bind(account.rev)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    state.bus.publish(changes);
    let synced = calsync::sync_account(&state, &account.id, true).await?;
    Ok(Json(CalendarAccountView::from(&synced)))
}

#[utoipa::path(delete, path = "/calendar/account", tag = "calendar", summary = "Disconnect your calendar account (its events disappear)", responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_account(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<StatusCode> {
    let _guard = state.calendar_lock.lock().await;
    let mut tx = state.db.write.begin().await?;
    let Some(a) = account_for(&mut tx, user.id()).await? else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let cals: Vec<Calendar> =
        sqlx::query_as("SELECT * FROM calendars WHERE account_id = ? AND deleted_at IS NULL")
            .bind(&a.id)
            .fetch_all(&mut *tx)
            .await?;
    let mut changes = vec![];
    for cal in &cals {
        changes.extend(calsync::remove_calendar(&mut tx, cal).await?);
    }
    let rev = next_rev(&mut tx).await?;
    let ts = now();
    sqlx::query("UPDATE calendar_accounts SET deleted_at = ?, updated_at = ?, secret = NULL, rev = ? WHERE id = ?")
        .bind(&ts)
        .bind(&ts)
        .bind(rev)
        .bind(&a.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    changes.push(Change::calendar_account(user.id(), None));
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/calendar/sync", tag = "calendar", summary = "Sync your calendars now", responses((status = 200, body = CalendarAccountView), (status = 404, description = "No calendar account"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn sync_now(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<CalendarAccountView>> {
    let a = account_for(&mut *state.db.read.acquire().await?, user.id())
        .await?
        .ok_or(AppError::NotFound)?;
    let synced = calsync::sync_account(&state, &a.id, true).await?;
    Ok(Json(CalendarAccountView::from(&synced)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchCalendar {
    enabled: Option<bool>,
    /// All-day events of this calendar block the whole day (free time and overlaps).
    all_day_busy: Option<bool>,
    /// `#rrggbb`, or null for the server's colour.
    #[serde(default, deserialize_with = "double_option")]
    user_color: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/calendars/{id}", tag = "calendar", summary = "Show or hide a calendar, or change its colour", params(("id" = String, Path, description = "ULID")), request_body = PatchCalendar, responses((status = 200, body = Calendar), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_calendar(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchCalendar>,
) -> ApiResult<Json<Calendar>> {
    let mut tx = state.db.write.begin().await?;
    let mut cal: Calendar = sqlx::query_as(
        "SELECT * FROM calendars WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(&id)
    .bind(user.id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    let mut changes = vec![];
    if let Some(color) = c.user_color {
        if let Some(v) = &color
            && !(v.len() == 7
                && v.starts_with('#')
                && v[1..].chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(bad("colour must look like #3b82f6"));
        }
        cal.user_color = color;
    }
    if let Some(v) = c.all_day_busy {
        cal.all_day_busy = v;
    }
    let turned_on = c.enabled == Some(true) && !cal.enabled;
    if let Some(on) = c.enabled
        && on != cal.enabled
    {
        cal.enabled = on;
        if !on {
            changes.extend(calsync::clear_calendar(&mut tx, &cal).await?);
        }
        cal.ctag = None;
        cal.expanded_for = None;
    }
    cal.updated_at = now();
    cal.rev = next_rev(&mut tx).await?;
    upsert_calendar(&mut tx, &cal).await?;
    tx.commit().await?;
    changes.push(Change::calendar(&cal));
    state.bus.publish(changes);
    if turned_on {
        let (state, account) = (state.clone(), cal.account_id.clone());
        tokio::spawn(async move {
            if let Err(e) = calsync::sync_account(&state, &account, false).await {
                tracing::warn!("calendar sync failed: {e:#}");
            }
        });
    }
    Ok(Json(cal))
}
