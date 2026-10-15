//! Printed day sheets (7.3, D-14). Printing a day creates a sheet: its tasks with short
//! codes, and a QR code to a reconcile page. The page works without signing in, but only
//! for 48 hours and only to complete the tasks printed on that sheet, as the sheet's owner
//! (recorded as coming from paper).

use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::{
    AppState,
    auth::AuthUser,
    error::{ApiResult, AppError, bad},
    util::{new_id, now, parse_date, random_token, sha256_hex},
};

/// How long a sheet's link works.
const VALID_HOURS: i64 = 48;

#[derive(Serialize, Deserialize, Clone, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct SheetItem {
    pub code: String,
    pub task_id: String,
}

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct PrintSheet {
    pub id: String,
    pub date: String,
    pub items: Vec<SheetItem>,
    /// The reconcile page, for the QR code.
    pub url: String,
    /// The QR code of `url` as an SVG image.
    pub qr_svg: String,
    pub expires_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct NewSheet {
    /// `YYYY-MM-DD`
    date: String,
}

/// Where this server is reached: `PUBLIC_URL`, else the address the request came to.
fn base_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(u) = &state.config.public_url {
        return u.trim_end_matches('/').to_string();
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').next().unwrap_or("").trim().to_string())
    };
    let host = state
        .config
        .trust_proxy
        .then(|| header("x-forwarded-host"))
        .flatten()
        .or_else(|| header("host"))
        .unwrap_or_else(|| "localhost".into());
    let scheme = if crate::auth::is_secure(state, headers) {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{host}")
}

/// Print a day: the open and done tasks planned for it, in plan order (timed ones first).
#[utoipa::path(post, path = "/print/sheets", tag = "days", summary = "Make a printable sheet of a day, with a QR code to tick items off later", request_body = NewSheet, responses((status = 200, body = PrintSheet), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Json(c): Json<NewSheet>,
) -> ApiResult<Json<PrintSheet>> {
    parse_date(&c.date)?;
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT t.id FROM day_entries e JOIN tasks t ON t.id = e.task_id
          WHERE e.user_id = ?1 AND e.date = ?2 AND e.deleted_at IS NULL AND t.deleted_at IS NULL AND t.status IN ('open', 'done')
          ORDER BY e.start_time IS NULL, e.start_time, e.position",
    )
    .bind(user.id())
    .bind(&c.date)
    .fetch_all(&state.db.read)
    .await?;
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let items: Vec<SheetItem> = streamline_domain::print::codes(&refs)
        .into_iter()
        .zip(ids.iter())
        .map(|(code, id)| SheetItem {
            code,
            task_id: id.clone(),
        })
        .collect();
    let token = random_token();
    let id = new_id();
    let expires = (chrono::Utc::now() + chrono::Duration::hours(VALID_HOURS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query("INSERT INTO print_sheets (id, user_id, date, items, token_hash, created_at, expires_at) VALUES (?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(user.id())
        .bind(&c.date)
        .bind(serde_json::to_string(&items)?)
        .bind(sha256_hex(&token))
        .bind(now())
        .bind(&expires)
        .execute(&state.db.write)
        .await?;
    // Old sheets go once they've expired.
    let _ = sqlx::query("DELETE FROM print_sheets WHERE expires_at < ?")
        .bind(now())
        .execute(&state.db.write)
        .await;
    let url = format!("{}/r/{token}", base_url(&state, &headers));
    let qr_svg = qrcode::QrCode::new(url.as_bytes())
        .map_err(|e| anyhow::anyhow!("QR code: {e}"))?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(160, 160)
        .quiet_zone(false)
        .build();
    Ok(Json(PrintSheet {
        id,
        date: c.date,
        items,
        url,
        qr_svg,
        expires_at: expires,
    }))
}

#[derive(sqlx::FromRow)]
struct SheetRow {
    user_id: String,
    date: String,
    items: String,
    expires_at: String,
}

async fn sheet(state: &AppState, token: &str) -> ApiResult<SheetRow> {
    let row: Option<SheetRow> = sqlx::query_as(
        "SELECT user_id, date, items, expires_at FROM print_sheets WHERE token_hash = ?",
    )
    .bind(sha256_hex(token))
    .fetch_optional(&state.db.read)
    .await?;
    match row {
        Some(r) if r.expires_at > now() => Ok(r),
        // Unknown and expired look the same.
        _ => Err(AppError::NotFound),
    }
}

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ReconcileItem {
    pub code: String,
    pub title: String,
    pub start_time: Option<String>,
    pub done: bool,
    /// No longer something to tick: deleted, skipped, missed or won't do.
    pub gone: bool,
}

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Reconcile {
    pub date: String,
    pub name: String,
    pub expires_at: String,
    pub items: Vec<ReconcileItem>,
}

async fn view(state: &AppState, row: &SheetRow) -> ApiResult<Reconcile> {
    let items: Vec<SheetItem> = serde_json::from_str(&row.items).map_err(anyhow::Error::from)?;
    let mut out = Vec::new();
    for it in items {
        let t: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT t.title, t.status, (SELECT e.start_time FROM day_entries e WHERE e.task_id = t.id AND e.date = ?2 AND e.deleted_at IS NULL)
               FROM tasks t WHERE t.id = ?1 AND t.deleted_at IS NULL",
        )
        .bind(&it.task_id)
        .bind(&row.date)
        .fetch_optional(&state.db.read)
        .await?;
        let (title, status, start_time) =
            t.unwrap_or_else(|| ("(deleted)".into(), "deleted".into(), None));
        out.push(ReconcileItem {
            code: it.code,
            title,
            start_time,
            done: status == "done",
            gone: status != "done" && status != "open",
        });
    }
    let name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
        .bind(&row.user_id)
        .fetch_optional(&state.db.read)
        .await?
        .unwrap_or_default();
    Ok(Reconcile {
        date: row.date.clone(),
        name,
        expires_at: row.expires_at.clone(),
        items: out,
    })
}

/// The reconcile page's data: no sign-in, the link is the key.
#[utoipa::path(get, path = "/reconcile/{token}", tag = "days", summary = "A printed sheet's items (no sign-in; the link expires after 48 h)", params(("token" = String, Path, description = "From the sheet's QR code")), responses((status = 200, body = Reconcile), (status = 404, description = "Unknown or expired", body = crate::error::Problem)), security(()))]
pub async fn get(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<Reconcile>> {
    let row = sheet(&state, &token).await?;
    Ok(Json(view(&state, &row).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Done {
    /// Codes of the printed items that were done.
    codes: Vec<String>,
}

/// Mark printed items done. Only items on this sheet, only open ones, as the sheet's owner.
#[utoipa::path(post, path = "/reconcile/{token}", tag = "days", summary = "Complete items of a printed sheet (no sign-in; only that sheet's open items)", params(("token" = String, Path, description = "From the sheet's QR code")), request_body = Done, responses((status = 200, body = Reconcile), (status = 404, description = "Unknown or expired", body = crate::error::Problem)), security(()))]
pub async fn complete(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(d): Json<Done>,
) -> ApiResult<Json<Reconcile>> {
    let row = sheet(&state, &token).await?;
    if d.codes.len() > 200 {
        return Err(bad("too many items"));
    }
    let items: Vec<SheetItem> = serde_json::from_str(&row.items).map_err(anyhow::Error::from)?;
    let user: crate::models::User =
        sqlx::query_as("SELECT * FROM users WHERE id = ? AND deleted_at IS NULL")
            .bind(&row.user_id)
            .fetch_optional(&state.db.read)
            .await?
            .ok_or(AppError::NotFound)?;
    let groups = crate::auth::groups_of(&state, &user.id).await?;
    for it in items.iter().filter(|i| d.codes.contains(&i.code)) {
        let open: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ? AND status = 'open' AND deleted_at IS NULL)")
            .bind(&it.task_id)
            .fetch_one(&state.db.read)
            .await?;
        if !open {
            continue;
        }
        // A task of a project shared from another server is completed there (Phase 8).
        if let Some(res) = crate::federation::write_as(
            &state,
            &user.id,
            axum::http::Method::PATCH,
            &format!("/tasks/{}", it.task_id),
            json!({"status": "done"}),
        )
        .await
        {
            if res.is_ok() {
                sqlx::query(
                    "UPDATE task_events SET data = ?2 WHERE rowid = (SELECT rowid FROM task_events WHERE task_id = ?1 AND kind = 'completed' ORDER BY at DESC LIMIT 1)",
                )
                .bind(&it.task_id)
                .bind(json!({"source": "print"}).to_string())
                .execute(&state.db.write)
                .await?;
            }
            continue;
        }
        // The usual completion (dependents, waiting, live updates), as the owner.
        let as_owner = AuthUser {
            user: user.clone(),
            session_hash: None,
            token_hash: None,
            groups: groups.clone(),
        };
        if crate::routes::tasks::patch(
            State(state.clone()),
            as_owner,
            Path(it.task_id.clone()),
            Json(json!({"status": "done"})),
        )
        .await
        .is_err()
        {
            continue;
        }
        // Noted as done on paper.
        sqlx::query(
            "UPDATE task_events SET data = ?2 WHERE rowid = (SELECT rowid FROM task_events WHERE task_id = ?1 AND kind = 'completed' ORDER BY at DESC LIMIT 1)",
        )
        .bind(&it.task_id)
        .bind(json!({"source": "print"}).to_string())
        .execute(&state.db.write)
        .await?;
    }
    Ok(Json(view(&state, &row).await?))
}
