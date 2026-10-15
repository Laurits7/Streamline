//! GitHub, GitLab and Gitea/Forgejo accounts (7.6, D-97): connect with an access token,
//! choose what to bring in and where it goes, poll now, remove. The token is write-only:
//! stored encrypted and never returned.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use streamline_domain::forge::Rule;

use crate::{
    AppState,
    auth::AuthUser,
    error::{ApiResult, AppError, bad},
    forge::{self, Forge},
    models::{ForgeAccount, ForgeAccountView, ForgeRule},
    util::{new_id, now},
};

const MAX_ACCOUNTS: i64 = 20;
const MAX_RULES: usize = 50;

async fn load(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<ForgeAccount> {
    sqlx::query_as("SELECT * FROM forge_accounts WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id())
        .fetch_optional(&state.db.read)
        .await?
        .ok_or(AppError::NotFound)
}

fn check_kind(kind: &str) -> ApiResult<()> {
    if forge::KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(bad("kind must be github, gitlab or gitea"))
    }
}

fn check_token(token: &str) -> ApiResult<&str> {
    let t = token.trim();
    if t.is_empty() || t.len() > 1000 {
        return Err(bad("enter the access token"));
    }
    Ok(t)
}

#[utoipa::path(get, path = "/forges", tag = "forges", summary = "Your GitHub, GitLab and Gitea accounts", responses((status = 200, body = Vec<ForgeAccountView>), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ForgeAccountView>>> {
    let rows: Vec<ForgeAccount> =
        sqlx::query_as("SELECT * FROM forge_accounts WHERE user_id = ? ORDER BY created_at")
            .bind(user.id())
            .fetch_all(&state.db.read)
            .await?;
    Ok(Json(rows.iter().map(ForgeAccountView::from).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct NewForge {
    /// `github`, `gitlab` or `gitea` (Forgejo too).
    kind: String,
    /// The server's web address; empty for github.com or gitlab.com.
    #[serde(default)]
    url: String,
    token: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ForgeTest {
    ok: bool,
    /// Whose token it is.
    login: Option<String>,
    error: Option<String>,
}

#[utoipa::path(post, path = "/forges/test", tag = "forges", summary = "Try a token without saving it", request_body = NewForge, responses((status = 200, body = ForgeTest), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn test(_user: AuthUser, Json(c): Json<NewForge>) -> ApiResult<Json<ForgeTest>> {
    check_kind(&c.kind)?;
    let token = check_token(&c.token)?;
    let url = forge::normalize_url(&c.kind, &c.url).map_err(|e| bad(e.to_string()))?;
    let res = async { Forge::new(&c.kind, &url, token)?.login().await }.await;
    Ok(Json(match res {
        Ok(login) => ForgeTest {
            ok: true,
            login: Some(login),
            error: None,
        },
        Err(e) => ForgeTest {
            ok: false,
            login: None,
            error: Some(format!("{e:#}")),
        },
    }))
}

/// Connect an account (the token is checked first), then bring in what's yours.
#[utoipa::path(post, path = "/forges", tag = "forges", summary = "Connect an account with an access token, then poll it", request_body = NewForge, responses((status = 200, body = ForgeAccountView), (status = 400, description = "Invalid input or the token doesn't work", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<NewForge>,
) -> ApiResult<Json<ForgeAccountView>> {
    check_kind(&c.kind)?;
    let token = check_token(&c.token)?;
    let url = forge::normalize_url(&c.kind, &c.url).map_err(|e| bad(e.to_string()))?;
    let login = async { Forge::new(&c.kind, &url, token)?.login().await }
        .await
        .map_err(|e| bad(format!("{e:#}")))?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM forge_accounts WHERE user_id = ?")
        .bind(user.id())
        .fetch_one(&state.db.read)
        .await?;
    if count >= MAX_ACCOUNTS {
        return Err(bad("that's as many accounts as can be connected"));
    }
    let same: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM forge_accounts WHERE user_id = ? AND kind = ? AND url = ? AND login = ?)")
        .bind(user.id())
        .bind(&c.kind)
        .bind(&url)
        .bind(&login)
        .fetch_one(&state.db.read)
        .await?;
    if same {
        return Err(bad(format!(
            "{login} on {url} is already connected; change its token there"
        )));
    }
    let ts = now();
    let id = new_id();
    sqlx::query(
        "INSERT INTO forge_accounts (id, user_id, kind, url, login, secret, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(user.id())
    .bind(&c.kind)
    .bind(&url)
    .bind(&login)
    .bind(state.secrets.encrypt(token))
    .bind(&ts)
    .bind(&ts)
    .execute(&state.db.write)
    .await?;
    let synced = forge::sync_account(&state, &id).await?;
    Ok(Json(ForgeAccountView::from(&synced)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchForge {
    /// A new token (checked first); leave out to keep the stored one.
    token: Option<String>,
    issues: Option<bool>,
    prs: Option<bool>,
    reviews: Option<bool>,
    mentions: Option<bool>,
    /// Replaces the rules; the first matching one wins.
    rules: Option<Vec<ForgeRule>>,
}

#[utoipa::path(patch, path = "/forges/{id}", tag = "forges", summary = "Change what an account brings in, where it goes, or its token", params(("id" = String, Path, description = "ULID")), request_body = PatchForge, responses((status = 200, body = ForgeAccountView), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 404, description = "Not found"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(p): Json<PatchForge>,
) -> ApiResult<Json<ForgeAccountView>> {
    let mut a = load(&state, &user, &id).await?;
    // Anything that changes what's yours polls again on the next round.
    let mut repoll = false;
    if let Some(t) = &p.token {
        let token = check_token(t)?;
        a.login = async { Forge::new(&a.kind, &a.url, token)?.login().await }
            .await
            .map_err(|e| bad(format!("{e:#}")))?;
        a.secret = Some(state.secrets.encrypt(token));
        repoll = true;
    }
    for (field, v) in [
        (&mut a.issues, p.issues),
        (&mut a.prs, p.prs),
        (&mut a.reviews, p.reviews),
        (&mut a.mentions, p.mentions),
    ] {
        if let Some(v) = v
            && *field != v
        {
            *field = v;
            repoll = true;
        }
    }
    if let Some(rules) = p.rules {
        if rules.len() > MAX_RULES {
            return Err(bad("too many rules"));
        }
        let mut conn = state.db.read.acquire().await?;
        let mut out = vec![];
        for r in rules {
            let repo = r.repo.trim();
            if repo.is_empty() || repo.len() > 200 {
                return Err(bad(
                    "each rule needs a repository (like owner/repo or owner/*)",
                ));
            }
            crate::routes::projects::load_visible(&mut conn, &user, &r.project_id)
                .await
                .map_err(|_| bad("unknown project"))?;
            out.push(Rule {
                repo: repo.into(),
                project_id: r.project_id,
            });
        }
        a.rules = sqlx::types::Json(out);
    }
    a.updated_at = now();
    if repoll {
        a.last_sync_at = None;
    }
    sqlx::query("UPDATE forge_accounts SET login = ?, secret = ?, issues = ?, prs = ?, reviews = ?, mentions = ?, rules = ?, last_sync_at = ?, updated_at = ? WHERE id = ?")
        .bind(&a.login)
        .bind(&a.secret)
        .bind(a.issues)
        .bind(a.prs)
        .bind(a.reviews)
        .bind(a.mentions)
        .bind(&a.rules)
        .bind(&a.last_sync_at)
        .bind(&a.updated_at)
        .bind(&a.id)
        .execute(&state.db.write)
        .await?;
    Ok(Json(ForgeAccountView::from(&a)))
}

#[utoipa::path(post, path = "/forges/{id}/sync", tag = "forges", summary = "Poll an account now", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = ForgeAccountView), (status = 404, description = "Not found"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn sync_now(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ForgeAccountView>> {
    let a = load(&state, &user, &id).await?;
    let synced = forge::sync_account(&state, &a.id).await?;
    Ok(Json(ForgeAccountView::from(&synced)))
}

/// Remove an account and its token. Tasks it brought in stay, as ordinary tasks.
#[utoipa::path(delete, path = "/forges/{id}", tag = "forges", summary = "Remove an account (its tasks stay)", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 404, description = "Not found"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let a = load(&state, &user, &id).await?;
    let mut tx = state.db.write.begin().await?;
    sqlx::query("DELETE FROM forge_items WHERE account_id = ?")
        .bind(&a.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM forge_accounts WHERE id = ?")
        .bind(&a.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
